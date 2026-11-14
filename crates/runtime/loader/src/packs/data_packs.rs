//! Data packs as mods: a folder `mods/<id>/` with a `pack.toml` and no
//! DLL. The loader gives each pack a record like a mod's, built from its
//! manifest, so the mod list, the lock and the join check see
//! it as they see a DLL, and runs it through the API's [`Load::add_pack`]
//! during init; a pack with a `main.luau` is a Luau mod, whose scripts run
//! in a sandbox of their own (`enshwick-luau`). A change to a pack's files
//! takes a restart.
//!
//! [`Load::add_pack`]: enshwick_core::Load::add_pack

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use enshwick_core::abi::{
    Dependency, Header, HostApi, Level, Manifest, ManifestFlags, ModHandle, Side, Status, Str,
};
use enshwick_core::packs::{MANIFEST_FILE, Pack, PackFiles, PackManifest, PackSide};
use rustc_hash::FxHashMap;

use enshwick_base::Relock;

/// A pack folder found in `mods/`.
pub(crate) struct Found {
    /// Its manifest file relative to `mods/`, `<id>/pack.toml`.
    pub(crate) file: String,
    /// Its manifest, built from `pack.toml` for the process.
    pub(crate) manifest: &'static Manifest,
}

/// A pack the loader runs: its folder, the hash of its files and, once it
/// initialised, its running scripts.
struct Loaded {
    dir: PathBuf,
    hash: u32,
    runtime: Option<enshwick_luau::Runtime>,
}

/// The scripts of `files`, when they hold a `main.luau`.
fn scripts_of(files: &PackFiles) -> Option<enshwick_luau::Scripts> {
    let mut scripts = enshwick_luau::Scripts::new();
    for (name, bytes) in files.scripts() {
        scripts = scripts.with(name, bytes);
    }
    scripts.has_main().then_some(scripts)
}

/// The packs domain's state: the packs `discover` found, by id. It lives for
/// the process, not a hosted game: packs load once at the start, and one
/// that turns off stays off until a restart, so no part's
/// [`forget`](crate::engine::parts::Part::forget) drops it; only [`forget`]
/// drops the pack that turned off.
struct State {
    packs: Mutex<FxHashMap<String, Loaded>>,
}

static STATE: State = State {
    packs: Mutex::new(FxHashMap::with_hasher(rustc_hash::FxBuildHasher)),
};

fn with<R>(f: impl FnOnce(&mut FxHashMap<String, Loaded>) -> R) -> R {
    f(&mut STATE.packs.relock())
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
        _ => Side::BOTH,
    };
    let dependencies: Vec<Dependency> = manifest
        .dependencies()
        .map(|(relation, _, dependency)| Dependency {
            id: keep(&dependency.id),
            range: keep(&dependency.version),
            relation,
        })
        .collect();
    let (dependencies, dependency_count) = keep_slice(dependencies);
    let authors: Vec<Str> = manifest.authors.iter().map(|a| keep(a)).collect();
    let (authors, author_count) = keep_slice(authors);
    let exports: Vec<Str> = manifest.exports.iter().map(|name| keep(name)).collect();
    let (exports, export_count) = keep_slice(exports);
    Box::leak(Box::new(Manifest {
        header: Header::of::<Manifest>(),
        id: keep(&manifest.id),
        version: keep(&manifest.version),
        side,
        flags: ManifestFlags::NONE,
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
        exports,
        export_count,
    }))
}

