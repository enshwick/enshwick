//! The content domain: served resources, content changes, rates, recipes, item
//! kinds, named content for pickers and textures.

#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "only the running game serves buff types")
)]
pub(crate) mod buff_types;
#[cfg(windows)]
pub(crate) mod clones;
#[cfg_attr(
    not(windows),
    expect(dead_code, reason = "only the running game takes mods' renames")
)]
pub(crate) mod content_renames;
#[cfg_attr(
    all(not(windows), not(test)),
    expect(
        dead_code,
        reason = "only the running game reads the crafting prop mask"
    )
)]
pub(crate) mod crafting_props;
pub mod data_changes;
pub mod item_kinds;
#[cfg_attr(
    not(windows),
    expect(dead_code, reason = "only the client's interface lists named content")
)]
pub(crate) mod names;
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
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "only the client's interface lists templates")
)]
pub(crate) mod template_names;
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "only the running game serves terrain materials")
)]
pub(crate) mod terrain_materials;
#[cfg(windows)]
pub(crate) mod textures;

#[cfg(windows)]
use crate::base::remote::Message;
#[cfg(windows)]
use crate::engine::parts::{Part, ServerLink, fused, part};

/// The content domain's parts, in the order the joins call each of their
/// phases.
#[cfg(windows)]
pub(crate) static PARTS: &[&Part] = &[
    &Part {
        table_ids: recipes::TABLE_IDS,
        seal: Some(|_| {
            crate::base::remote::on_client(crate::base::remote::RECIPES, |message| {
                if let Message::Recipes { first, last, ids } = message {
                    recipes::received(first, last, &ids);
                }
            });
        }),
        link: Some(recipes::init),
        forget: Some(|_| {
            recipes::forget();
        }),
        settings: Some(recipes::world_changed),
        poll: Some(recipes::poll),
        server_link: Some(|link| {
            if link == ServerLink::Joining {
                recipes::joining();
            }
        }),
        handover: Some(recipes::hello),
        ..part!("recipes", recipes)
    },
    &Part {
        table_ids: served::TABLE_IDS,
        seal: Some(served::seal_and_link),
        status: Some(fused!("content status", |summary| {
            summary.served = served::summary();
        })),
        shared: Some(fused!("content shared resources", served::shared)),
        ..part!("content", served)
    },
    &Part {
        table_ids: textures::TABLE_IDS,
        link: Some(textures::link),
        ..part!("textures", textures)
    },
    &Part {
        world_event: Some(fused!(
            "terrain materials at world events",
            |loader, event| {
                terrain_materials::world_event(loader, event);
            }
        )),
        ..part!("terrain_materials", terrain_materials)
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
        seal: Some(|_| rate_links::seal()),
        link: Some(rate_links::link),
        world_event: Some(fused!("recipe rates at world events", |_, event| {
            rate_links::world_event(event);
        })),
        follows: Some(|_| rate_links::recipes_rated()),
        ..part!("rate_links", rate_links)
    },
];
