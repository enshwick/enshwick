# Enshwick

Enshwick is the modding framework and API for
[Enshrouded](https://enshrouded.com). Its mod loader runs mods on your PC or
dedicated server. Write mods in Rust, Luau, C, C++ or as data.

<!--
The paragraph before this comment is `site.description` of src/data/texts.json
in enshwick/docs, the source of the project's words. Change it there first.
-->

Enshwick is in alpha: version 1.0 is not out yet. Until 1.0, anything may
change between alphas, so back up your saves before you update. The site,
[enshwick.com](https://enshwick.com), has the guides for players and server
admins and the docs for mod makers.

## For mod makers

- **A typed Rust API.** [`enshwick`](crates/enshwick) is the one crate a mod
  depends on. It covers settings, events, the world, items and recipes, the
  game's interface and messages between server and client. Where it has no call,
  a mod adds a hook to a game function.
- **Luau scripts.** A [Luau mod](book/src/luau.md) is a folder with a
  `main.luau`. It needs no compiler and runs in a sandbox of its own.
- **Data packs without code.** A [data pack](book/src/content/packs.md) adds
  items, recipes, workshops, loot and texts from TOML files.
- **Tests without the game.** The `test` feature of `enshwick` adds a
  [fake host](book/src/testing.md), so a mod's unit tests run on any computer.
- **Translations.** A mod carries its [texts](book/src/content/texts.md) in the
  15 languages the game offers. Each player sees their own language.
- **One tool.** [`cargo enshwick`](crates/cargo-enshwick) starts a Rust mod, a
  Luau mod or a data pack, checks it and installs it into the game.

Parts of the API behind the `unstable` feature can change in any release.
[Stability](book/src/stability.md) lists them. Mods in C, C++ or any other
language that can call a C ABI use the C header ([Mods in C and C++](book/src/mods-in-c.md)).

Start with [the modding book](book/src/introduction.md). Contributions follow
[CONTRIBUTING.md](CONTRIBUTING.md).

## For players

The loader sits next to the game or the dedicated server as `powrprof.dll`. It
loads every mod from the `mods/` folder beside it.

### Install

1. Download `enshwick.zip` from the newest alpha on the
   [releases page](https://github.com/enshwick/enshwick/releases). It holds
   the loader and no mod. Until the first release, 1.0.0, every release is an
   alpha: anything may change between alphas, so back up your saves.
2. Extract it into the folder with `enshrouded.exe` (players) or
   `enshrouded_server.exe` (servers). You get `powrprof.dll` and a `mods`
   folder.
3. Download the mods you want. Each mod's page on
   [the site](https://enshwick.com/#mods) links its own archive.
   Extract each one into the same folder.
4. Start the game or the server. "Mods" in the main menu and in the pause menu
   lists the mods.

On Linux under Proton, add `WINEDLLOVERRIDES="powrprof=n,b" %command%` to the
game's launch options. For a server in Docker or Kubernetes, see
[Run a server in Docker or Kubernetes](https://enshwick.com/guides/run-a-server-in-docker/).

Each mod lives in a folder of its own, `mods/<id>/`, and runs while that folder
is there. To turn a mod off, move its folder out of `mods/`. To remove a mod,
delete its folder. To remove Enshwick, delete `powrprof.dll`.

### Mods

[Mods by the Enshwick team](https://enshwick.com/#mods) lists our mods, each
with a page that says what it changes. Their code is in
[enshwick/mods](https://github.com/enshwick/mods).

### Playing on a server

Some mods count in the join check. Each mod's page says if it does. A player and
their server need the same versions of these mods. A player who lacks one, or
has another version, sees which and does not join. Such a mod that the player
has and the server does not run is off for that session. Other mods are each
side's own.

At each world load, a server records the mods that ran, with the SHA-256 of
each DLL. The record is `<save>-mod-data/enshwick/mods.lock` beside the
world's save.

### Settings

Mod settings belong to each world. As a world loads, the loader writes each
mod's settings into a file beside the world's save, from the mod's defaults.
When a mod gets a newer version, the loader adds its new settings there. The
file is `<save>-mod-settings/<id>/settings.toml`.

To change a world's settings, load the world and press the mod's Settings
button on the Mods screen. On a dedicated server, an administrator changes the
server's settings from the same page. The server's console also takes
`enshwick config <mod> <key> <value>`. A change applies when the world loads
again.

### Recipes taken out of a world

A world's `<save>-mod-settings/enshwick/recipes.toml` takes recipes out of
that world ([Disabling recipes](book/src/content/disabled-recipes.md)).

### Logs

The logs are in `logs/`, one file per source and run, such as
`logs/enshwick_2026-11-03_10-00-00.log`.

### The loader's own files

`enshwick.toml` in the game's folder sets how much the logs keep. Each world
has a loader rule of its own, `require_enshwick`, in
`<save>-mod-settings/enshwick/enshwick.toml`.

## What is in this repository

| Path                                                   | What                                                          |
| ------------------------------------------------------ | ------------------------------------------------------------- |
| [`crates/enshwick`](crates/enshwick)                   | the API crate, with the fake host for tests                   |
| [`crates/cargo-enshwick`](crates/cargo-enshwick)       | the `cargo enshwick` tool                                     |
| [`crates/enshwick-macros`](crates/enshwick-macros)     | the macros `enshwick` re-exports                              |
| [`crates/enshwick-gamedata`](crates/enshwick-gamedata) | the engine routes and anchors and the reader of the game's reflection data |
| [`crates/internal`](crates/internal)                   | the loader DLL, its `powrprof.dll` stub and the Luau runtime  |
| [`examples`](examples)                                 | the whole mods and packs the modding book follows             |
| [`book`](book)                                         | the modding book                                              |

Mods depend on `enshwick` only. The other crates are parts of the platform.

### What the repository holds of the game

The repository holds no code, assets, archives or dumps of the game. It holds
these facts about the game:

- Type and field names, sizes and offsets, read from the game's reflection
  data: in the generated typed API (`crates/enshwick/src/typed`), in the
  catalogue of engine facts and in the engine routes
  (`crates/enshwick-gamedata/anchors/routes.toml`), which also name
  instruction shapes and short texts that the game's code loads.
- Short code patterns that the loader uses to find functions, with the bytes
  that change between builds left open, each at most 192 bytes
  (`crates/enshwick-gamedata/anchors/patterns.toml`).
- Names, counts and ids of what the loader finds on each build
  (`crates/enshwick-gamedata/reports`).

The reflection catalogues that the typed API is generated from, and the
tables of past builds, stay outside the repository. `cargo enshwick generate`
reads them from the folder that `ENSH_GAMEDATA` names. The tests that compare
with them skip when it is not set.

## License

[MIT](LICENSE). Enshrouded is a game by Keen Games. Enshwick is not affiliated
with Keen Games. The repository holds none of Keen's code or assets, only the
facts that [What the repository holds of the game](#what-the-repository-holds-of-the-game)
lists.
