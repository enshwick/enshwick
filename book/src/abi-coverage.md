<!--
type: reference
description: How each entry of the host table is tested before 1.0, on the fake host, on a server copy and in game, and which entries 1.0 freezes.
-->

# ABI test coverage

Every entry of the host table, `HostApi`, has a test on the fake host. An
entry that rests on the game is also tested in the running game. 1.0 freezes
only the entries tested both ways; [Stability](stability.md) says what the
freeze promises.

## How is an entry tested?

| Test        | What it shows                                                                                                                                                                                                                                                                                                                                |
| ----------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Fake host   | `enshwick::test::FakeHost` answers the entry as the loader does, and a test of the `enshwick` crate calls it through the safe API. Every entry has one.                                                                                                                                                                                      |
| Server copy | The [probe](https://github.com/enshwick/mods/tree/main/devtools/enshwick-probe), a development mod, calls the entry on a copy of the dedicated server and logs `PASS` or `FAIL`. The column names the probe's check. "every start" marks an entry the probe or the loader calls as it starts, so every server copy that starts calls it. |
| In game     | The entry needs a player in the world, or it acts only in the client. A person playing checks it.                                                                                                                                                                                                                                         |

An entry that rests on the loader alone, such as logging, services, calls
and settings, has no engine fact to check in the game. Its fake host test and
the loader's own tests cover it.

## Which entries does 1.0 freeze?

| 1.0             | When                                                                                                    |
| --------------- | ------------------------------------------------------------------------------------------------------- |
| frozen          | The entry rests on the loader, or it rests on the game and passes on a server copy.                     |
| after its check | The entry rests on the game and needs a player or the client. It is frozen once its in-game check passes. |
| unstable        | No test in the running game yet. The entry may change in any release.                                   |

## How is the stub's start contract tested?

The `powrprof.dll` stub starts the loader through
`enshwick_start(stub_abi, missing_forwards, release)`. A test checks that the
stub and the loader name the same contract number. Every server copy runs the
call as it starts.

## The entries

The table follows the order of `HostApi`. A test of the `enshwick` crate
checks that it names every entry once, in that order.

| Entry | Rests on | Server copy | In game | 1.0 |
| --- | --- | --- | --- | --- |
| `header` | loader | layout test | — | frozen |
| `host_version` | loader | every start | — | frozen |
| `build` | game | every start | — | frozen |
| `symbol` | game | probe: reflection | — | frozen |
| `system` | game | probe: reflection | — | frozen |
| `field_offset` | game | probe: reflection | — | frozen |
| `type_size` | game | probe: reflection | — | frozen |
| `hook` | game | probe: hook | — | frozen |
| `caller_frame` | game | probe: hook | — | frozen |
| `fault` | loader | — | — | frozen |
| `log` | loader | every start | — | frozen |
| `settings` | loader | every start | — | frozen |
| `publish` | loader | — | — | frozen |
| `service` | loader | — | — | frozen |
| `constant` | game | every start | — | frozen |
| `data_dir` | loader | — | — | frozen |
| `log_level` | loader | every start | — | frozen |
| `subscribe` | loader | every start | — | frozen |
| `type_hash` | game | every start | — | frozen |
| `component` | game | probe: entity queries | — | frozen |
| `entity_template` | game | probe: spawns | — | frozen |
| `player` | game | — | with a player | after its check |
| `players` | game | — | with a player | after its check |
| `serve_resource` | game | — | client | after its check |
| `add_text` | game | — | client | after its check |
| `mods` | loader | — | — | frozen |
| `code_range` | game | probe: code section | — | frozen |
| `patch_code` | game | — | — | unstable |
| `workshop_recipes` | game | probe: host reads | — | frozen |
| `hosting` | game | probe: host reads | — | frozen |
| `server_world` | game | — | — | unstable |
| `chat` | game | — | with a player | after its check |
| `set_label` | game | probe: label | — | frozen |
| `rename_filter` | game | — | with a player | after its check |
| `join_missing` | loader | — | with a player | frozen |
| `hook_at` | game | — | — | unstable |
| `inventory_open` | game | probe: affixes | — | frozen |
| `item_info` | game | probe: affixes | — | frozen |
| `inventory_submit` | game | — | — | unstable |
| `status` | loader | — | — | frozen |
| `run_console` | game | probe: console line | — | frozen |
| `altars_open` | game | probe: altars | — | frozen |
| `altars` | game | probe: altars | — | frozen |
| `building_zone_check_filter` | game | — | with a player | after its check |
| `survivor_lookup` | game | — | with a player | after its check |
| `survivor_bed_check` | game | — | with a player | after its check |
| `summon_filter` | game | — | with a player | after its check |
| `summon_item` | game | — | with a player | after its check |
| `survivor_status` | game | — | client | after its check |
| `interaction_filter` | game | — | with a player | after its check |
| `skill_override` | game | — | with a player | after its check |
| `comfort_filter` | game | — | with a player | after its check |
| `experience_filter` | game | — | with a player | after its check |
| `raise_base_limit` | game | probe: base limits | — | frozen |
| `base_zones` | game | probe: altars | — | frozen |
| `game_event` | game | probe: ping | — | frozen |
| `entities` | game | probe: entity queries | — | frozen |
| `destroy` | game | probe: destroy | — | frozen |
| `save_world` | game | probe: saves | — | frozen |
| `world_key` | game | probe: world key | — | frozen |
| `component_write` | game | probe: writes | — | frozen |
| `emit` | game | probe: ping | — | frozen |
| `teleport` | game | probe: spawns | — | frozen |
| `spawn` | game | probe: spawns | — | frozen |
| `craft_filter` | game | every start | with a player | after its check |
| `build_filter` | game | every start | with a player | after its check |
| `net_channel` | loader | probe: channel refused | — | frozen |
| `net_send` | game | — | client | after its check |
| `net_ready` | game | — | client | after its check |
| `notify` | game | — | client | after its check |
| `time_of_day` | game | probe: clock | — | frozen |
| `set_time_of_day` | game | probe: clock | — | frozen |
| `time_speed_control` | game | probe: clock | — | frozen |
| `set_time_speed` | game | probe: clock | — | frozen |
| `game_settings` | game | probe: game settings | — | frozen |
| `set_game_settings` | game | probe: game settings | — | frozen |
| `damage_filter` | game | every start | with a player | after its check |
| `buff_types` | game | probe: buffs | — | frozen |
| `buffs` | game | — | with a player | after its check |
| `apply_buff` | game | — | with a player | after its check |
| `remove_buff` | game | — | with a player | after its check |
| `knowledge` | game | probe: knowledge | — | frozen |
| `grant_knowledge` | game | probe: knowledge | — | frozen |
| `quests` | game | probe: knowledge | — | frozen |
| `weather_at` | game | probe: weather | — | frozen |
| `weather_fronts` | game | probe: weather | — | frozen |
| `set_weather` | game | probe: weather | — | frozen |
| `terrain_materials` | game | probe: terrain | — | frozen |
| `terrain_edit` | game | probe: terrain | — | frozen |
| `config_schema` | loader | every start | — | frozen |
| `summoned` | game | — | with a player | after its check |
| `item_data_open` | loader | — | — | frozen |
| `item_data_get` | game | probe: entity data | — | frozen |
| `item_data_set` | game | probe: entity data | — | frozen |
| `ui_frame` | game | — | client | after its check |
| `ui_widget` | game | — | client | after its check |
| `ui_dialog` | game | — | client | after its check |
| `ui_hud` | game | — | client | after its check |
| `ui_markers` | game | — | client | after its check |
| `config_check` | loader | — | — | frozen |
| `texts` | loader | — | — | frozen |
| `content_serve` | game | — | client | after its check |
| `content_read` | game | probe: archive | — | frozen |
| `icons_ready` | game | — | client | after its check |
| `stat_open` | loader | probe: stats | — | frozen |
| `stat_info` | loader | probe: stats | — | frozen |
| `stat_value` | game | — | with a player | after its check |
| `stat_modify` | game | — | with a player | after its check |
| `stat_buff` | game | — | with a player | after its check |
| `stats_changed` | game | every start | with a player | after its check |
| `stat_sources` | game | — | with a player | after its check |
| `stat_watch` | game | — | with a player | after its check |
| `define_event` | loader | — | — | frozen |
| `subscribe_event` | loader | — | — | frozen |
| `send_event` | loader | — | — | frozen |
| `affix_open` | loader | probe: affixes | — | frozen |
| `affix_roll` | game | probe: affixes | — | frozen |
| `item_lines` | game | probe: affixes | — | frozen |
| `affix_info` | loader | probe: affixes | — | frozen |
| `rename_content` | game | — | client | after its check |
| `declare_content` | loader | — | — | frozen |
| `entity_data_open` | loader | probe: entity data | — | frozen |
| `mod_dir` | loader | every start | — | frozen |
| `knowledge_input_filter` | game | every start | with a player | after its check |
| `language` | game | — | client | after its check |
| `keep_game_settings` | game | probe: game settings | — | frozen |
| `forget_game_settings` | game | probe: game settings | — | frozen |
| `revoke_knowledge` | game | probe: knowledge | — | frozen |
| `account_knowledge` | game | probe: knowledge | — | frozen |
| `resource_read` | game | probe: archive | — | frozen |
| `game_text` | game | — | client | after its check |
| `rebuild_resource` | game | — | client | after its check |
| `recipe_rule` | game | — | client | after its check |
| `recipe_disabled` | game | — | client | after its check |
| `define_point` | loader | — | — | frozen |
| `attach_point` | loader | — | — | frozen |
| `run_point` | loader | — | — | frozen |
| `actions` | loader | every start | client | frozen |
| `event_types` | game | every start | — | frozen |
| `recipe_kind` | game | — | client | after its check |
| `feature` | loader | every start | — | frozen |
| `rate` | game | — | with a player | after its check |
| `set_rate` | game | — | with a player | after its check |
| `change_data` | game | — | client | after its check |
| `loop_wait` | game | probe: loop waits | — | frozen |
| `server_wait` | game | probe: loop waits | — | frozen |
| `task_workers` | game | probe: task workers | — | frozen |
| `altar_zone_filter` | game | probe: altars | — | frozen |
| `altar_zones_changed` | game | probe: altars | — | frozen |
| `altar_limit_filter` | game | — | with a player | after its check |
| `base_limit` | game | probe: base limits | — | frozen |
| `voxel_read` | game | probe: world reads | — | frozen |
| `declare_change` | game | — | client | after its check |
| `set_change` | game | — | client | after its check |
| `build_rights` | game | — | with a player | after its check |
| `transaction_commit` | game | — | — | unstable |
| `transaction_undo` | game | — | — | unstable |
| `transaction_status` | game | — | — | unstable |
| `transaction_commit_for` | game | — | with a player | after its check |
| `world_quiet` | game | — | with a player | after its check |
| `fog_open` | game | — | client | after its check |
| `fog_read` | game | — | client | after its check |
| `serve_call` | loader | — | — | frozen |
| `find_call` | loader | — | — | frozen |
| `call` | loader | — | — | frozen |
| `transaction_entities` | game | — | — | unstable |
| `world_persists` | game | probe: world reads | — | frozen |
| `map_markers` | game | probe: map | — | frozen |
| `account_action` | game | — | with a player | after its check |
| `subscribe_tick` | game | probe: ticks | — | frozen |
| `set_tick_every` | game | probe: ticks | — | frozen |
| `map_frame` | game | probe: map | — | frozen |
| `request_base_limits` | game | probe: base limits | — | frozen |
| `map_marker_types` | game | — | client | after its check |
| `use_content` | loader | — | — | frozen |
| `chat_mute_set` | game | — | with a player | after its check |
| `chat_mute_has` | loader | — | — | frozen |
| `set_base_cap` | game | probe: base limits | — | frozen |
| `resource_keys` | game | probe: archive | — | frozen |
| `content_hashes` | game | probe: archive | — | frozen |
| `types` | game | probe: host reads | — | frozen |
| `game_version` | game | probe: host reads | — | frozen |
| `control` | loader | every start | client | frozen |
| `run_action` | game | — | client | after its check |
| `ui_extension` | game | — | client | after its check |
| `item_filter` | game | every start | with a player | after its check |
| `chat_filter` | game | every start | with a player | after its check |
| `save_info` | game | probe: saves | — | frozen |
| `hold_saves` | game | probe: saves | — | frozen |
| `crafting_prop` | game | — | client | after its check |
| `set_saved` | game | probe: spawns | — | frozen |
| `run_action_at` | game | — | client | after its check |
| `local_held` | game | — | client | after its check |
| `read_only` | loader | — | — | frozen |
| `resource_served` | game | probe: archive | — | frozen |
| `commands` | loader | — | — | frozen |
| `run_command` | loader | — | — | frozen |
| `command_list` | loader | — | — | frozen |
| `actions_changed` | loader | — | — | frozen |
| `resource_buffer` | loader | — | — | frozen |
| `game_query` | game | — | with a player | after its check |
| `action_window` | game | — | client | after its check |
