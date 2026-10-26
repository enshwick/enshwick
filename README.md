# Enshwick Loader

A mod loader and modding API for [Enshrouded](https://enshrouded.com), with the
mods built on it. The loader sits next to the game or the dedicated server, and
every mod is a DLL in the `mods/` folder beside it. Four things make it up:

- **Enshwick Loader**, for players and servers: one archive, `enshwick.zip`,
  with the loader, its `powrprof.dll` stub and every mod below. See
  [Install](#install).
- **[`enshwick`](crates/enshwick)**, for mod authors: the
  one crate a mod depends on. Its `test` feature adds a fake loader for unit
  tests.
- **[`cargo enshwick`](crates/cargo-enshwick)**, the one tool: it starts a mod,
  builds and installs it, shows a DLL's manifest, and writes the build table
  of a new game build.
- **Mods**, grouped by purpose below.

The crates under [`crates/internal`](crates/internal) (the loader DLL and the
stub) are parts of the loader, not for mods; [`enshwick-macros`](crates/enshwick-macros),
[`enshwick-reflect`](crates/enshwick-reflect) and
[`enshwick-gamedata`](crates/enshwick-gamedata) (the macros, the reflection
reader and the build tables) are published for `enshwick` and
`cargo enshwick` to build on.
[`examples`](examples) holds small mods that show one part of the API each;
they are never released.

## Mods

### Gameplay

| Mod                                                                         | What it does                                                       |
| --------------------------------------------------------------------------- | ------------------------------------------------------------------ |
| [Flame altar zones](mods/enshwick-flame-altar-zones)                        | touching altar zones grow together and can be built across         |
| [Flame altar count](https://github.com/enshwick/mod-flame-altar-count)      | how many altars each Flame level allows, twice vanilla by default  |
| [Furniture comfort](https://github.com/enshwick/mod-furniture-comfort)      | more comfort from furniture, twice vanilla by default              |
| [Survivors per altar](https://github.com/enshwick/mod-survivors-per-altar)  | a copy of each survivor at every altar, with beds of its own altar |
| [Safes](mods/enshwick-safes)                                                | a craftable chest only the player who placed it can use            |
| [Building materials x2](https://github.com/enshwick/mod-building-materials) | twice the output of block, roof, overgrowth and decor recipes      |
| [Build camera](https://github.com/enshwick/mod-build-camera)                | the building fly mode for every player, without the skill          |
| [Shared experience](https://github.com/enshwick/mod-shared-experience)      | every character keeps up with the one furthest ahead in experience |
| [Area loot](https://github.com/enshwick/mod-area-loot)                      | emptying one enemy corpse takes the loot of the others around it   |

### Server

| Mod                                      | What it does                                                              |
| ---------------------------------------- | ------------------------------------------------------------------------- |
| [Admin tools](mods/enshwick-admin-tools) | administrator actions and a command file run on the server's console      |
| [Optimizer](mods/enshwick-optimizer)     | less CPU on Windows and under Wine and Proton, with a report and measures |

## Install

1. Download
   [`enshwick.zip`](https://github.com/enshwick/enshwick/releases/download/dev/enshwick.zip):
   the loader with every mod of this repository. It is a development build
   of the newest commit; until the first release, 1.0.0, there are no others.
2. Extract it into the folder that holds `enshrouded.exe` (players) or
   `enshrouded_server.exe` (servers). You get `powrprof.dll` there and a `mods`
   folder beside it.
3. Start the game or the server. Each mod writes its settings to
   `mod-configs/<id>.toml` on first start (see [Settings](#settings)); the
   log is `logs/enshwick/` (`all.log`, `loader.log`, `mods/<id>.log`), and
   how much it keeps is set in `mod-configs/enshwick.toml`. In game, "Mods" in
   the main menu and in the pause menu lists the mods.
   `mod-configs/enshwick.recipes.toml` takes recipes out of the game, vanilla
   or a mod's ([Disabling recipes](book/src/content/disabled-recipes.md)).

Players and their server need the same versions of the mods that count in the
join check, which each mod's page names; the server advertises them. A
player's such mods the server does not run are turned off for that session,
with a note; a player who lacks one the server runs, or has another version,
is told which and does not join. Other mods are each side's own. To turn a mod off, set `enabled = false` for it in
`mods/enshwick/mods.toml`, which the loader writes on first start with every
mod it finds; to remove one, delete its DLL from `mods/`; to remove Enshwick
Loader, delete `powrprof.dll`. A server keeps the exact set that loaded, with
each DLL's SHA-256, in `mods/enshwick/enshwick.lock`.

On Linux under Proton, add `WINEDLLOVERRIDES="powrprof=n,b" %command%` to the
game's launch options.

A server in Docker or Kubernetes: see
[Running a server in Docker or Kubernetes](book/src/servers.md).

## Settings

`mod-configs/<id>.toml` holds a mod's settings as each new world starts with
them, the few settings that are each player's own, and the few that only a
start of the game sets up, which every world shares. A world takes a copy
when it first loads, single player, hosted or on a dedicated server, and
keeps it in `mod-data/<id>/worlds/<world>/settings.toml`; from then on that
copy is the one in force, so an edit of `mod-configs/<id>.toml` reaches only
worlds made later, each player's own settings and the ones read at start. A world's settings are
changed in the game, while hosting or on the screen that creates or starts a
world: the world settings list every mod's settings under their last tab.
On a dedicated server an administrator changes them from "Server settings"
on the Mods screen, or with `enshwick config <mod> <key> <value>` in the
[admin tools'](mods/enshwick-admin-tools) command file. Such a change applies at once, except where a mod's page says it
waits for the world to load again or for the game to start again. A hand
edit of the running world's copy applies about a second after it is saved, as
such a change does.

## For mod authors

Mods are written in Rust against
[`enshwick`](crates/enshwick); start with
[the modding book](book/src/SUMMARY.md).
[`cargo enshwick`](crates/cargo-enshwick) starts a mod and installs it into the
game. Contributions follow [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[MIT](LICENSE). Enshrouded is a game by Keen Games; Enshwick is not affiliated
with them, and Keen's code and data are not part of this repository.
