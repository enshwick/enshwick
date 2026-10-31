# The loader and the ABI

## Two DLLs

The loader comes in two parts:

- `powrprof.dll`, the stub, in the folder of `enshrouded.exe` or
  `enshrouded_server.exe`. Windows loads it with the game. It forwards every
  export to the system's `powrprof.dll`, then, on a thread of its own, loads
  the DLLs in `mods/chain/` and then the loader. Without the loader the game
  runs vanilla.
- `mods/enshwick/enshwick.dll`, the loader. It identifies the game build,
  loads the mods, initialises them, arms their hooks and answers their calls.

On a game build the loader has no table for, it finds what it needs by code
patterns; a mod whose entries are not found stays off with the reason, and
`build_patterns = false` keeps such a build vanilla instead; see
[Finding the game](../finding-the-game.md).

## How mods are found

Every `*.dll` directly in `mods/` is a candidate, taken in file name order. The
loader loads each and keeps it when it exports the four required functions of
[`abi::exports`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/abi.rs)
and was built for the loader's ABI version. The fifth, `enshwick_mod_shutdown`,
is optional. A DLL without `enshwick_mod_abi` is a native mod: it stays loaded
and runs on its own, and the status summary and the Mods screen list it. One
with only part of the interface is logged and unloaded. The file name is free; the id in the manifest
names the mod. Every folder in `mods/` with a `pack.toml` is a
[data pack](../content/packs.md), which the loader runs as a mod of its
own; its entry's file is `<id>/pack.toml`.

## The mod list and the lock

`mods/enshwick/mods.toml` names every mod the loader has found, by id, with
its file. The loader writes it on first start with every mod turned on, adds
mods it finds later, and keeps everything else, comments included:

```toml
[you.trading-post]
enabled = false # off until the next release
file = "trading_post.dll"
```

