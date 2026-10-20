# Writing a mod

An Enshwick mod is a Rust `cdylib` built against [`enshwick-api`](.). Players
drop its DLL into the game's `mods/` folder; the loader finds it, reads its
manifest, initialises it, links its hooks into the game and keeps the game
running if the mod fails.

## A first mod

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
   reason in the log. The rest load in dependency order.
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
    join = true,                    // server and players must match
    requires: ["flame-altar-zones" => "^0.1"],
    after: ["flame-altar-count"],   // order only, may be absent
    builds: [{
        side: SERVER,
        guid: "1c129660cd6efc4f9f0b21652d5ff2bf",
        age: 387,
        symbols: ["survivors.spawn_return" => 0x4_05cb],
        constants: ["survivors.bed_id" => 0x48],
    }],
)
```

`join = true` puts the mod in the mod set the server advertises; a player whose
set differs is warned before joining. Leave it off for mods that only change
what one side sees or decides, such as a server-only balance change.

## Settings

The easiest way: derive `enshwick_api::Config` on a settings type that also
derives serde's `Deserialize` and `Serialize` and implements `Default`, and call
`load.config::<Settings>()`. The file a player sees is written from the type:
the struct's doc comment heads it, each field's doc comment precedes its key,
and values come from `Default`. Override `Config::validate` to refuse values.

`Load::settings(DEFAULTS)` returns the mod's `mod-configs/<id>.toml`, written
from `DEFAULTS` on first start, parsed with serde into your type. Write
`DEFAULTS` as the commented file a player should see, and use
`#[serde(deny_unknown_fields)]` so a typo turns the mod off with a message
instead of being ignored.

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

`detour!` declares a link: a static `Hook` and an entry point that catches
panics. `load.hook(&HOOK, address)` puts it into the chain of that function.

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

## Working inside a system

`enshwick_api::ecs::Ecs::resolve(load)` gives access to the engine's ECS from a
hooked system: `query::<N>(ctx)` walks the system's rows with the engine's own
iterator on a stack buffer, `entity(ctx)` names the current row's entity,
`set_row` restarts a walk, and `mark_changed` makes replication send a written
component.

## Services between mods

A mod can publish a `repr(C)` table of `extern "system"` functions with
`load.publish(name, version, &TABLE)`; another mod finds it with
`load.service(name, version)` and declares the publisher in `requires`, so it
loads first. Put the table type in a small crate both mods depend on, as
`enshwick-geometry::service` does for altar zones.

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

`enshwick_api::info!`, `warn!` and `error!` write to `logs/enshwick/`
(`all.log`, `loader.log`, `mods/<id>.log`) under the mod's id, formatted like
`format!`. Log milestones and failures, never per frame.

## Releasing

Name the crate `enshwick-<id>` if it lives in this repository, and follow the
repository's CONTRIBUTING.md. A player installs a mod by copying its DLL into
`mods/`; mods that count in the join check must match the server's.
