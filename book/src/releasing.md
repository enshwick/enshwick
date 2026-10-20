# Releasing and updates

A player installs a mod by copying its DLL into `mods/`. This chapter covers
what a release carries: the version, where updates come from, who signs them,
how an updater installs them, and how releases are signed.

## Versions

Enshwick Loader has one version, such as Enshwick Loader 0.2.0: the loader
(`enshwick-loader`), the `powrprof.dll` stub (`enshwick-loader-stub`),
`enshwick-loader-api` with its test fake and its macros
(`enshwick-loader-macros`), `enshwick-loader-reflect` and `cargo-enshwick`
always carry the same number and are released together. Mods have versions of their own, released apart from the
platform and from each other.

Which loader a mod runs under is not read from that version but from the ABI
version `enshwick-loader-api` was built with (see
[ABI reference](abi-reference.md#versions)). A mod built against
`enshwick-loader-api` X.Y needs a loader of the same ABI major, and for each feature
it uses, a loader whose ABI minor is at least the one that feature lists; under
an older loader such a feature turns itself off with a note such as
`mid-function hooks: needs a newer loader`. Any loader of the
same ABI major loads a mod built against an older `enshwick-loader-api`.

A mod's version is [SemVer](https://semver.org). Leave `version` out of
[`manifest!`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/declare.rs)
and the manifest takes the crate's version (`CARGO_PKG_VERSION`), so the
version you release is the one the DLL reports. Set `version: "0.3.0"` only
when the two must differ.

The loader names the version wherever the mod comes up:

- `safes 0.2.0 initialised` in `loader.log`;
- `requires flame-altar-zones ^0.2, found 0.1.3` when a requirement is out of
  range;
- `safes: server 0.2.0, yours 0.1.0` in the note of a refused join.

`cargo enshwick manifest` shows what a built DLL says about itself (id,
version, side, join check, dependencies). With `--expect-version` it fails
unless the DLL has that version:

```bash
cargo enshwick manifest target/release/my_mod.dll --expect-version 0.3.0
```

## Releases in this repository

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
working: `cargo enshwick new` pins `enshwick-loader-api-v<version>`, and the
updater and the loader's own `updates:` source look for `enshwick-loader-v*`
releases. The loader's release is the platform release, titled
`Enshwick Loader <version>` and kept as the repository's latest release. A platform
release also bumps the patch version of every mod, since they all depend on
`enshwick-loader-api`.

Before 1.0, a `feat` bumps the patch version and a breaking change the minor
(see
[release-please-config.json](https://github.com/themaiby/enshwick/blob/main/release-please-config.json)).

The
[release workflow](https://github.com/themaiby/enshwick/blob/main/.github/workflows/release-please.yml)
then builds the workspace and attaches to each mod's release:

| File                  | What it holds                         |
| --------------------- | ------------------------------------- |
| `<crate>.zip`         | `mods/<crate>.dll` and `LICENSE`      |
| `<crate>.zip.minisig` | The minisign signature of the archive |

The platform release carries `enshwick-loader.zip` (the stub and the loader)
and `enshwick.zip` (everything that goes next to the game), each with its
`.minisig`. Before it signs anything, the workflow checks with
`cargo enshwick manifest --expect-version` that every released mod's DLL
reports the version of its tag.

## Where updates come from

`updates:` in the manifest names the source of new releases:

```rust
let manifest = enshwick_loader_api::manifest!(
    id: "safes",
    side: BOTH,
    join = true,
    updates: github("themaiby/enshwick", "enshwick-safes"),
    signed_by: ["RWTjqgtfpy3lSOLC4uaUXuvvKSLQKOFf6nUnEytoiFjSzxMEhLnoAdJF"],
);
assert!(!manifest.updates.is_null());
```

| Source                         | Meaning                                                            |
| ------------------------------ | ------------------------------------------------------------------ |
| `github("owner/name", "comp")` | GitHub releases of that repository whose tags are `comp-v<semver>` |
| `url("https://...")`           | A JSON document at that URL that lists the mod's versions          |

A mod without `updates:` is updated by hand.

The loader checks the source and lists it; it fetches nothing itself. It
accepts a repository of the form `owner/name`, a component of `[a-z0-9_-]`
(up to 64 characters) and a URL that starts with `https://`. A malformed
source is dropped with a warning such as
`my_mod.dll: no update source: the update URL "http://..." is not https://`,
and the mod loads without one.

The updater in this repository reads both. The document a `url(...)` source
serves is described below.

## Author keys

`signed_by:` names the [minisign](https://jedisct1.github.io/minisign/)
public keys you sign your releases with, each the second line of a `.pub`
file (`RW...`). Name one; name two while you move to a new key. The build
fails on a string that is not a minisign public key.

An updater trusts a mod the way a phone trusts an app: on first install. The
keys the installed DLL names are the ones a new release must be signed with,
so nobody else can publish an update of your mod, even from your repository
or your URL. Without `signed_by:` the updater does not install your updates;
it tells the player to update the mod by hand.

The loader checks each key and lists it in `ModEntry::signed_by`. A key that
is not a minisign public key is dropped with a warning such as
`my_mod.dll: key ignored: the signed_by key "RW..." is not a minisign public key (RW..., 56 characters)`,
and the mod loads with the others.

To move to a new key, release one version that names both keys, signed with
the old one. Releases after it can be signed with the new key. A player who
skipped that version is refused the newer release with a note that it is
signed by another key, and updates by hand or adds the new key to
`trusted_keys`.

## The document of a URL source

A `url("https://...")` source is a JSON document that lists the mod's
versions:

```json
{
  "versions": [
    {
      "version": "1.2.0",
      "url": "https://example.com/my-mod/my-mod-1.2.0.zip",
      "signature": "https://example.com/my-mod/my-mod-1.2.0.zip.minisig"
    }
  ]
}
```

- `version` is SemVer. A version with a prerelease part, such as
  `1.3.0-rc.1`, is offered only to players who turn `prereleases` on.
- `url` is where the archive downloads from. It is an `https://` address
  whose path ends in a file name with `.zip`, the archive's file name.
- `signature` is where the archive's minisign signature downloads from, also
  `https://`.
- Versions may come in any order. Fields the format does not name are
  ignored, so a document may carry more, such as release notes.
- An entry that breaks a rule (a version that is not SemVer, an address that
  is not `https://`, an archive that is not a `.zip`) is skipped; the other
  entries still count.

The archive is the same as a GitHub release's: the DLL at `mods/<file>.dll`.
Its signature's trusted comment names `<id>-v<version>`, `<id>` being the
mod id from the manifest, and the archive's file name:

```bash
minisign -S -s my-mod.key -m my-mod-1.2.0.zip -t "my-mod-v1.2.0 my-mod-1.2.0.zip"
```

The updater asks for the document once per check, refuses any address that
is not `https://` (redirects included), and reads at most 4 MiB of it.

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
| `updates`                                       | `Some(Updates::GitHub { .. })`, `Some(Updates::Url(..))` or `None`              |
| `signed_by`                                     | The author keys the manifest names, checked; empty for none                     |
| `name`                                          | What players read; `entry.label()` gives `Name (id)`, or the id without a name  |
| `description`, `authors`, `license`, `homepage` | The manifest's credits; empty for none                                          |

```rust
use enshwick_loader_api::mods::Updates;

for entry in enshwick_loader_api::mods() {
    if let Some(Updates::GitHub { repository, component }) = &entry.updates {
        enshwick_loader_api::info!("{} {}: {repository}, tags {component}-v*", entry.id, entry.version);
    }
}
```

The list is empty before the loader initialises the mod and under a loader
older than ABI minor 5. The loader lists itself first, under
`mods::LOADER_ID`; `mods::LOADER_FILES` names the files it is made of, the
stub and the loader's DLL, which an update of a mod never writes.

## A server's sources and the join check

A server advertises each running join mod's source in the Steam rule
`enshwick.src.<id>`, as one line written by
[`Updates::to_line`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/mods.rs):
`github:<owner/name>:<component>` or `url:<url>`. A line longer than a Steam
rule holds (127 bytes) is left out. `Updates::parse` reads a line back. A
running join mod that names author keys also gets the rule
`enshwick.key.<id>`: its keys separated by a space, written by
`mods::keys_line` and read back by `mods::parse_keys`.

```rust
use enshwick_loader_api::mods::Updates;

let safes = Updates::GitHub {
    repository: "themaiby/enshwick".into(),
    component: "enshwick-safes".into(),
};
assert_eq!(safes.to_line(), "github:themaiby/enshwick:enshwick-safes");
assert_eq!(safes.tag_prefix().as_deref(), Some("enshwick-safes-v"));
```

When the join check keeps a client out because the server runs join mods the
client lacks or has in another version, the loader shows the player a note,
then raises
[`events::JoinRefused`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/events.rs)
on the client.
[`mods::join_missing()`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/mods.rs)
then lists those mods: the server's version, `running` false, the client's
DLL path when it has the mod (empty otherwise), the source the server
advertises, else the client's own source for the mod, and in `signed_by` the
author keys the server advertises (empty when it advertises none, or under a
loader older than ABI minor 14).

The handler runs on the game thread once the player closed the note. Hand
downloads to a thread of your own:

```rust
use enshwick_loader_api::events::JoinRefused;

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    load.on::<JoinRefused>(|()| {
        let missing = enshwick_loader_api::mods::join_missing();
        std::thread::spawn(move || {
            for entry in missing {
                enshwick_loader_api::info!("the server runs {} {}", entry.id, entry.version);
            }
        });
    })
}
```

`join_missing()` is empty before a refused join and under a loader older than
ABI minor 10. See [The join check](concepts/join-check.md) for the check
itself.

## Staged updates

An updater never replaces a loaded DLL. It writes the new file to
`mods/.staged/<path>`, where `<path>` is the file's path relative to the
game's folder (`ModEntry::file` for a mod). On the next start, before the
loader runs, the `powrprof.dll` stub moves each staged file to `<path>` and
keeps the file it replaces as `<path>.previous`, for one rollback.

A file that cannot be moved stays staged for the next start; the game starts
anyway. The stub writes what it moved to `logs/enshwick/stub.log`.

## The updater mod

[The updater](https://github.com/themaiby/enshwick/tree/main/mods/enshwick-updater)
is such a mod, for players:

- a short while after start, on a thread of its own, it asks each loaded mod's
  source (its GitHub repository or its URL source's document) for newer
  releases, and asks the player before it downloads anything;
- after a refused join it offers the server's versions of the missing mods,
  from the sources the server advertises;
- it downloads `<component>.zip` and `<component>.zip.minisig` from the
  GitHub release, or the archive and signature a URL source's document names,
  checks the signature, and stages the archive's DLLs;
- it takes only `powrprof.dll` and `mods/*.dll` from an archive, and refuses
  an archive with a path that leaves the game's folder;
- it never touches settings files.

It installs an archive only when the right key signed it and the signature's
trusted comment names both the release's tag and the archive's file name, so a
signed archive cannot pass for another release. For a URL source the tag is
`<id>-v<version>`. The right key is:

| The mod is                                  | The archive must be signed by                                               |
| ------------------------------------------- | --------------------------------------------------------------------------- |
| installed and names keys in `signed_by:`    | one of those keys, or a key in `trusted_keys` of `mod-configs/updater.toml` |
| installed without `signed_by:`              | nothing: the updater lists it as "update it by hand" and downloads nothing  |
| not installed, offered after a refused join | the key the server advertises in `enshwick.key.<id>`                        |

A release signed by another key than the installed version names is refused
with a note: it may have been tampered with, or the author changed keys, and
the player updates it by hand. For a mod offered after a refused join, the
dialog shows the version, the source and the author key the server names,
and says that the player trusts the mod because they trust the server. Mods
of this repository released before author keys are checked against
Enshwick's own key, which has signed all of them.

## Signing your own releases

A mod outside this repository gets updates from the updater when:

1. its manifest says `updates: github("you/your-repo", "my-mod")`;
2. its manifest names your public key with `signed_by: ["RW..."]`;
3. its releases are tagged `my-mod-v<semver>`;
4. each release carries `my-mod.zip`, with the DLL at `mods/<file>.dll`;
5. each release carries `my-mod.zip.minisig`, a
   [minisign](https://jedisct1.github.io/minisign/) signature by that key
   whose trusted comment names the tag and the archive.

Releases hosted elsewhere work the same way with `updates: url("https://...")`
and a [document](#the-document-of-a-url-source) that lists them, the tag in
the trusted comment being `my-mod-v<semver>`.

Make a key pair once and keep the secret key out of the repository:

```bash
minisign -G -p my-mod.pub -s my-mod.key
```

Sign each archive with the tag and file name in the trusted comment:

```bash
minisign -S -s my-mod.key -m my-mod.zip -t "my-mod-v1.2.0 my-mod.zip"
```

Declare the public key, the second line of `my-mod.pub` (`RW...`), in the
manifest, and publish it where players can compare it, such as your README:

```rust,ignore
enshwick_loader_api::manifest!(
    id: "my-mod",
    side: BOTH,
    updates: github("you/your-repo", "my-mod"),
    signed_by: ["RWQ..."],
)
```

Players need nothing else: the key comes with the DLL they install. Keep the
secret key safe; a release signed with a lost key cannot reach players who
have your mod, and one signed with a stolen key can.

## Removed mods and holds

A mod that keeps a world the vanilla game cannot open safely can set a
[`state::Hold`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/state.rs).
While `mod-data/<id>/hold.txt` exists, the loader stops the game before the
world loads when that mod does not run: removed, skipped, off, or on a game
build the loader does not know. Removing such a mod therefore needs the
player to delete its `hold.txt` as well, at their own risk. See
[State and data folders](state.md).
