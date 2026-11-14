//! Running a data pack in the game: its `init` through the API's
//! [`Load::add_pack`], its Luau scripts, and its loot and scripts dropped
//! when it turns off.
//!
//! [`Load::add_pack`]: enshwick_core::Load::add_pack

use std::path::PathBuf;

use enshwick_core::abi::{HostApi, Level, ModHandle, Side, Status};
use enshwick_core::packs::{MANIFEST_FILE, Pack, PackFiles, PackManifest};

use super::with;

/// The scripts of `files`, when they hold a `main.luau`.
fn scripts_of(files: &PackFiles) -> Option<enshwick_luau::Scripts> {
    let mut scripts = enshwick_luau::Scripts::new();
    for (name, bytes) in files.scripts() {
        scripts = scripts.with(name, bytes);
    }
    scripts.has_main().then_some(scripts)
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

/// Forgets the loot and scripts of pack `id`, which is off; its hash stays
/// for the join check, which lists the pack as off with its files.
pub(crate) fn forget(id: &str) {
    // Dropped after the lock: a Luau runtime's teardown must not block the
    // join check's `hash_of`.
    let Some(runtime) = with(|packs| packs.get_mut(id).map(|p| p.runtime.take())) else {
        return;
    };
    let _forgotten = enshwick_core::packs::forget_loot(id);
    drop(runtime);
}
