use std::time::Instant;

use enshwick::Entity;
use enshwick::abi::PlayerInfo;
use rustc_hash::FxHashSet;

use crate::host::Loader;

use super::{Combat, HEALING, SNAPSHOT, Sealed, State, TYPES};

/// The hit factors of an entity with `values`; `None` when it changes no
/// hit.
pub(super) fn combat(sealed: &Sealed, values: &[f32]) -> Option<Combat> {
    let at = |i: u32, default: f32| values.get(i as usize).copied().unwrap_or(default);
    let mut row = Combat {
        deals: [1.0; TYPES],
        takes: [1.0; TYPES],
    };
    let all_deal = at(sealed.damage, 1.0);
    let all_take = at(sealed.resistance, 0.0);
    for (i, ids) in sealed.by_type.iter().enumerate() {
        if i == HEALING {
            continue;
        }
        let (deal, take) = ids.map_or((1.0, 0.0), |(d, t)| (at(d, 1.0), at(t, 0.0)));
        row.deals[i] = all_deal * deal;
        row.takes[i] = 1.0 - (all_take + take).clamp(-1.0, 1.0);
    }
    (row.deals
        .iter()
        .chain(&row.takes)
        .any(|f| f.to_bits() != 1.0f32.to_bits()))
    .then_some(row)
}

/// Applies the hit factors of `source` and `target` to a hit's damage by
/// type. On the damage site's worker thread: no lock, no allocation.
pub(crate) fn on_hit(amounts: &mut [f32; TYPES], target: u32, source: u32) {
    let Some(values) = SNAPSHOT.get() else {
        return;
    };
    let values = values.load();
    if values.combat.is_empty() {
        return;
    }
    let deals = values.combat.get(&source).map(|c| c.deals);
    let takes = values.combat.get(&target).map(|c| c.takes);
    if deals.is_none() && takes.is_none() {
        return;
    }
    for (i, amount) in amounts.iter_mut().enumerate() {
        if i == HEALING {
            continue;
        }
        *amount *= deals.map_or(1.0, |d| d[i]) * takes.map_or(1.0, |t| t[i]);
    }
}

/// Drops the modifiers that ran out, by `now`.
pub(super) fn expire(state: &mut State, now: Instant) {
    if state.next_expiry.is_none_or(|at| at > now) {
        return;
    }
    let mut next = None;
    for watched in state.watched.values_mut() {
        let before = watched.modifiers.len();
        watched
            .modifiers
            .retain(|h| h.until.is_none_or(|at| at > now));
        watched.dirty |= watched.modifiers.len() != before;
        for at in watched.modifiers.iter().filter_map(|h| h.until) {
            next = Some(next.map_or(at, |n: Instant| n.min(at)));
        }
    }
    state.next_expiry = next;
}

/// Watches the players online and forgets the entities nothing keeps
/// watched or that left the world; whether it forgot any.
pub(super) fn follow(loader: &Loader, state: &mut State, players: &[PlayerInfo]) -> bool {
    let online: FxHashSet<u32> = players.iter().map(|p| p.entity.0).collect();
    for p in players {
        let watched = state.watched.entry(p.entity.0).or_default();
        if watched.player.is_none() {
            watched.player = Some(p.key.get());
            watched.dirty = true;
        }
    }
    let before = state.watched.len();
    state.watched.retain(|entity, w| {
        // An entry with values still to work out, or maxima points still
        // added, stays until a step takes them back.
        let keep = online.contains(entity)
            || w.asked
            || w.dirty
            || w.applied.iter().any(|&points| points != 0)
            || w.modifiers.iter().any(|h| !h.saved);
        keep && crate::world::template(loader, Entity(*entity)).is_ok()
    });
    state.watched.len() != before
}
