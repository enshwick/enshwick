# Updater

Keeps Enshwick and the mods you have up to date, with your say. A short
while after the game starts, the updater asks for new releases of every
loaded mod that names where its releases come from (`updates:` in its
manifest): GitHub, or a document at an `https://` address. It does the same
for the loader itself. When there are any, a dialog in the game's own style
lists them, such as `safes: 0.1.0 → 0.2.0`, and asks whether to download
them. Nothing is downloaded without a Yes, and the updater never offers mods
you do not have.

What you accept is downloaded from the mod's release and put in
`mods/.staged/`. It takes effect the next time you start the game: the
`powrprof.dll` stub moves each file into place before the loader starts and
keeps the file it replaces as `<file>.previous`. Settings files in
`mod-configs/` are never touched.

A mod renamed since you installed it names its old id (`replaces:` in its
manifest), and the loader no longer starts the old DLL beside it. When the
updater installs or updates the renamed mod, it deletes that old DLL from
`mods/`: only the file the loader lists for the old id, never another mod's
or the loader's.

When a server runs mods you lack, or has them in another version, the join
check keeps you out and says which. The updater then offers the server's
versions, checked the same way. A mod you have comes from its own source,
never one the server names; a mod you lack comes from the source the server
advertises. A version older than yours is marked `(older)` in the dialog.
Restart the game once they are downloaded and join again.

## What is installed

An update comes only from the source the mod you have names, over `https://`,
and is at most 64 MiB. You are never asked about keys:

