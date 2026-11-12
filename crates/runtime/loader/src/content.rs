//! The content domain: served resources, content changes, rates, recipes, item
//! kinds and textures.

#[cfg(windows)]
pub(crate) mod clones;
#[cfg_attr(
    not(windows),
    expect(dead_code, reason = "only the running game takes mods' renames")
)]
pub(crate) mod content_renames;
pub mod data_changes;
pub mod item_kinds;
#[cfg(windows)]
pub(crate) mod rate_links;
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "only the running game applies rates")
)]
pub(crate) mod rates;
pub mod recipes;
#[cfg(windows)]
pub(crate) mod served;
#[cfg(windows)]
pub(crate) mod standins;
#[cfg(windows)]
pub(crate) mod textures;

#[cfg(windows)]
use crate::engine::parts::{Part, fused, part};

/// The content domain's parts, in the order the joins call each of their
/// phases.
#[cfg(windows)]
pub(crate) static PARTS: &[&Part] = &[
    &Part {
        link: Some(recipes::init),
        forget: Some(|_| recipes::left()),
        ..part!("recipes", recipes)
    },
    &Part {
        table_ids: served::TABLE_IDS,
        seal: Some(served::seal_and_link),
        ..part!("content", served)
    },
    &Part {
        table_ids: textures::TABLE_IDS,
        link: Some(textures::link),
        ..part!("textures", textures)
    },
    &Part {
        world_event: Some(fused!("data changes at world events", |loader, event| {
            data_changes::world_event(loader, event);
        })),
        follows: Some(data_changes::wanted),
        ..part!("data_changes", data_changes)
    },
    &Part {
        table_ids: rate_links::TABLE_IDS,
        link: Some(rate_links::link),
        world_event: Some(fused!("recipe rates at world events", |_, event| {
            rate_links::world_event(event);
        })),
        follows: Some(|_| rate_links::recipes_rated()),
        ..part!("rate_links", rate_links)
    },
];
