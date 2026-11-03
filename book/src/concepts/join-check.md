# The join check

A mod has one of two sides, its manifest's `side`:

- **`server`**: it runs only where the world runs, on the dedicated server or
  on a client that hosts a game or plays alone. Players need not install it.
- **`both`**: it runs where the world runs and on every client playing in the
  world, which must have it in the world's version: content, or a limit the
  client's interface also checks.

Only mods of side `both` count in the join check, and only they open
[message channels](../messages.md) to clients. There is no mod for the client
alone and no join flag.

A client runs only the mods of the world it plays in: on a dedicated server,
the server's mods of side `both`; in a game it plays alone or hosts, its own.
Its other mods are turned off for that world, so a modded client can join a
vanilla server.

## What the server advertises

Once the dedicated server has logged on to Steam, the loader advertises its
running mods of side `both` (the ones that are on):

- `[mods: 6]`, their count, at the end of the server's Steam name, so players
  see in the server browser that it runs mods;
- the Steam rule `enshwick`: the fingerprint, eight hex digits over the loader
  version and every `id@version`;
- `enshwick.mods`, `enshwick.mods.1`, ...: the mods, `id@version`, sorted and
  split to fit Steam's rule length; a [data pack](../content/packs.md)'s
  version carries the hash of its files, `lanterns@1.0.0+3f2a9c1e`.

A server advertises no homepage or name for its mods: a player reads a mod's
id and version, never a place a server chose. The loader sets the name and
rules again now and then, since the game sets its name once at start. The
rules carry no settings: the server sends them to each player once the player
joined ([Settings](#settings)).

## Games a client hosts

A client that hosts a game has no Steam rules. Every game, hosted or on a
dedicated server, sits in a Steam lobby that each player enters before the
game connects, so a hosting client with the loader puts two keys into its
lobby's data: `enshwick`, the loader version, and `enshwick.mods`, its running
mods of side `both` as `id@version,...`.

A friend's modded client that enters such a lobby compares the lists. The
host's settings follow over the loader's messages once the friend's game has
connected, as on a dedicated server; until then the friend's mods run on
their defaults. The check is weaker than a dedicated server's, since the game
has already chosen to join when the client reads the lobby:

- **The host does not run some of the player's mods.** They are turned off for
  that game, and a note lists them.
- **The host runs mods the player lacks or has in another version.** A note
  lists each, and the player joins all the same; no `JoinRefused` is raised.
  The host's own check then removes the player, as a dedicated server does.
- **The host runs no Enshwick**, or a lobby without the keys. Nothing is
  checked and the client's mods run on their defaults, since the client cannot
  tell such a host from a dedicated server, whose check already ran.

## What a player sees

Before a modded client joins a server, it asks the server for its rules and
compares the fingerprint with its own:

- **The same fingerprint.** The player joins.
- **The server does not run some of the player's mods**, or runs no Enshwick
  at all. Those mods are turned off for that world, a note lists them, and the
  player joins.
- **The server runs mods the player lacks or has in another version.** The
  client does not join. The main menu opens the mismatch screen, below, and
  the loader raises `JoinRefused`.
- **The same mods, another loader version.** A note says so, and the player
  joins.
- **The server does not answer the query.** The player joins without a check.

A mod turned off for a world is back on before the client joins another
server or opens a world of its own, and that world decides again. A data pack
stays off until the game restarts, since its data went when it turned off;
the Mods screen says so.

Notes show as dialogs in the game's own style, over the game, which goes on
drawing. A refused join is refused at once and the game goes back to its
menu; the game's own dialog about a failed join is held back.

## The mismatch screen

After a refused join, or a kick for its mods, the client shows the mods that
differ in the main menu, on a page of the Mods screen, once:

- **Missing**: the server's mods the player lacks, or has turned off until the
  game restarts;
- **In another version**: the server's mods the player has in another version,
  or another build of a data pack's files;
- **Off for this server**: the player's mods the server does not run. They
  never keep the player out.

Each line names the mod's id, the version the world needs and the version the
player has: `you.lanterns: needs 0.3.0, you have none`. The list scrolls, and
OK closes the screen. It names no place to get a mod and no name a server
wrote. On a game build where the screen cannot show, the refusal shows one
dialog with the first mods instead.

## What does a server do with a player who lacks its mods?

A dedicated server that runs mods of side `both` checks every player itself,
since a client without the loader runs no check. A client that hosts a game
checks its guests the same way. When a player enters the world, the server
waits for the player's game to say which mods of side `both` it runs:

- **The game names every one in the server's version.** The player stays.
  Extra mods do not count.
- **The game says nothing within 30 seconds.** The game runs without
  Enshwick, or its own check did not run. The server sends the player a chat
  line naming Enshwick, where to get it, and each mod with its version.
- **The game lacks a mod or runs it in another version.** The chat line comes
  at once and names the player's version beside the server's. The server also
  sends the game its list of mods, which the game shows on the mismatch screen
  once it is back in the menu.

Ten seconds after the chat line the server disconnects the player through the
game's own kick. A matching hello in that time calls the kick off. With
`require_enshwick = false` in `mod-configs/enshwick.toml`, the server lets
such players in. On a game build whose table lacks the kick, the player gets
the chat line and stays, and the log says so.

A server without mods of side `both` admits every player. A hosted game never
checks or kicks its host, and checks guests only after the host's own game has
said hello and while the host is in the world. Administrators are checked and
kicked like every other player.

The check keeps games with different mods apart; it does not stop cheating. A
game that lies about its mods passes it.

## Settings

Once a player's game has connected and said hello, the server sends it every
setting of its mods of side `both`, without a `secret`, and again after each
change. The player's mods run on those values for that world, and the
player's game never writes them to disk. See
[Settings](../settings.md#on-a-joined-server).

## Mods the player lacks

A mod hears of a refused join through `JoinRefused` and reads the mods the
player lacks with `mods::join_missing()`: each with the id and version the
server advertises, and the player's DLL path when the player has the mod.
