//! Writing `enshwick.lock`, the record of what ran.

use std::path::Path;
use std::sync::atomic::Ordering;

use crate::host::Loader;
use crate::modlist;

/// Writes `enshwick.lock` on a thread of its own: the loader and the mods
/// that run, `files` naming each mod's DLL by id; logs what changed since
/// the last start.
pub(super) fn write_lock(loader: &'static Loader, files: Vec<(String, String)>) {
    let spawned = std::thread::Builder::new()
        .name("enshwick-lock".into())
        .spawn(move || {
            let dir = loader.mods_dir.join(crate::settings::LOADER_ID);
            let hash = |path: &Path| {
                std::fs::read(path)
                    .map(|bytes| modlist::sha256(&bytes))
                    .unwrap_or_default()
            };
            let mods = loader
                .mods
                .iter()
                .filter(|m| !m.off.load(Ordering::Acquire))
                .filter_map(|m| {
                    let (_, file) = files.iter().find(|(id, _)| *id == m.id)?;
                    let sha256 = if m.pack {
                        crate::packs::sha256_of(&m.id).unwrap_or_default()
                    } else {
                        hash(&loader.mods_dir.join(file))
                    };
                    Some(modlist::Locked {
                        id: m.id.clone(),
                        version: m.version.clone(),
                        file: file.clone(),
                        sha256,
                    })
                })
                .collect();
            let lock = modlist::Lock {
                loader: env!("CARGO_PKG_VERSION").to_owned(),
                loader_sha256: hash(&dir.join(format!("{}.dll", crate::settings::LOADER_ID))),
                game: loader.build.name.clone(),
                mods,
                replaced: modlist::replaced().to_vec(),
            };
            let path = dir.join(modlist::LOCK_FILE);
            let old = match enshwick::files::read_to_string_if_present(&path) {
                Ok(text) => text.and_then(|text| modlist::Lock::parse(&text)),
                Err(e) => {
                    crate::log!(
                        WARN,
                        "{} is not read, so it is left as it is: {e}",
                        modlist::LOCK_FILE
                    );
                    return;
                }
            };
            if let Some(old) = &old {
                for change in lock.changes(old) {
                    crate::log!(INFO, "since the last start: {change}");
                }
            }
            if old.as_ref() != Some(&lock)
                && let Err(e) = enshwick::files::write_atomically(&path, lock.render())
            {
                crate::log!(WARN, "{} not written: {e}", modlist::LOCK_FILE);
            }
        });
    if let Err(e) = spawned {
        crate::log!(WARN, "{} not written: {e}", modlist::LOCK_FILE);
    }
}
