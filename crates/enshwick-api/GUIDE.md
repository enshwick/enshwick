# Writing a mod

An Enshwick mod is a Rust `cdylib` built against [`enshwick-api`](.). Players
drop its DLL into the game's `mods/` folder; the loader finds it, reads its
manifest, initialises it, links its hooks into the game and keeps the game
running if the mod fails.

## A first mod

[`cargo enshwick new <id>`](../cargo-enshwick) writes this crate for you, and
`cargo enshwick install --game <game folder>` builds it and copies it into the
game. By hand:

```toml
# Cargo.toml
[package]
name = "my-mod"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
enshwick-api = "0.1"
serde = { version = "1", features = ["derive"] }
```

```rust
// src/lib.rs
use enshwick_api::{Load, Mod, Next, Off};

const DEFAULTS: &str = "\
# Altars allowed at every Flame level.
altars = 16
";

#[derive(serde::Deserialize)]
struct Settings {
    altars: u8,
}

struct MyMod {
    altars: u8,
}

enshwick_api::detour! {
    static MAX_ALTARS: unsafe extern "system" fn(balancing: u64, level: u32) -> u8 = max_altars;
}

fn max_altars(next: Next<unsafe extern "system" fn(u64, u32) -> u8>, balancing: u64, level: u32) -> u8 {
    match MyMod::instance() {
        Some(state) => state.altars,
        // SAFETY: the original takes the arguments the game passed.
        None => unsafe { next.get()(balancing, level) },
    }
}

impl Mod for MyMod {
    fn manifest() -> &'static enshwick_api::abi::Manifest {
        enshwick_api::manifest!(id: "my-mod", side: BOTH, join = true)
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = load.settings(DEFAULTS)?;
        load.hook(&MAX_ALTARS, load.symbol("building.max_altars")?)?;
        Ok(Self { altars: settings.altars })
    }
}

enshwick_api::declare_mod!(MyMod);
```

Build it with `cargo build --release` for `x86_64-pc-windows-msvc`, copy the DLL
into `mods/`, start the game and read `logs/enshwick/` (`all.log`, `loader.log`,
`mods/<id>.log`).
[`enshwick-flame-altar-count`](../../mods/enshwick-flame-altar-count) is this
mod in full.

## The life of a mod

1. **Query.** The loader calls `enshwick_mod_query` and reads the manifest
   before running any of the mod's code: id, version, sides, dependencies, and
   the symbols the mod pins itself.