A file whose entry says `enabled = false` is never loaded, so none of its code
runs; `loader.log` says `trading_post.dll skipped: off in
mods/enshwick/mods.toml`. `links_off = ["you.trading-post:price"]` in an entry keeps the
mod on and only its link on that [extension point](../extending.md) off. Changes take effect at the next start. A list that
is not valid TOML is left as it is and every mod loads, with a warning.
`enshwick` on the [console](../chat.md#the-console) and the status summary name the mods that are off.

`mods/enshwick/enshwick.lock` is written at every start: the loader's version
and SHA-256, the game build, and each running mod's id, version, file and
SHA-256, in load order, then under `[[replaced]]` the mods a present mod
replaces (see `replaces` in [the manifest](#the-manifest)). Copying it with the DLLs sets up the same server
again, and comparing two locks shows where two servers differ. What changed
since the last start goes to `loader.log`, such as
`since the last start: you.trading-post 0.3.0 -> 0.3.1`.

## The ABI

A mod and the loader are built separately, so they talk through a C ABI,
[`enshwick::abi`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/abi.rs):

- `ABI_VERSION` is a hash of the layout of every ABI type, with each
  field's type and each function pointer's signature. A loader loads
  only mods built for its own version and tells the player which Enshwick
  release to rebuild any other mod for. Structs that cross the boundary on
  their own start with a `Header` (size and ABI version).
- `HostApi` is the loader's table of functions, passed to the mod at init.

A mod rarely touches the ABI. [`Load`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/load.rs)
wraps the table during init, and
[`host()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/host.rs)
returns a `Host` afterwards, from any thread, for logging, `fault`,
`hosting()` and the raw table. Each mod DLL links its own copy of the crate,
so `host()` always answers for the mod that calls it. The
[ABI reference](../abi-reference.md) has the details.

## The manifest

The manifest is the static data the loader reads before it runs any of the
mod's code. A mod names itself once, in its crate's `Cargo.toml`, and
[`declare_mod!`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/lib.rs)
builds the manifest from it when the mod compiles. Only `id` and `side` are
required:

```toml
[package]
name = "trading-post"
version = "0.2.0"
description = "Trade with travelling merchants"
authors = ["Your Name"]
license = "MIT OR Apache-2.0"
homepage = "https://github.com/you/trading-post"

[package.metadata.enshwick]
id = "you.trading-post"            # <author>.<name>, each of [a-z0-9-]
side = "both"                      # "server", "client" or "both"
name = "Trading post"              # what players read; the id when left out
join = true                        # counts in the join check
reload = true                      # a development server may reload it
exports = ["price-of", "sold"]     # what other mods may use
replaces = ["barter"]              # a mod this one folds in

[package.metadata.enshwick.requires]
stalls = { id = "you.market-stalls", version = ">=0.2, <1.0" }

[package.metadata.enshwick.after]
stalls_order = "you.market-stalls"

[package.metadata.enshwick.recommends]
caravans = { id = "you.caravans", version = ">=0.3, <1.0" }

[package.metadata.enshwick.breaks]
old_market = { id = "old-market", version = "<0.5" }

[[package.metadata.enshwick.builds]]
side = "server"
guid = "1c129660cd6efc4f9f0b21652d5ff2bf"
age = 387

[package.metadata.enshwick.builds.symbols]
"survivors.spawn_return" = 0x4_05cb

[package.metadata.enshwick.builds.constants]
"survivors.bed_id" = 0x48
```

A key the table does not take, a side, an id or an alias that is wrong fails
the build with its name. A changed `Cargo.toml` rebuilds the mod.

- `id` is `<author>.<name>`, each part of `[a-z0-9-]`, at most 64 characters
  in all. It names the mod in logs, its settings file `mod-configs/<id>.toml`
  and its data folder `mod-data/<id>`, and is the namespace of its content,
  stats, points, services and events (`you.trading-post:prices`). An invalid
  id fails the build; `enshwick` is the loader's own and is refused.
- `side`: see [Sides and hosting](sides-and-hosting.md).
- The version is the crate's, SemVer; the loader names it in every message
  about the mod.
- `name`, and the crate's `description`, `authors`, `license` and `homepage`
  (else `repository`), are what players read: on the Mods screen, in the
  status summary and in the join check's notes, where the homepage is the
  place a player who lacks the mod is sent to. A mod without a name is shown
  by its id. The mod's texts translate its name and description under
  [`mod.name` and `mod.description`](../content/texts.md#the-mods-own-name).
- `join = true`: see [The join check](join-check.md).
- `reload = true`: a development server may reload the mod while it runs; see
  [Testing](../testing.md#reloading-a-mod-while-the-game-runs).
- `developer = true`: the mod is a developer tool, such as a probe that logs
  what the game does. While it runs, every player reads a warning that
  developer tools are on; see [The interface](../interface.md).
- `exports` lists the local names of what other mods may use; see
  [What other mods may use](#what-other-mods-may-use).
- `requires`, `after`, `recommends` and `breaks` are tables of other mods,
  each under an alias of the mod's choosing (a Rust name in snake case,
  unique across the four), as a mod id alone or `{ id, version }` with a
  Cargo-style version range; no range is any version.
- `requires` names mods that must be present in the range; they load first.
  `after` names mods that load first when present, and takes no range.
- `recommends` names mods that work well beside this one: they load first when
  present, and the loader notes in `loader.log` and the status summary when
  one is missing or outside its range. The mod loads either way.
- `breaks` names mods this one does not work beside, in the range: when one
  of them loads, this mod is skipped with
  `does not work beside <id> <version>; remove one of them`.
- A dev build, version `0.0.0-dev`, meets every range of `requires` and
  `recommends`, and no range of `breaks` names it, as two dev builds join each
  other.
- `replaces` names mods this one takes the place of, such as mods it folds
  in. Players rarely delete the old DLL, so without it both would run. While
  this mod is present, the loader never starts a replaced mod: it unloads
  that DLL right after reading its manifest and logs once
  `<old> is replaced by <new>; delete mods/<file>`, which `enshwick` on the
  console and the status summary repeat and the lock lists under
  `[[replaced]]`. Their settings stay where they are; take the ones this mod
  keeps with a [migration](../migrations.md#settings-from-another-mod).
- `builds` pins symbols and constants for game builds the loader's table does
  not cover, per side, by the build's GUID and age; see
  [Finding the game](../finding-the-game.md).

The aliases name the dependencies in code too, so a mod writes another mod's
id once. `enshwick::deps!()` writes a module `deps` with a module per alias of
`requires`, `after` and `recommends`. Each holds `ID`, the mod's id, and
`id("<name>")`, its content id `<id>:<name>`. A required mod's ids are
constants, since the loader starts this mod only beside it. An optional mod's
ids, from `after` or `recommends`, are `None` while that mod does not run.
`deps::vanilla` names the game's own content by the debug names its data
gives it, such as `deps::vanilla::item("Prop_Production_T1_Forge")`; no
dependency takes the alias `vanilla`.

A call, point, service or event takes a local name: `name = "price"` is this
mod's `<id>:price`, and `of = <alias>` puts the dependency's id in front. An
alias the table lacks fails the build and lists the aliases it has. A name
with a `:` stays as written, for tests and tools. With `lockers` required and
`lanterns` recommended:

```toml
[package.metadata.enshwick.requires]
lockers = { id = "you.lockers", version = ">=0.1, <1.0" }

[package.metadata.enshwick.recommends]
lanterns = "you.lanterns"
```

```rust
use enshwick::content::Id;
use enshwick::services::ModEvent;

enshwick::deps!();

const LOCKER: Id = deps::lockers::id("locker");
assert_eq!(LOCKER.to_string(), "you.lockers:locker");

if let Some(lantern) = deps::lanterns::id("lantern") {
    enshwick::info!("lanterns run: {lantern} lights the lockers");
}

/// A locker opened, an event of `lockers`.
#[enshwick::event(of = lockers, name = "opened")]
#[derive(Clone, Copy, Debug)]
pub struct Opened {
    /// The locker.
    pub locker: u32,
}

assert_eq!(Opened::NAME, "you.lockers:opened");
```

### What other mods may use

A mod's calls, points, services and events are its own until it exports
them. `exports` lists their local names, and the loader refuses another mod a
reference to any other name of the mod. With `exports = ["price-of", "sold"]`
in `you.trading-post`, another mod may call `you.trading-post:price-of` and
hear `you.trading-post:sold`. A link to the point `you.trading-post:margin`
is refused, and the log says
`point you.trading-post:margin: you.trading-post does not export margin; it exports price-of, sold`.

- `caller`, `extend_point`, `observe_point` and `on_mod_event` return `Off`,
  which turns the asking mod off at init.
- `service` returns `Off` with the reason, and `optional_service` returns
  `None`.
- The mod's own names need no export.
- A mod that is not loaded is not checked; its names stay unmatched.
- Content ids are not checked: an item or recipe of another mod is named by
  its GUID inside the game's data, where the loader does not see whose it is.

A [data pack or Luau mod](../content/packs.md) lists its exports the same way,
as `exports` in its `pack.toml`.

### Schemas for other mods

What the mod defines in Rust goes to `declare_mod!` after its type:
`points: [..]` lists the [extension points](../extending.md#defining-a-point)
the mod defines, by their input types, and `events: [..]` and `calls: [..]`
its [events](../services.md) and [calls](../calls.md). Their schemas go into
the manifest and into the DLL's `.enshpt` section, where `cargo enshwick
point`, `event` and `call` read them to write other mods' definitions. A
listed point, event or call is for other mods, so a name missing from
`exports` fails the build. With `exports = ["price"]` in the mod's
`Cargo.toml`:

```rust,standalone_crate
#[enshwick::point(name = "price", mode = Override, output = u32)]
#[derive(Clone, Copy, Debug)]
pub struct Price {
    /// The item.
    pub item: u32,
}

struct Hello;

impl enshwick::Mod for Hello {
    fn init(_load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
        Ok(Self)
    }
}

enshwick::declare_mod!(Hello, points: [Price]);
```

## The mod

A mod implements
[`Mod`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/load.rs) on its
state type:

- `init(load)` registers hooks, settings, events and content, and returns the
  state or an [`Off`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/load.rs);
- `ready(&'static self, host)`, optional, runs once the hooks are armed;
- `shutdown(&'static self)`, optional, runs before a development reload
  unloads a mod with `reload = true`.

`Off::new(reason)` turns the mod off with a reason for the log. Every fallible
call on `Load` returns `Result<_, Off>`, so `?` is the usual way out.

`declare_mod!` exports the type with the manifest: `enshwick_mod_abi`,
`enshwick_mod_query`, `enshwick_mod_init`, `enshwick_mod_ready` and
`enshwick_mod_shutdown`, each catching panics. It also
adds `instance()`, the mod's state once `init` has returned it.

```rust,standalone_crate
struct Quiet;

impl enshwick::Mod for Quiet {
    fn init(_load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
        Err(enshwick::Off::new("nothing to do on this build"))
    }
}

enshwick::declare_mod!(Quiet);
# assert!(Quiet::instance().is_none());
```
