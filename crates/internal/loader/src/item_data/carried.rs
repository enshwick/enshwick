//! Values on the items a player carries: kept under the character and slot
//! when the player leaves, taken back when the character joins again.

use std::sync::atomic::Ordering;

use enshwick::Entity;
use rustc_hash::FxHashSet;

use super::{
    KEPT, SERVER, Scope, Server, game, insert_found, kind, messages_for, scope, send, serves,
    wanted,
};
use crate::host::Loader;
use crate::item_records::anchor::{self, Anchor, CharacterId, Pending};
use crate::locks::Relock;

/// Steps between two looks of a joined character for its carried items,
/// about half a second.
pub(super) const JOIN_LOOK_EVERY: u64 = 30;
/// Looks a joined character's records take once its inventories are
/// filled, about ten seconds; past them the rest wait for its next join.
pub(super) const JOIN_LOOKS: u32 = 20;
/// Looks a joined character may take to fill its inventories at all, about
/// two minutes; past them its records wait for its next join.
pub(super) const JOIN_LOOKS_CAP: u32 = 240;

/// A joined character whose carried records look for their items.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Joining {
    pub(super) entity: Entity,
    /// Looks left before its inventories must be filled.
    cap: u32,
    /// Looks left once they are filled; [`JOIN_LOOKS`] until then.
    looks: u32,
}

impl Joining {
    pub(super) const fn new(entity: Entity) -> Self {
        Self {
            entity,
            cap: JOIN_LOOKS_CAP,
            looks: JOIN_LOOKS,
        }
    }
}

/// A player's character `entity` joined: when records it carried wait, they
/// look for its items once its inventories fill.
pub(crate) fn player_joined(loader: &Loader, entity: Entity) {
    if !wanted() || !serves() {
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

/// A player's character `entity` leaves, its items still in the world: the
/// values on them wait for the character's next join under its GUID, or go
/// aside where no GUID reads, rather than be dropped when the game destroys
/// the items it removes with the character. Its slot still names it.
#[cold]
pub(crate) fn player_left(loader: &Loader, entity: Entity) {
    if !wanted() || !serves() {
        return;
    }
    let characters = crate::world::characters(loader);
    let character_of = |root| crate::world::character_in(&characters, root);
    let mut server = SERVER.relock();
    let Some(server) = server.as_mut() else {
        return;
    };
    server.joining.retain(|joining| joining.entity != entity);
    if let Some(character) = character_of(entity) {
        // The inventory reads exactly now: what an earlier join could not
        // find takes its item back or is gone, so no slot waits stale.
        let _back = take_back_carried(server, character, |link, slot| {
            game::in_slot(loader, entity.0, link, slot)
        });
        server.changed |= set_aside_waiting(&mut server.pending, character) > 0;
    }
    let (waiting, aside) = keep_carried(server, entity, |pide| {
        game::held(loader, pide, character_of)
    });
    if waiting + aside > 0 {
        crate::log!(
            INFO,
            "mod data: {waiting} values on a leaving player's items wait for its next join, {aside} set aside in the world's file"
        );
    }
}

/// Moves the item values of `server` whose item `held` finds in an
/// inventory of `character` into its pending records: under the
/// character's GUID, slot and link to wait for its next join where the
/// anchor names one, else aside under the item's session id. An older
/// waiting record on the same slot goes aside. The number waiting and the
/// number aside.
pub(super) fn keep_carried(
    server: &mut Server,
    character: Entity,
    held: impl Fn(u32) -> Option<game::Held>,
) -> (usize, usize) {
    let mut left = Vec::new();
    server.values.retain(|&(kind, pide), record| {
        if scope(kind, record.anchor) != Some(Scope::Item) {
            return true;
        }
        let Some(held) = held(pide).filter(|held| held.root == character.0) else {
            return true;
        };
        let carried = held
            .anchor
            .filter(|anchor| matches!(anchor, Anchor::Carried { .. }));
        left.push(Pending {
            kind,
            anchor: carried.unwrap_or(Anchor::Session(pide)),
            check: held.item,
            value: core::mem::take(&mut record.value),
            aside: carried.is_none(),
        });
        false
    });
    let aside = left.iter().filter(|record| record.aside).count();
    let waiting = left.len() - aside;
    let newer = left
        .iter()
        .filter(|record| !record.aside)
        .map(|record| (record.kind, record.anchor));
    let _older = set_aside_older(&mut server.pending, newer);
    server.changed |= !left.is_empty();
    server.pending.append(&mut left);
    (waiting, aside)
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

/// Sets aside every record of `pending` still waiting for `character`, as
/// it leaves with its slots read: their items are gone from those slots.
/// Logs how many; the number.
pub(super) fn set_aside_waiting(pending: &mut [Pending], character: CharacterId) -> usize {
    let mut stale = 0usize;
    for record in pending.iter_mut().filter(|record| {
        !record.aside
            && matches!(record.anchor, Anchor::Carried { character: c, .. } if c == character)
    }) {
        record.aside = true;
        stale += 1;
    }
    if stale > 0 {
        crate::log!(
            INFO,
            "mod data: {stale} carried values set aside in the world's file: a leaving player's slots no longer hold their items"
        );
    }
    stale
}

/// Looks for the items the joined characters carried, every
/// [`JOIN_LOOK_EVERY`] steps on the game thread. A look first waits for
/// the character's inventories to fill, at most [`JOIN_LOOKS_CAP`] looks;
/// from then on it takes back what lies in its slot for [`JOIN_LOOKS`]
/// looks. What finds no item waits in the world's file for the next join.
#[cold]
pub(super) fn find_carried(loader: &Loader) {
    let (messages, stats) = {
        let mut server = SERVER.relock();
        let Some(server) = server.as_mut() else {
            return;
        };
        if server.joining.is_empty() {
            return;
        }
        let characters = crate::world::characters(loader);
        let mut joining = std::mem::take(&mut server.joining);
        let mut found = Vec::new();
        joining.retain_mut(
            |joining| match look(
                server,
                &characters,
                joining,
                &mut found,
                |entity| game::ready(loader, entity.0),
                |entity, link, slot| game::in_slot(loader, entity.0, link, slot),
            ) {
                Look::Again => true,
                Look::Done => false,
                Look::Stop(why) => {
                    crate::log!(
                        INFO,
                        "mod data: a joined player's carried values wait in the world's file for its next join: {why}"
                    );
                    false
                }
            },
        );
        server.joining = joining;
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
    if !back.found.is_empty() || back.replaced + back.older > 0 {
        crate::log!(
            INFO,
            "mod data: a joined player's items took {} values back; {} set aside, their items holding a newer value, and {} that named one slot twice",
            back.found.len(),
            back.replaced,
            back.older
        );
    }
    found.extend(back.found);
    if back.waiting == 0 {
        return Look::Done;
    }
    joining.looks = joining.looks.saturating_sub(1);
    if joining.looks == 0 {
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
    let pending = std::mem::take(&mut server.pending);
    let values = &server.values;
    let outcome = anchor::resolve_carried(pending, character, held, |kind, pide| {
        values.contains_key(&(kind, pide))
    });
    let back = TakenBack {
        found: insert_found(&mut server.values, outcome.found),
        replaced: outcome.replaced,
        older: outcome.ambiguous,
        waiting: outcome.waiting,
    };
    server.pending = outcome.left;
    server.changed |= !back.found.is_empty() || back.replaced + back.older > 0;
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
    pub(super) older: usize,
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