2. **Resolve.** Mods for the other side, duplicates, mods whose requirements are
   missing or too old, and the later of two conflicting mods are skipped with a
   reason in the log. The rest load in dependency order. The client loads
   `SERVER` mods too: see [Hosting](#hosting).
3. **Init.** `Mod::init` runs once. It links hooks, reads settings, finds game
   functions and services. Nothing takes effect yet; returning `Err(Off)` turns
   the mod off and leaves the game vanilla.
4. **Arm.** Once every mod is initialised, the loader detours each hooked
   function once and wires the chains.
5. **Ready.** `Mod::ready` runs; start background work here.

## The manifest

```rust
enshwick_api::manifest!(
    id: "survivors-per-altar",      // [a-z0-9-]+; names the settings file
    side: BOTH,                     // SERVER, CLIENT or BOTH
    version: "0.2.0",               // optional; the crate's version by default
    join = true,                    // server and players must match
    requires: ["flame-altar-zones" => "^0.1"],
    after: ["flame-altar-count"],   // order only, may be absent
    updates: github("themaiby/enshwick", "enshwick-survivors-per-altar"),
    builds: [{
        side: SERVER,
        guid: "1c129660cd6efc4f9f0b21652d5ff2bf",
        age: 387,
        symbols: ["survivors.spawn_return" => 0x4_05cb],
        constants: ["survivors.bed_id" => 0x48],
    }],
)
```

The version is SemVer, and the loader names it wherever the mod comes up: a
dependency out of range reads `requires flame-altar-zones ^0.2, found 0.1.3`,
and a player joining a server with other versions sees each mod with the
server's version and theirs. Leave `version` out to use the crate's, so the
version you release is the one the DLL reports; `cargo enshwick manifest <dll>`
shows what a built DLL says about itself.

`join = true` puts the mod in the mod set the server advertises; a player whose
set differs is warned before joining. Leave it off for mods that only change
what one side sees or decides, such as a server-only balance change.

## Settings

The easiest way: derive `enshwick_api::Config` on a settings type that also
derives serde's `Deserialize` and `Serialize` and implements `Default`, and call
`load.config::<Settings>()`. The file a player sees is written from the type:
the struct's doc comment heads it, each field's doc comment precedes its key,
and values come from `Default`. `#[config(validate = Settings::check)]` names a
`fn(&Self) -> Result<(), String>` that refuses values.

`load.on_config_change(|settings: Settings| ...)` applies an edited file while
the game runs: about a second after the file changes, the handler gets the new
values on a loader thread. A file that no longer parses or validates is logged
and skipped, and the running values stay. Keep what a hook reads in atomics or
an `ArcSwap`, so the handler can replace it under a running game.

Some values must be equal on the server and its players, such as a table the
client's interface also reads. Mark such a field `#[config(sync)]`: the server
advertises the join values of each join mod in its Steam rules
(`enshwick.sync.<id>`), and they count in the mod set's fingerprint. A client
that joins calls the mod's `on_config_change` handler with its settings and the
server's join values over them; its file stays as it is. Joining a server
without them calls the handler with the file's values again. Apply join values
in `on_config_change`: a mod without a handler runs with its file's values
until the next start. A server's edited join values reach players when they
next join.

`Load::settings(DEFAULTS)` returns the mod's `mod-configs/<id>.toml`, written
from `DEFAULTS` on first start, parsed with serde into your type. Write
`DEFAULTS` as the commented file a player should see, and use
`#[serde(deny_unknown_fields)]` so a typo turns the mod off with a message
instead of being ignored.

## Events

`load.on::<events::Kind>(|data| ...)` runs a handler each time the loader
raises the event, on the thread that raises it, until the mod is off; a panic in
the handler turns the mod off. Subscribe during `init`; the loader hooks the game
for an event only when some mod subscribed to it.

| Event | Data | When |
| --- | --- | --- |
| `Tick` | `TickData` (`step`) | after each simulation step on the server's game thread: about 20 a second with nobody online, 60 with players |
| `WorldLoaded` | none | the world is loaded and the server runs |
| `WorldSaved` | none | a save finished writing to disk |
| `PlayerJoined` | `PlayerData` (`slot`, `entity`, `key`) | a player's character entered the world |
| `PlayerLeft` | `PlayerData` | a player who joined is leaving; the character is still there |
| `Shutdown` | none | an orderly stop begins, with the world and players alive; the final save follows |
| `BuildingPlaced` | `BuildingData` (`entity`, `player`, `item`, `template`, `position`) | a player placed a piece or prop |
| `BuildingRemoved` | `BuildingData` (`entity`, `player`, `kind`) | a player picked up or tore down a piece |
| `SettingsChanged` | none | the mod's settings file changed; `on_config_change` wraps it |
| `LoaderSettingsChanged` | none | `mod-configs/enshwick.toml` changed |

Game events come from the server only, and from its game thread: keep handlers
short. On a client they come from the built-in server while it hosts a game. `key` is the same for the same player account in every session.
`load.subscribe(Event, |raw| ...)` is the untyped form.

## The world

`load.world()?` reads the server's world from its game thread (a game event
or a scheduled task; elsewhere the reads return `None`), on a client the
built-in server's while it hosts:

- `position(entity)` in metres, `template(entity)` as a GUID, and
  `used_item(entity)`, the item a placed prop came from, which it keeps across
  saves;
