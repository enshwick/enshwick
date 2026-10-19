# Enshwick

A mod loader and modding API for [Enshrouded](https://enshrouded.com), with the
mods built on it. The loader sits next to the game or the dedicated server, and
every mod is a DLL in the `mods/` folder beside it.

## Mods

| Mod                                                     | What it does                                                       |
| ------------------------------------------------------- | ------------------------------------------------------------------ |
| [Flame altar zones](docs/mods/flame-altar-zones.md)     | touching altar zones grow together and can be built across         |
| [Flame altar count](docs/mods/flame-altar-count.md)     | how many altars each Flame level allows, twice vanilla by default  |
| [Survivors per altar](docs/mods/survivors-per-altar.md) | a copy of each survivor at every altar, with beds of its own altar |

## Install

1. Download `enshwick.zip` from the
   [latest release](https://github.com/themaiby/enshwick/releases).
2. Extract it into the folder that holds `enshrouded.exe` (players) or
   `enshrouded_server.exe` (servers). You get `powrprof.dll` there and a `mods`
   folder beside it.
3. Start the game or the server. Each mod writes its settings to
   `mods/<mod>.toml` on first start; the log is `mods/enshwick.log`.

Players and their server need the same mods: the server advertises its mod set
and a player with a different one is warned before joining. To remove a mod,
delete its DLL from `mods/`; to remove Enshwick, delete `powrprof.dll`.

On Linux under Proton, add `WINEDLLOVERRIDES="powrprof=n,b" %command%` to the
game's launch options.

## For mod authors

Mods are written in Rust against [`enshwick-api`](crates/enshwick-api). How the
repository is laid out, checked and released:
[docs/conventions/](docs/conventions/). Contributions follow
[CONTRIBUTING.md](CONTRIBUTING.md).

## License

[MIT](LICENSE). Enshrouded is a game by Keen Games; Enshwick is not affiliated
with them, and Keen's code and data are not part of this repository.
