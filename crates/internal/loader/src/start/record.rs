//! Taking the record of what runs at start, and writing it into each world
//! that loads.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use crate::host::Loader;
use crate::record::{self, Ran, Record};

/// Takes the record on a thread of its own, reading each DLL's hash: the
/// loader and the mods that run, `folders` naming each mod's folder in
/// `mods/` and its file there, as `(id, folder, file)`.
pub(super) fn take(loader: &'static Loader, folders: Vec<(String, String, String)>) {
    let spawned = std::thread::Builder::new()
        .name("enshwick-record".into())
        .spawn(move || {
            let hash = |path: &Path| {
                std::fs::read(path)
                    .map(|bytes| record::sha256(&bytes))
                    .unwrap_or_default()
            };
            let mods = loader
                .mods
                .iter()
                .filter(|m| !m.off.load(Ordering::Acquire))
                .filter_map(|m| {
                    let (_, folder, file) = folders.iter().find(|(id, _, _)| *id == m.id)?;
                    let sha256 = if m.pack {
                        crate::packs::sha256_of(&m.id).unwrap_or_default()
                    } else {
                        hash(&loader.mods_dir.join(file))
                    };
                    Some(Ran {
                        id: m.id.clone(),
                        version: m.version.clone(),
                        folder: folder.clone(),
                        sha256,
                    })
                })
                .collect();
            let loader_dll = loader
                .mods_dir
                .join(crate::settings::LOADER_ID)
                .join(format!("{}.dll", crate::settings::LOADER_ID));
            record::set_taken(Record {
                loader: env!("CARGO_PKG_VERSION").to_owned(),
                loader_sha256: hash(&loader_dll),
                game: loader.build.name.clone(),
                mods,
                replaced: record::replaced().to_vec(),
            });
        });
    if let Err(e) = spawned {
        crate::log!(WARN, "{} not taken: {e}", record::RECORD_FILE);
    }
}

/// Where the world `key` keeps its record.
fn path_of(loader: &Loader, key: &str) -> PathBuf {
    loader
        .data_dir
        .join(crate::settings::LOADER_ID)
        .join("worlds")
        .join(key)
        .join(record::RECORD_FILE)
}

/// Writes the record into the world that loaded, on a thread of its own,
/// and logs what changed since that world last loaded.
pub(crate) fn write_for_world(loader: &'static Loader) {
    let (Some(taken), Some(key)) = (record::taken(), crate::worlds::current()) else {
        return;
    };
    let path = path_of(loader, &key);
    let spawned = std::thread::Builder::new()
        .name("enshwick-record".into())
        .spawn(move || {
            let old = match enshwick::files::read_to_string_if_present(&path) {
                Ok(text) => text.and_then(|text| Record::parse(&text)),
                Err(e) => {
                    crate::log!(
                        WARN,
                        "{} is not read, so it is left as it is: {e}",
                        record::RECORD_FILE
                    );
                    return;
                }
            };
            if let Some(old) = &old {
                for change in taken.changes(old) {
                    crate::log!(INFO, "since this world last loaded: {change}");
                }
            }
            if old.as_ref() != Some(taken)
                && let Err(e) = enshwick::files::write_atomically(&path, taken.render())
            {
                crate::log!(WARN, "{} not written: {e}", record::RECORD_FILE);
            }
        });
    if let Err(e) = spawned {
        crate::log!(WARN, "{} not written: {e}", record::RECORD_FILE);
    }
}
