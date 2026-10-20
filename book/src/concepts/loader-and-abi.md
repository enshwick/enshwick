# The loader and the ABI

## Two DLLs

The loader comes in two parts:

- `powrprof.dll`, the stub, in the folder of `enshrouded.exe` or
  `enshrouded_server.exe`. Windows loads it with the game. It forwards every
  export to the system's `powrprof.dll`, then, on a thread of its own, moves
  staged updates from `mods/.staged/` into place and loads the loader. Without
  the loader the game runs vanilla.
- `mods/enshwick/enshwick.dll`, the loader. It identifies the game build,
  loads the mods, initialises them, arms their hooks and answers their calls.

On a game build the loader does not know, it loads no mod and the game runs
vanilla; see [Finding the game](../finding-the-game.md).

## How mods are found

Every `*.dll` directly in `mods/` is a candidate, taken in file name order. The
loader loads each and keeps it when it exports the four functions of
[`abi::exports`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/abi.rs)
and was built for the loader's ABI major. Other DLLs are logged as "not an
Enshwick mod" and unloaded. The file name is free; the id in the manifest
names the mod.

## The ABI

A mod and the loader are built separately, so they talk through a C ABI,
[`enshwick_loader_api::abi`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/abi.rs):

- `ABI_MAJOR` changes only with an incompatible change. A loader refuses a mod
  built for another major.
- `ABI_MINOR` grows when fields are appended. Structs that either side may
  outgrow start with a `Header` (size, major, minor), so an older reader
  ignores what it does not know.
- `HostApi` is the loader's table of functions, passed to the mod at init.
  The safe layer checks `Header::covers` before it calls a field newer than
  the loader, and falls back or reports `Off` where the loader is too old.

A mod rarely touches the ABI. [`Load`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/load.rs)
wraps the table during init, and
[`host()`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/host.rs)
returns a `Host` afterwards, from any thread, for logging, `fault`,
`hosting()` and the raw table. Each mod DLL links its own copy of the crate,
so `host()` always answers for the mod that calls it. The
[ABI reference](../abi-reference.md) has the details.

## The manifest

[`manifest!`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/declare.rs)
builds the static data the loader reads before it runs any of the mod's code.
Fields go in this order; only `id` and `side` are required:

```rust
let manifest = enshwick_loader_api::manifest!(
    id: "survivors-per-altar",      // 1 to 64 of [a-z0-9-]
    side: BOTH,                     // SERVER, CLIENT or BOTH
    version: "0.2.0",               // the crate's version when left out
    join = true,                    // counts in the join check
    requires: ["flame-altar-zones" => ">=0.1, <1.0"],
    after: ["flame-altar-count"],
    updates: github("themaiby/enshwick", "enshwick-survivors-per-altar"),
    signed_by: ["RWTjqgtfpy3lSOLC4uaUXuvvKSLQKOFf6nUnEytoiFjSzxMEhLnoAdJF"],
    builds: [{
        side: SERVER,
        guid: "1c129660cd6efc4f9f0b21652d5ff2bf",
        age: 387,
        symbols: ["survivors.spawn_return" => 0x4_05cb],
        constants: ["survivors.bed_id" => 0x48],
    }],
);
# assert_eq!(manifest.dependency_count, 2);
```

- `id` names the mod in logs, its settings file `mod-configs/<id>.toml` and
  its data folder `mod-data/<id>`. An invalid id fails the build; `enshwick`
  is the loader's own and is refused.
- `side`: see [Sides and hosting](sides-and-hosting.md).
- `version` is SemVer. Leave it out so the DLL reports the version you
  release; the loader names it in every message about the mod.
- `join = true`: see [The join check](join-check.md).
- `requires` names mods that must be present in a Cargo-style version range;
  they load first. `after` names mods that load first when present.
- `updates` says where new releases come from, `github(...)` or
  `url("https://...")`; see [Releasing and updates](../releasing.md).
- `signed_by` names the minisign public keys (one, or two while you change
  keys) your releases are signed with; an updater installs only releases one
  of them signed. See [Author keys](../releasing.md#author-keys).
- `builds` pins symbols and constants for game builds the loader's table does
  not cover, per side, by the build's GUID and age; see
  [Finding the game](../finding-the-game.md).

## The mod

A mod implements
[`Mod`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/load.rs) on its
state type:

- `manifest()` returns the `manifest!`;
- `init(load)` registers hooks, settings, events and content, and returns the
  state or an [`Off`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/load.rs);
- `ready(&'static self, host)`, optional, runs once the hooks are armed.

`Off::new(reason)` turns the mod off with a reason for the log. Every fallible
call on `Load` returns `Result<_, Off>`, so `?` is the usual way out.

[`declare_mod!`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/declare.rs)
exports the type: `enshwick_mod_abi`, `enshwick_mod_query`,
`enshwick_mod_init` and `enshwick_mod_ready`, each catching panics. It also
adds `instance()`, the mod's state once `init` has returned it.

```rust
struct Quiet;

impl enshwick_loader_api::Mod for Quiet {
    fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
        enshwick_loader_api::manifest!(id: "quiet", side: SERVER)
    }

    fn init(_load: &mut enshwick_loader_api::Load<'_>) -> Result<Self, enshwick_loader_api::Off> {
        Err(enshwick_loader_api::Off::new("nothing to do on this build"))
    }
}

enshwick_loader_api::declare_mod!(Quiet);
# assert!(Quiet::instance().is_none());
```
