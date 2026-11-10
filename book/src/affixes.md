<!--
type: how-to
description: An affix is a named bundle of stat modifiers whose values roll within ranges. Items roll affixes as they drop or are crafted; their wearer gets the modifiers.
-->

# Affixes

An affix is a named bundle of [stat](stats.md) modifiers whose values roll
within ranges, such as "Sturdy" giving 10 to 30 extra health. The
[`affixes`](https://github.com/enshwick/enshwick/blob/main/crates/internal/core/src/affixes.rs)
module holds the API.

## Affixes on items

Single items roll affixes as they drop from random loot or come out of
crafting. They also roll when a mod asks. What an item rolled stays with it,
and the entity that wears it gets the modifiers.

Unstable: rolling affixes onto items needs the `unstable` feature and may
change in any release ([Stability](stability.md)). What an item rolled is
kept as [item data](item-and-entity-data.md). So after a restart, a roll on an
item outside a placed container or a player's inventory stays aside. Defining
affixes and reading what an item rolled are stable.

```rust
use enshwick::affixes::{AffixCraft, AffixDef, AffixLoot, Range, ItemRarity, RollRules};
use enshwick::content::categories::ItemCategoryMask;
use enshwick::content::id;
use enshwick::loot::Source;
use enshwick::stats::{Op, builtin};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let health = load.open_stat(builtin::HEALTH_MAX)?;
    let fire = load.open_stat(builtin::RESISTANCE_FIRE)?;
    load.define_affix(
        id!("sturdy"),
        &AffixDef::new(ItemCategoryMask::EQUIPMENT)
            .with(Range::new(health, Op::Add, 10.0, 30.0).step(1.0)),
    )?;
    load.define_affix(
        id!("fireproof"),
        &AffixDef::new(ItemCategoryMask::EQUIPMENT)
            .with(Range::new(fire, Op::Add, 0.05, 0.2).step(0.01))
            .rarity(ItemRarity::Rare)
            .weight(20)
            .group("element"),
    )?;
    // Half the armour bosses drop rolls one or two affixes.
    load.add_loot_affixes(AffixLoot::new(Source::bosses(), 0.5).count(1, 2))?;
    // A fifth of what players craft rolls one.
    load.add_craft_affixes(AffixCraft::new(0.2, RollRules::new(1, 1)))
}
```

```luau
local health = enshwick.stats.open_stat("enshwick:health_max")
local fire = enshwick.stats.open_stat("enshwick:resistance_fire")
enshwick.affixes.define_affix("you.affixes:sturdy", {
    ranges = { { stat = health, op = "Add", min = 10, max = 30, step = 1 } },
    weight = 100,
    rarity = "Common",
    categories = { "Equipment" },
})
enshwick.affixes.define_affix("you.affixes:fireproof", {
    ranges = { { stat = fire, op = "Add", min = 0.05, max = 0.2, step = 0.01 } },
    weight = 20,
    rarity = "Rare",
    categories = { "Equipment" },
    group = "element",
})
-- Half the armour bosses drop rolls one or two affixes.
enshwick.affixes.add_loot_affixes({
    from = { kinds = { "CORPSES" }, bosses_only = true },
    chance = 0.5,
    rules = { min = 1, max = 2, max_rarity = "Legendary" },
})
-- A fifth of what players craft rolls one.
enshwick.affixes.add_craft_affixes({
    chance = 0.2,
    rules = { min = 1, max = 1, max_rarity = "Legendary" },
})
```

In Luau, an `AffixDef` is a table with every field but `group`. A range's
`step` may be left out. `enshwick.affixes.roll` rolls one item. The pure roll
from a pool is `enshwick.affixes.roll_pool`.

## Defining affixes

`load.define_affix(id!("sturdy"), &def)` defines `<mod>:sturdy`, under the
mod's own id as [how ids are namespaced](concepts/ids.md) requires. It runs
during `init`, on both sides, so clients can name what items rolled. Its text
is `sturdy.name` in the mod's [texts](content/texts.md). A stat kind or an
item of the mod named `sturdy` reads the same text, so give them different
names. An `AffixDef` holds:

| Field | Means |
| --- | --- |
| `ranges` | each modifier's stat, operation and range; `Range::step` rolls in steps |
| `weight` | how likely it rolls against the others, 100 by default |
| `rarity` | `Common` to `Legendary`; a roll may cap it |
| `categories` | the item categories it rolls on. Examples are `EQUIPMENT` (armour and rings) and `WEAPONS`. Others include `TOOLS`. |
| `group` | affixes of one group never roll together on an item |

Every mod's affixes form one pool, so a mod that only rolls loot can roll
affixes another mod defines.

## Rolling

A roll takes the pool's affixes that fit the item's category and rarity cap.
It draws the count first. Then it draws each affix by weight, among those left
that share no group with one taken. Then it draws the affix's values.
`roll(&pool, category, rules, seed)` is the same roll as a pure function. The
same seed, category and pool give the same affixes, whatever order the pool
comes in.

- `load.add_loot_affixes(AffixLoot)` rolls items that random loot puts into
  containers the rule's [`Source`](content/random-loot.md) matches. It rolls
  only items with an entity of their own: weapons, armour, tools. Each rolls
  at the rule's chance, right after the container rolls its loot.
- `load.add_craft_affixes(AffixCraft)` rolls the items that turn up in a
  player's inventories within a second of their craft request.
- `affixes.roll(item, rules, seed, once)` rolls one item on purpose. It comes
  from `load.affixes()`. With `once`, an item that rolled before stays as it
  is.

An item rolls once. The loot and craft rules skip items that rolled before, even
those that rolled nothing, and an item no affix fits is left unmarked. Rolls
run on the server. The item's category comes from the game's item data,
which the server has once a player has been online.

## What an item rolled

The roll is part of the item's `ItemStats`. It sits in the fields `affixes`
and `rolled`. It is kept as [item data](item-and-entity-data.md) and sent to
clients that run Enshwick.

`affixes.lines(item)` lists what an item gives, a `Line` per modifier. Its own
modifiers come first, then each affix's. A line holds the stat's index for
`stats.info` and the affix's index for `affixes.info`. `affixes.info` names
the affix's text key and rarity. That is the hook for per-item tooltips on the
client.

## Testing

The [fake loader](testing.md) rolls as the server does:

1. Give items a category with `FakeHost::item_category`.
2. Roll them with `affixes.roll` and a seed.
3. Put them on an entity with `loader.wear` to see the stats.
