# Admin tools

Tools for test servers, for mod authors and server owners trying things out.
Administrators get `/admin` commands in the chat, and a command file runs
commands on the server without anyone joining: each line appended to it runs
as if the server's console had typed it.

| Command                  | What it does                                                     |
| ------------------------ | ---------------------------------------------------------------- |
| `/admin world`           | the players online, their slots, entities and positions          |
| `/admin entity <id>`     | an entity's template GUID, the item it was placed from, position |
| `/admin placed [count]`  | the last pieces and props placed, newest first, with entity ids  |
| `/admin mods`            | every loaded mod, its version, sides and whether it runs         |
| `/admin reload <mod id>` | the mod reads `mod-configs/<mod id>.toml` again                  |
| `/admin say <text>`      | a chat line to every player                                      |
| `/admin give <item> [n]` | `n` new items (an id or a GUID) for you, or `... n <player>`     |
| `/enshwick status`       | the loader's status summary (any administrator, tools on or off) |

The tools use only what Enshwick offers: world reads, chat, its inventory
API and settings reloads. Teleporting, setting the time of day and saving on
demand wait until the platform has them. From the command file, `/admin give`
names the player: `/admin give 0x0c01 10 Ann`.

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
file starts it over.

## Settings

`mod-configs/admin-tools.toml`, written on first start:

```toml
enabled = false
```

The tools stay off until `enabled = true`: `/admin` then answers administrators
only, and the command file runs. Keep them off on a server people play on.
An edit takes effect about a second after saving the file, without a restart.

## From dev tools

This mod was `enshwick-dev-tools`, id `dev-tools`. On its first start it moves
`mod-configs/dev-tools.toml` to `mod-configs/admin-tools.toml` and
`mod-data/dev-tools` to `mod-data/admin-tools`, unless the new ones already
hold something. `/dev` still answers as `/admin`, with a note, until the next
release. Delete the old `mods/enshwick-dev-tools.dll`: the updater does not
replace a mod under another name.

## Who needs it

Only the server, the dedicated one or a game a client hosts. Players need
nothing, and it does not count in the join check.

Licensed under [MIT](../../LICENSE).
