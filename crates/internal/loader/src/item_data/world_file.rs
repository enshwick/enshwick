//! A world's file of values: read when the world loads, written when it saves and shuts down.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use enshwick::Entity;
use enshwick::abi::Event;
use rustc_hash::FxHashSet;

use super::carried::set_aside_older;
use super::{
    FILE, FINDING, KEPT, KINDS, Kind, SERVER, Scope, Server, UNREAD, Values, game, kind_of, scope,
    serves, take_destroyed, template_id, wanted,
};
use crate::host::Loader;
use crate::item_records::anchor::{Anchor, Pending};
use crate::item_records::file::{self, Saved};
use crate::locks::Relock;

/// The file of world `key`'s values.
fn file(loader: &Loader, key: &str) -> PathBuf {
    loader
        .data_dir
        .join("enshwick")
        .join("worlds")
        .join(key)
        .join(FILE)
}

/// The records a world's file holds, each waiting for its entity, and the
/// kinds among them no mod opened, which number on from the opened ones;
/// `None` for a file the loader does not read, such as one of a newer
/// loader or a damaged one.
pub(super) fn read_file(bytes: &[u8]) -> Option<(Vec<Pending>, Vec<String>)> {
    let opened = KINDS.sealed().map_or(0, <[Kind]>::len);
    let mut unopened: Vec<String> = Vec::new();
    let mut pending = Vec::new();
    for saved in file::read(bytes)? {
        let kind = if let Some(kind) = kind_of(saved.id) {
            kind
        } else {
            let at = unopened
                .iter()
                .position(|id| id == saved.id)
                .unwrap_or_else(|| {
                    unopened.push(saved.id.to_owned());
                    unopened.len() - 1
                });
            u32::try_from(opened + at).ok()?
        };
        pending.push(Pending {
            kind,
            anchor: saved.anchor,
            check: saved.check,
            value: saved.value.into(),
            aside: saved.aside,
        });
    }
    Some((pending, unopened))
}

/// The file's bytes: `server`'s values under their anchors, a value seen
/// with none under its entity id, which sets it aside at the next load;
/// then the records waiting or aside.
pub(super) fn write_file(server: &Server) -> Vec<u8> {
    let mut sorted: Vec<_> = server.values.iter().collect();
    sorted.sort_by_key(|(key, _)| **key);
    let values = sorted.into_iter().filter_map(|(&(kind, entity), record)| {
        Some(Saved {
            id: server.kind_id(kind)?,
            anchor: record.anchor.unwrap_or(Anchor::Session(entity)),
            check: record.item,
            value: &record.value,
            aside: false,
        })
    });
    let pending = server.pending.iter().filter_map(|record| {
        Some(Saved {
            id: server.kind_id(record.kind)?,
            anchor: record.anchor,
            check: record.check,
            value: &record.value,
            aside: record.aside,
        })
    });
    file::write(values.chain(pending))
}

/// The records of the world file at `path`, as [`read_file`] gives them;
/// none when no file is there.
///
/// # Errors
///
/// Why a file that is there does not read: the system refused it, it is cut
/// short, or it is of a version this loader does not know.
pub(super) fn read_world(path: &Path) -> Result<(Vec<Pending>, Vec<String>), ReadWorldDataError> {
    match enshwick::files::read_if_present(path) {
        Ok(None) => Ok(Default::default()),
        Ok(Some(bytes)) => read_file(&bytes).ok_or_else(|| ReadWorldDataError::Unknown {
            path: path.to_owned(),
        }),
        Err(source) => Err(ReadWorldDataError::Io {
            path: path.to_owned(),
            source,
        }),
    }
}

/// Why a world's file of mod data that is there does not read.
#[derive(Debug, thiserror::Error)]
pub(super) enum ReadWorldDataError {
    /// The system refused the file.
    #[error("{} does not read: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The file is cut short or of a version this loader does not know.
    #[error(
        "{} does not read to its end, or is of a version this loader does not know",
        path.display()
    )]
    Unknown { path: PathBuf },
}

/// A write of world `key`'s values failed: the next save writes them again,
/// while that world still runs.
pub(super) fn unwritten(server: &mut Option<Server>, key: &str) {
    if let Some(server) = server.as_mut().filter(|s| s.key.as_deref() == Some(key)) {
        server.changed = true;
    }
}

/// Follows the server's world: reads the world's values once it has
/// loaded, writes them when it saves and when it shuts down.
pub(crate) fn world_event(loader: &Loader, event: Event) {
    if !wanted() || !serves() {
        return;
    }
    match event {
        Event::WORLD_LOADED => {
            let key = crate::saves::world_key(loader).map(str::to_owned);
            let read = key.as_deref().map_or_else(
                || Ok(Default::default()),
                |key| read_world(&file(loader, key)),
            );
            let (pending, unopened) = match read {
                Ok(records) => records,
                Err(why) => {
                    // Keep a file this loader cannot read from being
                    // overwritten: the world runs without its values.
                    crate::log!(
                        WARN,
                        "mod data: {why}; it stays as it is and this world keeps no mod data"
                    );
                    *SERVER.relock() = None;
                    return;
                }
            };
            crate::log!(
                INFO,
                "mod data: {} values for world {}",
                pending.len(),
                key.as_deref().unwrap_or("?")
            );
            KEPT.store(false, Ordering::Relaxed);
            UNREAD.store(false, Ordering::Relaxed);
            FINDING.store(!pending.is_empty(), Ordering::Relaxed);
            let _earlier_world = take_destroyed();
            *SERVER.relock() = Some(Server {
                key,
                values: Values::default(),
                pending,
                unopened,
                joining: Vec::new(),
                changed: false,
            });
        }
        Event::WORLD_SAVED | Event::SHUTDOWN_STARTED => save(loader, event == Event::WORLD_SAVED),
        _ => {}
    }
}