- `component_type(load, "keen::ecs::Name")` at init, then `component(entity, ty)`
  for a read-only pointer to any reflected component;
- `player(entity)` and `players()`: slot, a per-account `key`, permission bits
  (`is_admin()`) and the name the client sent, which is untrusted.
- `set_label(entity, text)`, `broadcast(text)` and `tell(&player, text)`: see
  [Labels](#labels) and [Chat commands](#chat-commands).

## Chat commands

`load.command(name, help, handler)` registers `/name` on the server. When a
player types `/name words...`, the line never reaches anyone's chat: the
loader runs `handler(ctx, args)` on the game thread, where world reads work,
with the player who typed it (`ctx.player`, `ctx.is_admin()`) and the words
after the name. The `Reply` it returns goes to that player alone, a chat line
per text line; `Reply::none()` sends nothing.

```rust
use enshwick_api::commands::Reply;

load.command("safe", "allow, deny or list players at your nearest Safe", move |ctx, args| {
    match args {
        ["list"] => Reply::from(format!("{} asked for the list", ctx.player.name)),
        _ => Reply::from("/safe allow <player>, /safe deny <player>, /safe list"),
    }
})?;
```

A name is 1 to 32 of `[a-z0-9-]` and belongs to one mod: a name another mod
holds, or `enshwick`, is refused at init. The loader answers `/enshwick`
itself with the mods and versions running, and `/enshwick help` with every
command and its help line; an unknown `/` command gets a short answer.
Commands, labels and rename filters work on the dedicated server and on a
client while it hosts a game.

`world.broadcast(text)` sends a line to everyone and `world.tell(&player, text)`
to one player, from any thread; the server posts them after the next step.

## Labels

`world.set_label(entity, text)` names an entity as a player's rename does: the
game shows the name on hover, sends it to every client, vanilla ones included,
and saves it with the world. It keeps 255 bytes; an empty text removes the
name. Call it on the game thread.

The game checks no permission when a player renames something.
`load.on_rename(|rename| ...)` runs before every rename on the server, with the
entity, the player (`None` when the server cannot tell) and the new text;
returning `false` refuses it, and the entity keeps its name.

```rust
load.on_rename(move |rename| {
    rename.player.as_ref().is_some_and(|player| player.is_admin())
})?;
```

## Scheduling

`load.scheduler()?` returns a `Copy` handle that puts work on the server's
game thread after a simulation step, on a client while it hosts: `next_tick(f)`, `after(delay, f)` and
`every(period, f)`, where `f` returns `ControlFlow::Continue(())` to keep
repeating. Call it from any thread; tasks run where the game's server code runs,
so they may call into the game. An idle step costs one atomic load.

## Hosting

A client that hosts a game, solo play included, runs the whole server on a
thread of its own, `GameServer`, beside its own world. Every mod must work
there as on a dedicated server, so the client loads `SERVER` mods as well,
and the server's features act whenever the client hosts:

- game events, the world reads and the scheduler serve the built-in server
  from its first step until the game stops. Each hosted game starts with
  `WorldLoaded` and ends with `Shutdown`; reset state tied to one game, such
  as entity ids, on `Shutdown` (the next game has a new server and new
  entities);
- `host.hosting()` tells whether this process runs a server now: always on
  the dedicated server, on the client while it hosts;
- server systems run only in the server's world, but some functions serve
  both worlds of the host, such as cursor checks and base stock. A hook on
  one of them acts for the server only: `ecs::Worlds::resolve(load)` gives
  `is_server(ctx)`, true for a system context of the server's world (always
  on the dedicated server). Entity ids of the two worlds are unrelated;
- a mod pins its server symbols for the client too, in a `CLIENT` build row:
  the client binary holds twins of the server's functions. When a mod's
  server and client halves hook one function, it links one detour there that
  answers for both.

## Content: items, recipes, workshops and texts

