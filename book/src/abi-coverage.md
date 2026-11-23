<!--
type: reference
description: How each entry of the loader's host table is tested: on the fake host, on a copy of the dedicated server and by a person playing the game.
-->

# ABI test coverage

Every `HostApi` entry has a test on the fake host. The table names the test
in the running game of each entry that rests on the game. The table stays
fixed from 1.0, as [Stability](stability.md#the-abi) says.

## How is an entry tested?

| Test        | What it shows                                                                                                                                                                                                                                                                                                                                |
| ----------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Fake host   | `enshwick::test::FakeHost` answers the entry as the loader does, and a test of the `enshwick` crate calls it through the safe API. Every entry has one.                                                                                                                                                                                      |
| Server copy | The [probe](https://github.com/enshwick/mods/tree/main/devtools/enshwick-probe), a development mod, calls the entry on a copy of the dedicated server and logs `PASS` or `FAIL`. The column names the probe's check. "every start" marks an entry the probe or the loader calls as it starts, so every server copy that starts calls it. |
| In game     | The entry needs a player in the world, or it acts only in the client. A person playing checks it.                                                                                                                                                                                                                                         |

An entry that rests on the loader alone, such as logging, services, calls
and settings, has no engine fact to check in the game. Its fake host test and
the loader's own tests cover it.

## How is the stub's start contract tested?

The `powrprof.dll` stub starts the loader through one export,
`enshwick_start_v3(missing_forwards, release)`. Its name carries the contract's
version, so a stub and a loader of different releases find no common export,
and the game runs without mods. Both take the name and the type from one
crate, and the loader does not compile when its export has another type. A
test fails when the type changes without a new name. Every server copy runs
the call as it starts.

## The entries

The table follows the order of `HostApi`. `cargo xtask check` fails unless it
names every entry of the golden layout once, in that order.

| Entry | Rests on | Server copy | In game |
| --- | --- | --- | --- |
| `header` | loader | layout test | — |
| `host_version` | loader | every start | — |
| `build` | game | every start | — |
| `symbol` | game | probe: reflection | — |
| `system` | game | probe: reflection | — |
| `field_offset` | game | probe: reflection | — |
| `type_size` | game | probe: reflection | — |
| `hook` | game | probe: hook | — |
| `caller_frame` | game | probe: hook | — |
| `fault` | loader | — | — |
| `log` | loader | every start | — |
| `settings` | loader | every start | — |
| `publish` | loader | — | — |
| `service` | loader | — | — |
| `constant` | game | every start | — |
| `data_dir` | loader | — | — |
| `log_level` | loader | every start | — |
| `subscribe` | loader | every start | — |
| `type_hash` | game | every start | — |
| `component` | game | probe: entity queries | — |
| `entity_template` | game | probe: spawns | — |
| `mods` | loader | — | — |
| `code_range` | game | probe: code section | — |
| `patch_code` | game | probe: code patch | — |
| `workshop_recipes` | game | probe: host reads | — |
| `hosting` | game | probe: host reads | — |
| `server_world` | game | probe: server world | — |
| `join_missing` | loader | — | with a player |
| `hook_at` | game | probe: hook at | — |
| `status` | loader | — | — |
| `subscribe_game_event` | game | probe: ping | — |
| `net_channel` | loader | probe: channel refused | — |
| `net_send` | game | — | client |
| `net_ready` | game | — | client |
| `config_schema` | loader | every start | — |
| `config_check` | loader | — | — |
| `texts` | loader | — | — |
| `define_event` | loader | — | — |
| `subscribe_mod_event` | loader | — | — |
| `send_event` | loader | — | — |
| `rename_content` | game | — | client |
| `mod_dir` | loader | every start | — |
| `language` | game | — | client |
| `define_point` | loader | — | — |
| `attach_point` | loader | — | — |
| `run_point` | loader | — | — |
| `event_types` | game | every start | — |
| `feature` | loader | every start | — |
| `loop_wait` | game | probe: loop waits | — |
| `server_wait` | game | probe: loop waits | — |
| `task_workers` | game | probe: task workers | — |
| `serve_call` | loader | — | — |
| `find_call` | loader | — | — |
| `call` | loader | — | — |
| `subscribe_tick` | game | probe: ticks | — |
| `set_tick_every` | game | probe: ticks | — |
| `set_config` | loader | — | — |
| `subscribe_client_game_event` | game | — | on a client, hosting and joined |
| `serve_resource` | game | — | client |
| `add_text` | game | — | client |
| `define_item_data` | loader | — | — |
| `item_data_get` | game | probe: entity data | — |
| `set_item_data` | game | probe: entity data | — |
| `content_serve` | game | — | client |
| `content_read` | game | probe: archive | — |
| `icons_ready` | game | — | client |
| `declare_content` | loader | — | — |
| `define_entity_data` | loader | probe: entity data | — |
| `resource_read` | game | probe: archive | — |
| `game_text` | game | — | client |
| `rebuild_resource` | game | — | client |
| `recipe_rule` | game | — | client |
| `recipe_disabled` | game | — | client |
| `recipe_kind` | game | — | client |
| `rate` | game | — | with a player |
| `set_rate` | game | — | with a player |
| `change_data` | game | — | client |
| `set_change` | game | — | client |
| `use_content` | loader | — | — |
| `resource_keys` | game | probe: archive | — |
| `content_hashes` | game | probe: archive | — |
| `types` | game | probe: host reads | — |
| `game_version` | game | probe: host reads | — |
| `crafting_prop` | game | — | client |
| `read_only` | loader | — | — |
| `resource_served` | game | probe: archive | — |
| `resource_buffer` | loader | — | — |
| `content_name` | loader | — | — |
| `player` | game | — | with a player |
| `players` | game | — | with a player |
| `chat` | game | — | with a player |
| `chat_all` | game | — | with a player |
| `set_label` | game | probe: label | — |
| `rename_filter` | game | — | with a player |
| `item_info` | game | probe: affixes | — |
| `request_altars` | game | probe: altars | — |
| `altars` | game | probe: altars | — |
| `building_zone_filter` | game | — | with a player |
| `base_zones` | game | probe: altars | — |
| `entities` | game | probe: entity queries | — |
| `destroy` | game | probe: destroy | — |
| `save_world` | game | probe: saves | — |
| `world_key` | game | probe: world key | — |
| `component_write` | game | probe: writes | — |
| `notify` | game | — | client |
| `time_of_day` | game | probe: clock | — |
| `set_time_of_day` | game | probe: clock | — |
| `request_time_speed` | game | probe: clock | — |
| `set_time_speed` | game | probe: clock | — |
| `game_settings` | game | probe: game settings | — |
| `set_game_settings` | game | probe: game settings | — |
| `buff_types` | game | probe: buffs | — |
| `buffs` | game | — | with a player |
| `apply_buff` | game | — | with a player |
| `remove_buff` | game | — | with a player |
| `weather_at` | game | probe: weather | — |
| `weather_fronts` | game | probe: weather | — |
| `set_weather` | game | probe: weather | — |
| `terrain_materials` | game | probe: terrain | — |
| `terrain_edit` | game | probe: terrain | — |
| `altar_zone_filter` | game | probe: altars | — |
| `altar_zones_changed` | game | probe: altars | — |
| `altar_limit_filter` | game | — | with a player |
| `base_limit` | game | probe: base limits | — |
| `voxel_read` | game | probe: world reads | — |
| `build_rights` | game | — | with a player |
| `transaction_commit` | game | probe: transaction | — |
| `transaction_undo` | game | probe: transaction | — |
| `transaction_status` | game | probe: transaction | — |
| `world_quiet` | game | — | with a player |
| `transaction_entities` | game | probe: transaction | — |
| `world_persists` | game | probe: world reads | — |
| `map_markers` | game | probe: map | — |
| `account_action` | game | — | with a player |
| `map_frame` | game | probe: map | — |
| `request_base_limits` | game | probe: base limits | — |
| `set_chat_mute` | game | — | with a player |
| `chat_muted` | loader | — | — |
| `set_base_limit` | game | probe: base limits | — |
| `save_info` | game | probe: saves | — |
| `hold_saves` | game | probe: saves | — |
| `set_saved` | game | probe: spawns | — |
| `progress_write` | game | — | with a player |
| `server_stop` | game | — | with a player |
| `fog_uncover` | game | — | with a player |
| `language_of` | game | — | with a player |
| `join_rule` | game | — | with a client joining |
| `request_inventory` | game | probe: affixes | — |
| `inventory_submit` | game | — | with a player |
| `survivor_lookup` | game | — | with a player |
| `survivor_bed_check` | game | — | with a player |
| `summon_filter` | game | — | with a player |
| `summon_item` | game | — | with a player |
| `survivor_status` | game | — | client |
| `emit` | game | probe: ping | — |
| `teleport` | game | probe: spawns | — |
| `spawn` | game | probe: spawns | — |
| `damage_filter` | game | every start | with a player |
| `summoned` | game | — | with a player |
| `define_stat` | loader | probe: stats | — |
| `stat_info` | loader | probe: stats | — |
| `stat_value` | game | — | with a player |
| `stat_modify` | game | — | with a player |
| `stat_buff` | game | — | with a player |
| `stats_changed` | game | every start | with a player |
| `stat_sources` | game | — | with a player |
| `stat_watch` | game | — | with a player |
| `define_affix` | loader | probe: affixes | — |
| `affix_roll` | game | probe: affixes | — |
| `item_lines` | game | probe: affixes | — |
| `affix_info` | loader | probe: affixes | — |
| `item_filter` | game | every start | with a player |
| `drop_items` | game | — | with a player |
| `skill_override` | game | — | with a player |
| `comfort_filter` | game | — | with a player |
| `experience_filter` | game | — | with a player |
| `knowledge` | game | probe: knowledge | — |
| `change_knowledge` | game | probe: knowledge | — |
| `quests` | game | probe: knowledge | — |
| `knowledge_input_filter` | game | every start | with a player |
| `game_query` | game | — | with a player |
| `run_console` | game | probe: console line | — |
| `interaction_filter` | game | — | with a player |
| `craft_filter` | game | every start | with a player |
| `build_filter` | game | every start | with a player |
| `actions` | loader | every start | client |
| `control` | loader | every start | client |
| `run_action` | game | — | client |
| `chat_filter` | game | every start | with a player |
| `commands` | loader | — | — |
| `run_command` | loader | — | — |
| `command_list` | loader | — | — |
| `actions_changed` | loader | — | — |
| `action_window` | game | — | client |
| `ui_frame` | game | — | client |
| `ui_widget` | game | — | client |
| `ui_dialog` | game | — | client |
| `ui_hud` | game | — | client |
| `ui_markers` | game | — | client |
| `request_fog` | game | — | client |
| `fog_read` | game | — | client |
| `map_marker_types` | game | — | client |
| `ui_extension` | game | — | client |
| `local_held` | game | — | client |
| `local_input_status` | loader | — | not exercised |
| `client_snapshot` | game | — | client |
| `loader_off` | loader | — | — |
| `client_command` | game | — | client |
| `client_command_status` | game | — | client |
| `terrain_material_add` | game | — | client |
| `interaction_prompt` | game | — | with a player, client |
| `ui_item_tooltip` | game | — | client |
| `change_permissions` | game | — | with a player |
| `transaction_settled` | game | probe: transaction | — |
| `prop_placement_info` | game | — | not exercised |
| `container_rule` | game | — | with a player |
| `show_pickup` | game | — | with a player |
| `respawn_point` | game | — | with a player |
| `set_respawn_point` | game | — | with a player |
| `buff_type_add` | game | — | client |
| `damage_causes_off` | loader | — | — |
| `waypoint` | game | — | with a player |
| `sound_play` | game | — | client |
| `sound_steer` | game | — | client |
| `sound_state` | loader | — | client |
| `sound_play_for` | game | — | client |
| `sound_steer_for` | game | — | client |
| `rekey_resource` | game | — | client |
