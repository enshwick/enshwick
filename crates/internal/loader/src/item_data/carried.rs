//! Values on the items a player carries: kept under the character and slot
//! when the player leaves, taken back when the character joins again.

use std::sync::atomic::Ordering;

use enshwick::Entity;
use rustc_hash::FxHashSet;

use super::{
    KEPT, SERVER, Scope, Server, UNREAD, game, insert_found, kind, messages_for, removals, scope,
    send, serves, wanted,
};
use crate::fuse::Fuse;
use crate::host::Loader;
use crate::item_records::anchor::{self, Anchor, CharacterId, Pending};
use crate::locks::Relock;

/// Server steps between two looks of a joined character for its carried
/// items.
pub(super) const JOIN_LOOK_EVERY: u64 = 30;
/// Looks a joined character's records take once its inventories are
/// filled; past them the rest wait for its next join.
pub(super) const JOIN_LOOKS: u32 = 20;
/// Looks a joined character may take to fill its inventories at all; past
/// them its records wait for its next join.
pub(super) const JOIN_LOOKS_CAP: u32 = 12 * JOIN_LOOKS;

/// A joined character whose carried records look for their items.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Joining {
    pub(super) entity: Entity,
    /// Looks left before its inventories must be filled.
    cap: u32,
    /// Looks left once they are filled; [`JOIN_LOOKS`] until then.
    looks: u32,
    /// Whether its looks ran to their end on filled inventories: only then
    /// may its leave set aside what they did not find.
    pub(super) settled: bool,
}

impl Joining {
    pub(super) const fn new(entity: Entity) -> Self {
        Self {
            entity,
            cap: JOIN_LOOKS_CAP,
            looks: JOIN_LOOKS,
            settled: false,
        }
    }
}

/// Fuses of the carried item data's joins, looks and leaves: a panic in one
/// turns off only that, not the others nor the events of every mod. The
/// leave keeps its own, since it saves a leaving player's values.
static JOIN_FUSE: Fuse = Fuse::new("carried item data at a join");
static LOOK_FUSE: Fuse = Fuse::new("carried item data looks");
static LEAVE_FUSE: Fuse = Fuse::new("carried item data at a leave");

/// A player's character `entity` joined: when records it carried wait, they
/// look for its items once its inventories fill.
pub(crate) fn player_joined(loader: &Loader, entity: Entity) {
    let _ran = JOIN_FUSE.run("a player's join", || joined(loader, entity));
}

/// Looks for the joined characters' carried items, as [`find_carried`].
pub(super) fn look_for_carried(loader: &Loader) {
    let _ran = LOOK_FUSE.run("the post-step link", || find_carried(loader));
}

/// A player's character `entity` leaves, its items still in the world: the
/// values on them wait for the character's next join under its GUID, or go
/// aside where no GUID reads, rather than be dropped when the game destroys
/// the items it removes with the character. Its slot still names it.
#[cold]
pub(crate) fn player_left(loader: &Loader, entity: Entity) {
    let _ran = LEAVE_FUSE.run("a player's leave", || left(loader, entity));
}

fn joined(loader: &Loader, entity: Entity) {
    // A world whose things did not read attaches nothing this session.
    if !wanted() || !serves() || UNREAD.load(Ordering::Relaxed) {
        return;
    }
    let Some(character) = crate::world::character_of(loader, entity) else {
        return;
    };
    let mut server = SERVER.relock();
    let Some(server) = server.as_mut() else {
        return;
    };
    let waits = server.pending.iter().any(|record| {
        !record.aside
            && matches!(record.anchor, Anchor::Carried { character: c, .. } if c == character)
    });
    server.joining.retain(|joining| joining.entity != entity);
    if waits {
        server.joining.push(Joining::new(entity));
    }
}

fn left(loader: &Loader, entity: Entity) {
    if !wanted() || !serves() {
        return;
    }
    let characters = crate::world::characters(loader);
    let character_of = |root| crate::world::character_in(&characters, root);
    let messages = {
        let mut server = SERVER.relock();
        let Some(server) = server.as_mut() else {
            return;
        };
        let settled = server
            .joining
            .iter()
            .any(|joining| joining.entity == entity && joining.settled);
        server.joining.retain(|joining| joining.entity != entity);
        if let Some(character) = character_of(entity)
            && settled
            && !UNREAD.load(Ordering::Relaxed)
            && game::filled(loader, entity) == Some(true)
        {
            // Its looks have run on filled inventories, which read exactly
            // now: a record whose slot holds another item will not find its
            // own at a later join either.
            let gone = set_aside_waiting(&mut server.pending, character, |link, slot| {
                game::in_slot(loader, entity.0, link, slot).map(|(_, item)| item)
            });
            server.changed |= gone > 0;
        }
        let kept = keep_carried(server, entity, |pide| {
            game::held(loader, pide, character_of)
        });
        if kept.waiting + kept.aside > 0 {
            crate::log!(
                INFO,
                "mod data: {} values on a leaving player's items wait for its next join, {} set aside in the world's file",
                kept.waiting,
                kept.aside
            );
        }
        removals(&kept.moved)
    };
    send(&messages);
}