New game content works on the server and the client alike; the mod runs on
both (`side: BOTH`, `join = true`).

```rust
use enshwick_api::content::{Guid, items::NewItem, recipes::NewRecipe};

const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
const SAFE: Guid = Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31");

let texts = load.texts(include_str!("texts.toml"))?;
load.add_texts(&texts, &[("safe.name", SAFE_NAME)])?;
load.add_item(NewItem { guid: SAFE, like: CHEST, name: Some(SAFE_NAME), description: None, debug_name: "Safe".into() })?;
load.add_recipe(NewRecipe { guid: SAFE_RECIPE, like: CHEST, output: SAFE, output_count: 1, cost: 2, workshop: None })?;
```

- `add_item` clones a vanilla item under a new GUID (its game id is
  `Guid::id`) and lists it in the item registry; `add_recipe` copies the
  recipe of a vanilla item for a new output, at the same workshop and unlocked
  with it. `derive_recipes(salt, |recipe| ...)` looks at every vanilla
  recipe (debug name, output) and adds a scaled copy of the ones it picks.
  Several mods can add items and recipes; their changes compose. The game
  takes at most 512 recipes per workshop: derived copies stop 16 short of it,
  leaving room for single recipes, and the log names what was left out. A mod
  that adds many recipes should give them a workshop or an NPC of its own
  rather than crowd the vanilla ones, which other mods share.
- `add_workshop(NewWorkshop { like: stations::WORKBENCH, item, template,
  workshop, name, description, station_recipe, debug_name })` clones a vanilla
  station: its template, crafting at a new workshop, its item, placing the new
  template, the workshop registry entry and, with `station_recipe`, a copy of
  the vanilla station's recipe. It returns the `WorkshopId`; recipes go there
  with `NewRecipe { workshop: Some(id), .. }` or `Derived { workshop: Some(id),
  .. }`, and a new workshop starts with none of the 512 taken. Keep every GUID
  fixed for good: saves hold them.

  ```rust
  use enshwick_api::content::workshops::{NewWorkshop, StationRecipe, stations};

  let bench = load.add_workshop(NewWorkshop {
      like: stations::WORKBENCH,
      item: BENCH, template: BENCH_TEMPLATE, workshop: BENCH_WORKSHOP,
      name: Some(BENCH_NAME), description: Some(BENCH_DESCRIPTION),
      station_recipe: Some(StationRecipe { guid: BENCH_RECIPE, cost: 1 }),
      debug_name: "Craftingstation_BulkWorkbench".into(),
  })?;
  load.derive_recipes("my-mod", move |recipe| {
      recipe.debug_name().starts_with("Block_")
          .then_some(Derived { output: 2, cost: 1, workshop: Some(bench) })
  })?;
  ```

- Texts live in one TOML file per mod, a table per language code (`en`, `uk`,
  `de`, ... see `i18n::Language`), `en` being the fallback. A server owner
  overrides or adds texts in `mod-configs/<id>.texts.toml`.
- Lower down, `load.serve(key, base, |bytes| ...)` serves any game resource,
  new or changed, built from its base with `content::Blob`, which grows arrays
  and keeps every offset right; `load.add_text(tag, language, text)` gives one
  tag one text.

Items a mod adds disappear from inventories when the mod is removed, as the
game drops unknown items; a placed object keeps working as its vanilla
template. The loader records every template and item a mod clones, with its
vanilla source, in `mod-data/enshwick/clones.toml`; when no mod serves a cloned
template any more, the game reads the vanilla template under the clone's GUID,
so a placed station of a removed mod loads as the vanilla station it was cloned
from.

## State between sessions

`load.data_dir()` is the mod's own folder, `mod-data/<id>`. `state::Store<T>`
keeps a serde value there as TOML, written to a temporary file and renamed, so a
crash never leaves half a file; `load()` returns `T::default()` until the first
`save`.

## Finding the game

