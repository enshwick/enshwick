//! Content packs as mods: a folder `mods/<id>/` with a `pack.toml` and no
//! DLL. The loader gives each pack a record like a mod's, built from its
//! manifest, so the mod list, the lock and the join check see
//! it as they see a DLL, and runs it through the API's [`Load::add_pack`]
//! during init. The dedicated server watches a pack's files and applies a
//! changed `loot.toml` at once; other changes wait for a restart.
//!
//! [`Load::add_pack`]: enshwick::Load::add_pack

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use enshwick::abi::{
    Dependency, Header, HostApi, Level, Manifest, ManifestFlags, ModHandle, Relation, Side, Status,
    Str,
};
use enshwick::packs::{MANIFEST_FILE, Pack, PackFiles, PackManifest, PackSide};
use rustc_hash::FxHashMap;

use crate::host::Loader;

/// A pack folder found in `mods/`.
pub(crate) struct Found {
    /// Its manifest file relative to `mods/`, `<id>/pack.toml`.
    pub(crate) file: String,
    /// Its manifest, built from `pack.toml` for the process.
    pub(crate) manifest: &'static Manifest,
}

/// A pack the loader runs: its folder, the hash of its files and, once it
/// initialised, what it added.
struct Loaded {
    dir: PathBuf,
    hash: u32,
    pack: Option<Pack>,
}

static PACKS: Mutex<Option<FxHashMap<String, Loaded>>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut FxHashMap<String, Loaded>) -> R) -> R {
    let mut packs = PACKS.lock().unwrap_or_else(PoisonError::into_inner);
    f(packs.get_or_insert_with(FxHashMap::default))
}

/// A string for the process.
fn keep(text: &str) -> Str {
    Str::new(Box::leak(text.to_owned().into_boxed_str()))
}

/// A slice for the process; null for none.
fn keep_slice<T: 'static>(items: Vec<T>) -> (*const T, usize) {
    if items.is_empty() {
        return (core::ptr::null(), 0);
    }
    let items: &'static [T] = Box::leak(items.into_boxed_slice());
    (items.as_ptr(), items.len())
}

/// The manifest a mod would declare for the pack `manifest` says.
fn manifest_of(manifest: &PackManifest) -> &'static Manifest {
    let side = match manifest.side {
        PackSide::Server => Side::SERVER,
        PackSide::Client => Side::CLIENT,
        _ => Side::BOTH,
    };
    let dependencies: Vec<Dependency> = manifest
        .requires
        .iter()
        .map(|(id, range)| Dependency {
            id: keep(id),
            range: keep(range),
            relation: Relation::REQUIRES,
        })
        .collect();
    let (dependencies, dependency_count) = keep_slice(dependencies);
    let authors: Vec<Str> = manifest.authors.iter().map(|a| keep(a)).collect();
    let (authors, author_count) = keep_slice(authors);
    Box::leak(Box::new(Manifest {
        header: Header::of::<Manifest>(),
        id: keep(&manifest.id),
        version: keep(&manifest.version),
        side,
        flags: if manifest.join {
            ManifestFlags::JOIN
        } else {
            ManifestFlags::NONE
        },
        dependencies,
        dependency_count,
        builds: core::ptr::null(),
        build_count: 0,
        name: keep(&manifest.name),
        description: keep(&manifest.description),
        authors,
        author_count,
        license: keep(&manifest.license),
        homepage: keep(&manifest.homepage),
        points: Str::EMPTY,
    }))
}

