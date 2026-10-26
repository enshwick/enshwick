# Stats

Stats are named numbers on entities: extra health, a chance to crit, a
damage factor, whether a character glows. Items, buffs and mods modify them;
the server works the values out once per change, and any mod reads them for
free. The
[`stats`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/stats.rs)
module holds the API.

```rust
use core::time::Duration;
use enshwick::stats::{Lasting, Modifier, Op, StatDef, builtin};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    // A kind of this mod's own, `<mod>:luck`, from 0 to 1.
    let luck = load.define_stat("luck", &StatDef::additive("stat.luck").range(0.0, 1.0))?;
    // A built-in kind the platform applies to the game.
    let health = load.open_stat(builtin::HEALTH_MAX)?;
    let stats = load.stats()?;
    load.add_command("bless", "more health and luck for a minute", move |ctx, _args| {
        let Some(me) = ctx.character() else {
            return "the console has no character".into();
        };
        let player = me;
        let minute = Lasting::For(Duration::from_secs(60));
        let _health = stats.set(player, "bless", Modifier::new(health, Op::Add, 50.0), minute);
        let _luck = stats.set(player, "bless", Modifier::new(luck, Op::Add, 0.1), minute);
        "blessed".into()
    })
}
```

## Kinds

A stat kind has an id `<mod>:<name>`, each side 1 to 64 of `[a-z0-9_-]`,
and a `StatDef`:

| Field | Means |
| --- | --- |
| `ty` | `Additive` (points, 0 by default), `Multiplicative` (a factor, 1 by default) or `Flag` (on or off) |
| `default` | the value without modifiers; `starting_at(v)` sets it |
| `min`, `max` | the range the value stays in; `range(min, max)` sets it |
| `text` | the text key that names the stat, in the mod's [texts](content/texts.md) |
| `format` | how the value reads: `Number`, `Signed` or `Percent` |

`load.define_stat(name, &def)` defines a kind in the mod's own namespace during
`init`; `load.open_stat(id)` opens any kind by its full id, another mod's or
a built-in one. Every mod that opens one id shares its values, so mods read
each other's stats without depending on each other. A kind no mod defines
reads 0, and the loader logs it. `stats.info(stat)` gives a kind's id and
definition, for a menu or a tooltip.

Item stats store a kind by its key, the game id of the GUID its id turns
into, as for any [content id](content.md#content-ids). Two ids with one key
would read as one kind, so the loader turns off the mod that opens the
second and names both ids and both mods; affixes are checked the same way.

## Modifiers

A `Modifier` is a stat, an `Op` and a value. They come from three sources,
summed in this order:

1. **Worn items**: an item keeps its `ItemStats` as [item data](item-data.md)
   of the platform's kind `enshwick:stats`; `stats.item(item)` reads it and
   `stats.set_item(item, &stats)` writes it, on the server. The items an
   entity wears are its equipment: armour, rings, the ranged weapon and the
   shield. Clients that run Enshwick get every item's stats.
2. **Buffs**: `load.buff_modifiers(buff, &modifiers)` gives every entity
   that has the buff type the modifiers while it has it.
3. **Mods**: `stats.set(entity, key, modifier, lasting)` keeps a modifier
   under the mod's own key; the same key and stat replace the one before,
   and `stats.remove(entity, key, stat)` takes it away. `Lasting` is
   `UntilRemoved`, `For(duration)` of real time, or `Saved`: kept with the
   world for the player's account and given back each time the player
   joins.

The operations apply in a fixed order, whatever order the sources give them
in: every `Add` to the default, then every `Multiply`, then the greatest
`Max` as a floor; an `Override` replaces the result, and the last one in
source order wins. The kind's range applies last, and a `Flag` reads 1 for
any value but 0. `aggregate(&def, modifiers)` is the same sum, for a mod's
own use and its tests:

```rust
use enshwick::stats::{Op, StatDef, aggregate};

let health = StatDef::additive("stat.health").range(-100.0, 1000.0);
// (0 + 50 + 30) * 1.5, raised to at least 100.
let value = aggregate(&health, [(Op::Add, 50.0), (Op::Multiply, 1.5), (Op::Add, 30.0), (Op::Max, 100.0)]);
assert_eq!(value, 120.0);
```

`stats.sources(entity, Some(stat))` lists the modifiers that count toward a
stat with their `Origin` (`Item`, `Buff` or `Mod`), in the order they apply.

## Values

`stats.value(entity, stat)` reads a value, and `stats.is_on(entity, stat)` a
flag. The server works an entity's values out after each step in which one
of its sources changed: a mod set or removed a modifier, a timed one ran
out, an item's stats changed, or what it wears or its buffs differ from the
last look, which comes once a second and at once when a player's equipment
changes. The values go out as one snapshot, so a read takes no lock and
costs a hash lookup, from any thread: a damage filter may read them. A
client reads defaults; what a player sees of a built-in kind comes through
the game itself.

The server watches players, every entity a mod keeps a modifier on, and the
entities a mod asks it to watch with `stats.watch(entity, true)`, such as an
enemy whose curse buff should count. Others keep their defaults.

`load.on_stats_changed(|change| ...)` runs on the server's game thread after
the step that moved a value, with the entity, the stat and the values before
and after.

## Built-in kinds

The platform defines these kinds in the `enshwick` namespace and applies
them to the game on the server, the dedicated one or a hosting client:

| Kinds | Type | What the game does with them |
| --- | --- | --- |
| `health_max`, `stamina_max`, `mana_max` | points | added to the entity's maximum as gear adds to it; clients see the new maximum |
| `damage`, `damage_<type>` | factor | multiply the hits the entity deals, every type or one |
| `resistance`, `resistance_<type>` | part | the part of a hit's damage the entity takes less: 0.25 takes a quarter less, 1 takes none, a negative part takes more |

The types are `cut`, `pierce`, `blunt`, `fire`, `ice`, `shroud`,
`lightning` and `poison`. Damage and resistance act through the same site
as [damage filters](damage.md), before them: on hits only, after the game's
own resistances and before its armour. The attacker of a hit is the root of
what hit, so an archer's damage factors count for its arrows. Health,
stamina and mana maxima go through the game's own adders, which perks, food
and gear change the same way, so they stay as the game keeps them; the
current value follows the game's own rules.

The game's other attributes, such as movement speed, critical chance or
armour points, are not built-in kinds yet; their sets work alike and may
follow.

## Tooltips

What an item gives is item data, so a client that runs Enshwick knows every
item's `ItemStats`: [`affixes.lines(item)`](affixes.md#what-an-item-rolled)
lists it a modifier per line, and `stats.info(stat)` names each line's text
key and format. Per-item tooltip lines need a client hook of their own;
until it lands, a mod item that always gives the same can show it with
[tooltip lines](content/items.md#tooltip-lines) on its type.

## Testing

The [fake loader](testing.md) answers stats as the server does, working
values out on every read: `loader.wear(entity, item)` puts an item on an
entity, `loader.set_buffs` and the buff calls give buffs, and
`loader.settle_stats()` runs the change handlers as a step would.
