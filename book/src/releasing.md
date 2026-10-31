# Releasing mods

A player installs a mod by copying its DLL into `mods/`, by hand or through a
mod manager. This chapter covers what a release carries: the version, the
archives, and the page players are sent to when a server runs a mod they lack.

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

A mod's version is [SemVer](https://semver.org). Leave `version` out of
[`manifest!`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/declare.rs)
and the manifest takes the crate's version (`CARGO_PKG_VERSION`), so the
version you release is the one the DLL reports. Set `version: "0.3.0"` only
when the two must differ.

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

## Releases in this repository

Until the first public release, 1.0.0, this repository has no regular
releases. The platform's crates carry prerelease versions, `1.0.0-alpha.N`,
and a mod depends on `enshwick = "=1.0.0-alpha.N"`.

The
[dev-build workflow](https://github.com/enshwick/enshwick/blob/main/.github/workflows/dev-build.yml),
runs on every push to `main` that changes the crates or the tools, and by
hand. It builds the commit with `cargo xtask bundle` and replaces the files
of the one prerelease tagged `dev`, whose tag moves to that commit, so
`dev` always carries `main`. `enshwick.zip` there holds everything that goes into the game
folder, each mod of `tools/mods.toml` taken from the `dev` prerelease of its
own repository. `cargo xtask bundle` writes the same archives into
`target/bundle` on any machine with `cargo-xwin`. Two development builds of
one commit join each other; builds of different commits carry the same
versions, so the join check does not tell them apart.

Versions come from [release-plz](https://release-plz.dev), set up in
[release-plz.toml](https://github.com/enshwick/enshwick/blob/main/release-plz.toml)
and run by hand through the
[release-plz workflow](https://github.com/enshwick/enshwick/blob/main/.github/workflows/release-plz.yml):

1. `release-pr` reads the conventional commits since each crate's last
   release, checks the public API with cargo-semver-checks, and opens or
   updates one release pull request with the new versions and each crate's
   `CHANGELOG.md`.
2. Once that pull request merges, `release` tags each released crate
   `<package>-v<version>`, such as `enshwick-v1.0.0-alpha.3`, makes its
   GitHub release, and publishes the public crates to crates.io. The loader
   and the stub are tagged only; they never go to crates.io.

The loader's build tables (see [Finding the game](finding-the-game.md)) are
compiled into the loader, so a table for a new game build ships with the next
loader build.

## The list of loaded mods

[`enshwick::mods::all()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/mods.rs)
lists every loaded mod, the loader first (id `enshwick`), in load order. Each
[`ModEntry`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/mods.rs)
has:

| Field                                           | What it holds                                                                   |
| ----------------------------------------------- | ------------------------------------------------------------------------------- |
| `id`                                            | The mod id                                                                      |
| `version`                                       | Its SemVer version                                                              |
| `side`                                          | The sides it runs on                                                            |
| `join`                                          | Whether it counts in the join check                                             |
| `running`                                       | Not refused at init and not faulted since                                       |
| `file`                                          | The DLL's path relative to the game's folder, such as `mods/trading_post.dll`   |
| `name`                                          | What players read; `entry.label()` gives the name, or the id without a name     |
| `description`, `authors`, `license`, `homepage` | The manifest's credits; empty for none                                          |
| `replaced_by`                                   | In `mods::replaced()`, the mod that replaces this one; empty elsewhere          |

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

A mod that keeps a world the vanilla game cannot open safely can set a
[`state::Hold`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/state.rs).
While `mod-data/<id>/hold.txt` exists, the loader stops the game before the
world loads when that mod does not run: removed, skipped, off, or on a game
build the loader does not know. Removing such a mod therefore needs the
player to delete its `hold.txt` as well, at their own risk. See
[State and data folders](state.md).
