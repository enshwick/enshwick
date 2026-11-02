//! Mods: one per folder of `mods/`, a DLL with the Enshwick exports or a
//! pack; finding them, loading them and reading their exports.

use std::os::windows::ffi::OsStrExt as _;
use std::path::{Path, PathBuf};

use enshwick::abi::{HostApi, Manifest, ModHandle, Status, check_major, exports};
use windows_sys::Win32::Foundation::{FreeLibrary, HMODULE};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

use crate::discovery::Skip;
use crate::module::Export;
use crate::others::OtherDll;
use crate::resolve::{self, Candidate};

type AbiFn = extern "system" fn() -> u32;

type QueryFn = extern "system" fn() -> *const Manifest;

pub(super) type InitFn = unsafe extern "system" fn(*const HostApi, ModHandle) -> Status;

pub(super) type ReadyFn = unsafe extern "system" fn(*const HostApi, ModHandle);

/// A mod DLL with its exports, or a data pack with the loader's.
pub(super) struct Library {
    /// Its folder in `mods/`.
    pub(super) folder: String,
    /// Its file relative to `mods/`, `<folder>/<file>`, for messages.
    pub(super) file: String,
    /// The loaded DLL, freed again when it is skipped; null for a pack.
    pub(super) module: HMODULE,
    /// Whether it is a data pack.
    pub(super) pack: bool,
    /// Where the DLL is mapped, for crash attribution.
    pub(super) range: Option<(usize, usize)>,
    /// Its manifest, the loader's copy.
    pub(super) manifest: &'static Manifest,
    /// Its `init` export.
    pub(super) init: InitFn,
    /// Its `ready` export.
    pub(super) ready: ReadyFn,
}

/// What `mods/` holds: the mods with readable manifests, one per folder, and
/// their manifests; in a game also the proxies of other mods beside it.
pub(super) fn libraries(
    mods_dir: &Path,
    run: Run,
    loaded: &mut dyn FnMut(&str),
) -> (Vec<Library>, Vec<Candidate>, Vec<OtherDll>) {
    if run == Run::Game {
        note_old_layout(mods_dir);
    }
    let packs = crate::packs::discover(mods_dir).into_iter().map(|found| {
        let folder = found
            .file
            .split_once('/')
            .map_or(found.file.as_str(), |(folder, _)| folder)
            .to_owned();
        Library {
            folder,
            file: found.file,
            module: core::ptr::null_mut(),
            pack: true,
            range: None,
            manifest: found.manifest,
            init: crate::packs::init,
            ready: crate::packs::ready,
        }
    });
    let found = discover(mods_dir, loaded);
    let others = if run == Run::Game {
        beside(mods_dir)
    } else {
        Vec::new()
    };
    let mut libraries = Vec::new();
    let mut candidates = Vec::new();
    for library in found.into_iter().chain(packs) {
        // SAFETY: every manifest here is the copy `manifest::read` made for the process
        // or a pack's manifest built for the process, their strings and lists
        // readable and never written; a string `read` left in the DLL is read
        // while `discover` keeps it loaded.
        match unsafe { crate::manifest::parse(library.manifest) } {
            Ok(candidate) => {
                candidates.push(candidate);
                libraries.push(library);
            }
            Err(reason) => log_skip(&Skip::Manifest(reason), &library.file),
        }
    }
    let (libraries, candidates) = drop_duplicates(mods_dir, libraries, candidates);
    (libraries, candidates, others)
}

/// Logs one line for each file or folder of the layout before mod folders
/// that the loader no longer reads.
fn note_old_layout(mods_dir: &Path) {
    /// The chain folder the stub loaded and the mod list, relative to `mods/`.
    const OLD: [&str; 2] = ["chain", "enshwick/mods.toml"];
    for dll in dlls(mods_dir) {
        crate::log!(
            WARN,
            "mods/{} is not read: a mod lives in a folder of its own, mods/<mod>/",
            dll.file_name()
                .map(|f| f.to_string_lossy())
                .unwrap_or_default()
        );
    }
    for path in OLD {
        if mods_dir.join(path).exists() {
            crate::log!(
                WARN,
                "mods/{path} is not read: a mod runs while its folder is in mods/"
            );
        }
    }
    if mods_dir.with_file_name("mod-data").exists() {
        crate::log!(
            WARN,
            "mod-data/ is not read: a world's mod data lives in <save>-mod-data/ beside its save, \
             and the loader's own files in mods/enshwick/; move them by hand or delete mod-data/"
        );
    }
}

