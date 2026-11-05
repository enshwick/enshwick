# Shrine

A content mod in two steps: a shrine tonic in data, then behaviour in a
script. Copy the folder into the game's `mods/` folder as `mods/you.shrine/`.

## Step 1: data only

These files need no code:

| File | What it adds |
| --- | --- |
| `pack.toml` | The pack, and two settings each world keeps: `tonic_chance` and `first_drink_experience` |
| `items.toml` | The Shrine Tonic, a clone of the game's first health potion with its own name |
| `recipes.toml` | Its recipe, a clone of a night potion's recipe at the same station |
| `loot.toml` | A tonic in chests, as often as the world's `tonic_chance` says |
| `rewards.toml` | The experience of `first_drink_experience` for a player's first tonic in a world, once |
| `texts.toml` | Names and descriptions in English and Ukrainian |

Without `main.luau` and the item's `on` table, the pack works as it is: the
tonic heals as the potion it clones, and the reward follows its first drink.

## Step 2: a script on the item

`main.luau` defines two global functions, and the `on` table of the tonic
in `items.toml` binds them:

- `drink_tonic` counts each player's tonics in the world's data beside its
  save and logs the count.
- `keep_tonic` returns `false`, so a tonic is never thrown away.

## Try it

1. Start a world with the mod, open the Mods screen and the mod's gear, and
   set `tonic_chance` to 1.
2. Load the world again and open a chest: it holds a tonic.
3. Drink the tonic: it heals, the first one gives the experience, and the
   mod's log in `logs/` says the count.
4. Try to throw a tonic away: it stays in the inventory.

The names of the cloned potion and recipe come from the game's item and
recipe registries. The book's chapters
[Data packs](../../book/src/content/packs.md),
[What players do with items](../../book/src/content/item-actions.md) and
[Rewards for actions](../../book/src/content/rewards.md) describe each file.
