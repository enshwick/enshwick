# The join check

Some mods only work when the server and every player run them in the same
version: content, or a limit the client's interface also checks. Such a mod
sets `join: true` in its manifest. Leave it off for mods that change only
what one side sees or decides, such as a server-only balance change.

## What the server advertises

Once the dedicated server has logged on to Steam, the loader advertises the
running join mods (the ones that are on):

- `[enshwick <fingerprint>]` at the end of the server's Steam name, so players
  see it in the server browser, with or without mods;
- the Steam rule `enshwick`: the fingerprint, eight hex digits over the loader
  version, every `id@version` and every mod's join values;
- `enshwick.mods`, `enshwick.mods.1`, ...: the mods, `id@version`, sorted and
  split to fit Steam's rule length; a [content pack](../content/packs.md)'s
  version carries the hash of its files, `lanterns@1.0.0+3f2a9c1e`;
- `enshwick.sync.<id>`: a mod's join values, as an inline TOML table;
- `enshwick.src.<id>`: a mod's update source, `github:<owner/name>:<component>`
  or `url:<url>`;
- `enshwick.key.<id>`: the author keys a mod declares with `signed_by:`, the
  `RW...` lines separated by a space;
- `enshwick.name.<id>`: a mod's name, so a client's notes name a mod it lacks
  as players know it.

The loader sets the name and rules again now and then, since the game sets its
name once at start and the join values may change with the settings.

## Games a client hosts

A client that hosts a game has no Steam rules. Every game, hosted or on a
dedicated server, sits in a Steam lobby that each player enters before the
game connects, so a hosting client with the loader puts two keys into its
lobby's data: `enshwick`, the loader version, and `enshwick.mods`, the running
join mods as `id@version,...`.

A friend's modded client that enters such a lobby compares the lists. The
host's join values follow over the loader's messages once the friend's game
has connected, as on a dedicated server, until then the friend's own values
apply. The check is weaker than a dedicated server's, since the game has
already chosen to join when the client reads the lobby:

- **The host does not run some of the player's join mods.** They are turned
  off on the client until the game restarts, and a note lists them.
- **The host runs join mods the player lacks or has in another version.** A
  note lists each, and the player joins all the same; no `JoinRefused` is
  raised. The host's own check then removes the player, as a dedicated server
  does.
- **The host runs no Enshwick**, or a lobby without the keys. Nothing is
  checked and the client keeps its own values, since the client cannot tell
  such a host from a dedicated server, whose check already ran.

## What a player sees

Before a modded client joins a server, it asks the server for its rules and
compares the fingerprint with its own:

- **The same fingerprint.** The player joins.
- **The server does not run some of the player's join mods**, or runs no
  Enshwick at all. Those mods are turned off on the client until the game
  restarts, a note lists them, and the player joins.
- **The server runs join mods the player lacks or has in another version.** A
  note lists each, with the server's version and the player's, and the client
  does not join. Then the loader raises `JoinRefused` (below). A mod the
  player turned off in `mods/enshwick/mods.toml` is named as such, left out of
  `JoinRefused`'s list so no updater offers it, and is the player's to turn
  back on.
- **The same mods, another loader version.** A note says so, and the player
  joins.
- **The server does not answer the query.** The player joins without a check.

Notes show as dialogs in the game's own style, over the game, which goes on
drawing; a refused join is refused at once, and the note follows.

Mods without `join: true` never count: the player may run any of them.

## What does a server do with a player who lacks its mods?

A dedicated server that runs join mods checks every player itself, since a
client without the loader runs no check. A client that hosts a game checks its
guests the same way. When a player enters the world, the server waits for the
player's game to say which join mods it runs:

- **The game names every join mod in the server's version.** The player
  stays. Extra mods do not count.
- **The game says nothing within `join_hello_seconds`**, 30 by default. The
  game runs without Enshwick, or its own check did not run. The server sends
  the player a chat line naming Enshwick, each join mod with its version, and
  where to get them.
- **The game lacks a join mod or runs it in another version.** The chat line
  comes at once and names the player's version beside the server's.

After `join_note_seconds`, 10 by default, the server disconnects the player
through the game's own kick. A matching hello in that time calls the kick off.
With `require_enshwick = false` in `mod-configs/enshwick.toml`, the server
lets such players in. On a game build whose table lacks the kick, the player
gets the chat line and stays, and the log says so.

A server without join mods admits every player. A hosted game never checks or
kicks its host, and checks guests only after the host's own game has said hello
and while the host is in the world. Administrators are checked and kicked like
every other player.

The check keeps games with different mods apart; it does not stop cheating. A
game that lies about its mods passes it.

## Join values

A field of a mod's settings marked `#[config(sync)]` is a join value: the
server advertises it, it counts in the fingerprint, and a joining client runs
with the server's value over its own file, which stays as it is. See
[Settings](../settings.md).

## Mods the player lacks

When the check keeps a player out, the loader raises
[`events::JoinRefused`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/events.rs)
on the client after the player closes the note, and
[`mods::join_missing()`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/mods.rs)
lists the missing mods with the server's versions and update sources. An
updater mod offers them from there; see
[Releasing and updates](../releasing.md).
