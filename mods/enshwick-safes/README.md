# Safes

The Safe: a chest crafted at the same workshop as the wooden chest, at twice
its ingredients by default, that only the player who placed it can open, empty,
pick up or dismantle. Crafting and building do not draw from someone else's
Safe either. The server's administrators may open every Safe unless the
settings say otherwise.

The owner shows on hover: a Safe is named after its owner when it is placed,
"Safe · Ann" in the server's language. An older Safe without a name gets it
a few seconds after its area loads, once the server knows the owner's name:
from its records, or because the owner is online.
Only the owner, and administrators when the settings allow, may rename a
Safe.

## Letting other players in

Stand within 4 m of one of your Safes: the bottom of the screen shows
"[F7] Who may open this Safe: your Safe at (x, y, z)". Press F7, and a
window lists the players online, and those you already let in, with a
switch each. Switch a player on, and they may open the Safe, take, put and
sort, and craft or build from it, as you do; they never pick it up,
dismantle it, rename it or change who may use it. Switch them off to take
that back, even while they are away. The game confirms each change with a
notice.

The same list is on the Mods screen, from the pause menu: Mods, then
Actions under Safes. It shows one action at a time: the list, and "Name
who may open it", which answers with a notice. The key is `actions_key` in
the loader's settings.

The Safe at hand is the nearest one within 4 m you may manage, your own
first: your own, or, for administrators while `admins_can_open` is on, any
Safe, or one another mod lets you manage (see For mod authors). Nobody else
ever removes a Safe, except administrators while `admins_can_open` is on.

Chat commands do the same, for a player whose game lacks Enshwick or who
prefers typing: `/safe allow <player>`, `/safe deny <player>` and
`/safe list`; `/safe help` lists them. The reply names the Safe and whose
it is.

All of it works on a dedicated server, in a game a player hosts and in
single player. A player who has just joined touches no Safe, their own
included, until the server has registered them, a moment after they arrive.

A Safe is the vanilla wooden chest underneath: without the mod it is an
ordinary chest with every item still in it. It cannot be upgraded, since an
upgrade would replace it with an ordinary chest.

## Settings

`mod-configs/safes.toml`, written on first start; each
world keeps its own copy of it (see [Settings](../../README.md#settings)):

```toml
cost_multiplier = 2
admins_can_open = true
label_language = "en"
```

`cost_multiplier`, 1 to 100, scales the Safe's recipe. It is the server's: a
joined player's crafting menu shows the server's cost, and a change, or a
world with another value, applies at once, with no restart of the game.
`admins_can_open` lets administrators open, remove and rename every Safe, and
applies while the game runs. `label_language` is the language of the owner's
name on a Safe and of the `/safe` replies in chat, a language code such as
`en`, `uk` or `de`; anything else is refused. A change applies while the
game runs, to Safes named from then on. The window and the hint speak each
player's own language.

Texts are in English and Ukrainian; German, French, Spanish and Polish name
and describe the Safe, and the window and replies in them fall back to
English; a server
can change or add them in `mod-configs/safes.texts.toml`, in the form of
[texts.toml](src/texts.toml). Owners and the players they allowed are kept per
world, in `mod-data/safes/worlds/<world>/owners.toml`, so every world a player
hosts has its own; files from 0.1 load as they are.

## Install

Download [`enshwick-safes.zip`](https://github.com/enshwick/enshwick/releases/download/dev/enshwick-safes.zip)
and extract it into the folder that holds `enshrouded.exe` or
`enshrouded_server.exe`. It needs Enshwick Loader:
[`enshwick-loader.zip`](https://github.com/enshwick/enshwick/releases/download/dev/enshwick-loader.zip)
alone, or [`enshwick.zip`](https://github.com/enshwick/enshwick/releases/download/dev/enshwick.zip)
with every mod. These are development builds of the newest commit: until
the first release, 1.0.0, there are no others.

## Who needs it

The server and every player: the item and its recipe come from the mod on each
side, and the mod counts in the join check.

## For mod authors

Other mods change who may open, use, remove or rename a Safe, and who may
manage the list of players at it, at the extension point
`safes:access`, and may refuse outright. See
[Extending other mods](https://github.com/enshwick/enshwick/blob/main/book/src/extending.md).

Licensed under [MIT](../../LICENSE).
