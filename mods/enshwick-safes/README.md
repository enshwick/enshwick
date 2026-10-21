# Safes

The Safe: a chest crafted at the same workshop as the wooden chest, at twice
its ingredients by default, that only the player who placed it can open, empty,
pick up or dismantle. Crafting and building do not draw from someone else's
Safe either. The server's administrators may open every Safe unless the
settings say otherwise.

The owner shows on hover: a Safe is named after its owner when it is placed,
"Safe · Ann" in the server's language, and older Safes get the name the first
time a player comes near one in a session, unless they already have a name.
Only the owner, and administrators when the settings allow, may rename a
Safe.

## Letting other players in

The owner stands within 4 m of one of their Safes and types in the chat:

- `/safe allow <player>`: the player, who must be online, may open the Safe,
  take, put and sort, and craft or build from it, as the owner does; they never
  pick it up, dismantle it, rename it or change who may use it;
- `/safe deny <player>`: takes that back;
- `/safe list`: names the players allowed.

Each acts on the owner's Safe nearest to them, and the reply says which.
Nobody else ever removes a Safe.

All of it works on a dedicated server and in a game a player hosts. A player
who has just joined touches no Safe, their own included, until the server has
registered them, a moment after they arrive.

A Safe is the vanilla wooden chest underneath: without the mod it is an
ordinary chest with every item still in it. It cannot be upgraded, since an
upgrade would replace it with an ordinary chest.

## Settings

`mod-configs/safes.toml`, written on first start:

```toml
cost_multiplier = 2
admins_can_open = true
label_language = "en"
```

`cost_multiplier` scales the Safe's recipe and applies after a restart.
`admins_can_open` lets administrators open, remove and rename every Safe, and
applies while the game runs. `label_language` is the language of the owner's
name on a Safe and of the `/safe` replies; a change applies after a restart, to Safes named from then
on.

Texts are in English and Ukrainian; German, French, Spanish and Polish name
and describe the Safe, and `/safe` replies in them fall back to English; a server
can change or add them in `mod-configs/safes.texts.toml`, in the form of
[texts.toml](src/texts.toml). Owners and the players they allowed are kept per
world, in `mod-data/safes/worlds/<world>/owners.toml`, so every world a player
hosts has its own; files from 0.1 load as they are.

## Install

Download [`enshwick-safes.zip`](https://github.com/themaiby/enshwick/releases/latest/download/enshwick-safes.zip),
always the newest version of this mod, and extract it into the folder that
holds `enshrouded.exe` or `enshrouded_server.exe`. It needs Enshwick Loader:
[`enshwick-loader.zip`](https://github.com/themaiby/enshwick/releases/latest/download/enshwick-loader.zip)
alone, or [`enshwick.zip`](https://github.com/themaiby/enshwick/releases/latest/download/enshwick.zip)
with every mod.

## Who needs it

The server and every player: the item and its recipe come from the mod on each
side, and the mod counts in the join check.

Licensed under [MIT](../../LICENSE).