- a mod that names its author's keys (`signed_by:` in its manifest), as
  Enshwick's own do, gets only updates signed by one of those keys, checked
  with [minisign](https://jedisct1.github.io/minisign/), or by one in
  `trusted_keys`. An update signed by another key is refused with a note: it
  may have been tampered with, or the author changed keys; update it by hand
  if you trust it;
- a mod that names no key gets its updates unsigned, marked `(unsigned)` in
  the dialog: you trust its updates as you trusted the mod when you
  installed it. Only its own DLL is replaced;
- a mod you do not have, offered after a refused join, comes from the source
  the server names and, when the server names an author key for it, must be
  signed by that key. The dialog shows the mod, its version, where it comes
  from and the key, or that it is unsigned. You trust it because you trust
  the server.

Mods of this repository released before author keys are checked against
Enshwick's own key, which has signed all of them. A signature's trusted
comment must name the release's tag and the file's name, so a signed file
cannot pass for another release. From an archive only the files Enshwick
reads are taken, and only where it keeps them: `powrprof.dll` beside the
game, `*.dll` under `mods/`, and a content pack's `.toml` and `.png` files in
its folder under `mods/`, never the loader's; from an unsigned one, the mod's
DLL alone. An archive with a path that leaves the game's folder, or that
would write another mod's DLL, another pack's folder or the loader's files,
is refused whole.

## On the Mods screen

The Mods screen shows what the last check found: `Update: <version>` under
each mod with an update, with an Update button when it can be installed.
An unsigned update says so under the version. What you press is asked once
more in a dialog and then downloaded like any update. A start that is not due
to check shows the updates of the lists kept at the last check, without going
online.

## Build tables

After a game update the loader may not have a build table for the new game
build yet. It then runs the game on the code patterns of an older table, and
the features those patterns miss stay off. The updater asks Enshwick's
`build-tables` release for the table of the running build, on the client and
on the dedicated server alike, without asking: the table is data, and both the
updater and the loader use it only when Enshwick's key signed it for that
build. It lands in `mods/enshwick/builds/` and takes effect at the next start;
the client says so in a dialog, the server in the log. With `enabled = false`
nothing is fetched.

## Settings

`mod-configs/updater.toml`, written on first start:

```toml
enabled = true
check_interval_hours = 24
trusted_keys = []
prereleases = false
```

- `enabled`: off, the updater never goes online.
- `check_interval_hours`: hours between checks at start; `0` checks at every
  start. A check that could not reach every source runs again at the next
  start. The offer of a server's versions does not wait for the interval.
- `trusted_keys`: public keys accepted for updates besides the ones each
  installed mod names, each the second line of a minisign `.pub` file
  (`RW...`). For an author who changed keys; add only keys you trust.
- `prereleases`: offer prereleases too.

GitHub allows 60 requests an hour without a token. The updater reads a
repository's release list a page of 100 at a time, newest first, and stops
once it has seen every mod it updates from there, at most 10 pages. It keeps
each page in `mod-data/updater/cache/` and asks GitHub whether it changed: an
unchanged page costs nothing of the hourly limit. When GitHub refuses, the
updater leaves it alone until the time GitHub names and checks again at a
later start. A download that breaks off or a server error is tried twice
more; an archive is installed only once its signature checks out, so a
download cut short is never staged. The log is
`logs/enshwick/mods/updater.log`.

The dialogs follow the game's language, English and Ukrainian for now. A
file `mod-configs/updater.texts.toml` rewords them or adds a language, in the
form of the updater's own
[`texts.toml`](src/texts.toml); the log stays in English.

## For mod authors

Name where your releases come from in the manifest, and release on GitHub
the way you already do:

```rust,ignore
enshwick_loader_api::manifest!(
    id: "my-mod",
    side: BOTH,
    updates: github("you/your-repo", "my-mod"),
)
```

| What         | Accepted                                                                                                                |
| ------------ | ----------------------------------------------------------------------------------------------------------------------- |
| Tag          | `my-mod-v1.2.0`, `my-mod-1.2.0`, `my-mod@1.2.0`; in a repository of this one mod also `v1.2.0` or `1.2.0`               |
| Release file | `my-mod.zip`; else the release's only `.zip`; else the one `.zip` whose name holds `my-mod`; else the same for a `.dll` |
| Archive      | the DLL at `mods/<file>.dll`, at the archive's root, or either inside one top-level folder                              |

The second argument of `github(...)` is the name your tags and files use. A
repository counts as one mod's when none of its tags names another component;
in a repository of several mods each tag starts with its mod's name. In names,
`-` and `_` count as one and case does not matter. Versions are SemVer; a
release marked as a prerelease, or a version such as `1.3.0-rc.1`, reaches
only players who turn `prereleases` on.

That is enough: players get your updates unsigned, over `https://`. To sign
them, name your minisign public key with `signed_by: ["RW..."]` and attach
`<file>.minisig` beside the release file, signed with the tag and file name
in the trusted comment:

```sh
minisign -S -s my.key -m my-mod.zip -t "my-mod-v1.2.0 my-mod.zip"
```

The key comes with the DLL players install, and from then on only releases
signed with it reach them. To move to a new key, release one version that
names both, signed with the old one.

Releases hosted elsewhere need `updates: url("https://example.com/my-mod.json")`
and a JSON document at that address that lists them:

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

Every address is `https://`, versions are SemVer in any order, and the file's
name ends in `.zip` or `.dll`. `signature` is needed only by a mod that names
a key. Entries that break a rule are skipped and other fields are ignored. A
signature's trusted comment names `<id>-v<version>`, `<id>` being the mod id,
and the file's name:

```sh
minisign -S -s my.key -m my-mod-1.2.0.zip -t "my-mod-v1.2.0 my-mod-1.2.0.zip"
```

## Install

Download [`enshwick-updater.zip`](https://github.com/themaiby/enshwick/releases/latest/download/enshwick-updater.zip),
always the newest version of this mod, and extract it into the folder that
holds `enshrouded.exe` or `enshrouded_server.exe`. It needs Enshwick Loader:
[`enshwick-loader.zip`](https://github.com/themaiby/enshwick/releases/latest/download/enshwick-loader.zip)
alone, or [`enshwick.zip`](https://github.com/themaiby/enshwick/releases/latest/download/enshwick.zip)
with every mod.

## Who needs it

Players, and servers for the build tables. On a dedicated server it fetches
only those: the server's loader advertises where each of its mods comes from
and who signs it, and players' updaters offer them.

Licensed under [MIT](../../LICENSE).
