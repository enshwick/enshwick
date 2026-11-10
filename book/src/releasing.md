<!--
type: how-to
description: A mod release carries the mod's version, its archives and the page players are sent to when a server runs a mod they lack, and loads under one ABI version.
-->

# Releasing mods

A release carries a mod's version, its archives and the page players are sent
to when a server runs a mod they lack.

## How a release is installed

A player installs a mod by copying its folder into `mods/`, as `mods/<id>/`,
by hand or through a mod manager. A release archive holds that folder. How server admins put the release into a container image is in
[Run a server in Docker or Kubernetes](https://enshwick.com/guides/run-a-server-in-docker/).

## Versions

The platform's crates carry versions of their own:

- the loader, `enshwick-loader`
- the `powrprof.dll` stub, `enshwick-loader-stub`
- `enshwick` with its test fake, and `enshwick-macros`
- `enshwick-values` and `enshwick-typed`, internal parts of the API crate
  released at its version
- `enshwick-abi`, the ABI types, an internal part of the API crate released
  at its version
- the engine routes and anchors with the reflection reader,
  `enshwick-gamedata`
- `cargo-enshwick`

A changed route or anchor is a loader patch release and leaves the API's
version alone. Mods have versions
of their own too, released apart from the platform and from each other.

Which loader a mod runs under is not read from that version but from the ABI
version `enshwick` was built with (see
[ABI reference](abi-reference.md#versions)). A mod loads only under a loader
of the same ABI version. Pin `enshwick` with `=`. Release a new build of
the mod for each Enshwick release that changes the ABI. A loader of another
version does not start the mod and logs
`rebuild it for Enshwick <version>`.

A mod's version is [SemVer](https://semver.org). The manifest takes the
crate's version from `Cargo.toml` (`CARGO_PKG_VERSION`), so the version you
release is the one the DLL reports.

The loader names the version wherever the mod comes up:

- `you.trading-post 0.2.0 initialised` in the loader's log;
- `requires you.lockers ^0.2, found 0.1.3` when a requirement is out of
  range;
- `Trading Post 0.2.0, yours 0.1.0` in the dialog of a refused join.

`cargo enshwick manifest` shows what a built DLL says about itself (id,
version, side, join check, dependencies). With `--expect-version` it fails
unless the DLL has that version:

```bash
cargo enshwick manifest target/x86_64-pc-windows-msvc/release/my_mod.dll --expect-version 0.3.0
```

## Alpha releases

Until the first public release, 1.0.0, Enshwick ships alphas. Its crates
carry prerelease versions, `1.0.0-alpha.N`, and alphas do not go to
crates.io, so a mod depends on the alpha's tag from git:

```toml
[dependencies]
enshwick = { git = "https://github.com/enshwick/enshwick", tag = "v1.0.0-alpha.N" }
```

Each alpha is a GitHub prerelease tagged
`v1.0.0-alpha.N` on the
[releases page](https://github.com/enshwick/enshwick/releases). Its
`enshwick.zip` holds the stub and the loader, and no mod. Its notes name the
game version the alpha was checked against and what changed, since Enshwick
supports the latest game version. Before 1.0 any alpha may change
the API, the ABI, file layouts, settings and mod ids. So a mod built for one
alpha may need a rebuild for the next. A build of `main` between two alphas
carries the version of the last one. So the join check does not tell it apart
from that alpha. How the platform builds and releases
its own crates is in
[CONTRIBUTING.md](https://github.com/enshwick/enshwick/blob/main/CONTRIBUTING.md#releases).

The loader's engine routes and anchors (see
[Finding the game](finding-the-game.md)) are compiled into the loader. They
serve every game build, a new one too. A route or anchor fixed for a new
build ships with the next loader release, with that build's compatibility
report.

## The list of loaded mods

[`enshwick::mods::all()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/mods.rs)
lists every loaded mod in load order, the loader first (id `enshwick`). Each
is a [`ModEntry`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/mods.rs)
with its id, version, side, file, whether it runs and the manifest's credits:

```rust
for entry in enshwick::mods::all() {
    enshwick::info!("{} {} at {}", entry.id, entry.version, entry.file);
}
```

The list is empty before the loader initialises the mod. The loader lists itself first, under
`mods::LOADER_ID`; `mods::LOADER_FILES` names the files it is made of, the
stub and the loader's DLL.

## The join check and a refused join

The join check keeps a client out when the server runs mods of side `both`
that the client lacks or has in another version. The loader lists those mods
on the mismatch screen, then raises
[`events::JoinRefused`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/events.rs)
on the client.
[`mods::join_missing()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/mods.rs)
then lists those mods. Each has the id and version the server advertises, `running`
false, and the client's DLL path when it has the mod (empty otherwise). A
server advertises no homepage or name, so players find a mod by its id.

The handler runs on the game thread:

```rust
use enshwick::events::JoinRefused;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.on::<JoinRefused>(|()| {
        for entry in enshwick::mods::join_missing() {
            enshwick::info!("the server runs {} {}", entry.id, entry.version);
        }
    })
}
```

`join_missing()` is empty before a refused join. See [The join check](concepts/join-check.md) for the check
itself.

## Where to publish

- **Publish every release on a page players can open**, such as GitHub
  releases. Set it in `Cargo.toml` as the crate's `homepage`. The
  `repository` key works too. The manifest takes it, and the Mods screen
  shows it.
- **Use the same files everywhere.** Upload the release archive to Nexus or
  other mod sites as it is. So players who install through a mod manager get
  what the release page has.

## Removed mods and holds

Removing a mod's folder removes the hold in it too. A hold the mod set in a
world's `<stem>-mod-data/<id>/hold.txt` stays until the player deletes it, at
their own risk. See [Holds](state.md#holds).
