# Safes

The Safe: a chest crafted at the same workshop as the wooden chest, at twice
its ingredients by default, that only the player who placed it can open, empty,
pick up or dismantle. Crafting and building do not draw from someone else's
Safe either. The server's administrators may open every Safe unless the
settings say otherwise.

A Safe is the vanilla wooden chest underneath: without the mod it is an
ordinary chest with every item still in it. It cannot be upgraded, since an
upgrade would replace it with an ordinary chest.

## Settings

`mod-configs/safes.toml`, written on first start:

```toml
cost_multiplier = 2
admins_can_open = true
```

Texts are in English, Ukrainian, German, French, Spanish and Polish; a server
can change or add them in `mod-configs/safes.texts.toml`, in the form of
[texts.toml](src/texts.toml). Owners are kept in `mod-data/safes/owners.toml`.

## Who needs it

The server and every player: the item and its recipe come from the mod on each
side, and the mod counts in the join check.

Licensed under [MIT](../../LICENSE).
