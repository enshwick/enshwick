//! The world domain: bases, altars, terrain and voxels, weather, time of day,
//! the map, buffs and game settings.

#[cfg(windows)]
pub(crate) mod altars;
#[cfg_attr(
    not(windows),
    expect(dead_code, reason = "only the running game groups altar zones")
)]
pub(crate) mod base_groups;
#[cfg(windows)]
pub(crate) mod bases;
#[cfg(windows)]
pub(crate) mod buffs;
#[cfg(windows)]
pub(crate) mod daytime;
#[cfg(windows)]
pub(crate) mod entities;
#[cfg(windows)]
pub(crate) mod fog;
pub mod fog_image;
#[cfg(windows)]
pub(crate) mod game_settings;
#[cfg(windows)]
pub(crate) mod label;
pub mod map_frame;
pub mod map_marker_types;
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "only the Windows build's parts open to mods")
)]
pub(crate) mod openers;
pub mod prop_holds;
#[cfg_attr(
    not(windows),
    expect(dead_code, reason = "only the Windows build asks the game's zones")
)]
pub(crate) mod rights;
#[cfg(windows)]
pub(crate) mod scene;
pub mod scene_markers;
#[cfg(windows)]
pub(crate) mod terrain;
pub mod terrain_pages;
#[cfg_attr(
    not(windows),
    expect(dead_code, reason = "only the Windows build writes the game's voxels")
)]
pub(crate) mod transactions;
#[cfg_attr(
    not(windows),
    expect(dead_code, reason = "only the Windows build reads the game's voxels")
)]
pub(crate) mod voxels;
#[cfg(windows)]
pub(crate) mod weather;
#[cfg(windows)]
pub(crate) mod writes;

#[cfg(windows)]
use crate::engine::parts::{Part, fused, part, step};

/// The world domain's parts, in the order the joins call each of their
/// phases.
#[cfg(windows)]
pub(crate) static PARTS: &[&Part] = &[
    &Part {
        table_ids: altars::TABLE_IDS,
        link: Some(altars::link),
        steps: &[step!(Follow, "altar steps", |s| altars::after_step(
            s.loader, s.now
        ))],
        forget: Some(|_| altars::forget()),
        ..part!("altars", altars)
    },
    &Part {
        table_ids: daytime::TABLE_IDS,
        link: Some(daytime::link),
        steps: &[step!(Follow, "day steps", |s| daytime::after_step(
            s.loader
        ))],
        forget: Some(|_| daytime::forget()),
        ..part!("daytime", daytime)
    },
    &Part {
        table_ids: game_settings::TABLE_IDS,
        steps: &[step!(Follow, "game settings steps", |s| {
            game_settings::after_step(s.loader);
        })],
        world_event: Some(fused!("game settings at world events", |loader, event| {
            game_settings::world_event(loader, event);
        })),
        forget: Some(|_| game_settings::forget_world()),
        ..part!("game_settings", game_settings)
    },
    &Part {
        table_ids: weather::TABLE_IDS,
        link: Some(weather::link),
        steps: &[step!(Follow, "weather steps", |s| weather::after_step(
            s.loader
        ))],
        world_event: Some(fused!("weather at world events", |loader, event| {
            weather::world_event(loader, event);
        })),
        follows: Some(weather::follows),
        forget: Some(|_| weather::forget()),
        ..part!("weather", weather)
    },
    &Part {
        table_ids: label::TABLE_IDS,
        link: Some(label::link),
        ..part!("label", label)
    },
    &Part {
        table_ids: bases::TABLE_IDS,
        link: Some(bases::link),
        forget: Some(|_| bases::forget()),
        ..part!("bases", bases)
    },
    &Part {
        table_ids: fog::TABLE_IDS,
        link: Some(fog::link),
        forget: Some(|_| fog::forget_world()),
        ..part!("fog", fog)
    },
    &Part {
        table_ids: transactions::TABLE_IDS,
        steps: &[step!(Output, "world edits", |s| transactions::after_step(
            s.loader
        ))],
        forget: Some(|_| transactions::forget()),
        ..part!("transactions", transactions)
    },
    &Part {
        table_ids: voxels::TABLE_IDS,
        link: Some(voxels::changes::link),
        steps: &[step!(Output, "voxel edits", |s| {
            voxels::changes::after_step(s.loader);
        })],
        forget: Some(|_| voxels::changes::forget()),
        ..part!("voxels", voxels)
    },
    &Part {
        link: Some(terrain_pages::link),
        steps: &[step!(Output, "terrain pages", |s| {
            terrain_pages::after_step(s.loader);
        })],
        forget: Some(|_| terrain_pages::forget()),
        ..part!("terrain_pages", terrain_pages)
    },
    &Part {
        table_ids: buffs::TABLE_IDS,
        ..part!("buffs", buffs)
    },
    &Part {
        table_ids: entities::TABLE_IDS,
        ..part!("entities", entities)
    },
    &Part {
        table_ids: rights::TABLE_IDS,
        ..part!("rights", rights)
    },
    &Part {
        table_ids: scene::TABLE_IDS,
        forget: Some(|_| scene::forget()),
        ..part!("scene", scene)
    },
    &Part {
        table_ids: terrain::TABLE_IDS,
        ..part!("terrain", terrain)
    },
    &Part {
        table_ids: writes::TABLE_IDS,
        ..part!("writes", writes)
    },
];
