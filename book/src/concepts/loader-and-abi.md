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
and was built for the loader's ABI major. The fifth, `enshwick_mod_shutdown`,
is optional. Other DLLs are logged as "not an
Enshwick mod" and unloaded. The file name is free; the id in the manifest
names the mod. Every folder in `mods/` with a `pack.toml` is a
[content pack](../content/packs.md), which the loader runs as a mod of its
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

- `ABI_MAJOR` changes only with an incompatible change. A loader refuses a mod
  built for another major.
- `ABI_MINOR` grows when fields are appended. A loader refuses a mod built
  for a newer minor, so a mod always meets a loader that has every call it
  knows. Structs that either side may outgrow start with a `Header` (size,
  major, minor), and the loader reads and writes a mod of an older minor
  only as far as that mod knows each struct.
- `HostApi` is the loader's table of functions, passed to the mod at init.

A mod rarely touches the ABI. [`Load`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/load.rs)
wraps the table during init, and
[`host()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/host.rs)
returns a `Host` afterwards, from any thread, for logging, `fault`,
`hosting()` and the raw table. Each mod DLL links its own copy of the crate,
so `host()` always answers for the mod that calls it. The
[ABI reference](../abi-reference.md) has the details.

## The manifest

[`manifest!`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/declare.rs)
builds the static data the loader reads before it runs any of the mod's code.
Fields go in this order; only `id` and `side` are required:

```rust
let manifest = enshwick::manifest!(
    id: "you.trading-post",         // <author>.<name>, each of [a-z0-9-]
    side: BOTH,                     // SERVER, CLIENT or BOTH
    version: "0.2.0",               // the crate's version when left out
    name: "Trading post",           // what players read; the id when left out
    description: "Trade with travelling merchants",
    authors: ["Your Name"],
    license: "MIT OR Apache-2.0",
    homepage: "https://github.com/you/trading-post",
    join: true,                    // counts in the join check
    reload: true,                  // a development server may reload it
    requires: ["market-stalls" => ">=0.2, <1.0"],
    after: ["market-stalls"],
    recommends: ["caravans" => ">=0.3, <1.0"],
    breaks: ["old-market" => "<0.5"],
    replaces: ["barter"],           // a mod this one folds in
    was: ["trade-post"],            // ids this mod had before a rename
    builds: [{
        side: SERVER,
        guid: "1c129660cd6efc4f9f0b21652d5ff2bf",
        age: 387,
        symbols: ["survivors.spawn_return" => 0x4_05cb],
        constants: ["survivors.bed_id" => 0x48],
    }],
);
# assert_eq!(manifest.dependency_count, 6);
# assert_eq!(manifest.author_count, 1);
```

- `id` is `<author>.<name>`, each part of `[a-z0-9-]`, at most 64 characters
  in all. It names the mod in logs, its settings file `mod-configs/<id>.toml`
  and its data folder `mod-data/<id>`, and is the namespace of its content,
  stats, points, services and events (`you.trading-post:prices`). An invalid
  id fails the build; `enshwick` is the loader's own and is refused.
- `side`: see [Sides and hosting](sides-and-hosting.md).
- `version` is SemVer. Leave it out so the DLL reports the version you
  release; the loader names it in every message about the mod.
- `name`, `description`, `authors`, `license` and `homepage` are what players
  read: on the Mods screen, in the status summary and in the join check's
  notes, where `homepage` is the place a player who lacks the mod is sent to.
  Each but `name` comes from the crate's `Cargo.toml` when left out
  (`homepage` falls back to `repository`); a mod without a name is shown by
  its id.
- `join: true`: see [The join check](join-check.md).
- `reload: true`: a development server may reload the mod while it runs; see
  [Testing](../testing.md#reloading-a-mod-while-the-game-runs).
- `requires` names mods that must be present in a Cargo-style version range;
  they load first. `after` names mods that load first when present.
- `recommends` names mods that work well beside this one: they load first when
  present, and the loader notes in `loader.log` and the status summary when
  one is missing or outside its range. The mod loads either way.
- `breaks` names mods this one does not work beside, in a version range (`""`
  for any): when one of them loads, this mod is skipped with
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
- `was` names the ids this mod had before a rename. It replaces them as
  `replaces` does, a `requires` or an [extension point](../extending.md)
  named by an old id finds this mod, and on the first start the loader copies
  `mod-configs/<old>.toml`, its texts file and everything in `mod-data/<old>/`
  (each world's settings and stores, a hold) to the new id, where the new id
  has no such file yet, and counts the migrations that ran under the old id as
  run. The old files stay, for a player who goes back. `migrations.toml`
  records the copy under `[carried]`, so it happens once.
- `points` lists the [extension points](../extending.md#defining-a-point) the
  mod defines, by their input types. Their schemas go into the manifest and
  into the DLL's `.enshpt` section, where `cargo enshwick point` reads them
  to write other mods' definitions.
- `builds` pins symbols and constants for game builds the loader's table does
  not cover, per side, by the build's GUID and age; see
  [Finding the game](../finding-the-game.md).

## The mod

A mod implements
[`Mod`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/load.rs) on its
state type:

- `manifest()` returns the `manifest!`;
- `init(load)` registers hooks, settings, events and content, and returns the
  state or an [`Off`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/load.rs);
- `ready(&'static self, host)`, optional, runs once the hooks are armed;
- `shutdown(&'static self)`, optional, runs before a development reload
  unloads a mod with `reload: true`.

`Off::new(reason)` turns the mod off with a reason for the log. Every fallible
call on `Load` returns `Result<_, Off>`, so `?` is the usual way out.

[`declare_mod!`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/declare.rs)
exports the type: `enshwick_mod_abi`, `enshwick_mod_query`,
`enshwick_mod_init`, `enshwick_mod_ready` and `enshwick_mod_shutdown`, each
catching panics. It also
adds `instance()`, the mod's state once `init` has returned it.

```rust,standalone_crate
struct Quiet;

impl enshwick::Mod for Quiet {
    fn manifest() -> &'static enshwick::abi::Manifest {
        enshwick::manifest!(id: "you.quiet", side: SERVER)
    }

    fn init(_load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
        Err(enshwick::Off::new("nothing to do on this build"))
    }
}

enshwick::declare_mod!(Quiet);
# assert!(Quiet::instance().is_none());
```
