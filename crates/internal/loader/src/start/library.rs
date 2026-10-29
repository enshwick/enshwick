//! Mod DLLs: finding them in `mods/`, loading them and reading their exports.

use std::os::windows::ffi::OsStrExt as _;
use std::path::{Path, PathBuf};

use enshwick::abi::{ABI_VERSION, HostApi, Manifest, ModHandle, Status, exports};
use windows_sys::Win32::Foundation::{FreeLibrary, HMODULE};
use windows_sys::Win32::System::LibraryLoader::{
    GetModuleHandleW, LOAD_WITH_ALTERED_SEARCH_PATH, LoadLibraryExW,
};

use crate::discovery::{self, ListRead, Skip};
use crate::modlist;
use crate::module::Export;
use crate::others::{Origin, OtherDll};
use crate::resolve::{self, Candidate};

type AbiFn = extern "system" fn() -> u32;

type QueryFn = extern "system" fn() -> *const Manifest;

pub(super) type InitFn = unsafe extern "system" fn(*const HostApi, ModHandle) -> Status;

pub(super) type ReadyFn = unsafe extern "system" fn(*const HostApi, ModHandle);

/// A mod DLL with its exports, or a data pack with the loader's.
pub(super) struct Library {
    /// The DLL's file name, for messages.
    pub(super) file: String,
    /// The loaded DLL, freed again when the mod list turns its id off; null
    /// for a pack.
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
    /// With dev reload: the DLL in `mods/` and the copy the mod runs from.
    pub(super) copied: Option<(PathBuf, PathBuf)>,
}

/// A mod DLL a dev reload loaded from a new copy.
pub(crate) struct Loaded {
    /// The module.
    pub(crate) module: HMODULE,
    /// Its manifest, the loader's copy.
    pub(crate) manifest: &'static Manifest,
    /// Its `init`.
    pub(crate) init: InitFn,
    /// Its `ready`.
    pub(crate) ready: ReadyFn,
}

/// Loads the mod DLL at `path` for a dev reload; `None`, logged, when it is
/// no mod of this ABI.
pub(crate) fn load_copy(path: &Path) -> Option<Loaded> {
    if crate::others::is_mod_file(path) == Some(false) {
        crate::log!(
            WARN,
            "{} is no Enshwick mod any more; it is not loaded",
            path.display()
        );
        return None;
    }
    let library = load_from(path, None)?;
    Some(Loaded {
        module: library.module,
        manifest: library.manifest,
        init: library.init,
        ready: library.ready,
    })
}

/// What `mods/` holds: the mods with readable manifests that the mod list
/// leaves on, their manifests, and the DLLs that run without the API; the
/// list gains the mods it lacked.
pub(super) fn libraries(
    mods_dir: &Path,
    run: Run,
    loaded: &mut dyn FnMut(&str),
) -> (Vec<Library>, Vec<Candidate>, Vec<OtherDll>) {
    let list_file = mods_dir
        .join(crate::settings::LOADER_ID)
        .join(modlist::LIST_FILE);
    let ListRead {
        mut list,
        writable,
        problem,
    } = discovery::read_list(std::fs::read_to_string(&list_file));
    if let Some(problem) = problem {
        crate::log!(WARN, "{problem}");
    }
    let mut changed = !list_file.exists();
    let mut libraries = Vec::new();
    let mut candidates = Vec::new();
    let packs = crate::packs::discover(mods_dir)
        .into_iter()
        .map(|found| Library {
            file: found.file,
            module: core::ptr::null_mut(),
            pack: true,
            range: None,
            manifest: found.manifest,
            copied: None,
            init: crate::packs::init,
            ready: crate::packs::ready,
        });
    let (found, mut others) = discover(mods_dir, &list, run, loaded);
    if run == Run::Game {
        others.extend(chained(mods_dir));
        others.extend(beside(mods_dir));
    }
    for library in found.into_iter().chain(packs) {
        // SAFETY: every manifest here is the copy `manifest::read` made for the process
        // or a pack's manifest built for the process, their strings and lists
        // readable and never written; a string `read` left in the DLL is read
        // while `discover` keeps it loaded.
        match unsafe { crate::manifest::parse(library.manifest) } {
            Ok(candidate) => {
                changed |= list.note(&candidate.id, &library.file);
                let ids = std::iter::once(candidate.id.as_str()).chain(candidate.former_ids());
                if let Some(off) = discovery::off_id(&list, ids) {
                    log_skip(&Skip::IdOff(off.to_owned()), &library.file);
                    unload(&library);
                    continue;
                }
                candidates.push(candidate);
                libraries.push(library);
            }
            Err(reason) => log_skip(&Skip::Manifest(reason), &library.file),
        }
    }
    let (libraries, candidates) = drop_duplicates(mods_dir, libraries, candidates);
    modlist::set_disabled(list.off());
    modlist::set_links_off(list.links_off());
    if changed
        && writable
        && let Err(e) = std::fs::write(&list_file, list.render())
    {
        crate::log!(WARN, "{} not written: {e}", modlist::LIST_PATH);
    }
    (libraries, candidates, others)
}

/// Writes the line of `skip` for the DLL `file` to the loader's log.
fn log_skip(skip: &Skip, file: &str) {
    let level = skip.level();
    if crate::log::enabled(level, crate::settings::LOADER_ID) {
        crate::log::write(level, crate::settings::LOADER_ID, &skip.line(file));
    }
}

