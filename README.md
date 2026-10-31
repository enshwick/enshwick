# Enshwick

Enshwick is a modding API platform for [Enshrouded](https://enshrouded.com). Mod
makers write mods against its API. The Enshwick loader comes with the platform
and runs every mod, so players install one thing.

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
[Stability](book/src/stability.md) lists them. Mods in C, C++ or Zig use the C
header ([Mods in C](book/src/mods-in-c.md)).

Start with [the modding book](book/src/introduction.md). Contributions follow
[CONTRIBUTING.md](CONTRIBUTING.md).

## For players

The loader sits next to the game or the dedicated server as `powrprof.dll`. It
loads every mod from the `mods/` folder beside it.

### Install

1. Download
   [`enshwick.zip`](https://github.com/enshwick/enshwick/releases/download/dev/enshwick.zip).
   It holds the loader and no mod. It is a development build of the newest
   commit. Until the first release, 1.0.0, there are no other builds.
2. Extract it into the folder with `enshrouded.exe` (players) or
   `enshrouded_server.exe` (servers). You get `powrprof.dll` and a `mods`
   folder.
3. Download the mods you want. Each mod's page below links its own archive.
   Extract each one into the same folder.
4. Start the game or the server. "Mods" in the main menu and in the pause menu
   lists the mods.

On Linux under Proton, add `WINEDLLOVERRIDES="powrprof=n,b" %command%` to the
game's launch options. For a server in Docker or Kubernetes, see
[Running a server in Docker or Kubernetes](book/src/servers.md).

To turn a mod off, set `enabled = false` for it in `mods/enshwick/mods.toml`. To
remove a mod, delete its DLL from `mods/`. To remove Enshwick, delete
`powrprof.dll`.

### Mods

| Mod                                                                                   | What it does                                                       |
| ------------------------------------------------------------------------------------- | ------------------------------------------------------------------ |
| [Better Altars](https://github.com/enshwick/mods/tree/main/better-altars)             | twice the altars per Flame level; touching zones grow together     |
| [Survivors per Altar](https://github.com/enshwick/mods/tree/main/survivors-per-altar) | a copy of each survivor at every altar, with beds of its own altar |
| [Personal Safes](https://github.com/enshwick/mods/tree/main/personal-safes)           | a craftable chest only the player who placed it can use            |
| [Game Rates](https://github.com/enshwick/mods/tree/main/game-rates)                   | building recipe output and furniture comfort, twice by default     |
| [Build Camera](https://github.com/enshwick/mods/tree/main/build-camera)               | the building fly mode for every player, without the skill          |
| [Shared Experience](https://github.com/enshwick/mods/tree/main/shared-experience)     | every character keeps up with the one furthest ahead in experience |
| [Area Loot](https://github.com/enshwick/mods/tree/main/area-loot)                     | emptying one enemy corpse takes the loot of the others around it   |
| [Optimizer](https://github.com/enshwick/mods/tree/main/optimizer)                     | less CPU on Windows and under Wine and Proton, for servers         |

### Playing on a server

Some mods count in the join check. Each mod's page says if it does. A player and
their server need the same versions of these mods. A player who lacks one, or
has another version, sees which and does not join. Such a mod that the player
has and the server does not run is off for that session. Other mods are each
side's own. A server writes the set of mods that loaded, with the SHA-256 of
each DLL, to `mods/enshwick/enshwick.lock`.

### Settings

Each mod writes its settings to `mod-configs/<id>.toml` on first start. These
are the settings a new world starts with, plus a few that belong to each player
or to the game as a whole.

A world takes a copy of the file when it first loads and keeps it in
`mod-data/<id>/worlds/<world>/settings.toml`. From then on the world uses its
copy. An edit of `mod-configs/<id>.toml` reaches only new worlds, each player's
own settings and the settings read at start.

Change a world's settings in the game, in the world settings, under the last
tab. On a dedicated server, an administrator uses "Server settings" on the Mods
screen. The server's command file, `mod-data/enshwick/commands.txt`, also takes
`enshwick config <mod> <key> <value>`. A change applies at once, unless the
mod's page says it waits for the world or the game to start again.

`mod-configs/enshwick.recipes.toml` takes recipes out of the game
([Disabling recipes](book/src/content/disabled-recipes.md)). The logs are in
`logs/enshwick/`: `all.log`, `loader.log` and `mods/<id>.log`.
`mod-configs/enshwick.toml` sets how much they keep.

## What is in this repository

| Path                                                   | What                                                          |
| ------------------------------------------------------ | ------------------------------------------------------------- |
| [`crates/enshwick`](crates/enshwick)                   | the API crate, with the fake host for tests                   |
| [`crates/cargo-enshwick`](crates/cargo-enshwick)       | the `cargo enshwick` tool                                     |
| [`crates/enshwick-macros`](crates/enshwick-macros)     | the macros `enshwick` re-exports                              |
| [`crates/enshwick-gamedata`](crates/enshwick-gamedata) | the build tables and the reader of the game's reflection data |
| [`crates/internal`](crates/internal)                   | the loader DLL, its `powrprof.dll` stub and the Luau runtime  |
| [`examples`](examples)                                 | small mods that each show one part of the API                 |
| [`book`](book)                                         | the modding book                                              |

Mods depend on `enshwick` only. The other crates are parts of the platform.

## License

[MIT](LICENSE). Enshrouded is a game by Keen Games. Enshwick is not affiliated
with Keen Games, and Keen's code and data are not part of this repository.
