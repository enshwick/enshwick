# Updater

Keeps Enshwick and its mods up to date, with your say. A short while after
the game starts, the updater asks for new releases of every loaded mod that
names where its releases come from (`updates:` in its manifest): GitHub, or a
document at an `https://` address. It does the same for the loader itself. When there are any, a dialog in the game's own style lists them, such as
`safes: 0.1.0 → 0.2.0`, and asks whether to download them. Nothing is
downloaded without a Yes.

What you accept is downloaded from the mod's release, checked against
its [minisign](https://jedisct1.github.io/minisign/) signature and put in
`mods/.staged/`. It takes effect the next time you start the game: the
`powrprof.dll` stub moves each file into place before the loader starts and
keeps the file it replaces as `<file>.previous`. Settings files in
`mod-configs/` are never touched.

When a server runs mods you lack, or has them in another version, the join
check keeps you out and says which. The updater then offers the server's
versions, checked the same way. A mod you have comes from its own source,
never one the server names; a mod you lack comes from the source the server
advertises. A version older than yours is marked `(older)` in the dialog.
Restart the game once they are downloaded and join again.

## What is installed

Only archives signed by the mod's author. Each mod names the keys it is
signed with (`signed_by:` in its manifest), and an update must be signed by a
key the version you have names, or by one in `trusted_keys`. You are never
asked about keys:

- a mod whose version you have names no key is not downloaded; the dialog
  lists it as "the author does not sign updates — update it by hand";
- an update signed by another key than the one you have is refused with a
  note: it may have been tampered with, or the author changed keys; update it
  by hand if you trust it;
- a mod you do not have, offered after a refused join, must be signed by the
  author key the server names; the dialog shows the mod, its version, where
  it comes from and that key. You trust it because you trust the server.

Mods of this repository released before author keys are checked against
Enshwick's own key, which has signed all of them. The signature's trusted
comment must name the release's tag and the archive's file name, so a signed
archive cannot pass for another release. From an archive only DLLs
are taken, and only where Enshwick keeps them: `powrprof.dll` beside the game
and `*.dll` under `mods/`. An archive with a path that leaves the game's
folder, or that would write another mod's DLL or the loader's files, is
refused whole.

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

GitHub allows 60 requests an hour without a token; the updater asks once per
repository per check. The log is `logs/enshwick/mods/updater.log`.

## For mod authors

A mod gets updates when its manifest says
`updates: github("owner/name", "component")` and names your minisign public
key with `signed_by: ["RW..."]`, its releases are tagged
`<component>-v<semver>` and each carries `<component>.zip`, with the DLL at
`mods/<file>.dll`, and `<component>.zip.minisig` beside it, signed with the
tag and file name in the trusted comment:

```sh
minisign -S -s my.key -m my-mod.zip -t "my-mod-v1.2.0 my-mod.zip"
```

The key comes with the DLL players install; later releases must be signed
with it. To move to a new key, release one version that names both, signed
with the old one.

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

Every address is `https://`, versions are SemVer in any order, and the
archive's name ends in `.zip`. Entries that break a rule are skipped and
other fields are ignored. The archive holds the DLL at `mods/<file>.dll`, and
its signature's trusted comment names `<id>-v<version>`, `<id>` being the mod
id, and the archive's file name:

```sh
minisign -S -s my.key -m my-mod-1.2.0.zip -t "my-mod-v1.2.0 my-mod-1.2.0.zip"
```

## Who needs it

Players. The server does not run it; its loader advertises where each of
its mods comes from and who signs it.

Licensed under [MIT](../../LICENSE).