/// Moves the item values of `server` whose item `held` finds in an
/// inventory of `character` into its pending records: under the
/// character's GUID, slot and link to wait for its next join where the
/// anchor names one, else aside under the item's session id. A value whose
/// entity now holds another item is dropped, as a save drops it; an older
/// waiting record on the same slot goes aside. Every game read runs before
/// any value moves, so a panic in one loses none.
pub(super) fn keep_carried(
    server: &mut Server,
    character: Entity,
    held: impl Fn(u32) -> Option<game::Held>,
) -> Kept {
    let carried: Vec<((u32, u32), game::Held)> = server
        .values
        .iter()
        .filter(|&(&(kind, _), record)| scope(kind, record.anchor) == Some(Scope::Item))
        .filter_map(|(&key, _)| {
            held(key.1)
                .filter(|held| held.root == character.0)
                .map(|held| (key, held))
        })
        .collect();
    let mut left = Vec::with_capacity(carried.len());
    let mut moved = Vec::with_capacity(carried.len());
    for ((kind, pide), held) in carried {
        let Some(record) = server.values.remove(&(kind, pide)) else {
            continue;
        };
        moved.push((kind, pide));
        if held.item != record.item {
            continue;
        }
        let anchor = held
            .anchor
            .filter(|anchor| matches!(anchor, Anchor::Carried { .. }));
        left.push(Pending {
            kind,
            anchor: anchor.unwrap_or(Anchor::Session(pide)),
            check: held.item,
            value: record.value,
            aside: anchor.is_none(),
        });
    }
    let aside = left.iter().filter(|record| record.aside).count();
    let waiting = left.len() - aside;
    let newer = left
        .iter()
        .filter(|record| !record.aside)
        .map(|record| (record.kind, record.anchor));
    set_aside_older(&mut server.pending, newer);
    server.changed |= !moved.is_empty();
    server.pending.append(&mut left);
    Kept {
        moved,
        waiting,
        aside,
    }
}

/// What [`keep_carried`] did.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Kept {
    /// The kinds and item entities whose values left `values`.
    pub(super) moved: Vec<(u32, u32)>,
    /// Values that wait for the character's next join.
    pub(super) waiting: usize,
    /// Values set aside: no character GUID reads.
    pub(super) aside: usize,
}

/// Sets aside every waiting carried record of `pending` on a kind and slot
/// that `newer` names again: the newer value counts, so no slot is ever
/// named twice. Logs how many went aside; the number.
pub(super) fn set_aside_older(
    pending: &mut [Pending],
    newer: impl Iterator<Item = (u32, Anchor)>,
) -> usize {
    let newer: FxHashSet<(u32, Anchor)> = newer
        .filter(|(_, anchor)| matches!(anchor, Anchor::Carried { .. }))
        .collect();
    if newer.is_empty() {
        return 0;
    }
    let mut older = 0usize;
    for record in pending
        .iter_mut()
        .filter(|record| !record.aside && newer.contains(&(record.kind, record.anchor)))
    {
        record.aside = true;
        older += 1;
    }
    if older > 0 {
        crate::log!(
            INFO,
            "mod data: {older} older carried values set aside in the world's file, a newer value on the same slot counting instead"
        );
    }
    older
}

/// Sets aside each record of `pending` still waiting for `character` whose
/// slot, as `item_in(link, slot)` reads it when the character leaves with
/// filled inventories, holds another item. One whose slot reads empty or
/// not at all keeps waiting, as does one whose slot holds an item of its
/// id: neither says for certain that its item is gone. Logs how many; the
/// number.
pub(super) fn set_aside_waiting(
    pending: &mut [Pending],
    character: CharacterId,
    item_in: impl Fn(u8, u8) -> Option<u32>,
) -> usize {
    let mut gone = 0usize;
    for record in pending.iter_mut().filter(|record| !record.aside) {
        let Anchor::Carried {
            character: c,
            link,
            slot,
        } = record.anchor
        else {
            continue;
        };
        if c == character && item_in(link, slot).is_some_and(|item| item != record.check) {
            record.aside = true;
            gone += 1;
        }
    }
    if gone > 0 {
        crate::log!(
            INFO,
            "mod data: {gone} carried values set aside in the world's file: a leaving player's slots hold other items"
        );
    }
    gone
}