/// Writes the line of `skip` for the DLL `file` to the loader's log.
fn log_skip(skip: &Skip, file: &str) {
    let level = enshwick::abi::Level::WARN;
    if crate::log::enabled(level, crate::settings::LOADER_ID) {
        crate::log::write(level, crate::settings::LOADER_ID, &skip.line(file));
    }
}

/// Other mods' proxies in the game's folder, as
/// [`crate::others::beside_game`] finds them, and whether each is loaded.
fn beside(mods_dir: &Path) -> Vec<OtherDll> {
    let Some(game_dir) = mods_dir.parent() else {
        return Vec::new();
    };
    crate::others::beside_game(game_dir)
        .into_iter()
        .map(|name| {
            let loaded = is_loaded(&game_dir.join(&name));
            crate::log!(
                INFO,
                "{name} beside the game is another mod's proxy{}",
                if loaded {
                    "; it runs"
                } else {
                    "; it was not loaded at start"
                }
            );
            OtherDll { file: name, loaded }
        })
        .collect()
}

/// Whether a module loaded from `path` is in the process now.
fn is_loaded(path: &Path) -> bool {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    // SAFETY: a NUL-terminated path; a module of that path is looked up,
    // never loaded.
    !unsafe { GetModuleHandleW(wide.as_ptr()) }.is_null()
}

/// The `*.dll` files directly in `dir`, sorted; none when it is missing.
fn dlls(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("dll")))
        .collect();
    files.sort_by_cached_key(|path| path.file_name().map(std::ffi::OsStr::to_ascii_lowercase));
    files
}

/// Keeps one file per mod id, the one [`resolve::duplicates`] chooses, and
/// unloads the others, naming in the log each file that can be deleted.
fn drop_duplicates(
    mods_dir: &Path,
    libraries: Vec<Library>,
    candidates: Vec<Candidate>,
) -> (Vec<Library>, Vec<Candidate>) {
    let installed: Vec<resolve::Installed<'_>> = libraries
        .iter()
        .zip(&candidates)
        .map(|(library, candidate)| resolve::Installed {
            id: &candidate.id,
            version: &candidate.version,
            modified: std::fs::metadata(mods_dir.join(&library.file))
                .and_then(|meta| meta.modified())
                .ok(),
            file: &library.file,
        })
        .collect();
    let duplicates = resolve::duplicates(&installed);
    if duplicates.is_empty() {
        return (libraries, candidates);
    }
    for &(extra, runs) in &duplicates {
        let skip = Skip::Duplicate {
            id: candidates[extra].id.clone(),
            version: candidates[extra].version.to_string(),
            runs: libraries[runs].file.clone(),
            runs_version: candidates[runs].version.to_string(),
        };
        log_skip(&skip, &libraries[extra].file);
    }
    libraries
        .into_iter()
        .zip(candidates)
        .enumerate()
        .filter_map(|(i, pair)| {
            if !duplicates.iter().any(|&(extra, _)| extra == i) {
                return Some(pair);
            }
            unload(&pair.0);
            None
        })
        .unzip()
}

/// Unloads a mod DLL that `discover` loaded only for its manifest, so none of
/// its code runs and its file can be deleted while the game runs.
pub(super) fn unload(library: &Library) {
    if !library.module.is_null() {
        // SAFETY: loaded by `discover`; none of its code is in use, and its
        // manifest was parsed into an owned `Candidate` before this call.
        unsafe { FreeLibrary(library.module) };
    }
}

/// Loads the one DLL with the Enshwick exports in each folder of `mods/`,
/// told apart from the folder's other DLLs, the mod's own, by their export
/// tables before they load. A second such DLL in a folder is skipped, named
/// with the first; the loader's own folder holds no mod.
fn discover(mods_dir: &Path, loaded: &mut dyn FnMut(&str)) -> Vec<Library> {
    let mut mods = Vec::new();
    for folder in folders(mods_dir) {
        let name = folder
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut first: Option<String> = None;
        for path in dlls(&folder) {
            if crate::others::is_mod_file(&path) == Some(false) {
                continue;
            }
            let file = format!(
                "{name}/{}",
                path.file_name()
                    .map(|f| f.to_string_lossy())
                    .unwrap_or_default()
            );
            if let Some(other) = &first {
                log_skip(
                    &Skip::SecondMod {
                        other: other.clone(),
                    },
                    &file,
                );
                continue;
            }
            let library = load(&path, &name, &file);
            loaded(&file);
            first = Some(file);
            mods.extend(library);
        }
    }
    mods
}

