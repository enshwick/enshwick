# Updater

Keeps Enshwick and its mods up to date, with your say. A short while after
the game starts, the updater asks GitHub for new releases of every loaded mod
that names its releases (`updates:` in its manifest), and of the loader
itself. When there are any, a window lists them, such as
`safes: 0.1.0 → 0.2.0`, and asks whether to download them. Nothing is
downloaded without a Yes.

What you accept is downloaded from the mod's GitHub release, checked against
its [minisign](https://jedisct1.github.io/minisign/) signature and put in
`mods/.staged/`. It takes effect the next time you start the game: the
`powrprof.dll` stub moves each file into place before the loader starts and
keeps the file it replaces as `<file>.previous`. Settings files in
`mod-configs/` are never touched.

When a server runs mods you lack, or has them in another version, the join
check keeps you out and says which. The updater then offers the server's
versions, from the sources the server advertises, checked the same way.
Restart the game once they are downloaded and join again.

## What is installed

Only archives signed by a trusted key: Enshwick's own, which signs every
release of this repository, and the keys in `trusted_keys`. The signature's
trusted comment must name the release's tag and the archive's file name, so
a signed archive cannot pass for another release. From an archive only DLLs
are taken, and only where Enshwick keeps them: `powrprof.dll` beside the game
and `*.dll` under `mods/`. An archive with a path that leaves the game's
folder is refused whole.

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
  start. The offer of a server's versions does not wait for the interval.
- `trusted_keys`: public keys of other authors whose signed releases you
  trust, each the second line of their minisign `.pub` file (`RW...`).
- `prereleases`: offer prereleases too.

GitHub allows 60 requests an hour without a token; the updater asks once per
repository per check. The log is `logs/enshwick/mods/updater.log`.

## For mod authors

A mod gets updates when its manifest says
`updates: github("owner/name", "component")`, its releases are tagged
`<component>-v<semver>` and each carries `<component>.zip`, with the DLL at
`mods/<file>.dll`, and `<component>.zip.minisig` beside it, signed with the
tag and file name in the trusted comment:

```sh
minisign -S -s my.key -m my-mod.zip -t "my-mod-v1.2.0 my-mod.zip"
```

Players add your public key to `trusted_keys`.

## Who needs it

Players. The server does not run it; its loader advertises where each of
its mods comes from.

Licensed under [MIT](../../LICENSE).