/// Looks for the items the joined characters carried, every
/// [`JOIN_LOOK_EVERY`] steps on the game thread. A look first waits for
/// the character's inventories to fill, at most [`JOIN_LOOKS_CAP`] looks;
/// from then on it takes back what lies in its slot for [`JOIN_LOOKS`]
/// looks. What finds no item waits in the world's file for the next join.
/// A character whose looks ran to their end stays listed, settled, until
/// it leaves.
#[cold]
fn find_carried(loader: &Loader) {
    let (messages, stats) = {
        let mut server = SERVER.relock();
        let Some(server) = server.as_mut() else {
            return;
        };
        if server.joining.iter().all(|joining| joining.settled) {
            return;
        }
        let characters = crate::world::characters(loader);
        let mut found = Vec::new();
        let mut at = 0;
        while let Some(&entry) = server.joining.get(at) {
            if entry.settled {
                at += 1;
                continue;
            }
            let mut entry = entry;
            let look = look(
                server,
                &characters,
                &mut entry,
                &mut found,
                |entity| game::ready(loader, entity),
                |entity, link, slot| game::in_slot(loader, entity.0, link, slot),
            );
            if let Look::Stop(why) = look {
                crate::log!(
                    INFO,
                    "mod data: a joined player's carried values wait in the world's file for its next join: {why}"
                );
            }
            match look {
                Look::Again => server.joining[at] = entry,
                Look::Done | Look::Stop(_) if entry.settled => server.joining[at] = entry,
                Look::Done | Look::Stop(_) => {
                    server.joining.swap_remove(at);
                    continue;
                }
            }
            at += 1;
        }
        KEPT.store(!server.values.is_empty(), Ordering::Relaxed);
        (
            messages_for(&server.values, &found),
            touches_item_stats(&found),
        )
    };
    send(&messages);
    if stats {
        crate::stats::items_changed();
    }
}

/// What a look of a joined character leaves to do.
pub(super) enum Look {
    /// Some records wait and the character looks again.
    Again,
    /// Nothing of the character waits any more.
    Done,
    /// Some records wait for the next join, for this reason.
    Stop(&'static str),
}

/// One look of `joining` for its carried items, adding the keys taken back
/// to `found`. `ready(entity)` is whether a character's inventories are
/// filled, `in_slot(entity, link, slot)` the item entity and item there.
pub(super) fn look(
    server: &mut Server,
    characters: &[(Entity, CharacterId)],
    joining: &mut Joining,
    found: &mut Vec<(u32, u32)>,
    ready: impl Fn(Entity) -> bool,
    in_slot: impl Fn(Entity, u8, u8) -> Option<(u32, u32)>,
) -> Look {
    let entity = joining.entity;
    let character = crate::world::character_in(characters, entity).filter(|_| ready(entity));
    let Some(character) = character else {
        joining.cap = joining.cap.saturating_sub(1);
        return if joining.cap == 0 {
            Look::Stop("no look ran, its inventories never read as filled")
        } else {
            Look::Again
        };
    };
    let back = take_back_carried(server, character, |link, slot| in_slot(entity, link, slot));
    if !back.found.is_empty() || back.replaced + back.ambiguous > 0 {
        crate::log!(
            INFO,
            "mod data: a joined player's items took {} values back; {} set aside, their items holding a newer value, and {} that named one slot twice",
            back.found.len(),
            back.replaced,
            back.ambiguous
        );
    }
    found.extend(back.found);
    if back.waiting == 0 {
        joining.settled = true;
        return Look::Done;
    }
    joining.looks = joining.looks.saturating_sub(1);
    if joining.looks == 0 {
        joining.settled = true;
        Look::Stop("its slots held no matching item")
    } else {
        Look::Again
    }
}

/// Attaches the pending records `character` carried to the item `held`
/// finds in the same link and slot, holding the same item. A record whose
/// item already has a newer value, or that shares its slot with another,
/// goes aside; the character's other records keep waiting.
pub(super) fn take_back_carried(
    server: &mut Server,
    character: CharacterId,
    held: impl Fn(u8, u8) -> Option<(u32, u32)>,
) -> TakenBack {
    let values = &server.values;
    let outcome = anchor::resolve_carried(&mut server.pending, character, held, |kind, pide| {
        values.contains_key(&(kind, pide))
    });
    let back = TakenBack {
        found: insert_found(&mut server.values, outcome.found),
        replaced: outcome.replaced,
        ambiguous: outcome.ambiguous,
        waiting: outcome.waiting,
    };
    server.changed |= !back.found.is_empty() || back.replaced + back.ambiguous > 0;
    back
}

/// What [`take_back_carried`] did.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct TakenBack {
    /// The kinds and item entities that took a value back.
    pub(super) found: Vec<(u32, u32)>,
    /// Records aside: their item already had a newer value.
    pub(super) replaced: usize,
    /// Records aside: two records named the same slot.
    pub(super) ambiguous: usize,
    /// Records of the character that still wait.
    pub(super) waiting: usize,
}

/// Whether any of the `found` kinds and entities is an item's stats, which
/// the entities that wear the item read.
pub(super) fn touches_item_stats(found: &[(u32, u32)]) -> bool {
    found
        .iter()
        .any(|&(id, _)| kind(id).is_some_and(|k| k.id == enshwick::stats::ITEM_KIND))
}
