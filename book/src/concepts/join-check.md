# The join check

Some mods only work when the server and every player run them in the same
version: content, or a limit the client's interface also checks. Such a mod
sets `join = true` in its manifest. Leave it off for mods that change only
what one side sees or decides, such as a server-only balance change.

## What the server advertises

Once the dedicated server has logged on to Steam, the loader advertises the
running join mods (the ones that are on):

- `[enshwick <fingerprint>]` at the end of the server's Steam name, so players
  see it in the server browser, with or without mods;
- the Steam rule `enshwick`: the fingerprint, eight hex digits over the loader
  version, every `id@version` and every mod's join values;
- `enshwick.mods`, `enshwick.mods.1`, ...: the mods, `id@version`, sorted and
  split to fit Steam's rule length;
- `enshwick.sync.<id>`: a mod's join values, as an inline TOML table;
- `enshwick.src.<id>`: a mod's update source, `github:<owner/name>:<component>`
  or `url:<url>`;
- `enshwick.key.<id>`: the author keys a mod declares with `signed_by:`, the
  `RW...` lines separated by a space.

The loader sets the name and rules again now and then, since the game sets its
name once at start and the join values may change with the settings. A client
that hosts a game advertises nothing.

## What a player sees

Before a modded client joins a server, it asks the server for its rules and
compares the fingerprint with its own:

- **The same fingerprint.** The player joins.
- **The server does not run some of the player's join mods**, or runs no
  Enshwick at all. Those mods are turned off on the client until the game
  restarts, a note lists them, and the player joins.
- **The server runs join mods the player lacks or has in another version.** A
  note lists each, with the server's version and the player's, and the client
  does not join. Then the loader raises `JoinMissing` (below).
- **The same mods, another loader version.** A note says so, and the player
  joins.
- **The server does not answer the query.** The player joins without a check.

Mods without `join = true` never count: the player may run any of them.

## Join values

A field of a mod's settings marked `#[config(sync)]` is a join value: the
server advertises it, it counts in the fingerprint, and a joining client runs
with the server's value over its own file, which stays as it is. See
[Settings](../settings.md).

## Mods the player lacks

When the check keeps a player out, the loader raises
[`events::JoinMissing`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/events.rs)
on the client after the player closes the note, and
[`mods::join_missing()`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/mods.rs)
lists the missing mods with the server's versions and update sources. An
updater mod offers them from there; see
[Releasing and updates](../releasing.md).