/// Every pack folder under `mods_dir` with a readable `pack.toml` whose id
/// is the folder's name; the rest are logged and left out.
pub(crate) fn discover(mods_dir: &Path) -> Vec<Found> {
    let Ok(entries) = std::fs::read_dir(mods_dir) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join(MANIFEST_FILE).is_file())
        .collect();
    dirs.sort();
    let mut found = Vec::new();
    for dir in dirs {
        let name = dir
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name == crate::settings::LOADER_ID || name.starts_with('.') {
            continue;
        }
        let file = format!("{name}/{MANIFEST_FILE}");
        let manifest = match std::fs::read_to_string(dir.join(MANIFEST_FILE)) {
            Ok(text) => match PackManifest::parse(&text) {
                Ok(manifest) => manifest,
                Err(error) => {
                    crate::log!(WARN, "{name}/{error}");
                    continue;
                }
            },
            Err(e) => {
                crate::log!(WARN, "{file} skipped: {e}");
                continue;
            }
        };
        if manifest.id != name {
            crate::log!(
                WARN,
                "{file} skipped: its id {:?} is not its folder's name",
                manifest.id
            );
            continue;
        }
        let files = match PackFiles::read_dir(&dir) {
            Ok(files) => files,
            Err(why) => {
                crate::log!(WARN, "{name} skipped: {why}");
                continue;
            }
        };
        with(|packs| {
            packs.insert(
                manifest.id.clone(),
                Loaded {
                    dir: dir.clone(),
                    hash: files.hash(),
                    pack: None,
                },
            );
        });
        found.push(Found {
            file,
            manifest: manifest_of(&manifest),
        });
    }
    found
}

/// The hash of pack `id`'s files, as eight hex digits: the join check
/// counts it, so a server and a player with other files of one version
/// differ.
pub(crate) fn hash_of(id: &str) -> Option<String> {
    with(|packs| packs.get(id).map(|p| format!("{:08x}", p.hash)))
}

/// The SHA-256 of pack `id`'s files, for the lock: every file's name and
/// bytes in order.
pub(crate) fn sha256_of(id: &str) -> Option<String> {
    let dir = with(|packs| packs.get(id).map(|p| p.dir.clone()))?;
    let files = PackFiles::read_dir(&dir).ok()?;
    let mut all = Vec::new();
    for name in files.names() {
        all.extend_from_slice(name.as_bytes());
        all.push(0);
        all.extend_from_slice(files.get(name).unwrap_or_default());
    }
    Some(crate::modlist::sha256(&all))
}

/// Logs every problem of a pack under its id, one line each.
fn report(id: &str, errors: &enshwick::packs::ParsePackError) {
    for error in errors.list() {
        crate::log::write(Level::ERROR, id, &error.to_string());
    }
}

/// The record of the calling pack and its folder.
fn calling(me: ModHandle) -> Option<(&'static Loader, usize, String, PathBuf)> {
    let loader = crate::host::loader()?;
    let index = me.0.checked_sub(1)?;
    let record = loader.mods.get(index)?;
    let dir = with(|packs| packs.get(&record.id).map(|p| p.dir.clone()))?;
    Some((loader, index, record.id.clone(), dir))
}

/// A pack's `init`: reads its files and adds what they describe.
///
/// # Safety
///
/// - `api` is the loader's process-wide host table, valid for the process.
pub(crate) unsafe extern "system" fn init(api: *const HostApi, me: ModHandle) -> Status {
    let Some((_, _, id, dir)) = calling(me) else {
        return Status::BAD_ARGS;
    };
    let files = match PackFiles::read_dir(&dir) {
        Ok(files) => files,
        Err(why) => {
            crate::log::write(Level::ERROR, &id, &why.to_string());
            return Status::REFUSED;
        }
    };
    let pack = match Pack::parse(&id, &files) {
        Ok(pack) => pack,
        Err(errors) => {
            report(&id, &errors);
            return Status::REFUSED;
        }
    };
    // SAFETY: `api` is the loader's process-wide table, by this function's
    // `# Safety` contract, which is what `init_pack` asks for.
    let status = unsafe { enshwick::__private::init_pack(api, me, |load| load.add_pack(&pack)) };
    if status.is_ok() {
        crate::log::write(
            Level::INFO,
            &id,
            &format!(
                "content pack: {} ids, {} loot drops",
                pack.ids().len(),
                pack.drops().len()
            ),
        );
        with(|packs| {
            if let Some(loaded) = packs.get_mut(&id) {
                loaded.pack = Some(pack);
            }
        });
    }
    status
}

