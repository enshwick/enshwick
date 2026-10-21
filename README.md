# Enshwick Loader

A mod loader and modding API for [Enshrouded](https://enshrouded.com), with the
mods built on it. The loader sits next to the game or the dedicated server, and
every mod is a DLL in the `mods/` folder beside it. Four things make it up:

- **Enshwick Loader**, for players and servers: one archive, `enshwick.zip`,
  with the loader, its `powrprof.dll` stub and every mod below. See
  [Install](#install).
- **[`enshwick-loader-api`](crates/enshwick-loader-api)**, for mod authors: the
  one crate a mod depends on. Its `test` feature adds a fake loader for unit
  tests.
- **[`cargo enshwick`](crates/cargo-enshwick)**, the one tool: it starts a mod,
  builds and installs it, shows a DLL's manifest, and writes the build table
  of a new game build.
- **Mods**, grouped by purpose below.

The crates under [`crates/internal`](crates/internal) (the loader DLL, the
stub, the reflection reader, the build tables and the macros) are parts of the
loader, not for mods.
[`examples`](examples) holds small mods that show one part of the API each;
they are never released.

## Mods

### Gameplay

| Mod                                                       | What it does                                                       |
| --------------------------------------------------------- | ------------------------------------------------------------------ |
| [Flame altar zones](mods/enshwick-flame-altar-zones)      | touching altar zones grow together and can be built across         |
| [Flame altar count](mods/enshwick-flame-altar-count)      | how many altars each Flame level allows, twice vanilla by default  |
| [Furniture comfort](mods/enshwick-furniture-comfort)      | more comfort from furniture, twice vanilla by default              |
| [Survivors per altar](mods/enshwick-survivors-per-altar)  | a copy of each survivor at every altar, with beds of its own altar |
| [Safes](mods/enshwick-safes)                              | a craftable chest only the player who placed it can use            |
| [Building materials x2](mods/enshwick-building-materials) | twice the output of block, roof, overgrowth and decor recipes      |
| [Build camera](mods/enshwick-build-camera)                | the building fly mode for every player, without the skill          |
| [Shared experience](mods/enshwick-shared-experience)      | every character keeps up with the one furthest ahead in experience |
| [Area loot](mods/enshwick-area-loot)                      | emptying one enemy corpse takes the loot of the others around it   |

### Server

| Mod                                      | What it does                                                              |
| ---------------------------------------- | ------------------------------------------------------------------------- |
| [Admin tools](mods/enshwick-admin-tools) | `/admin` commands and a command file run as the server console            |
| [Optimizer](mods/enshwick-optimizer)     | less CPU on Windows and under Wine and Proton, with a report and measures |

### For players

| Mod                              | What it does                                                     |
| -------------------------------- | ---------------------------------------------------------------- |
| [Updater](mods/enshwick-updater) | signed updates from GitHub releases, installed with your consent |

## Install

1. Download `enshwick.zip` from the
   [latest release](https://github.com/themaiby/enshwick/releases/latest),
   titled `Enshwick Loader <version>`: the loader with every mod of this
   repository.
   The loader and the API share that one version; each mod has its own.
2. Extract it into the folder that holds `enshrouded.exe` (players) or
   `enshrouded_server.exe` (servers). You get `powrprof.dll` there and a `mods`
   folder beside it.
3. Start the game or the server. Each mod writes its settings to
   `mod-configs/<id>.toml` on first start; the log is `logs/enshwick/`
   (`all.log`, `loader.log`, `mods/<id>.log`), and how much it keeps is set in
   `mod-configs/enshwick.toml`. In game, "Mods" in the main menu and in the
   pause menu lists the mods and changes their settings.

Players and their server need the same mods: the server advertises its mod set.
A player's mods the server does not run are turned off for that session, with a
note; a player who lacks a server mod, or has another version, is told which
and does not join. To turn a mod off, set `enabled = false` for it in
`mods/enshwick/mods.toml`, which the loader writes on first start with every
mod it finds; to remove one, delete its DLL from `mods/`; to remove Enshwick
Loader, delete `powrprof.dll`. A server keeps the exact set that loaded, with
each DLL's SHA-256, in `mods/enshwick/enshwick.lock`.

On Linux under Proton, add `WINEDLLOVERRIDES="powrprof=n,b" %command%` to the
game's launch options.

## For mod authors

Mods are written in Rust against
[`enshwick-loader-api`](crates/enshwick-loader-api); start with
[the modding book](book/src/SUMMARY.md).
[`cargo enshwick`](crates/cargo-enshwick) starts a mod and installs it into the
game. Contributions follow [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[MIT](LICENSE). Enshrouded is a game by Keen Games; Enshwick is not affiliated
with them, and Keen's code and data are not part of this repository.
