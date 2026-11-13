//! The knowledge domain: the game's knowledge store and the players' progress.

#[cfg(windows)]
pub(crate) mod progress;
#[cfg(windows)]
pub(crate) mod store;

#[cfg(windows)]
use crate::engine::parts::{Part, fused, part, step};

/// The knowledge domain's parts, in the order the joins call each of their
/// phases.
#[cfg(windows)]
pub(crate) static PARTS: &[&Part] = &[
    &Part {
        table_ids: store::TABLE_IDS,
        link: Some(store::guard::link),
        steps: &[step!(Follow, "knowledge steps", |s| store::after_step(
            s.loader
        ))],
        forget: Some(|_| store::forget()),
        ..part!("knowledge", store)
    },
    &Part {
        table_ids: progress::TABLE_IDS,
        link: Some(progress::link),
        steps: &[step!(Follow, "level writes", |s| progress::after_step(
            s.loader
        ))],
        end: Some(fused!("level writes at a game's end", |loader| {
            progress::game_ended(loader);
        })),
        ..part!("progress", progress)
    },
    &Part {
        steps: &[step!(Follow, "knowledge change steps", |s| {
            store::changes::after_step(s.loader, s.step);
        })],
        world_event: Some(fused!(
            "knowledge changes at world events",
            |loader, event| {
                store::changes::world_event(loader, event);
            }
        )),
        ..part!("knowledge changes", store::changes)
    },
];
