# Admin tools

Tools for test servers, for mod authors and server owners trying things out.
Administrators get actions on the mod's page of the Mods screen, and a
command file runs lines on the server's console without anyone joining.

| Action    | What it does                                                                             |
| --------- | ---------------------------------------------------------------------------------------- |
| `world`   | the players online, their slots, entities and positions                                  |
| `entity`  | an entity's template GUID, the item it was placed from, position                         |
| `placed`  | the last pieces and props placed in this world, newest first, with entity ids            |
| `mods`    | every loaded mod, its version, sides and whether it runs                                 |
| `reload`  | the mod picked rereads the running world's settings, or else `mod-configs/<mod id>.toml` |
| `give`    | new items for you, at most 10000                                                         |
| `give-to` | new items for the player picked, at most 10000                                           |

The tools use only what Enshwick offers: world reads, its inventory API and
settings reloads. An item is its number. `placed` keeps the last 32
placements and forgets them when the world stops.

## The command file

`mod-data/admin-tools/commands.txt`, beside the game. Append a line, save, and
within a second it runs on the server's game thread as the server's console,
an administrator with no character. A line is one of:

- a mod's id, an action and its values, such as `admin-tools world`,
  `admin-tools give-to 3073 10 Ann` or `safes help`; `<mod id> help` lists
  a mod's actions;
- `enshwick` and one of the loader's tools: `status`, `profile [reset]`,
  `conflicts`, `log [mod] [level] [n]`, `recipes [find <text>]`,
  `config [mod] [key] [value]`, `ui-demo <player>` or a mod's id;
  `enshwick help` lists them.

The line and its answer go to `logs/enshwick/mods/admin-tools.log`.

```text
# lines starting with # are notes
admin-tools world
admin-tools placed 3
enshwick status
```

Each line runs once. Lines already in the file when the server starts are
skipped, and a line runs only once it ends with a line break. Emptying the
file starts it over a second later: a file that got shorter or changed waits
for one more read, so an editor's save caught half written runs no line
twice.

## Settings

`mod-configs/admin-tools.toml`, written on first start; each
world keeps its own copy of it (see [Settings](../../README.md#settings)):

```toml
enabled = false
```

The actions answer administrators only. The tools stay off until
`enabled = true`, which an administrator sets behind the mod's gear on the
Mods screen: until then the actions show as off, and lines appended to the
command file are skipped, not kept for later. Keep
them off on a server people play on. A change applies
at once, without a restart.

## From dev tools

This mod was `enshwick-dev-tools`, id `dev-tools`. On its first start it moves
`mod-configs/dev-tools.toml` to `mod-configs/admin-tools.toml` and
`mod-data/dev-tools` to `mod-data/admin-tools`, unless
`mod-configs/admin-tools.toml` already exists or `mod-data/admin-tools` is not
empty. The loader never starts
an old `mods/enshwick-dev-tools.dll` beside this mod and logs that it can be
deleted.

## Install

Download [`enshwick-admin-tools.zip`](https://github.com/enshwick/enshwick/releases/download/dev/enshwick-admin-tools.zip)
and extract it into the folder that holds `enshrouded.exe` or
`enshrouded_server.exe`. It needs Enshwick Loader:
[`enshwick-loader.zip`](https://github.com/enshwick/enshwick/releases/download/dev/enshwick-loader.zip)
alone, or [`enshwick.zip`](https://github.com/enshwick/enshwick/releases/download/dev/enshwick.zip)
with every mod. These are development builds of the newest commit: until
the first release, 1.0.0, there are no others.

## Who needs it

Only the server, the dedicated one or a game a client hosts. Players need
nothing, and it does not count in the join check.

Licensed under [MIT](../../LICENSE).
