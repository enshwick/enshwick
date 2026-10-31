# Releasing mods

A release carries a mod's version, its archives and the page players are sent
to when a server runs a mod they lack.

## How a release is installed

A player installs a mod by copying its DLL into `mods/`, by hand or through a
mod manager. How server admins put the release into a container image is in
[Run a server in Docker or Kubernetes](https://enshwick.com/guides/run-a-server-in-docker/).

## Versions

The platform's crates carry versions of their own: the loader
(`enshwick-loader`), the `powrprof.dll` stub (`enshwick-loader-stub`),
`enshwick` with its test fake, `enshwick-macros`, the build tables with the
reflection reader (`enshwick-gamedata`) and `cargo-enshwick`. A new build table is
a loader patch release and leaves the API's version alone. Mods have versions
of their own too, released apart from the platform and from each other.

Which loader a mod runs under is not read from that version but from the ABI
version `enshwick` was built with (see
[ABI reference](abi-reference.md#versions)). A mod loads only under a loader
of the same ABI version. Pin `enshwick` with `=`, and release a new build of
the mod for each Enshwick release that changes the ABI; a loader of another
version does not start the mod and logs
`rebuild it for Enshwick <version>`.

A mod's version is [SemVer](https://semver.org). The manifest takes the
crate's version from `Cargo.toml` (`CARGO_PKG_VERSION`), so the version you
release is the one the DLL reports.

The loader names the version wherever the mod comes up:

- `you.trading-post 0.2.0 initialised` in `loader.log`;
- `requires you.lockers ^0.2, found 0.1.3` when a requirement is out of
  range;
- `Trading Post 0.2.0, yours 0.1.0` in the dialog of a refused join.

`cargo enshwick manifest` shows what a built DLL says about itself (id,
version, side, join check, dependencies). With `--expect-version` it fails
unless the DLL has that version:

```bash
cargo enshwick manifest target/x86_64-pc-windows-msvc/release/my_mod.dll --expect-version 0.3.0
```

## The development build

Until the first public release, 1.0.0, Enshwick has no regular releases. Its
crates carry prerelease versions, `1.0.0-alpha.N`, and a mod depends on
`enshwick = "=1.0.0-alpha.N"`. The prerelease tagged `dev` always carries the
newest `main`:
[`enshwick.zip`](https://github.com/enshwick/enshwick/releases/download/dev/enshwick.zip)
there holds the stub and the loader, and no mod. Two development builds of
one commit join each other; builds of different commits carry the same
versions, so the join check does not tell them apart. How the platform
builds and releases its own crates is in
[CONTRIBUTING.md](https://github.com/enshwick/enshwick/blob/main/CONTRIBUTING.md#releases).

The loader's build tables (see [Finding the game](finding-the-game.md)) are
compiled into the loader, so a table for a new game build ships with the next
loader build.

## The list of loaded mods

[`enshwick::mods::all()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/mods.rs)
lists every loaded mod, the loader first (id `enshwick`), in load order, each
a [`ModEntry`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/mods.rs)
with its id, version, side, file, whether it runs and the manifest's credits:

```rust
for entry in enshwick::mods::all() {
    enshwick::info!("{} {} at {}", entry.id, entry.version, entry.file);
}
```

The list is empty before the loader initialises the mod. The loader lists itself first, under
`mods::LOADER_ID`; `mods::LOADER_FILES` names the files it is made of, the
stub and the loader's DLL.

## A server's pages and the join check

A server advertises each running join mod's homepage in the Steam rule
`enshwick.page.<id>`: the manifest's `homepage`, an `https://` or `http://`
address. A page longer than a Steam rule holds (127 bytes) is left out. A
client the join check keeps out names that page in its dialog as the place to
get the mod, and the server's own chat note to such a player does the same;
a mod without a page is "ask the server's admin".

When the join check keeps a client out because the server runs join mods the
client lacks or has in another version, the loader shows the player a dialog,
then raises
[`events::JoinRefused`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/events.rs)
on the client.
[`mods::join_missing()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/mods.rs)
then lists those mods: the server's version, `running` false, the client's
DLL path when it has the mod (empty otherwise), and the name and homepage the
server advertises.

The handler runs on the game thread once the player closed the dialog:

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

`mods::replaced()` lists the mods in `mods/` that a present mod replaces
(`replaces:` in the [manifest](concepts/loader-and-abi.md#the-manifest)),
which the loader never started, each with `replaced_by` and its `file`.

## Where to publish

- **Publish every release on a page players can open**, such as GitHub
  releases, and set it as the crate's `homepage` (or `repository`) in
  `Cargo.toml`: the manifest takes it, and a refused join sends players there.
- **Use the same files everywhere.** Upload the release archive to Nexus or
  other mod sites as it is, so players who install through a mod manager get
  what the release page has.

## Removed mods and holds

Removing a mod that set a hold needs the player to delete its
`mod-data/<id>/hold.txt` as well, at their own risk; see
[Holds](state.md#holds).
