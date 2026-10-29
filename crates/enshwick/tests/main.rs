//! The integration tests of the API crate, one module per area. They build
//! into one binary: a binary per file cost a link and several megabytes each.
//! Every test still runs in a process of its own under nextest, and the fake
//! host takes its turn behind a lock under `cargo test`.

#[cfg(feature = "unstable")]
mod accounts;
#[cfg(feature = "unstable")]
mod actions;
mod affixes;
mod altar_limit;
mod altar_zones;
mod base_limits;
mod buffs;
#[cfg(feature = "unstable")]
mod build_rights;
#[cfg(feature = "unstable")]
pub mod call_schema;
#[cfg(feature = "unstable")]
mod calls;
#[cfg(feature = "unstable")]
mod capture;
mod config_check;
mod config_errors;
mod config_groups;
mod config_lists;
#[cfg(feature = "unstable")]
mod containers;
#[cfg(feature = "unstable")]
mod damage;
mod daytime;
#[cfg(feature = "unstable")]
mod effects;
mod engine_names;
mod entity_data;
#[cfg(feature = "unstable")]
mod events_described;
mod fake_per_test;
mod fake_second_install;
mod features;
#[cfg(feature = "unstable")]
mod fog;
mod game_event_kinds;
mod game_events;
mod game_names;
mod game_settings;
mod hook_macro;
#[cfg(feature = "unstable")]
mod hud;
mod icons;
mod icons_hidden;
mod init_off;
mod inventory;
mod item_changes;
mod item_data;
mod knowledge;
mod language;
mod lifecycle;
mod live_changes;
mod live_changes_join;
mod loot_drops;
mod loot_table_changes;
#[cfg(feature = "unstable")]
mod map;
mod mid_hook;
mod migrations;
mod net;
mod no_inventories;
mod no_mod_points;
mod no_ui;
#[cfg(feature = "unstable")]
mod notices;
mod pacing;
#[cfg(feature = "schema")]
mod pack_schemas;
mod packs;
mod panic_abort;
#[cfg(feature = "unstable")]
mod players;
pub mod point_schema;
#[cfg(feature = "unstable")]
mod points_described;
mod points_links;
mod points_owner;
mod published_texts;
#[cfg(feature = "unstable")]
mod rates;
#[cfg(feature = "unstable")]
mod rates_comfort;
#[cfg(feature = "unstable")]
mod rates_join;
mod recipe_changes;
#[cfg(feature = "unstable")]
mod recipe_kinds;
#[cfg(feature = "unstable")]
mod recipe_menu;
mod recipe_rules;
mod resource_types;
mod services;
mod shapes;
mod skills;
mod startup_sync;
mod stat_kinds;
#[cfg(feature = "unstable")]
mod static_halves;
#[cfg(feature = "unstable")]
mod stats;
mod step_clock;
mod survivor_lookup;
mod symbols;
mod table_config;
mod task_workers;
mod terrain;
mod texts_override;
mod ticks;
#[cfg(feature = "unstable")]
mod transactions;
mod ui;
mod vetoes;
mod voxels;
mod weather;
#[cfg(feature = "unstable")]
mod world_blocks;
#[cfg(feature = "unstable")]
mod world_props;
#[cfg(feature = "unstable")]
mod world_queries;
mod world_settings;
mod worlds;
#[cfg(feature = "unstable")]
mod writes;