/// Every pack folder under `mods_dir` with a readable `pack.toml` whose id
/// is the folder's name and that is made for this Enshwick; the rest are
/// logged and left out.
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
        if name == crate::engine::settings::LOADER_ID || name.starts_with('.') {
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
        if let Some(why) = crate::base::manifest::enshwick_refusal(&manifest.enshwick) {
            crate::log!(
                WARN,
                "{name} skipped: {why} ({})",
                crate::engine::errors::OTHER_ABI
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
                    runtime: None,
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
    Some(crate::base::record::sha256(&all))
}

/// Logs every problem of a pack under its id, one line each.
fn report(id: &str, errors: &enshwick_core::packs::ParsePackError) {
    for error in errors.list() {
        crate::engine::log::write(Level::ERROR, id, &error.to_string());
    }
}

/// Logs every change a pack skips because the game dropped what it names.
fn report_skipped(id: &str, pack: &Pack) {
    for skipped in pack.skipped() {
        crate::engine::log::write(Level::WARN, id, &skipped.to_string());
    }
}

/// The side, id and folder of the calling pack, as the loader keeps them;
/// its other calls check the same side.
fn calling(me: ModHandle) -> Option<(Side, String, PathBuf)> {
    let loader = crate::engine::loader::loader()?;
    let record = loader.mods.get(me.0.checked_sub(1)?)?;
    let dir = with(|packs| packs.get(&record.id).map(|p| p.dir.clone()))?;
    Some((record.manifest.side, record.id.clone(), dir))
}

/// A pack's `init`: reads its files and adds what they describe.
///
/// # Safety
///
/// - `api` is the loader's process-wide host table, valid for the process.
pub(crate) unsafe extern "system" fn init(api: *const HostApi, me: ModHandle) -> Status {
    let Some((side, id, dir)) = calling(me) else {
        return Status::BAD_ARGS;
    };
    let files = match PackFiles::read_dir(&dir) {
        Ok(files) => files,
        Err(why) => {
            crate::engine::log::write(Level::ERROR, &id, &why.to_string());
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
    report_skipped(&id, &pack);
    let manifest = std::fs::read_to_string(dir.join(MANIFEST_FILE))
        .ok()
        .and_then(|text| PackManifest::parse(&text).ok().map(|m| (m, text)));
    if let Some((manifest, text)) = &manifest
        && let Err(problem) = manifest.check(text, &pack)
    {
        crate::engine::log::write(Level::ERROR, &id, &problem.to_string());
        return Status::REFUSED;
    }
    let scripts = scripts_of(&files);
    with(|packs| {
        if let Some(loaded) = packs.get_mut(&id) {
            loaded.hash = files.hash();
        }
    });
    let mut runtime = None;
    // SAFETY: `api` is the loader's process-wide table, by this function's
    // `# Safety` contract, which is what `init_pack` asks for.
    let status = unsafe {
        enshwick_core::__private::init_pack(api, me, side, |load| {
            load.add_pack(&pack)?;
            let scripts = scripts.map(|s| s.with_items(&pack.item_bindings()));
            let limits = enshwick_luau::Limits::default();
            runtime = match (scripts, &manifest) {
                (None, _) => None,
                (Some(scripts), Some((manifest, _))) => {
                    Some(enshwick_luau::start_pack(load, manifest, scripts, limits)?)
                }
                (Some(scripts), None) => {
                    Some(enshwick_luau::start(load, &id, scripts, None, limits)?)
                }
            };
            Ok(())
        })
    };
    if status.is_ok() {
        let luau = if runtime.is_some() {
            ", Luau scripts running"
        } else {
            ""
        };
        crate::engine::log::write(
            Level::INFO,
            &id,
            &format!(
                "data pack: {} ids, {} loot drops, {} changes, {} selections{luau}",
                pack.ids().len(),
                pack.drops().len(),
                pack.changes().len(),
                pack.selections().len() + pack.recipe_selections().len()
            ),
        );
        with(|packs| {
            if let Some(loaded) = packs.get_mut(&id) {
                loaded.runtime = runtime;
            }
        });
    }
    status
}

/// A pack's `ready`: nothing to start.
pub(crate) extern "system" fn ready(_api: *const HostApi, _me: ModHandle) {}

/// Forgets the loot of pack `id`, which is off.
pub(crate) fn forget(id: &str) {
    // Dropped after the lock: a Luau runtime's teardown must not block the
    // join check's `hash_of`.
    let Some(gone) = with(|packs| packs.remove(id)) else {
        return;
    };
    let _forgotten = enshwick_core::packs::forget_loot(id);
    drop(gone);
}

#[cfg(test)]
mod tests {
    use enshwick_core::abi::Relation;

    use super::*;

    #[test]
    fn a_pack_manifest_reads_as_a_mod_s() {
        let manifest = PackManifest::parse(
            "id = \"you.lanterns\"\nversion = \"1.2.0\"\nname = \"Lanterns\"\nhomepage = \"https://example.com/lanterns\"\n[requires]\nmarket = { id = \"you.trading-post\", version = \">=0.5\" }\n[after]\nlockers = \"you.lockers\"\n[breaks]\nold = \"old.lanterns\"\n",
        )
        .unwrap();
        let manifest = manifest_of(&manifest);
        // SAFETY: `manifest_of` leaks the manifest and every string and slice
        // in it, so all its pointers are valid for the process.
        let candidate = unsafe { crate::base::manifest::parse(manifest) }.unwrap();
        assert_eq!(candidate.id, "you.lanterns");
        assert_eq!(candidate.version.to_string(), "1.2.0");
        assert_eq!(candidate.side, Side::BOTH);
        let relations: Vec<_> = candidate
            .dependencies
            .iter()
            .map(|d| {
                (
                    d.id.as_str(),
                    d.range.as_ref().map(ToString::to_string),
                    d.relation,
                )
            })
            .collect();
        assert_eq!(
            relations,
            [
                (
                    "you.trading-post",
                    Some(">=0.5".to_owned()),
                    Relation::REQUIRES
                ),
                ("you.lockers", None, Relation::AFTER),
                ("old.lanterns", None, Relation::BREAKS),
            ]
        );
        assert_eq!(manifest.flags, ManifestFlags::NONE);
        // SAFETY: `manifest_of` leaks the manifest and every string and slice
        // in it, so all its pointers are valid for the process.
        let meta = unsafe { crate::base::manifest::meta(manifest) };
        assert_eq!(meta.homepage, "https://example.com/lanterns");
    }
}
