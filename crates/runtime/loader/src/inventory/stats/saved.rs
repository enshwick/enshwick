//! A world's file of saved modifiers: read when the world loads, written when
//! it saves and shuts down.

use std::path::PathBuf;

use enshwick_core::abi::Event;

use super::{STATE, Snapshot, State, World, serves, wanted};
use crate::engine::loader::Loader;
use enshwick_base::Relock;

/// The file of a world's saved modifiers.
const FILE: &str = "stats.bin";

/// A saved modifier, kept by the player's account, the mod's id and the
/// stat's id, so it reads back whatever the load order.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(super) struct Saved {
    pub(super) player: u64,
    pub(super) owner: String,
    pub(super) key: u32,
    pub(super) stat: String,
    pub(super) op: u8,
    pub(super) value: f32,
}

impl World {
    /// The key and bytes of the saved modifiers when they changed, which
    /// counts them written; `None` leaves them changed.
    pub(super) fn take_changed(&mut self) -> Option<(String, Vec<u8>)> {
        if !self.saved_changed {
            return None;
        }
        let key = self.key.clone()?;
        let bytes = postcard::to_allocvec(&self.saved).ok()?;
        self.saved_changed = false;
        Some((key, bytes))
    }

    /// A write of world `key`'s modifiers failed: they count as changed
    /// again, while that world still runs.
    pub(super) fn unwritten(&mut self, key: &str) {
        if self.key.as_deref() == Some(key) {
            self.saved_changed = true;
        }
    }
}

impl State {
    /// Writes the saved modifiers when they changed since the last write; a
    /// failed write leaves them changed, for the next save of that world.
    pub(super) fn save(&self) {
        let Some((key, bytes)) = self.with_world(World::take_changed) else {
            return;
        };
        let path = file(&key);
        if let Err(error) = enshwick_core::files::write_atomically(&path, &bytes) {
            self.with_world(|world| world.unwritten(&key));
            crate::log!(
                WARN,
                "stats: {} not written, kept for the next save: {error}",
                path.display()
            );
        }
    }
}

/// The file of world `key`'s saved modifiers.
fn file(key: &str) -> PathBuf {
    crate::base::world_folders::loader_data(key, FILE)
}

/// Follows the server's world: reads the saved modifiers once it has
/// loaded, writes them when it saves and when it shuts down.
pub(crate) fn world_event(loader: &Loader, event: Event) {
    if !wanted() || !serves() {
        return;
    }
    match event {
        Event::WORLD_LOADED => {
            let mut key = crate::base::saves::world_key(loader).map(str::to_owned);
            let read = key.as_deref().map_or(Ok(None), |key| {
                let path = file(key);
                enshwick_core::files::read_if_present(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|bytes| {
                        bytes
                            .map(|bytes| postcard::from_bytes::<Vec<Saved>>(&bytes))
                            .transpose()
                            .map_err(|e| e.to_string())
                    })
                    .map_err(|why| format!("{}: {why}", path.display()))
            });
            let saved = match read {
                Ok(saved) => saved.unwrap_or_default(),
                // With no world to write to, the file stays as it is.
                Err(why) => {
                    crate::log!(
                        WARN,
                        "stats: saved modifiers do not read and stay as they are: {why}"
                    );
                    key = None;
                    Vec::new()
                }
            };
            crate::log!(INFO, "stats: {} saved modifiers", saved.len());
            *STATE.world.relock() = Some(World {
                saved,
                key,
                ..World::default()
            });
            STATE.swap_in(Snapshot::default());
        }
        Event::WORLD_SAVED | Event::SHUTDOWN_STARTED => STATE.save(),
        _ => {}
    }
}
