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
| `player` | game | — | with a player |
| `players` | game | — | with a player |
| `serve_resource` | game | — | client |
| `add_text` | game | — | client |
| `mods` | loader | — | — |
| `code_range` | game | probe: code section | — |
| `patch_code` | game | — | — |
| `workshop_recipes` | game | probe: host reads | — |
| `hosting` | game | probe: host reads | — |
| `server_world` | game | — | — |
| `chat` | game | — | with a player |
| `set_label` | game | probe: label | — |
| `rename_filter` | game | — | with a player |
| `join_missing` | loader | — | with a player |
| `hook_at` | game | — | — |
| `inventory_open` | game | probe: affixes | — |
| `item_info` | game | probe: affixes | — |
| `inventory_submit` | game | — | — |
| `status` | loader | — | — |
| `run_console` | game | probe: console line | — |
| `altars_open` | game | probe: altars | — |
| `altars` | game | probe: altars | — |
| `building_zone_check_filter` | game | — | with a player |
| `survivor_lookup` | game | — | with a player |
| `survivor_bed_check` | game | — | with a player |
| `summon_filter` | game | — | with a player |
| `summon_item` | game | — | with a player |
| `survivor_status` | game | — | client |
| `interaction_filter` | game | — | with a player |
| `skill_override` | game | — | with a player |
| `comfort_filter` | game | — | with a player |
| `experience_filter` | game | — | with a player |
| `raise_base_limit` | game | probe: base limits | — |
| `base_zones` | game | probe: altars | — |
| `game_event` | game | probe: ping | — |
| `entities` | game | probe: entity queries | — |
| `destroy` | game | probe: destroy | — |
| `save_world` | game | probe: saves | — |
| `world_key` | game | probe: world key | — |
| `component_write` | game | probe: writes | — |
| `emit` | game | probe: ping | — |
| `teleport` | game | probe: spawns | — |
| `spawn` | game | probe: spawns | — |
| `craft_filter` | game | every start | with a player |
| `build_filter` | game | every start | with a player |
| `net_channel` | loader | probe: channel refused | — |
| `net_send` | game | — | client |
| `net_ready` | game | — | client |
| `notify` | game | — | client |
| `time_of_day` | game | probe: clock | — |
| `set_time_of_day` | game | probe: clock | — |
| `time_speed_control` | game | probe: clock | — |
| `set_time_speed` | game | probe: clock | — |
| `game_settings` | game | probe: game settings | — |
| `set_game_settings` | game | probe: game settings | — |
| `damage_filter` | game | every start | with a player |
| `buff_types` | game | probe: buffs | — |
| `buffs` | game | — | with a player |
| `apply_buff` | game | — | with a player |
| `remove_buff` | game | — | with a player |
| `knowledge` | game | probe: knowledge | — |
| `grant_knowledge` | game | probe: knowledge | — |
| `quests` | game | probe: knowledge | — |
| `weather_at` | game | probe: weather | — |
| `weather_fronts` | game | probe: weather | — |
| `set_weather` | game | probe: weather | — |
| `terrain_materials` | game | probe: terrain | — |
| `terrain_edit` | game | probe: terrain | — |
| `config_schema` | loader | every start | — |
| `summoned` | game | — | with a player |
| `item_data_open` | loader | — | — |
| `item_data_get` | game | probe: entity data | — |
| `item_data_set` | game | probe: entity data | — |
| `ui_frame` | game | — | client |
| `ui_widget` | game | — | client |
| `ui_dialog` | game | — | client |
| `ui_hud` | game | — | client |
| `ui_markers` | game | — | client |
| `config_check` | loader | — | — |
| `texts` | loader | — | — |
| `content_serve` | game | — | client |
| `content_read` | game | probe: archive | — |
| `icons_ready` | game | — | client |
| `stat_open` | loader | probe: stats | — |
| `stat_info` | loader | probe: stats | — |
| `stat_value` | game | — | with a player |
| `stat_modify` | game | — | with a player |
| `stat_buff` | game | — | with a player |
| `stats_changed` | game | every start | with a player |
| `stat_sources` | game | — | with a player |
| `stat_watch` | game | — | with a player |
| `define_event` | loader | — | — |
| `subscribe_event` | loader | — | — |
| `send_event` | loader | — | — |
| `affix_open` | loader | probe: affixes | — |
| `affix_roll` | game | probe: affixes | — |
| `item_lines` | game | probe: affixes | — |
| `affix_info` | loader | probe: affixes | — |
| `rename_content` | game | — | client |
| `declare_content` | loader | — | — |
| `entity_data_open` | loader | probe: entity data | — |
| `mod_dir` | loader | every start | — |
| `knowledge_input_filter` | game | every start | with a player |
| `language` | game | — | client |
| `keep_game_settings` | game | probe: game settings | — |
| `forget_game_settings` | game | probe: game settings | — |
| `revoke_knowledge` | game | probe: knowledge | — |
| `account_knowledge` | game | probe: knowledge | — |
| `resource_read` | game | probe: archive | — |
| `game_text` | game | — | client |
| `rebuild_resource` | game | — | client |
| `recipe_rule` | game | — | client |
| `recipe_disabled` | game | — | client |
| `define_point` | loader | — | — |
| `attach_point` | loader | — | — |
| `run_point` | loader | — | — |
| `actions` | loader | every start | client |
| `event_types` | game | every start | — |
| `recipe_kind` | game | — | client |
| `feature` | loader | every start | — |
| `rate` | game | — | with a player |
| `set_rate` | game | — | with a player |
| `change_data` | game | — | client |
| `loop_wait` | game | probe: loop waits | — |
| `server_wait` | game | probe: loop waits | — |
| `task_workers` | game | probe: task workers | — |
| `altar_zone_filter` | game | probe: altars | — |
| `altar_zones_changed` | game | probe: altars | — |
| `altar_limit_filter` | game | — | with a player |
| `base_limit` | game | probe: base limits | — |
| `voxel_read` | game | probe: world reads | — |
| `declare_change` | game | — | client |
| `set_change` | game | — | client |
| `build_rights` | game | — | with a player |
| `transaction_commit` | game | — | — |
| `transaction_undo` | game | — | — |
| `transaction_status` | game | — | — |
| `transaction_commit_for` | game | — | with a player |
| `world_quiet` | game | — | with a player |
| `fog_open` | game | — | client |
| `fog_read` | game | — | client |
| `serve_call` | loader | — | — |
| `find_call` | loader | — | — |
| `call` | loader | — | — |
| `transaction_entities` | game | — | — |
| `world_persists` | game | probe: world reads | — |
| `map_markers` | game | probe: map | — |
| `account_action` | game | — | with a player |
| `subscribe_tick` | game | probe: ticks | — |
| `set_tick_every` | game | probe: ticks | — |
| `map_frame` | game | probe: map | — |
| `request_base_limits` | game | probe: base limits | — |
| `map_marker_types` | game | — | client |
| `use_content` | loader | — | — |
| `chat_mute_set` | game | — | with a player |
| `chat_mute_has` | loader | — | — |
| `set_base_cap` | game | probe: base limits | — |
| `resource_keys` | game | probe: archive | — |
| `content_hashes` | game | probe: archive | — |
| `types` | game | probe: host reads | — |
| `game_version` | game | probe: host reads | — |
| `control` | loader | every start | client |
| `run_action` | game | — | client |
| `ui_extension` | game | — | client |
| `item_filter` | game | every start | with a player |
| `chat_filter` | game | every start | with a player |
| `save_info` | game | probe: saves | — |
| `hold_saves` | game | probe: saves | — |
| `crafting_prop` | game | — | client |
| `set_saved` | game | probe: spawns | — |
| `run_action_at` | game | — | client |
| `local_held` | game | — | client |
| `read_only` | loader | — | — |
| `resource_served` | game | probe: archive | — |
| `commands` | loader | — | — |
| `run_command` | loader | — | — |
| `command_list` | loader | — | — |
| `actions_changed` | loader | — | — |
| `resource_buffer` | loader | — | — |
| `game_query` | game | — | with a player |
| `action_window` | game | — | client |
| `progress_write` | game | — | with a player |
| `drop_items` | game | — | with a player |
| `config_set` | loader | — | — |
| `server_stop` | game | — | — |
| `fog_uncover` | game | — | with a player |
| `content_name` | loader | — | — |
| `language_of` | game | — | with a player |
| `client_game_event` | game | — | client |
