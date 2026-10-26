# Releasing mods

A player installs a mod by copying its DLL into `mods/`, by hand or through a
mod manager. This chapter covers what a release carries: the version, the
archives, and the page players are sent to when a server runs a mod they lack.

## Versions

Enshwick Loader has one version, such as Enshwick Loader 0.2.0: the loader
(`enshwick-loader`), the `powrprof.dll` stub (`enshwick-loader-stub`),
`enshwick-loader-api` with its test fake and its macros
(`enshwick-loader-macros`), `enshwick-loader-reflect`, the build tables
(`enshwick-loader-builds`) and `cargo-enshwick` always carry the same number
and are released together. Mods have versions of their own, released apart from the
platform and from each other.

Which loader a mod runs under is not read from that version but from the ABI
version `enshwick-loader-api` was built with (see
[ABI reference](abi-reference.md#versions)). A mod built against
`enshwick-loader-api` X.Y needs a loader of the same ABI major whose ABI minor is
at least the one X.Y was built with; an older loader does not start the mod
and logs `update Enshwick`. Any loader of the same ABI major loads a mod built
against an older `enshwick-loader-api`.

A mod's version is [SemVer](https://semver.org). Leave `version` out of
[`manifest!`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/declare.rs)
and the manifest takes the crate's version (`CARGO_PKG_VERSION`), so the
version you release is the one the DLL reports. Set `version: "0.3.0"` only
when the two must differ.

The loader names the version wherever the mod comes up:

- `safes 0.2.0 initialised` in `loader.log`;
- `requires flame-altar-zones ^0.2, found 0.1.3` when a requirement is out of
  range;
- `Safes 0.2.0, yours 0.1.0` in the dialog of a refused join.

`cargo enshwick manifest` shows what a built DLL says about itself (id,
version, side, join check, dependencies). With `--expect-version` it fails
unless the DLL has that version:

```bash
cargo enshwick manifest target/release/my_mod.dll --expect-version 0.3.0
```

## Releases in this repository

Until the first public release, 1.0.0, this repository has no releases.
Every crate carries the version `0.0.0-dev`, and the
[dev-build workflow](https://github.com/themaiby/enshwick/blob/main/.github/workflows/dev-build.yml),
run by hand, builds the commit it runs on with `cargo xtask bundle` and
replaces the files of the one prerelease tagged `dev`, whose tag moves to
that commit. `enshwick.zip` there holds everything that goes into the game
folder: each mod of `tools/mods.toml` comes from the `dev` prerelease of its
own repository once that repository has one, and is built from this
repository until then. `cargo xtask bundle` writes the same archives into
`target/bundle` on any machine with `cargo-xwin`. Two development builds of one commit join each other; builds of
different commits carry the same versions, so the join check does not tell
them apart. The release workflow below runs by hand as well and makes 1.0.0
the first version of every crate.

Mods in this repository follow
[CONTRIBUTING.md](https://github.com/themaiby/enshwick/blob/main/CONTRIBUTING.md):

- the crate is `mods/enshwick-<id>`, named `enshwick-<id>`;
- the pull request title is a conventional commit, which becomes the squashed
  commit and the changelog entry;
- [release-please](https://github.com/googleapis/release-please) keeps one
  release pull request for the workspace and, once it is merged, tags each
  released crate `<component>-v<semver>`, the component being the crate name:
  `enshwick-safes-v0.2.0`.

The platform crates are linked (the `linked-versions` plugin, group
`enshwick`): a change to any of them releases all of them at one version,
each with its own tag, `enshwick-loader-v0.2.0`,
`enshwick-loader-api-v0.2.0` and so on. The tags keep what relies on them
working: `cargo enshwick new` pins `enshwick-loader-api-v<version>`. The
loader's release is the platform release, titled
`Enshwick Loader <version>` and kept as the repository's latest release. A platform
release also bumps the patch version of every mod, since they all depend on
`enshwick-loader-api`.

Before 1.0, a `feat` bumps the patch version and a breaking change the minor
(see
[release-please-config.json](https://github.com/themaiby/enshwick/blob/main/release-please-config.json)).

The
[release workflow](https://github.com/themaiby/enshwick/blob/main/.github/workflows/release-please.yml)
then builds the workspace and attaches to each mod's release:

| File          | What it holds                    |
| ------------- | -------------------------------- |
| `<crate>.zip` | `mods/<crate>.dll` and `LICENSE` |

The platform release carries `enshwick-loader.zip` (the stub and the loader)
and `enshwick.zip` (everything that goes next to the game). Before it uploads
anything, the workflow checks with `cargo enshwick manifest --expect-version`
that every released mod's DLL reports the version of its tag.

The loader's build tables (see [Finding the game](finding-the-game.md)) are
compiled into the loader, so a table for a new game build ships with the next
loader build.

## The list of loaded mods

[`enshwick_loader_api::mods()`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/mods.rs)
lists every loaded mod, the loader first (id `enshwick`), in load order. Each
[`ModEntry`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/mods.rs)
has:

| Field                                           | What it holds                                                                   |
| ----------------------------------------------- | ------------------------------------------------------------------------------- |
| `id`                                            | The mod id                                                                      |
| `version`                                       | Its SemVer version                                                              |
| `side`                                          | The sides it runs on                                                            |
| `join`                                          | Whether it counts in the join check                                             |
| `running`                                       | Not refused at init and not faulted since                                       |
| `file`                                          | The DLL's path relative to the game's folder, such as `mods/enshwick_safes.dll` |
| `name`                                          | What players read; `entry.label()` gives the name, or the id without a name     |
| `description`, `authors`, `license`, `homepage` | The manifest's credits; empty for none                                          |
| `replaced_by`                                   | In `mods::replaced()`, the mod that replaces this one; empty elsewhere          |

```rust
for entry in enshwick_loader_api::mods() {
    enshwick_loader_api::info!("{} {} at {}", entry.id, entry.version, entry.file);
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
[`events::JoinRefused`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/events.rs)
on the client.
[`mods::join_missing()`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/mods.rs)
then lists those mods: the server's version, `running` false, the client's
DLL path when it has the mod (empty otherwise), and the name and homepage the
server advertises.

The handler runs on the game thread once the player closed the dialog:

```rust
use enshwick_loader_api::events::JoinRefused;

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    load.on::<JoinRefused>(|()| {
        for entry in enshwick_loader_api::mods::join_missing() {
            enshwick_loader_api::info!("the server runs {} {}", entry.id, entry.version);
        }
    })
}
```

`join_missing()` is empty before a refused join. See [The join check](concepts/join-check.md) for the check
itself.

`mods::replaced()` lists the mods in `mods/` that a present mod replaces after
a rename (`replaces:` in the [manifest](concepts/loader-and-abi.md#the-manifest)),
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
[`state::Hold`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/state.rs).
While `mod-data/<id>/hold.txt` exists, the loader stops the game before the
world loads when that mod does not run: removed, skipped, off, or on a game
build the loader does not know. Removing such a mod therefore needs the
player to delete its `hold.txt` as well, at their own risk. See
[State and data folders](state.md).
