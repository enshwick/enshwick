//! The integration tests of the API crate, one module per area. They build
//! into one binary: a binary per file cost a link and several megabytes each.
//! Every test still runs in a process of its own under nextest, and the fake
//! host takes its turn behind a lock under `cargo test`.

mod abi_versions;
#[cfg(feature = "unstable")]
mod accounts;
#[cfg(feature = "unstable")]
mod action_arguments;
#[cfg(feature = "unstable")]
mod action_keys;
#[cfg(feature = "unstable")]
mod action_targets;
#[cfg(feature = "unstable")]
mod actions;
#[cfg(feature = "unstable")]
mod affixes;
mod altar_kinds;
mod altar_limit;
mod altar_zones;
mod base_limits;
mod buffs;
#[cfg(feature = "unstable")]
mod build_rights;
#[cfg(feature = "unstable")]
mod calls;
#[cfg(feature = "unstable")]
mod capture;
#[cfg(feature = "unstable")]
mod chat;
#[cfg(feature = "unstable")]
mod commands;
mod config_check;
mod config_errors;
mod config_files;
mod config_groups;
mod config_lists;
#[cfg(feature = "unstable")]
mod containers;
mod content_renames;
#[cfg(feature = "unstable")]
mod controls;
#[cfg(feature = "unstable")]
mod damage;
mod daytime;
pub mod deps;
mod developer_tools;
#[cfg(feature = "unstable")]
mod effects;
#[cfg(feature = "typed")]
mod enemies;
mod engine_names;
#[cfg(feature = "unstable")]
mod entity_calls_unavailable;
#[cfg(feature = "unstable")]
mod entity_data;
#[cfg(feature = "unstable")]
mod events_described;
#[cfg(feature = "typed")]
mod experience_content;
mod export_dir;
mod exports;
mod fake_per_test;
mod fake_refusals;
mod fake_second_install;
mod features;
#[cfg(feature = "unstable")]
mod fog;
#[cfg(feature = "unstable")]
mod fog_uncovered;
#[cfg(feature = "unstable")]
mod frame_reads;
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
#[cfg(feature = "unstable")]
mod input;
#[cfg(feature = "typed")]
mod input_hint;
#[cfg(feature = "unstable")]
mod interaction_targets;
mod inventory;
mod item_changes;
#[cfg(feature = "unstable")]
mod item_data;
#[cfg(feature = "unstable")]
mod items;
#[cfg(feature = "typed")]
mod journal;
mod knowledge;
#[cfg(feature = "typed")]
mod knowledge_content;
#[cfg(feature = "typed")]
mod knowledge_many;
mod language;
mod lifecycle;
mod live_changes;
mod live_changes_join;
mod loot_drops;
mod loot_table_changes;
mod manifest_flags;
#[cfg(feature = "unstable")]
mod map;
#[cfg(feature = "unstable")]
mod map_events;
#[cfg(feature = "unstable")]
mod map_marker_types;
mod mid_hook;
mod mod_index;
mod net;
mod no_inventories;
mod no_mod_points;
mod no_ui;
#[cfg(feature = "unstable")]
mod notices;
#[cfg(feature = "unstable")]
mod open_screen;
mod pacing;
#[cfg(feature = "unstable")]
mod pack_handles;
#[cfg(feature = "unstable")]
mod pack_owners;
#[cfg(feature = "schema")]
mod pack_schemas;
#[cfg(feature = "unstable")]
mod pack_settings;
mod packs;
mod panic_abort;
mod panic_reasons;
mod player_teleports;
#[cfg(feature = "unstable")]
mod players;
pub mod point_schema;
mod points_declared;
#[cfg(feature = "unstable")]
mod points_described;
#[cfg(feature = "unstable")]
mod points_described_owned;
mod points_links;
mod points_owner;
mod poisoned_locks;
mod props;
mod props_server;
mod published_texts;
#[cfg(feature = "typed")]
mod quest_rewards;
#[cfg(feature = "typed")]
mod quests_read;
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
#[cfg(feature = "typed")]
mod rekey_served;
#[cfg(feature = "typed")]
mod removal;
mod required_names;
mod resource_types;
mod resource_values;
#[cfg(feature = "unstable")]
mod reward_actions;
#[cfg(feature = "unstable")]
mod rewards;
#[cfg(feature = "unstable")]
mod saves;
mod schedule;
#[cfg(feature = "unstable")]
mod screen_extensions;
#[cfg(feature = "unstable")]
mod screen_hosts;
#[cfg(feature = "typed")]
mod served_reads;
#[cfg(feature = "unstable")]
mod server_controls;
mod services;
mod shapes;
mod sides;
#[cfg(feature = "typed")]
mod skill_tree;
mod skills;
mod sound_changes;
#[cfg(feature = "unstable")]
mod sound_files;
mod sound_made;
mod sounds;
#[cfg(feature = "typed")]
mod spawns;
#[cfg(feature = "unstable")]
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
#[cfg(feature = "typed")]
mod templates;
mod terrain;
mod terrain_pages;
mod ticks;
#[cfg(feature = "unstable")]
mod transactions;
#[cfg(feature = "typed")]
mod typed;
#[cfg(feature = "typed")]
mod typed_blobs;
#[cfg(feature = "typed")]
mod typed_variants;
mod ui;
mod vanilla_items;
mod vetoes;
mod voxel_edits;
mod voxels;
mod weather;
#[cfg(feature = "typed")]
mod weather_phenomena;
#[cfg(feature = "unstable")]
mod windows;
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