/// The folders directly in `mods/`, sorted, without the loader's own.
fn folders(mods_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(mods_dir) else {
        return Vec::new();
    };
    let mut folders: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .is_some_and(|f| !f.eq_ignore_ascii_case(crate::settings::LOADER_ID))
        })
        .collect();
    folders.sort_by_cached_key(|path| path.file_name().map(std::ffi::OsStr::to_ascii_lowercase));
    folders
}

/// Whether the loader starts a game or tries mods in a dry run, which loads
/// no DLL without the Enshwick interface and finds no stub before it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Run {
    /// In the game, from the stub.
    Game,
    /// A dry run, in a process of its own.
    Dry,
}

/// Loads the mod DLL at `path`, in `folder` of `mods/` and named `file`
/// there; `None`, logged, when it is no mod of this ABI.
fn load(path: &Path, folder: &str, file: &str) -> Option<Library> {
    let module = match crate::module::load_library(path) {
        Ok(module) => module,
        Err(error) => {
            crate::log!(WARN, "{}", crate::errors::load_failure(file, &error));
            return None;
        }
    };
    let library = exports_of(module, folder, file);
    if library.is_none() {
        // SAFETY: `module` was loaded by `load_library` in this function, and
        // `exports_of` returned `None` without calling into it, so nothing of
        // it is in use.
        unsafe { FreeLibrary(module) };
    }
    library
}

/// The mod in the loaded `module`, from `folder` of `mods/` and named `file`
/// in messages; `None`, logged, when it exports no Enshwick interface of
/// this ABI.
fn exports_of(module: HMODULE, folder: &str, file: &str) -> Option<Library> {
    // SAFETY: `module` was loaded by `load`, which frees it only after
    // this returns; the `Module` does not leave this function, and the
    // exports it finds are called only while the library stays loaded.
    let found = unsafe { crate::module::Module::from_handle(module) };
    let find = |name: &str| found.export(name);
    let (Some(abi), Some(query), Some(init), Some(ready)) = (
        find(exports::ABI),
        find(exports::QUERY),
        find(exports::INIT),
        find(exports::READY),
    ) else {
        log_skip(&Skip::NotAMod, file);
        return None;
    };
    // SAFETY: a mod's `ABI` export has the signature `enshwick::abi::exports`
    // documents for it.
    let abi = unsafe { core::mem::transmute::<Export, AbiFn>(abi) };
    // SAFETY: a mod's `QUERY` export has the signature `enshwick::abi::exports`
    // documents for it.
    let query = unsafe { core::mem::transmute::<Export, QueryFn>(query) };
    // SAFETY: a mod's `INIT` export has the signature `enshwick::abi::exports`
    // documents for it.
    let init = unsafe { core::mem::transmute::<Export, InitFn>(init) };
    // SAFETY: a mod's `READY` export has the signature `enshwick::abi::exports`
    // documents for it.
    let ready = unsafe { core::mem::transmute::<Export, ReadyFn>(ready) };
    // The export states only the major; the manifest's header holds the
    // minor, which `manifest::parse` checks.
    if let Err(why) = check_major(abi()) {
        log_skip(&Skip::OtherAbi(why), file);
        return None;
    }
    // SAFETY: `query` returns null or the mod's manifest, readable for the
    // size its header says, its strings and lists unwritten for the process
    // (`abi::exports::QUERY`).
    let manifest = match unsafe { crate::manifest::read(query()) } {
        Ok(manifest) => manifest,
        Err(reason) => {
            log_skip(&Skip::Manifest(reason), file);
            return None;
        }
    };
    Some(Library {
        folder: folder.to_owned(),
        file: file.to_owned(),
        module,
        pack: false,
        range: crate::crash::range_of(module),
        manifest,
        init,
        ready,
    })
}

#[cfg(test)]
mod tests;
