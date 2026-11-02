//! A loaded world's records finding their entities again at its first step, by their anchors.

use std::sync::atomic::Ordering;

use rustc_hash::FxHashSet;

use super::{FINDING, KEPT, MESSAGE_BYTES, Record, SERVER, Server, UNREAD, game, kind};
use crate::host::Loader;
use crate::item_records::anchor::{self, Anchor, Index, Pending};
use crate::item_records::encode;
use crate::locks::Relock;

/// The placed things whose templates the `pending` records name, by key;
/// `None` when any part of the walk does not read, since a thing missing
/// from the index would set its records aside for good.
fn index(loader: &Loader, pending: &[Pending]) -> Option<Index> {
    let mut index = Index::default();
    let wanted: FxHashSet<[u8; 16]> = pending
        .iter()
        .filter(|record| !record.aside)
        .filter_map(|record| match record.anchor {
            Anchor::Placed(placed) | Anchor::Held { root: placed, .. } => Some(placed.template()),
            Anchor::Session(_) => None,
        })
        .collect();
    if wanted.is_empty() {
        return Some(index);
    }
    let ty = game::transform_type(loader)?;
    let places = crate::entities::with_component(loader, ty).ok()?;
    let mut unread = 0usize;
    for place in places {
        let entity = place.entity.0;
        let Ok(template) = crate::world::template(loader, place.entity) else {
            unread += 1;
            continue;
        };
        if wanted.contains(&template) {
            let key = game::placed(loader, entity);
            unread += usize::from(key.is_none());
            if let Some(key) = key {
                index.add(key, entity);
            }
        }
    }
    if unread > 0 {
        crate::log!(
            WARN,
            "mod data: {unread} placed things did not read; the values wait as read for the next load"
        );
        return None;
    }
    Some(index)
}

/// What [`attach`] did, beside the values it attached.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Attached {
    /// The kinds and entities that took a value.
    found: Vec<(u32, u32)>,
    /// Records newly aside: on shared places, on empty ones, replaced.
    ambiguous: usize,
    missing: usize,
    replaced: usize,
    /// Records aside from before.
    earlier: usize,
}

/// Attaches `server`'s pending records to the entities `index` names, as
/// the first step after a load does. With no index, because the world did
/// not read, every record stays exactly as read and nothing is marked
/// changed: no record goes aside for a read that failed.
pub(super) fn attach(
    server: &mut Server,
    index: Option<&Index>,
    held: impl Fn(u32, u8, u8) -> Option<(u32, u32)>,
) -> Option<Attached> {
    let index = index?;
    let taken: FxHashSet<(u32, u32)> = server.values.keys().copied().collect();
    let outcome = anchor::resolve(
        std::mem::take(&mut server.pending),
        index,
        held,
        |kind, entity| taken.contains(&(kind, entity)),
    );
    let mut attached = Attached {
        found: Vec::with_capacity(outcome.found.len()),
        ambiguous: outcome.ambiguous,
        missing: outcome.missing,
        replaced: outcome.replaced,
        earlier: outcome.earlier,
    };
    for (kind, entity, record) in outcome.found {
        attached.found.push((kind, entity));
        server.values.insert(
            (kind, entity),
            Record {
                item: record.check,
                anchor: Some(record.anchor),
                value: record.value,
            },
        );
    }
    server.pending = outcome.left;
    // Records newly aside are written so, whatever else changes.
    server.changed |=
        attached.ambiguous + attached.missing + attached.replaced + attached.earlier > 0;
    Some(attached)
}

/// Finds the entities of the records a load read, once, on the game thread
/// after the world's first step; records it cannot name for certain go
/// aside, and all wait as read when the world does not read.
#[cold]
pub(super) fn find_pending(loader: &Loader) {
    if crate::world::server().is_err() {
        return;
    }
    FINDING.store(false, Ordering::Relaxed);
    let pending = {
        let server = SERVER.relock();
        let Some(server) = server.as_ref() else {
            return;
        };
        if server.pending.is_empty() {
            return;
        }
        server.pending.clone()
    };
    let started = std::time::Instant::now();
    let index = index(loader, &pending);
    let mut message = Vec::new();
    let attached = {
        let mut server = SERVER.relock();
        let Some(server) = server.as_mut() else {
            return;
        };
        let attached = attach(server, index.as_ref(), |root, link, slot| {
            game::in_slot(loader, root, link, slot)
        });
        for &(kind, entity) in attached.iter().flat_map(|a| &a.found) {
            let (Some(spec), Some(record)) = (
                self::kind(kind).filter(|k| k.sent),
                server.values.get(&(kind, entity)),
            ) else {
                continue;
            };
            encode(&mut message, &spec.id, entity, record.item, &record.value);
            if message.len() > MESSAGE_BYTES {
                crate::net::item_data(0, &std::mem::take(&mut message));
            }
        }
        KEPT.store(!server.values.is_empty(), Ordering::Relaxed);
        attached
    };
    if !message.is_empty() {
        crate::net::item_data(0, &message);
    }
    let Some(attached) = attached else {
        UNREAD.store(true, Ordering::Relaxed);
        crate::log!(
            WARN,
            "mod data: the world's placed things did not read on this game build; {} values wait as read and no mod data attaches this session",
            pending.len()
        );
        return;
    };
    crate::log!(
        INFO,
        "mod data: {} values found their entities again after the load in {} ms; {} stay aside from before",
        attached.found.len(),
        started.elapsed().as_millis(),
        attached.earlier
    );
    let aside = attached.ambiguous + attached.missing + attached.replaced;
    if aside > 0 {
        crate::log!(
            WARN,
            "mod data: {aside} values set aside in the world's file, attached to no entity and never again: {} at a place more than one entity shares, {} at a place no entity holds, {} replaced before the world's first step",
            attached.ambiguous,
            attached.missing,
            attached.replaced
        );
    }
}
