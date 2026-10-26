# ABI reference

The loader and a mod talk through a C ABI: `repr(C)` structs, integer enums,
`extern "system"` functions, strings borrowed for the call, and no unwinding
across the boundary. Mods normally use the safe layer of `enshwick`; this
chapter is for reading logs and the rare call to the raw table. The exact types and signatures are in the
[`abi` module's rustdoc](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/abi.rs).

## Versions

| Constant | Meaning |
| --- | --- |
| [`ABI_MAJOR`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/abi.rs) | A loader refuses a mod built for another major. |
| [`ABI_MINOR`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/abi.rs) | A newer minor only appends. |

The ABI version is apart from Enshwick Loader's version (such as 0.2.0, see
[Releasing mods](releasing.md#versions)): a platform release raises
`ABI_MINOR` only when it appends to the ABI.

The policy:

- **Major.** A mod exports `enshwick_mod_abi`, the major it was built for. A
  loader of another major does not load it and logs
  `<file> is built for ABI 2, the loader speaks 1`.
- **Minor.** A loader refuses a mod built for a newer minor and logs
  `built for ABI 2.67, newer than the loader's 2.66: update Enshwick`, so a
  mod always meets a loader that has every entry of the host table it knows,
  and the safe layer calls them without checks. Structs the other side may
  know an older version of start with a
  [`Header`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/abi.rs):
  the struct's size as its writer knew it and the writer's major and minor.
  Fields are only ever appended, and the loader reads and writes a mod's
  struct only as far as that mod's header says. `abi::read_headed` copies
  such a struct as far as its writer's header reaches and leaves the rest
  zero.

A mod built against an older `enshwick` loads under a newer loader:
the loader reads the manifest fields the mod's header covers and takes the
rest as empty, with no build rows and no names. A mod built against a
newer one waits for a loader update.

## How does `enshwick` version its API?

The `enshwick` crate follows [SemVer](https://semver.org) for its Rust API,
and the ABI version follows the crate:

| Change | ABI | `enshwick` version |
| --- | --- | --- |
| A field, struct or host table entry removed, moved or resized | major | major |
| A public item removed or renamed, a signature changed | none | major |
| A field appended to a struct, an entry appended to the host table | minor | minor |
| A new public item that adds no ABI | none | minor |
| A fix that changes neither the API nor the ABI | none | patch |

Until 1.0.0 the crate is a prerelease: any release may break the API, and a
break raises the minor of a `0.x` version or the prerelease number of
`1.0.0-alpha.N`. From 1.0.0 the table holds as written.

Three checks hold the repository to it:

- A layout file,
  [`abi/layout.txt`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/abi/layout.txt),
  lists the size, alignment and field offsets of every ABI type. A test
  fails when the layout differs from the file, and refuses to rewrite the
  file unless the change appends with a higher `ABI_MINOR` or comes with a
  higher `ABI_MAJOR`.
- CI runs [cargo-semver-checks](https://github.com/obi1kenobi/cargo-semver-checks)
  on `enshwick` against its last release on crates.io, so a release whose
  version claims less change than its API made fails.
- A test builds a mod against the last published `enshwick` and starts it
  against the current host table.

## The exports of a mod

[`declare_mod!`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/declare.rs)
writes five exports:

| Export                  | Signature                                 | When the loader calls it                               |
| ----------------------- | ----------------------------------------- | ------------------------------------------------------ |
| `enshwick_mod_abi`      | `fn() -> u32`                             | First: the major the mod was built for.                |
| `enshwick_mod_query`    | `fn() -> *const Manifest`                 | Before any of the mod's code runs; static data.        |
| `enshwick_mod_init`     | `fn(*const HostApi, ModHandle) -> Status` | Once, in load order; registers hooks and the rest.     |
| `enshwick_mod_ready`    | `fn(*const HostApi, ModHandle)`           | After every chain is armed, if `init` returned OK.     |
| `enshwick_mod_shutdown` | `fn()`                                    | Before a development reload unloads it; optional (53). |

A DLL in `mods/` without the first four is logged as `<file> is not an Enshwick mod`
and unloaded.

## The manifest

[`Manifest`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/abi.rs)
is what [`manifest!`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/declare.rs)
builds:

| Field          | What it holds                                                                                     | Since minor |
| -------------- | ------------------------------------------------------------------------------------------------- | ----------- |
| `header`       | Size and version of the struct                                                                    | 0           |
| `id`           | `<author>.<name>`, each of `[a-z0-9-]`, at most 64 characters; names settings and data (minor 70) | 0           |
| `version`      | SemVer                                                                                            | 0           |
| `side`         | `SERVER` (1), `CLIENT` (2) or `BOTH` (3)                                                          | 0           |
| `flags`        | `JOIN` (1): counts in the join check; `RELOAD` (2, minor 53): may be reloaded                     | 0           |
| `dependencies` | `Dependency { id, range, relation }`; `REQUIRES` (0), `AFTER` (1), `BREAKS` (2), `RECOMMENDS` (3) | 0           |
| `builds`       | `BuildRow { side, fingerprint, symbols, constants }`: symbols and constants pinned per game build | 0           |
| `name`         | What players read; empty for the id                                                               | 36          |
| `description`  | One sentence on what the mod does                                                                 | 36          |
| `authors`      | `*const Str` and `author_count`: the authors' names                                               | 36          |
| `license`      | An SPDX licence expression                                                                        | 36          |
| `homepage`     | Where players read about the mod                                                                  | 36          |
| `points`       | The schemas of the extension points the mod defines, as TOML; also in the `.enshpt` section       | 71          |

Since minor 54 a dependency may also be `REPLACES` (4), a mod this one takes
the place of, with an empty range. Since minor 69 it may be `WAS` (5), an id
this mod had before a rename, which replaces that id as well and carries its
settings and data over. `HostApi::replaced` lists the mods replaced either way,
with `ModInfo::replaced_by` (minor 54) naming the mod that replaces each.

The loader skips a mod whose manifest has another major, an invalid id, the
loader's own id `enshwick`, a version that is not SemVer, an unknown side, or a
dependency range that does not parse; the reason goes to `loader.log` as
`<file> skipped: <reason>`.

## Status codes

Every call that can fail returns a
[`Status`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/abi.rs).
The safe layer turns a failed status into an
[`Off`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/load.rs)
reason of the form `<what>: <why>`, such as
`symbol quest.reward: missing on this game build`:

| Code | Constant        | Meaning                                                                    | `<why>` in the Off reason          |
| ---- | --------------- | -------------------------------------------------------------------------- | ---------------------------------- |
| 0    | `OK`            | Success                                                                    |                                    |
| 1    | `MISSING`       | What was asked for does not exist on this game build                       | `missing on this game build`       |
| 2    | `AMBIGUOUS`     | More than one candidate answers a lookup                                   | `ambiguous on this game build`     |
| 3    | `REFUSED`       | Conflicts with another mod or with the loader's rules                      | `refused by the loader`            |
| 4    | `BAD_ARGS`      | An argument is malformed                                                   | `malformed`                        |
| 5    | `TOO_LATE`      | Called in a phase where it is not allowed, such as a hook after arming     | `asked for too late`               |
| 6    | `FAULTED`       | The mod has faulted and is off                                             | `unavailable: the mod has faulted` |
| 7    | `WRONG_THREAD`  | Called from a thread the function does not serve                           | `called off the thread it serves`  |
| 8    | `NOT_LOADED`    | The part of the world asked about is not loaded yet; try again in a second | `not loaded yet`                   |
| 9    | `OUTSIDE_WORLD` | The box asked about leaves the world, past its edge or above its height    | `outside the world`                |

`enshwick_mod_init` itself returns `OK`, `REFUSED` when `Mod::init` returned
`Err(Off)`, or `FAULTED` when it panicked; the loader logs
`off: init returned 3` or `off: init returned 6` under the mod's id.

Every host function answers a mod that is off with `FAULTED`, and a handle
the loader never gave out with `BAD_ARGS`. A panic in the loader's own code
behind a host function never reaches the mod: the call returns `REFUSED`
(or zero for a function that returns a value) and the loader logs the
fault.

## Events

A mod subscribes with
[`Load::on`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/load.rs)
and the types in
[`events`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/events.rs),
or with a raw [`Event`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/abi.rs)
id through `Load::on_raw_event`. See [Events and the scheduler](events.md).

| Id | Constant                  | Typed event             | Data              | Raised on       | Side   |
| -- | ------------------------- | ----------------------- | ----------------- | --------------- | ------ |
| 1  | `SETTINGS_CHANGED`        | `SettingsChanged`       | none              | not fixed       | both   |
| 2  | `LOADER_SETTINGS_CHANGED` | `LoaderSettingsChanged` | none              | a loader thread | both   |
| 3  | `TICKED`                  | `Ticked`                | `TickData`        | the game thread | server |
| 4  | `WORLD_LOADED`            | `WorldLoaded`           | none              | the game thread | server |
| 5  | `WORLD_SAVED`             | `WorldSaved`            | none              | the game thread | server |
| 6  | `PLAYER_JOINED`           | `PlayerJoined`          | `PlayerData`      | the game thread | server |
| 7  | `PLAYER_LEFT`             | `PlayerLeft`            | `PlayerData`      | the game thread | server |
| 8  | `SHUTDOWN_STARTED`        | `ShutdownStarted`       | none              | the game thread | server |
| 9  | `BUILDING_PLACED`         | `BuildingPlaced`        | `BuildingData`    | the game thread | server |
| 10 | `BUILDING_REMOVED`        | `BuildingRemoved`       | `BuildingData`    | the game thread | server |
| 11 | `JOIN_REFUSED`            | `JoinRefused`           | none              | the game thread | client |
| 12 | `ALTARS_CHANGED`          | `AltarsChanged`         | none              | the game thread | server |
| 13 | `DAY_STARTED`             | `DayStarted`            | none              | the game thread | server |
| 14 | `NIGHT_STARTED`           | `NightStarted`          | none              | the game thread | server |
| 15 | `KNOWLEDGE_CHANGED`       | `KnowledgeChanged`      | `KnowledgeData`   | the game thread | server |
| 16 | `QUEST_COMPLETED`         | `QuestCompleted`        | `QuestData`       | the game thread | server |
| 17 | `CHUNK_LOADED`            | `ChunkLoaded`           | `ChunkData`       | the game thread | server |
| 18 | `CHUNK_UNLOADED`          | `ChunkUnloaded`         | `ChunkData`       | the game thread | server |
| 19 | `SETTINGS_REPLACED`       | `SettingsReplaced`      | none              | the game thread | server |
| 20 | `PLAYER_DIED`             | `PlayerDied`            | `PlayerDeathData` | the game thread | server |

Subscribing needs minor 2; `JOIN_REFUSED` came with minor 10 and
`ALTARS_CHANGED`, raised for mods that opened altars, with minor 15;
`DAY_STARTED` and `NIGHT_STARTED` with minor 28; `KNOWLEDGE_CHANGED` and
`QUEST_COMPLETED` with minor 32; `CHUNK_LOADED` and `CHUNK_UNLOADED` with
minor 33; `SETTINGS_REPLACED` with minor 46; `PLAYER_DIED` with minor 60. "Server" events also reach a client while it hosts a game, solo
play included. The typed events hand a handler the fields the loader knows,
with fields added after the event zero; they skip the handler only when the
data lacks a field the event came with.

## The host table

[`HostApi`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/abi.rs)
is the table of functions the loader passes to `enshwick_mod_init`. Fields
marked "init only" are allowed during `enshwick_mod_init` only.

| Field                    | What it does                                                                                         | Since minor |
| ------------------------ | ---------------------------------------------------------------------------------------------------- | ----------- |
| `header`                 | Size and version of the table                                                                        | 0           |
| `host_version`           | The loader's own version                                                                             | 0           |
| `build`                  | Writes the running game's side, image base, build fingerprint and where its build table is from      | 0           |
| `symbol`                 | Resolves a symbolic id for the running build: the mod's own rows first, then the loader's table      | 0           |
| `system`                 | Resolves an ECS system's update function by its reflected name                                       | 0           |
| `field_offset`           | Resolves a field's offset in a reflected type                                                        | 0           |
| `type_size`              | Resolves a reflected type's size                                                                     | 0           |
| `hook`                   | Links a detour into a function's chain; init only                                                    | 0           |
| `caller_frame`           | Writes the game caller's context at entry of the current chain, for `HookFlags::CALLER_FRAME`        | 0           |
| `fault`                  | Reports a run-time failure; the loader unlinks the mod and turns it off                              | 0           |
| `log`                    | Writes a line to the log under the mod's id                                                          | 0           |
| `settings`               | The text of the mod's settings file, written from the defaults when missing                          | 0           |
| `publish`                | Publishes a service table with its layout for other mods; init only                                  | 47          |
| `service`                | Finds a published service and checks its layout starts with the wanted one                           | 47          |
| `constant`               | Resolves an engine constant of the running build by its symbolic id                                  | 0           |
| `data_dir`               | The mod's data folder, `mod-data/<id>`, created on first use                                         | 0           |
| `log_level`              | The most detailed level kept for the mod's lines                                                     | 1           |
| `subscribe`              | Calls a handler on every occurrence of an event; init only                                           | 2           |
| `type_hash`              | The ECS type hash of a reflected component type                                                      | 3           |
| `component`              | A read-only pointer to an entity's component; server game thread only                                | 3           |
| `entity_template`        | The template GUID of an entity; server game thread only                                              | 3           |
| `player`                 | The connected player whose character is an entity; server game thread only                           | 3           |
| `players`                | The connected players with characters in the world; server game thread only                          | 3           |
| `serve_resource`         | Serves a resource built from a base resource by a patch function; init only, one mod per key         | 4           |
| `add_text`               | Adds the text of a `LocaTag` in a language; init only                                                | 4           |
| `mods`                   | The loaded mods, the loader first                                                                    | 5           |
| `sync`                   | Reports the mod's join values and returns the joined server's values to run with                     | 6           |
| `code_range`             | Where the game's code section is loaded and its length                                               | 7           |
| `patch_code`             | Replaces bytes of game code that must read as expected at arming; init only                          | 7           |
| `workshop_recipes`       | The most recipes one workshop takes: 512, or `u32::MAX` once the loader lifted the limit             | 7           |
| `hosting`                | 1 while this process runs a game server                                                              | 8           |
| `server_world`           | The entity world of the game server this process runs; 0 for none                                    | 8           |
| `retired_command`        | Retired with chat commands; always `MISSING`                                                         | 9           |
| `chat`                   | Sends a chat line to one player or everyone, from any thread                                         | 9           |
| `set_label`              | Gives an entity a name, as a player's rename does; server game thread only                           | 9           |
| `rename_filter`          | Calls a filter before the server applies a player's rename; init only                                | 9           |
| `join_missing`           | The server's join mods the last refused join found missing on this client                            | 10          |
| `hook_at`                | Runs a handler before an instruction inside a game function; init only                               | 11          |
| `inventory_open`         | Asks for inventory transactions and item data; init only; `MISSING` on a build that lacks them       | 12          |
| `item_info`              | Writes an item's static data, since minor 88 with the template it places; any thread                 | 12          |
| `inventory_submit`       | Queues an inventory transaction, from any thread; the result comes on the game thread                | 12          |
| `status`                 | Adds a source of lines for the loader's status summary                                               | 13          |
| `run_console`            | Runs a console line as the server's console, an action's text form included; server game thread only | 13          |
| `altars_open`            | Asks the loader to follow the Flame Altars of the server's world; init only                          | 15          |
| `altars`                 | The altars with their levels and zones; any thread; `REFUSED` for a mod that did not open them       | 15          |
| `altar_zones_control`    | Retired in 78: always `REFUSED`; see `altar_zone_filter`                                             | 15          |
| `set_altar_zones`        | Retired in 78: always `REFUSED`; see `altar_zone_filter`                                             | 15          |
| `zone_check_filter`      | Calls a filter when the game refuses a placement outside a single building zone; init only           | 15          |
| `survivor_lookup`        | Answers the game's survivor lookups, one mod at a time; init only                                    | 16          |
| `survivor_bed_check`     | Calls a filter after a survivor's own bed check kept its bed; init only                              | 16          |
| `summon_filter`          | Calls a filter before a summoning staff spawns or moves a survivor; init only                        | 16          |
| `summon_item`            | Files an item as a survivor's summoning staff; init only                                             | 16          |
| `survivor_status`        | Merges the client's survivor menu statuses, one mod at a time; init only                             | 16          |
| `interaction_filter`     | Calls a filter before a player's interaction with an entity, in the server's world; init only        | 17          |
| `skill_override`         | Answers the game's checks of one skill node; init only                                               | 18          |
| `comfort_filter`         | Changes the comfort each provider registers in a base; init only                                     | 18          |
| `experience_filter`      | Changes the characters' gained experience before the server applies it; init only                    | 18          |
| `altar_limit`            | Retired in 82: always `REFUSED`; see `altar_limit_filter`                                            | 19          |
| `raise_base_limit`       | Lets the game group altar zones up to the loader's maximum; init only; `MISSING` on an unknown build | 19          |
| `base_zones`             | The zones and bases of the last grouping, while the raised limit is in force                         | 19          |
| `game_event`             | Calls a handler on every game event of a reflected type, or of all; init only                        | 20          |
| `entities`               | The entities with a component and a place in the world; server game thread only                      | 21          |
| `destroy`                | Asks the server to destroy an entity after the next step; server game thread only                    | 21          |
| `save_world`             | Asks the server to save the world now; server game thread only                                       | 21          |
| `world_key`              | The key of the world the server runs: its save file's name and a folder hash                         | 21          |
| `component_write`        | A component to write, marked changed; refuses client copies; server game thread only                 | 22          |
| `emit`                   | Raises a game event of an allowed type; server game thread only                                      | 23          |
| `teleport`               | Moves an entity as the admin teleport does, after the next step; server game thread only             | 23          |
| `spawn`                  | Spawns an entity from a template as the admin spawn does; server game thread only                    | 24          |
| `craft_filter`           | Calls a filter on each player's craft request in the server's world; init only                       | 25          |
| `build_filter`           | Calls a filter after the cursor check allows a placement, on both sides; init only                   | 25          |
| `net_channel`            | Opens a join mod's channel between its server and client parts; init only                            | 26          |
| `net_send`               | Sends a message on the mod's channel to the server or a player's client; any thread                  | 26          |
| `net_ready`              | Whether a peer takes messages now; any thread                                                        | 26          |
| `notify`                 | Shows a line on a player's screen through the client's loader; server game thread only               | 27          |
| `time_of_day`            | The clock of the server's world; server game thread only                                             | 28          |
| `set_time_of_day`        | Sets the time of day; clients follow after the next step; server game thread only                    | 28          |
| `time_speed_control`     | Lets the mod set a factor on the clock's speed; init only                                            | 28          |
| `set_time_speed`         | Sets the mod's factor on the clock's speed, 0 to 1000; server game thread only                       | 28          |
| `game_settings`          | Copies the server's live game settings and their preset; server game thread only                     | 29          |
| `set_game_settings`      | Replaces the live game settings as the admin command does, until the server stops                    | 29          |
| `damage_filter`          | Calls a filter on damage before the game applies it, on worker threads; init only                    | 30          |
| `buff_types`             | The buff and debuff types of the running game; server game thread only                               | 31          |
| `buffs`                  | The buffs an entity has; server game thread only                                                     | 31          |
| `apply_buff`             | Gives an entity a buff, or renews it, as the game restores saved buffs; server game thread only      | 31          |
| `remove_buff`            | Removes a buff type from an entity, as the game cleanses one; server game thread only                | 31          |
| `knowledge`              | A knowledge value of a player or the world; server game thread only                                  | 32          |
| `grant_knowledge`        | Grants knowledge as the game does, applied the next step; server game thread only                    | 32          |
| `quests`                 | The quest ids of the running game; server game thread only                                           | 32          |
| `weather_at`             | How clear, rainy, snowy and stormy it is at a place; server game thread only                         | 33          |
| `weather_fronts`         | The current weather fronts of the server's world; server game thread only                            | 33          |
| `set_weather`            | Starts new weather in every layer, or lets the game choose; server game thread only                  | 33          |
| `terrain_materials`      | The terrain materials of the running game; server game thread only                                   | 34          |
| `terrain_edit`           | Adds, digs or flattens ground as the game's terraforming does; server game thread only               | 34          |
| `config_schema`          | Hands the loader the mod's settings schema as TOML text; any time                                    | 35          |
| `summoned`               | Calls a handler after a summoning staff's use spawned or moved a survivor; init only                 | 37          |
| `item_data_open`         | Opens an item data kind by id; init only                                                             | 38          |
| `item_data_get`          | Copies an item's value of a kind; any thread                                                         | 38          |
| `item_data_set`          | Sets or removes an item's value of a kind and sends it to clients; server, any thread                | 38          |
| `ui_frame`               | Calls a callback each frame of the client's interface; init only                                     | 39          |
| `ui_widget`              | Draws a window, label, button, row, bar or sound in the mod's frame callback                         | 39          |
| `ui_dialog`              | Queues a dialog in the game's style, answered on the thread that draws                               | 39          |
| `ui_hud`                 | Calls a callback each frame the game draws its HUD; init only                                        | 40          |
| `ui_markers`             | Replaces the mod's markers on the client's map; any thread                                           | 40          |
| `config_check`           | The mod's check of a new settings text before the loader writes it; init only                        | 41          |
| `texts`                  | Hands the loader the mod's texts, for the labels of its settings on the Mods screen                  | 43          |
| `content_serve`          | Serves bytes as game content, a served texture's pixels, and gives their hash; any thread            | 44          |
| `content_read`           | Copies the game's own content from its archive files; any thread                                     | 44          |
| `icons_ready`            | Whether this client shows the UI images mods serve; any thread                                       | 44          |
| `stat_open`              | Defines or opens a stat kind by id; init only                                                        | 45          |
| `stat_info`              | A stat kind's definition and id; any thread                                                          | 45          |
| `stat_value`             | A stat's value on an entity, without locks; any thread                                               | 45          |
| `stat_modify`            | Sets or removes a mod's modifier on an entity; server, any thread                                    | 45          |
| `stat_buff`              | Gives a buff type modifiers; init only                                                               | 45          |
| `stats_changed`          | Calls a handler for every stat value that changed; init only                                         | 45          |
| `stat_sources`           | The modifiers that count toward an entity's stat; server game thread only                            | 45          |
| `stat_watch`             | Watches an entity for its worn items and buffs; any thread                                           | 45          |
| `define_event`           | Defines a mod event named after the mod, with its layout; init only                                  | 47          |
| `subscribe_event`        | Calls a handler on every mod event of a name whose layout serves the handler's; init only            | 47          |
| `send_event`             | Hands a mod event the mod defined to its subscribers on this thread; any thread                      | 47          |
| `affix_open`             | Defines an affix in the mod's namespace; init only                                                   | 48          |
| `affix_roll`             | Rolls affixes onto an item and keeps them; server game thread only                                   | 48          |
| `item_lines`             | The lines of what an item gives, own modifiers then affixes; any thread                              | 48          |
| `affix_info`             | An affix's text key and rarity; any thread                                                           | 48          |
| `migrate`                | Registers the mod's migrations and runs them on its settings file; init only                         | 49          |
| `declare_content`        | Declares a content id in the mod's namespace and returns its GUID; init only                         | 50          |
| `entity_data_open`       | Opens an entity data kind, sent to clients or not; init only                                         | 52          |
| `replaced`               | The mods a present mod replaces, never started; any thread                                           | 54          |
| `knowledge_input_filter` | A filter on the knowledge clients ask for in their input; server worker threads                      | 55          |
| `language`               | The game language the client last asked a text in; 0 on a server; any thread                         | 56          |
| `keep_game_settings`     | Changes the game settings and keeps the marked fields with the world; server game thread only        | 57          |
| `forget_game_settings`   | Forgets the world's kept game settings and puts back its loaded values; server game thread only      | 57          |
| `revoke_knowledge`       | Takes knowledge back and has the clients sent it again; server game thread only                      | 59          |
| `account_knowledge`      | Grants or takes back an account's knowledge now or once its character loads; server game thread only | 59          |
| `resource_read`          | Copies a vanilla resource from the game's archive files; any thread                                  | 61          |
| `game_text`              | The game's own text of a tag in a language, from its archive; client only, any thread                | 61          |
| `rebuild_resource`       | Builds a served resource again from its kept base with every mod's patches; any thread               | 63          |
| `recipe_rule`            | Adds or takes back the mod's rule that disables recipes by recipe, item or workshop; any thread      | 64          |
| `recipe_disabled`        | Whether a recipe is disabled in this process now; any thread                                         | 64          |
| `define_point`           | Defines an extension point named after the mod, with its layouts and mode; init only                 | 65          |
| `attach_point`           | Attaches a link to another mod's extension point, with its order; init only                          | 65          |
| `run_point`              | Runs the live links of the mod's own extension point over its answer; any thread                     | 65          |
| `actions`                | Registers the mod's player actions, asked for sheets, runs and hints; init only                      | 66          |
| `event_types`            | Every game event type the running build reflects, with hash and size; any thread                     | 72          |
| `recipe_kind`            | The kind of the recipes that make an item, from its loaded static data; any thread                   | 73          |
| `rate`                   | Declares a game rate the mod multiplies; init only                                                   | 75          |
| `set_rate`               | Puts the mod's factor for a declared rate in force, 0 to 100; any thread                             | 75          |
| `change_data`            | Declares a change of a vanilla value, which the loader merges with every mod's; init only            | 76          |
| `feature`                | Reports one of the mod's features on or off for the log, status summary and Mods screen              | 74          |
| `loop_wait`              | Runs a handler before every wait of the engine's event loop; init only                               | 77          |
| `server_wait`            | Runs a handler where the server loop picks its wait, which it may change; init only                  | 77          |
| `task_workers`           | Caps the task groups' worker threads, the lowest cap winning; init only                              | 77          |
| `altar_zone_filter`      | Adds a filter to the chain that sizes altar zones; init only                                         | 78          |
| `altar_zones_changed`    | Has the altar zones sized again on the server's next step; any thread                                | 78          |
| `altar_limit_filter`     | Adds a filter to the chain that decides how many altars a Flame level allows; init only              | 82          |
| `base_limit`             | The most altar zones and bases the game groups now, and its own grouping's zones; any thread         | 82          |
| `voxel_read`             | Reads a box of up to 64³ voxels at full detail, decoded; server game thread, quiet moment only       | 84          |
| `declare_change`         | Declares a change of a vanilla value whose value the mod sets later; init only                       | 85          |
| `set_change`             | Puts the mod's value of a declared change in force, or takes it out; any thread                      | 85          |
| `build_rights`           | Whether the server lets a player edit a box; game thread, quiet moment only, refused inside a step   | 86          |
| `transaction_commit`     | Queues terrain edits as one transaction written at quiet moments, with undo; any thread              | 87          |
| `transaction_undo`       | Queues the undo of one of the mod's transactions, refused once its bricks changed; any thread        | 87          |
| `transaction_status`     | Where one of the mod's transactions stands: queued, applied, undone, failed and why; any thread      | 87          |
| `transaction_commit_for` | As `transaction_commit`, for a player whose rights the loader checks first; any thread               | 89          |