/// A pack's `ready`: nothing to start.
pub(crate) extern "system" fn ready(_api: *const HostApi, _me: ModHandle) {}

/// Forgets the loot of pack `id`, which is off.
pub(crate) fn forget(id: &str) {
    let known = with(|packs| packs.remove(id).is_some());
    if known {
        let _forgotten = enshwick::packs::reload_loot(id, Vec::new());
    }
}

/// The files of every running pack, for the watcher: `(record index,
/// path)`.
pub(crate) fn watched(loader: &Loader) -> Vec<(usize, PathBuf)> {
    loader
        .mods
        .iter()
        .enumerate()
        .filter_map(|(index, record)| {
            let dir = with(|packs| packs.get(&record.id).map(|p| p.dir.clone()))?;
            Some((index, dir))
        })
        .flat_map(|(index, dir)| {
            enshwick::packs::FILES
                .iter()
                .map(move |file| (index, dir.join(file)))
        })
        .collect()
}

/// A pack's files changed: reads them again and applies a changed
/// `loot.toml` at once; a problem or any other change is logged and waits.
pub(crate) fn changed(loader: &Loader, index: usize) {
    let Some(record) = loader.mods.get(index) else {
        return;
    };
    if loader.side != Side::SERVER || record.off.load(std::sync::atomic::Ordering::Acquire) {
        return;
    }
    let id = record.id.as_str();
    let Some(dir) = with(|packs| packs.get(id).map(|p| p.dir.clone())) else {
        return;
    };
    let pack = match PackFiles::read_dir(&dir).map(|files| Pack::parse(id, &files)) {
        Ok(Ok(pack)) => pack,
        Ok(Err(errors)) => {
            report(id, &errors);
            crate::log::write(
                Level::WARN,
                id,
                "content pack not reloaded: fix the files above",
            );
            return;
        }
        Err(why) => {
            crate::log::write(
                Level::WARN,
                id,
                &format!("content pack not reloaded: {why}"),
            );
            return;
        }
    };
    let same = with(|packs| {
        packs
            .get(id)
            .and_then(|p| p.pack.as_ref())
            .is_some_and(|old| old.same_content(&pack))
    });
    if !same {
        crate::log::write(
            Level::WARN,
            id,
            "content pack changed beyond its loot: items, recipes, workshops and texts change at the next start",
        );
    }
    match enshwick::packs::reload_loot(id, pack.drops().to_vec()) {
        Ok(()) => crate::log::write(
            Level::INFO,
            id,
            &format!("content pack reloaded: {} loot drops", pack.drops().len()),
        ),
        Err(why) => crate::log::write(Level::WARN, id, &format!("loot not reloaded: {why}")),
    }
    if same {
        with(|packs| {
            if let Some(loaded) = packs.get_mut(id) {
                loaded.pack = Some(pack);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pack_manifest_reads_as_a_mod_s() {
        let manifest = PackManifest::parse(
            "id = \"you.lanterns\"\nversion = \"1.2.0\"\nname = \"Lanterns\"\nhomepage = \"https://example.com/lanterns\"\n[requires]\nsafes = \">=0.5\"\n",
        )
        .unwrap();
        let manifest = manifest_of(&manifest);
        // SAFETY: `manifest_of` leaks the manifest and every string and slice
        // in it, so all its pointers are valid for the process.
        let candidate = unsafe { crate::manifest::parse(manifest) }.unwrap();
        assert_eq!(candidate.id, "you.lanterns");
        assert_eq!(candidate.version.to_string(), "1.2.0");
        assert_eq!(candidate.side, Side::BOTH);
        assert_eq!(candidate.dependencies.len(), 1);
        assert!(manifest.flags.contains(ManifestFlags::JOIN));
        // SAFETY: `manifest_of` leaks the manifest and every string and slice
        // in it, so all its pointers are valid for the process.
        let meta = unsafe { crate::manifest::meta(manifest) };
        assert_eq!(meta.homepage, "https://example.com/lanterns");
    }
}
