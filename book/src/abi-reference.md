# ABI reference

The loader and a mod talk through a C ABI: `repr(C)` structs, integer enums,
`extern "system"` functions, strings borrowed for the call, and no unwinding
across the boundary. Mods normally use the safe layer of `enshwick-loader-api`; this
chapter is for reading logs, supporting older loaders, and the rare call to the
raw table. The exact types and signatures are in the
[`abi` module's rustdoc](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/abi.rs).

## Versions

| Constant                                                                                            | Value | Meaning                                         |
| --------------------------------------------------------------------------------------------------- | ----- | ----------------------------------------------- |
| [`ABI_MAJOR`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/abi.rs) | 1     | A loader refuses a mod built for another major. |
| [`ABI_MINOR`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/abi.rs) | 15    | A newer minor only appends.                     |

The ABI version is apart from Enshwick Loader's version (such as 0.2.0, see
[Releasing and updates](releasing.md#versions)): a platform release raises
`ABI_MINOR` only when it appends to the ABI.

The policy:

- **Major.** A mod exports `enshwick_mod_abi`, the major it was built for. A
  loader of another major does not load it and logs
  `<file> is built for ABI 2, the loader speaks 1`.
- **Minor.** Structs the other side may know an older or newer version of
  start with a [`Header`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/abi.rs):
  the struct's size as its writer knew it and the writer's major and minor.
  Fields are only ever appended, so a reader checks with `Header::covers(end)`
  that a field is there before it reads it. `abi::read_headed` copies such a
  struct as far as its writer's header reaches and leaves the rest zero;
  `abi::write_headed` writes only the bytes both sides know.

A mod built against a newer `enshwick-loader-api` therefore runs under an older
loader of the same major, as long as it does not need what that loader lacks.
A mod built against an older one loads under a newer loader: the loader reads
the manifest fields the mod's header covers and takes the rest as empty, with
no update source and no build rows.

## The exports of a mod

[`declare_mod!`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/declare.rs)
writes four exports:

| Export               | Signature                                 | When the loader calls it                           |
| -------------------- | ----------------------------------------- | -------------------------------------------------- |
| `enshwick_mod_abi`   | `fn() -> u32`                             | First: the major the mod was built for.            |
| `enshwick_mod_query` | `fn() -> *const Manifest`                 | Before any of the mod's code runs; static data.    |
| `enshwick_mod_init`  | `fn(*const HostApi, ModHandle) -> Status` | Once, in load order; registers hooks and the rest. |
| `enshwick_mod_ready` | `fn(*const HostApi, ModHandle)`           | After every chain is armed, if `init` returned OK. |

A DLL in `mods/` without all four is logged as `<file> is not an Enshwick mod`
and unloaded.

## The manifest

[`Manifest`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/abi.rs)
is what [`manifest!`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/declare.rs)
builds:

| Field          | What it holds                                                                                     | Since minor |
| -------------- | ------------------------------------------------------------------------------------------------- | ----------- |
| `header`       | Size and version of the struct                                                                    | 0           |
| `id`           | `[a-z0-9-]`, 1 to 64 characters; names the settings file and data folder                          | 0           |
| `version`      | SemVer                                                                                            | 0           |
| `side`         | `SERVER` (1), `CLIENT` (2) or `BOTH` (3)                                                          | 0           |
| `flags`        | `ManifestFlags::JOIN` (1) when the mod counts in the join check                                   | 0           |
| `dependencies` | `Dependency { id, range, relation }`; relation `REQUIRES` (0), `AFTER` (1) or `CONFLICTS` (2)     | 0           |
| `builds`       | `BuildRow { side, fingerprint, symbols, constants }`: symbols and constants pinned per game build | 0           |
| `updates`      | `UpdateSource { kind, location, component }`, or null; kind `NONE`, `GITHUB` or `URL`             | 5           |
| `signed_by`    | Two `Str`: the author's minisign public keys (`RW...`); empty strings for none                    | 14          |

The loader skips a mod whose manifest has another major, an invalid id, the
loader's own id `enshwick`, a version that is not SemVer, an unknown side, or a
dependency range that does not parse; the reason goes to `loader.log` as
`<file> skipped: <reason>`. A malformed update source is dropped with a
warning and the mod loads, and so is a `signed_by` key that is not a minisign
public key: `<file>: key ignored: the signed_by key "..." is not a minisign
public key`.

## Status codes

Every call that can fail returns a
[`Status`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/abi.rs).
The safe layer turns a failed status into an
[`Off`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/load.rs)
reason of the form `<what>: <why>`, such as
`symbol quest.reward: missing on this game build`:

| Code | Constant       | Meaning                                                                | `<why>` in the Off reason          |
| ---- | -------------- | ---------------------------------------------------------------------- | ---------------------------------- |
| 0    | `OK`           | Success                                                                |                                    |
| 1    | `MISSING`      | What was asked for does not exist on this game build                   | `missing on this game build`       |
| 2    | `AMBIGUOUS`    | More than one candidate answers a lookup                               | `ambiguous on this game build`     |
| 3    | `REFUSED`      | Conflicts with another mod or with the loader's rules                  | `refused by the loader`            |
| 4    | `BAD_ARGS`     | An argument is malformed                                               | `malformed`                        |
| 5    | `TOO_LATE`     | Called in a phase where it is not allowed, such as a hook after arming | `asked for too late`               |
| 6    | `FAULTED`      | The mod has faulted and is off                                         | `unavailable: the mod has faulted` |
| 7    | `WRONG_THREAD` | Called from a thread the function does not serve                       | `called off the thread it serves`  |

`enshwick_mod_init` itself returns `OK`, `REFUSED` when `Mod::init` returned
`Err(Off)`, or `FAULTED` when it panicked; the loader logs
`off: init returned 3` or `off: init returned 6` under the mod's id.

## Events

A mod subscribes with
[`Load::on`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/load.rs)
and the types in
[`events`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/events.rs),
or with a raw [`Event`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/abi.rs)
id through `Load::subscribe`. See [Events and the scheduler](events.md).

| Id  | Constant                  | Typed event             | Data           | Raised on       | Side   |
| --- | ------------------------- | ----------------------- | -------------- | --------------- | ------ |
| 1   | `SETTINGS_CHANGED`        | `SettingsChanged`       | none           | a loader thread | both   |
| 2   | `LOADER_SETTINGS_CHANGED` | `LoaderSettingsChanged` | none           | a loader thread | both   |
| 3   | `TICK`                    | `Tick`                  | `TickData`     | the game thread | server |
| 4   | `WORLD_LOADED`            | `WorldLoaded`           | none           | the game thread | server |
| 5   | `WORLD_SAVED`             | `WorldSaved`            | none           | the game thread | server |
| 6   | `PLAYER_JOINED`           | `PlayerJoined`          | `PlayerData`   | the game thread | server |
| 7   | `PLAYER_LEFT`             | `PlayerLeft`            | `PlayerData`   | the game thread | server |
| 8   | `SHUTDOWN`                | `Shutdown`              | none           | the game thread | server |
| 9   | `BUILDING_PLACED`         | `BuildingPlaced`        | `BuildingData` | the game thread | server |
| 10  | `BUILDING_REMOVED`        | `BuildingRemoved`       | `BuildingData` | the game thread | server |
| 11  | `JOIN_MISSING`            | `JoinMissing`           | none           | the game thread | client |
| 12  | `ALTARS_CHANGED`          | `AltarsChanged`         | none           | the game thread | server |

Subscribing needs minor 2; `JOIN_MISSING` came with minor 10 and
`ALTARS_CHANGED`, raised for mods that opened altars, with minor 15. "Server"
events also reach a client while it hosts a game, solo play included. The
typed events hand a handler the fields the loader knows, with fields added
after the event zero; they skip the handler only when the data lacks a field
the event came with.

## The host table

[`HostApi`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/abi.rs)
is the table of functions the loader passes to `enshwick_mod_init`. Fields
marked "init only" are allowed during `enshwick_mod_init` only.

| Field                 | What it does                                                                                    | Since minor |
| --------------------- | ----------------------------------------------------------------------------------------------- | ----------- |
| `header`              | Size and version of the table                                                                   | 0           |
| `host_version`        | The loader's own version                                                                        | 0           |
| `build`               | Writes the running game's side, image base and build fingerprint                                | 0           |
| `symbol`              | Resolves a symbolic id for the running build: the mod's own rows first, then the loader's table | 0           |
| `system`              | Resolves an ECS system's update function by its reflected name                                  | 0           |
| `field_offset`        | Resolves a field's offset in a reflected type                                                   | 0           |
| `type_size`           | Resolves a reflected type's size                                                                | 0           |
| `hook`                | Links a detour into a function's chain; init only                                               | 0           |
| `caller_frame`        | Writes the game caller's context at entry of the current chain, for `HookFlags::CALLER_FRAME`   | 0           |
| `fault`               | Reports a run-time failure; the loader unlinks the mod and turns it off                         | 0           |
| `log`                 | Writes a line to the log under the mod's id                                                     | 0           |
| `settings`            | The text of the mod's settings file, written from the defaults when missing                     | 0           |
| `publish`             | Publishes a versioned table of functions for other mods                                         | 0           |
| `service`             | Finds a table another mod published                                                             | 0           |
| `constant`            | Resolves an engine constant of the running build by its symbolic id                             | 0           |
| `data_dir`            | The mod's data folder, `mod-data/<id>`, created on first use                                    | 0           |
| `log_level`           | The most detailed level kept for the mod's lines                                                | 1           |
| `subscribe`           | Calls a handler on every occurrence of an event; init only                                      | 2           |
| `type_hash`           | The ECS type hash of a reflected component type                                                 | 3           |
| `component`           | A read-only pointer to an entity's component; server game thread only                           | 3           |
| `entity_template`     | The template GUID of an entity; server game thread only                                         | 3           |
| `player`              | The connected player whose character is an entity; server game thread only                      | 3           |
| `players`             | The connected players with characters in the world; server game thread only                     | 3           |
| `serve_resource`      | Serves a resource built from a base resource by a patch function; init only, one mod per key    | 4           |
| `add_text`            | Adds the text of a `LocaTag` in a language; init only                                           | 4           |
| `mods`                | The loaded mods, the loader first                                                               | 5           |
| `sync`                | Reports the mod's join values and returns the joined server's values to run with                | 6           |
| `code_range`          | Where the game's code section is loaded and its length                                          | 7           |
| `patch_code`          | Replaces bytes of game code that must read as expected at arming; init only                     | 7           |
| `workshop_recipes`    | The most recipes one workshop takes: 512, or `u32::MAX` once the loader lifted the limit        | 7           |
| `hosting`             | 1 while this process runs a game server                                                         | 8           |
| `server_world`        | The entity world of the game server this process runs; 0 for none                               | 8           |
| `command`             | Registers a chat command with a line of help; init only                                         | 9           |
| `chat`                | Sends a chat line to one player or everyone, from any thread                                    | 9           |
| `set_label`           | Gives an entity a name, as a player's rename does; server game thread only                      | 9           |
| `rename_filter`       | Calls a filter before the server applies a player's rename; init only                           | 9           |
| `join_missing`        | The server's join mods the last refused join found missing on this client                       | 10          |
| `hook_at`             | Runs a handler before an instruction inside a game function; init only                          | 11          |
| `inventory_open`      | Asks for inventory transactions and item data; init only; `MISSING` on a build that lacks them  | 12          |
| `item_info`           | Writes an item's static data; any thread                                                        | 12          |
| `inventory_submit`    | Queues an inventory transaction, from any thread; the result comes on the game thread           | 12          |
| `status`              | Adds a source of lines for the loader's status summary                                          | 13          |
| `run_command`         | Runs a chat command line as the server's console; server game thread only                       | 13          |
| `altars_open`         | Asks the loader to follow the Flame Altars of the server's world; init only                     | 15          |
| `altars`              | The altars with their levels and zones; any thread; `REFUSED` for a mod that did not open them  | 15          |
| `altar_zones_control` | Makes the mod the one that sizes altar zones, one mod at a time; init only                      | 15          |
| `set_altar_zones`     | Replaces the sizes the controlling mod gives altar zones; any thread                            | 15          |
| `zone_check_filter`   | Calls a filter when the game refuses a placement outside a single building zone; init only      | 15          |

## Under an older loader

The safe layer checks `header.covers` before it calls a field added after
minor 0. When the loader lacks a field, the feature either degrades or turns
the mod off with a reason, which the mod's log shows as `off: <reason>`:

| Needs minor | Feature                         | Under an older loader                                        |
| ----------- | ------------------------------- | ------------------------------------------------------------ |
| 1           | `Host::log_level`               | `Level::DEBUG` is assumed                                    |
| 2           | `Load::on`, `Load::subscribe`   | Off: `event <id>: missing on this game build`                |
| 3           | `Load::world`                   | Off: `world reads need a newer loader`                       |
| 4           | served resources                | Off: `served resources need a newer loader`                  |
| 4           | mod texts                       | Off: `mod texts need a newer loader`                         |
| 5           | `mods()`                        | An empty list                                                |
| 6           | join values                     | None: the mod runs with its file's values                    |
| 7           | `Load::code`, `Load::find_code` | Off: `code search needs a newer loader`                      |
| 7           | `Load::patch_code`              | Off: `code patches need a newer loader`                      |
| 8           | `Host::hosting`                 | Whether this is the dedicated server                         |
| 9           | chat commands                   | Off: `chat commands need a newer loader`                     |
| 9           | rename filters                  | Off: `rename filters need a newer loader`                    |
| 10          | `mods::join_missing()`          | An empty list                                                |
| 11          | `Load::hook_at`                 | Off: `mid-function hooks need a newer loader (ABI minor 11)` |
| 12          | `Load::inventories`             | Off: `inventories need a newer loader (ABI minor 12)`        |
| 13          | status lines                    | Off: `status lines need a newer loader`                      |
| 13          | `commands::run`                 | `Err(RunError::NoLoader)`                                    |
| 14          | `ModEntry::signed_by`           | Empty                                                        |
| 15          | altars and zone checks          | Off: `altars need a newer loader`                            |

A mod that can do without a feature catches the `Off` and goes on without it
instead of returning it from `init`.
