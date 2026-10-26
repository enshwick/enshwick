//! Mod DLLs: finding them in `mods/`, loading them and reading their exports.

use std::path::{Path, PathBuf};

use enshwick::abi::{ABI_MAJOR, HostApi, Manifest, ModHandle, Status, exports};
use windows_sys::Win32::Foundation::{FreeLibrary, HMODULE};
use windows_sys::Win32::System::LibraryLoader::{
    GetProcAddress, LOAD_WITH_ALTERED_SEARCH_PATH, LoadLibraryExW,
};

use crate::modlist;
use crate::resolve::{self, Candidate};

type AbiFn = extern "system" fn() -> u32;

type QueryFn = extern "system" fn() -> *const Manifest;

pub(super) type InitFn = unsafe extern "system" fn(*const HostApi, ModHandle) -> Status;

pub(super) type ReadyFn = unsafe extern "system" fn(*const HostApi, ModHandle);

type Export = unsafe extern "system" fn() -> isize;

/// A mod DLL with its exports, or a content pack with the loader's.
pub(super) struct Library {
    /// The DLL's file name, for messages.
    pub(super) file: String,
    /// The loaded DLL, freed again when the mod list turns its id off; null
    /// for a pack.
    pub(super) module: HMODULE,
    /// Whether it is a content pack.
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
    let library = load_from(path, None)?;
    Some(Loaded {
        module: library.module,
        manifest: library.manifest,
        init: library.init,
        ready: library.ready,
    })
}

/// The mods in `mods/` with readable manifests that the mod list leaves on,
/// and their manifests; the list gains the mods it lacked.
pub(super) fn libraries(mods_dir: &Path) -> (Vec<Library>, Vec<Candidate>) {
    let list_file = mods_dir
        .join(crate::settings::LOADER_ID)
        .join(modlist::LIST_FILE);
    let (mut list, writable) = match std::fs::read_to_string(&list_file) {
        Ok(text) => match modlist::ModList::parse(&text) {
            Ok(list) => (list, true),
            Err(reason) => {
                crate::log!(
                    WARN,
                    "{} is not read, every mod is on: {reason}",
                    modlist::LIST_PATH
                );
                (modlist::ModList::default(), false)
            }
        },
        Err(_) => (modlist::ModList::default(), true),
    };
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
    for library in discover(mods_dir, &list).into_iter().chain(packs) {
        // SAFETY: the manifest is static data inside its loaded DLL.
        match unsafe { crate::manifest::parse(library.manifest) } {
            Ok(candidate) => {
                if !enshwick::abi::is_valid_id(&candidate.id) {
                    crate::log!(
                        WARN,
                        "{}: the id {} has no author; the next Enshwick refuses it, so update the mod",
                        library.file,
                        candidate.id
                    );
                }
                changed |= list.note(&candidate.id, &library.file);
                let off = std::iter::once(candidate.id.as_str())
                    .chain(candidate.former_ids())
                    .find(|id| list.is_off(id));
                if let Some(off) = off {
                    crate::log!(
                        INFO,
                        "{} skipped: {off} is off in {}",
                        library.file,
                        modlist::LIST_PATH
                    );
                    unload(&library);
                    continue;
                }
                candidates.push(candidate);
                libraries.push(library);
            }
            Err(reason) => crate::log!(WARN, "{} skipped: {reason}", library.file),
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
    (libraries, candidates)
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
        crate::log!(
            WARN,
            "{} skipped: {} {} is also in {}, which runs {}; {} can be deleted",
            libraries[extra].file,
            candidates[extra].id,
            candidates[extra].version,
            libraries[runs].file,
            candidates[runs].version,
            libraries[extra].file
        );
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

/// Loads every `mods/*.dll` that exports the Enshwick interface.
fn discover(mods_dir: &Path, list: &modlist::ModList) -> Vec<Library> {
    let Ok(entries) = std::fs::read_dir(mods_dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("dll")))
        .collect();
    files.sort();
    files
        .iter()
        .filter(|path| {
            let file = path
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default();
            let off = list.is_file_off(&file);
            if off {
                crate::log!(INFO, "{file} skipped: off in {}", modlist::LIST_PATH);
            }
            !off
        })
        .filter_map(|path| load(path))
        .collect()
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
        crate::log!(WARN, "{file} could not be loaded");
        return None;
    }
    let library = exports_of(module, &file);
    if library.is_none() {
        // SAFETY: the module was loaded above and nothing of it is in use.
        unsafe { FreeLibrary(module) };
    }
    library
}

/// The mod in the loaded `module`, named `file` in messages; `None`, logged,
/// when it exports no Enshwick interface of this ABI.
fn exports_of(module: HMODULE, file: &str) -> Option<Library> {
    let find = |name: &str| {
        let name = format!("{name}\0");
        // SAFETY: `module` is loaded and `name` is NUL-terminated.
        unsafe { GetProcAddress(module, name.as_ptr()) }
    };
    let (Some(abi), Some(query), Some(init), Some(ready)) = (
        find(exports::ABI),
        find(exports::QUERY),
        find(exports::INIT),
        find(exports::READY),
    ) else {
        crate::log!(WARN, "{file} is not an Enshwick mod");
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
    if abi() != u32::from(ABI_MAJOR) {
        crate::log!(
            WARN,
            "{file} is built for ABI {}, the loader speaks {ABI_MAJOR}",
            abi()
        );
        return None;
    }
    // SAFETY: the manifest is static data inside the DLL, which stays loaded.
    let manifest = match unsafe { crate::manifest::read(query()) } {
        Ok(manifest) => manifest,
        Err(reason) => {
            crate::log!(WARN, "{file} skipped: {reason}");
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
