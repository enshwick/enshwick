# Affixes

An affix is a named bundle of [stat](stats.md) modifiers whose values roll
within ranges, such as "Sturdy" giving 10 to 30 extra health. Single items
roll affixes as they drop from random loot, come out of crafting, or when a
mod asks; what an item rolled stays with it, and the entity that wears it
gets the modifiers. The
[`affixes`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/affixes.rs)
module holds the API. It needs ABI minor 48.

```rust
use enshwick_loader_api::affixes::{AffixCraft, AffixDef, AffixLoot, Categories, Range, Rarity, RollRules};
use enshwick_loader_api::loot::Source;
use enshwick_loader_api::stats::{Op, builtin};

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    let health = load.open_stat(builtin::HEALTH_MAX)?;
    let fire = load.open_stat(builtin::RESISTANCE_FIRE)?;
    load.affix(
        "sturdy",
        &AffixDef::new("affix.sturdy", Categories::EQUIPMENT)
            .with(Range::new(health, Op::Add, 10.0, 30.0).step(1.0)),
    )?;
    load.affix(
        "fireproof",
        &AffixDef::new("affix.fireproof", Categories::EQUIPMENT)
            .with(Range::new(fire, Op::Add, 0.05, 0.2).step(0.01))
            .rarity(Rarity::Rare)
            .weight(20)
            .group("element"),
    )?;
    // Half the armour bosses drop rolls one or two affixes.
    load.affixes_on_loot(AffixLoot::new(Source::bosses(), 0.5).count(1, 2))?;
    // A fifth of what players craft rolls one.
    load.affixes_on_craft(AffixCraft::new(0.2, RollRules::new(1, 1)))
}
```

## Defining affixes

`load.affix(name, &def)` defines `<mod>:<name>` during `init`, on both
sides, so clients can name what items rolled. An `AffixDef` holds:

| Field | Means |
| --- | --- |
| `text` | the text key that names the affix, in the mod's [texts](content/texts.md) |
| `ranges` | each modifier's stat, operation and range; `Range::step` rolls in steps |
| `weight` | how likely it rolls against the others, 100 by default |
| `rarity` | `Common` to `Legendary`; a roll may cap it |
| `categories` | the item categories it rolls on: `EQUIPMENT` (armour and rings), `WEAPONS`, `TOOLS` and the others |
| `group` | affixes of one group never roll together on an item |

Every mod's affixes form one pool, so a mod that only rolls loot can roll
affixes another mod defines.

## Rolling

A roll takes the pool's affixes that fit the item's category and rarity cap,
draws the count, then each affix by weight among those left and not sharing
a group with one taken, then its values. `roll(&pool, category, rules,
seed)` is the same roll as a pure function: the same seed, category and
pool give the same affixes, whatever order the pool comes in.

- `load.affixes_on_loot(AffixLoot)` rolls the items with an entity of their
  own (weapons, armour, tools) that random loot puts into containers the
  rule's [`Source`](content/random-loot.md) matches, each at the rule's
  chance, right after the container rolls its loot.
- `load.affixes_on_craft(AffixCraft)` rolls the items that turn up in a
  player's inventories within a second of their craft request.
- `affixes.roll(item, rules, seed, once)` rolls one item on purpose, from
  `load.affixes()`; `once` leaves an item that rolled before as it is.

An item rolls once: the rules above skip items that rolled before, even
those that rolled nothing, and an item no affix fits is left unmarked. Rolls
run on the server; the item's category comes from the game's item data,
which the server has once a player has been online.

## What an item rolled

The roll is part of the item's `ItemStats` (`affixes` and `rolled`), kept as
[item data](item-data.md) and sent to clients that run Enshwick.
`affixes.lines(item)` lists what an item gives, a `Line` per modifier: its
own first, then each affix's, with the stat's index for `stats.info` and the
affix's index for `affixes.info`, which names its text key and rarity. That
is the hook for per-item tooltips on the client.

## Testing

The [fake loader](testing.md) rolls as the server does: give items a
category with `FakeHost::item_category`, roll them with `affixes.roll` and
a seed, and put them on an entity with `loader.wear` to see the stats.