Addresses change with every game update, so a mod never hard-codes one.

| Call                                                                           | Finds                                                                                  | Survives updates |
| ------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------- | ---------------- |
| `load.system("upgrade_building_range")`                                        | an ECS system's update function, by its reflected name                                 | yes              |
| `load.field_offset("keen::ecs::TriggerShape", "rangeX")`, `load.type_size(..)` | reflected types and fields                                                             | yes              |
| `load.symbol("npc.lookup")`                                                    | an engine function pinned per build: the mod's own rows first, then the loader's table | needs a new row  |
| `load.constant("ecs.context_row")`                                             | an engine offset or size pinned per build                                              | needs a new row  |

On a game build the loader does not know, nothing loads and the game runs
vanilla. A symbol or constant missing for the running build turns off only the
mod that asks for it.

## Hooks

`detour!` (or `#[hook]`, below) declares a link: a static `Hook` and an entry
point that catches panics. `load.hook(&HOOK, address)` puts it into the chain of that function.

- The body receives a `Next` and the function's arguments. Call
  `next.get()(args)` to continue to the next link or the original; skip it to
  replace the function. Calling through `Next` costs one atomic load and one
  indirect call.
- `priority = <i32>` orders links on one function, lower first; equal priorities
  run in load order.
- `flags = HookFlags::EXCLUSIVE` claims a function no other mod may link, for a
  link that replaces it.
- `flags = HookFlags::CALLER_FRAME` asks the loader to record the game caller's
  stack pointer and callee-saved registers. `host().caller_frame()` returns
  them; `return_address()` names the call site, so one hook can treat callers
  differently.
- A panic in a body is caught: the mod reports a fault, the loader unlinks every
  link of that mod by rewiring slots, and the call continues to the rest of the
  chain. The game never crashes for it.

`#[hook]` writes the same declaration from the body, naming the static after the
function in upper case:

```rust
use enshwick_api::{Next, abi::HookFlags, hook};

#[hook(priority = 5, flags = HookFlags::EXCLUSIVE)]
fn upgrade(next: Next<unsafe extern "system" fn(*mut u64)>, ctx: *mut u64) {
    // SAFETY: the original takes the same context.
    unsafe { next.get()(ctx) }
}

// in init: load.hook(&UPGRADE, load.system("upgrade_building_range")?)?;
```

## Testing a mod

[`enshwick-test`](../enshwick-test) is a fake loader for unit tests. Configure
what the game would answer, start the mod through its exports, then look at
what it did:

```rust
let loader = enshwick_test::FakeHost::new()
    .symbol("building.max_altars", enshwick_test::address(vanilla as extern "system" fn(u64, u32) -> u8))
    .settings("altars = 16\n")
    .install();
// SAFETY: the exports of `declare_mod!`.
assert_eq!(unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) }, Status::OK);
let max_altars = loader.detour(&MAX_ALTARS).unwrap();
assert_eq!(max_altars(0, 3), 16);
```

A hook's chain continues to its target, so a target set to `address` of a plain
function stands in for the game's. `logs()`, `faults()`, `hooks()`, `texts()`
and `resources()` show what the mod asked for; `raise::<events::Tick>(&data)`
raises an event, and `set_settings` with `SettingsChanged` plays a player's
edit. A mod initialises once per process, so write one test per mod life and run
them with `cargo nextest`, which gives each test its own process.

## Working inside a system

`enshwick_api::ecs::Ecs::resolve(load)` gives access to the engine's ECS from a
hooked system: `query::<N>(ctx)` walks the system's rows with the engine's own
iterator on a stack buffer, `entity(ctx)` names the current row's entity,
`set_row` restarts a walk, and `mark_changed` makes replication send a written
component. Such hooks run on worker threads; `ecs::Components::resolve(load)`
reads any entity's component from the system's context
(`component(ctx, entity, hash)`, the hash from `World::component_type`) and
finds the inventory an item slot belongs to (`slot_root`).
`ecs::Attributes` reads and writes attribute sets such as `Level` or
`Experience` by attribute hash (`value`, `set`); mark the component changed
before writing it. On a hosting client, `ecs::Worlds` tells the server's
systems from the client's.

