//! The knowledge domain: the game's knowledge store and the players' progress.

#[cfg(windows)]
pub(crate) mod progress;
#[cfg(windows)]
pub(crate) mod quest_events;
#[cfg(windows)]
pub(crate) mod quest_pay;
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
        seal: Some(|_| store::guard::seal()),
        link: Some(store::guard::link),
        steps: &[
            step!(Follow, "knowledge steps", |s| store::after_step(s.loader)),
            step!(Follow, "quest event queries", |s| quest_events::after_step(
                s.loader
            )),
        ],
        forget: Some(|_| {
            store::forget();
        }),
        ..part!("knowledge", store)
    },
    &Part {
        table_ids: progress::TABLE_IDS,
        seal: Some(|_| progress::seal()),
        link: Some(progress::link),
        steps: &[step!(Follow, "level writes", |s| progress::after_step(
            s.loader
        ))],
        end: Some(|loader| {
            progress::game_ended(loader);
        }),
        ..part!("progress", progress)
    },
    &Part {
        table_ids: quest_pay::TABLE_IDS,
        seal: Some(|_| quest_pay::seal()),
        link: Some(quest_pay::link),
        ..part!("quest rewards", quest_pay)
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
        forget: Some(|_| {
            store::changes::forget();
        }),
        ..part!("knowledge changes", store::changes)
    },
];
