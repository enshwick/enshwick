# Admin tools

Tools for test servers, for mod authors and server owners trying things out.
Administrators get `/admin` commands in the chat, and a command file runs
commands on the server without anyone joining: each line appended to it runs
as if the server's console had typed it.

| Command                           | What it does                                                                                |
| --------------------------------- | ------------------------------------------------------------------------------------------- |
| `/admin` or `/admin help`         | the list of commands                                                                        |
| `/admin world`                    | the players online, their slots, entities and positions                                     |
| `/admin entity <id>`              | an entity's template GUID, the item it was placed from, position                            |
| `/admin placed [count]`           | the last pieces and props placed in this world, newest first, with entity ids; 5 by default |
| `/admin mods`                     | every loaded mod, its version, sides and whether it runs                                    |
| `/admin reload <mod id>`          | the mod rereads the running world's settings, or else `mod-configs/<mod id>.toml`           |
| `/admin say <text>`               | a chat line to every player                                                                 |
| `/admin give <item> [n] [player]` | `n` new items (1 by default, at most 10000) for you or the named player                     |
| `/enshwick status`                | the loader's status summary (any administrator, tools on or off)                            |

The tools use only what Enshwick offers: world reads, chat, its inventory
API and settings reloads. Teleporting, setting the time of day and saving on
demand wait until the platform has them. An item is a number, decimal or
`0x` hex, or a GUID. `/admin placed` keeps the last 32 placements and forgets
them when the world stops. From the command file, `/admin give` names the
player: `/admin give 0x0c01 10 Ann`.

## The command file

`mod-data/admin-tools/commands.txt`, beside the game. Append a line, save, and
within a second it runs on the server's game thread with an administrator's
rights; any mod's command works, such as `/safe list` or `/enshwick status`.
The line and its reply go to `logs/enshwick/mods/admin-tools.log`.

```text
# lines starting with # are notes; the leading / may be left out
/admin world
admin placed 3
/enshwick status
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

`/admin` answers administrators only. The tools stay off until
`enabled = true`: until then `/admin` tells administrators to turn them on with
`/config admin-tools enabled true`,
and lines appended to the command file are skipped, not kept for later. Keep
them off on a server people play on. A change applies
at once, without a restart.

## From dev tools

This mod was `enshwick-dev-tools`, id `dev-tools`. On its first start it moves
`mod-configs/dev-tools.toml` to `mod-configs/admin-tools.toml` and
`mod-data/dev-tools` to `mod-data/admin-tools`, unless
`mod-configs/admin-tools.toml` already exists or `mod-data/admin-tools` is not
empty. `/dev` still answers as `/admin`, with a note. The loader never starts
an old `mods/enshwick-dev-tools.dll` beside this mod and logs that it can be
deleted.

## Install

Download [`enshwick-admin-tools.zip`](https://github.com/themaiby/enshwick/releases/download/dev/enshwick-admin-tools.zip)
and extract it into the folder that holds `enshrouded.exe` or
`enshrouded_server.exe`. It needs Enshwick Loader:
[`enshwick-loader.zip`](https://github.com/themaiby/enshwick/releases/download/dev/enshwick-loader.zip)
alone, or [`enshwick.zip`](https://github.com/themaiby/enshwick/releases/download/dev/enshwick.zip)
with every mod. These are development builds of the newest commit: until
the first release, 1.0.0, there are no others.

## Who needs it

Only the server, the dedicated one or a game a client hosts. Players need
nothing, and it does not count in the join check.

Licensed under [MIT](../../LICENSE).