/// The DLLs in `mods/chain/`, which the stub loads before the loader, and
/// whether each is loaded.
fn chained(mods_dir: &Path) -> Vec<OtherDll> {
    let Some(game_dir) = mods_dir.parent() else {
        return Vec::new();
    };
    dlls(&crate::others::chain_dir(game_dir))
        .into_iter()
        .filter_map(|path| {
            let file = path.file_name()?.to_string_lossy().into_owned();
            Some(OtherDll {
                file,
                origin: Origin::Chain,
                loaded: is_loaded(&path),
            })
        })
        .collect()
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
            OtherDll {
                file: name,
                origin: Origin::Game,
                loaded,
            }
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

/// Loads every `mods/*.dll` the mod list leaves on: the mods that export the
/// Enshwick interface, and in a game the DLLs without it, told apart by
/// their export tables before they load, which stay loaded and run on their
/// own.
fn discover(
    mods_dir: &Path,
    list: &modlist::ModList,
    run: Run,
    loaded: &mut dyn FnMut(&str),
) -> (Vec<Library>, Vec<OtherDll>) {
    let mut mods = Vec::new();
    let mut others = Vec::new();
    let files = dlls(mods_dir)
        .into_iter()
        .filter(|path| {
            let file = path
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default();
            let off = list.is_file_off(&file);
            if off {
                log_skip(&Skip::FileOff, &file);
            }
            !off
        })
        .collect::<Vec<_>>();
    for path in files {
        let file = path
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default();
        if crate::others::is_mod_file(&path) == Some(false) {
            if run == Run::Game {
                let native = load_native(&path);
                loaded(&file);
                if let Some(file) = native {
                    others.push(OtherDll {
                        file,
                        origin: Origin::Mods,
                        loaded: true,
                    });
                }
            }
        } else {
            let library = load(&path);
            loaded(&file);
            mods.extend(library);
        }
    }
    (mods, others)
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

/// Loads the native mod at `path` from where it is and keeps it loaded; its
/// file name, or `None`, logged, when it does not load.
fn load_native(path: &Path) -> Option<String> {
    let file = path.file_name()?.to_string_lossy().into_owned();
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    // SAFETY: `wide` is NUL-terminated; the DLL runs its `DllMain` as it
    // would from any loader and stays loaded for the process.
    let module = unsafe {
        LoadLibraryExW(
            wide.as_ptr(),
            core::ptr::null_mut(),
            LOAD_WITH_ALTERED_SEARCH_PATH,
        )
    };
    if module.is_null() {
        let error = std::io::Error::last_os_error();
        crate::log!(WARN, "{}", crate::errors::load_failure(&file, &error));
        return None;
    }
    crate::log!(INFO, "{file} is no Enshwick mod; it runs on its own");
    Some(file)
}

/// Loads the mod DLL at `path`, from a fresh copy with dev reload.
fn load(path: &Path) -> Option<Library> {
    if crate::reload::copies() {
        let mods_dir = path.parent()?;
        let copy = crate::reload::first_copy(mods_dir, path)?;
        let file = path.file_name()?.to_string_lossy().into_owned();
        let mut library = load_from(&copy, Some(&file))?;
        library.copied = Some((path.to_path_buf(), copy));
        return Some(library);
    }
    load_from(path, None)
}

/// Loads the DLL at `path`, named `file` in messages (its own name when
/// `None`).
fn load_from(path: &Path, named: Option<&str>) -> Option<Library> {
    let file = match named {
        Some(file) => file.to_owned(),
        None => path.file_name()?.to_string_lossy().into_owned(),
    };
    let wide: Vec<u16> = path
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain([0])
        .collect();
    // SAFETY: `wide` is NUL-terminated.
    let module = unsafe {
        LoadLibraryExW(
            wide.as_ptr(),
            core::ptr::null_mut(),
            LOAD_WITH_ALTERED_SEARCH_PATH,
        )
    };
    if module.is_null() {
        let error = std::io::Error::last_os_error();
        crate::log!(WARN, "{}", crate::errors::load_failure(&file, &error));
        return None;
    }
    let library = exports_of(module, &file);
    if library.is_none() {
        // SAFETY: `module` was loaded by `LoadLibraryExW` in this function, and
        // `exports_of` returned `None` without calling into it, so nothing of
        // it is in use.
        unsafe { FreeLibrary(module) };
    }
    library
}

/// The mod in the loaded `module`, named `file` in messages; `None`, logged,
/// when it exports no Enshwick interface of this ABI.
fn exports_of(module: HMODULE, file: &str) -> Option<Library> {
    // SAFETY: `module` was loaded by `load_from`, which frees it only after
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
    if abi() != ABI_VERSION {
        log_skip(&Skip::OtherAbi, file);
        return None;
    }
    // SAFETY: `query` returns the mod's static manifest, null or readable
    // for the size its header says, its strings and lists in the DLL's
    // statics, which stay loaded and unwritten for this call.
    let manifest = match unsafe { crate::manifest::read(query()) } {
        Ok(manifest) => manifest,
        Err(reason) => {
            log_skip(&Skip::Manifest(reason), file);
            return None;
        }
    };
    Some(Library {
        file: file.to_owned(),
        module,
        pack: false,
        range: crate::crash::range_of(module),
        manifest,
        init,
        ready,
        copied: None,
    })
}

#[cfg(test)]
mod tests;