/// After a save of the game's, reads each value's anchor afresh from its
/// entity. A value whose entity holds another item or template now is
/// dropped. One whose entity the world no longer holds goes aside: every
/// saved entity exists from the load on (server 399), so it is gone, and a
/// later entity may take its place. Nothing changes where the world does
/// not read. The number of values with no anchor.
fn refresh(loader: &Loader, server: &mut Server) -> usize {
    if !game::readable(loader) {
        return 0;
    }
    // The placed things the world holds; `None` when the walk does not read,
    // and then no entity value counts as gone.
    let placed: Option<FxHashSet<u32>> = game::transform_type(loader)
        .and_then(|ty| crate::entities::with_component(loader, ty).ok())
        .map(|places| places.iter().map(|p| p.entity.0).collect());
    let characters = crate::world::characters(loader);
    let character_of = |root| crate::world::character_in(&characters, root);
    let before = server.values.len();
    let mut anchorless = 0;
    let mut gone = Vec::new();
    server.values.retain(|&(kind, entity), record| {
        // `Some(None)`: the world no longer holds the entity.
        let now = match scope(kind, record.anchor) {
            Some(Scope::Item) => {
                Some(game::held(loader, entity, character_of).map(|held| (held.item, held.anchor)))
            }
            Some(Scope::Entity) => match &placed {
                Some(placed) if !placed.contains(&entity) => Some(None),
                Some(_) => template_id(Entity(entity)).map(|template| {
                    Some((template, game::placed(loader, entity).map(Anchor::Placed)))
                }),
                None => None,
            },
            None => None,
        };
        match now {
            None => true,
            Some(None) => {
                gone.push(Pending {
                    kind,
                    anchor: record.anchor.unwrap_or(Anchor::Session(entity)),
                    check: record.item,
                    value: core::mem::take(&mut record.value),
                    aside: true,
                });
                false
            }
            Some(Some((held, _))) if held != record.item => false,
            Some(Some((_, anchor))) => {
                // A live entity in no place a load or a join finds loses its
                // anchor: the old one names a slot another item may fill.
                if anchor != record.anchor {
                    record.anchor = anchor;
                    server.changed = true;
                }
                anchorless += usize::from(record.anchor.is_none());
                true
            }
        }
    });
    if !gone.is_empty() {
        crate::log!(
            WARN,
            "mod data: {} values set aside: the world no longer holds their entities",
            gone.len()
        );
    }
    server.pending.append(&mut gone);
    server.changed |= server.values.len() != before;
    anchorless
}

/// Reads afresh, at shutdown, the anchor of each value whose item a player
/// in game carries now or carried at the last save, so the next join finds
/// it where it is now and no old slot is left naming it. Other values keep
/// the anchors of the game's last save, which a load rebuilds; a value
/// whose item does not read or holds another item keeps its anchor.
fn refresh_carried(loader: &Loader, server: &mut Server) {
    let characters = crate::world::characters(loader);
    if characters.is_empty() || !game::readable(loader) {
        return;
    }
    let character_of = |root| crate::world::character_in(&characters, root);
    let mut changed = false;
    for (&(kind, pide), record) in &mut server.values {
        if scope(kind, record.anchor) != Some(Scope::Item) {
            continue;
        }
        let Some(held) = game::held(loader, pide, character_of).filter(|h| h.item == record.item)
        else {
            continue;
        };
        let carried = |anchor: Option<Anchor>| matches!(anchor, Some(Anchor::Carried { .. }));
        if carried(held.anchor) || carried(record.anchor) {
            changed |= core::mem::replace(&mut record.anchor, held.anchor) != held.anchor;
        }
    }
    server.changed |= changed;
}

/// Writes the values when they changed, under their anchors as the world
/// holds them now.
fn save(loader: &Loader, after_game_save: bool) {
    let (key, bytes, anchorless) = {
        let mut server = SERVER.relock();
        let Some(server) = server.as_mut() else {
            return;
        };
        // Before the first step the records wait as read, and the values set
        // since carry the anchors they were set with. At shutdown the world
        // is being torn down: the anchors of its last save stand, but for
        // the items players carry, whose slots change all the time.
        let anchorless = if FINDING.load(Ordering::Relaxed) {
            0
        } else if after_game_save {
            refresh(loader, server)
        } else {
            refresh_carried(loader, server);
            0
        };
        // A carried value, its anchor read afresh, on a slot a record from
        // an earlier leave still waits on counts: written so, the next load
        // would name the slot twice.
        if !FINDING.load(Ordering::Relaxed) {
            let newer = server
                .values
                .iter()
                .filter_map(|(&(kind, _), record)| record.anchor.map(|anchor| (kind, anchor)));
            server.changed |= set_aside_older(&mut server.pending, newer) > 0;
        }
        if !server.changed {
            return;
        }
        let Some(key) = server.key.clone() else {
            return;
        };
        server.changed = false;
        (key, write_file(server), anchorless)
    };
    if anchorless > 0 {
        crate::log!(
            WARN,
            "mod data: {anchorless} item values sit where neither a load nor a join can find them; after a restart they stay aside unless the item is in a placed container or a player's inventory when the world saves"
        );
    }
    let path = file(loader, &key);
    if let Err(error) = enshwick::files::write_atomically(&path, &bytes) {
        unwritten(&mut SERVER.relock(), &key);
        crate::log!(
            WARN,
            "mod data: {} not written, kept for the next save: {error}",
            path.display()
        );
    }
}