## Services between mods

A mod can publish a `repr(C)` table of `extern "system"` functions with
`load.publish(name, version, &TABLE)`; another mod finds it with
`load.service(name, version)` and declares the publisher in `requires`, so it
loads first. Put the table type in a small crate both mods depend on, as
`enshwick-geometry::service` does for altar zones.

## Updates

`updates:` in the manifest says where new releases of the mod come from:

- `github("owner/name", "component")`: GitHub releases of that repository
  whose tags are `<component>-v<semver>`, as release-please tags a workspace
  (`enshwick-safes-v0.2.0`);
- `url("https://example.com/my-mod/latest.json")`: a document on any other
  host that names the latest release.

The loader checks the source, ignores a malformed one with a warning, and
lists every loaded mod, itself first, with `enshwick_api::mods()`: id,
version, side, join flag, whether it runs, its DLL's path under the game's
folder and its source. An updater mod reads that list, fetches new releases
and writes the files to `mods/.staged/<path>`; on the next start, before the
loader runs, the `powrprof.dll` stub moves each to `<path>` and keeps the file
it replaces as `<path>.previous`. A mod without `updates:` is updated by hand.

```rust
for entry in enshwick_api::mods() {
    if let Some(enshwick_api::mods::Updates::GitHub { repository, component }) = &entry.updates {
        enshwick_api::info!("{} {}: {repository}, tags {component}-v*", entry.id, entry.version);
    }
}
```

A server advertises each join mod's source in the Steam rule
`enshwick.src.<id>`, written by `Updates::to_line` (`github:<owner/name>:<component>`).
When the join check keeps a client out because the server runs mods the
client lacks or has in another version, the loader raises
`events::JoinMissing` on the client, and `enshwick_api::mods::join_missing()`
lists those mods with the server's versions and sources. The handler runs on
the game thread, so it hands the work to a thread of its own.

[The updater](../../mods/enshwick-updater) is such a mod. It installs only
archives signed with minisign by a trusted key, with the release's tag and the
archive's name in the trusted comment; every release of this repository is
signed so (`<archive>.minisig`).

## Rules for code in the game's frame

- Hooks run on game threads, often many times per frame. In the common case they
  do not allocate, lock, format or log; state they read is atomic, thread-local
  or swapped whole (`arc-swap`); work that can wait goes to a background thread.
- The fast path comes first: decide "not ours" in a few instructions and forward
  to `Next`.
- Every `unsafe` block names its invariant in a `// SAFETY:` comment; raw reads
  of game memory go through one typed helper.
- Never free memory across the boundary with the loader; strings it passes are
  borrowed for the call.

## Logging

`enshwick_api::error!`, `warn!`, `info!` and `debug!` write to `logs/enshwick/`
(`all.log`, `loader.log`, `mods/<id>.log`) under the mod's id, formatted like
`format!`. Log milestones and failures, never per frame.

The player picks how much is kept in `mod-configs/enshwick.toml`:

```toml
log_level = "info"
log_levels = { flame-altar-zones = "debug" }
```

A line above the mod's level costs one atomic load: its arguments are neither
evaluated nor formatted. For lines that may repeat, `info_once!` and
`warn_once!` log the first time a call site runs, and `warn_every!` and
`error_every!` log at most once per period, noting how many they skipped:

```rust
enshwick_api::warn_every!(Duration::from_secs(60), "bed {bed} has no owner");
```

## Releasing

Name the crate `enshwick-<id>` if it lives in this repository, and follow the
repository's CONTRIBUTING.md. A player installs a mod by copying its DLL into
`mods/`; mods that count in the join check must match the server's.
