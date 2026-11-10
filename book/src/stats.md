<!--
type: how-to
description: Stats are named numbers on entities, such as extra health or a chance to crit. Items, buffs and mods modify them, and a mod defines stat kinds of its own.
-->

# Stats

Stats are named numbers on entities: extra health, a chance to crit, a
damage factor, whether a character glows. The
[`stats`](https://github.com/enshwick/enshwick/blob/main/crates/internal/core/src/stats.rs)
module holds the API.

## A mod's first stats

Items, buffs and mods modify stats; the server works the values out once per
change, and any mod reads them for free.

```rust
use core::time::Duration;
use enshwick::actions::{Action, Outcome};
use enshwick::content::id;
use enshwick::stats::{Lasting, Modifier, Op, StatDef, builtin};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    // A kind of this mod's own, `<mod>:luck`, from 0 to 1.
    let luck = load.define_stat(id!("luck"), &StatDef::additive().range(0.0, 1.0))?;
    // A built-in kind the platform applies to the game.
    let health = load.open_stat(builtin::HEALTH_MAX)?;
    let stats = load.stats()?;
    let actions = load.actions(enshwick::i18n::Texts::default())?;
    actions.add(Action::new("bless", "More health and luck for a minute"), move |request| {
        let player = request.player.entity;
        let minute = Lasting::For(Duration::from_secs(60));
        let _health = stats.set(player, "bless", Modifier::new(health, Op::Add, 50.0), minute);
        let _luck = stats.set(player, "bless", Modifier::new(luck, Op::Add, 0.1), minute);
        Outcome::done("Blessed")
    })
}
```

A Luau mod defines and opens kinds while it starts and sets modifiers
through `enshwick.stats`; here each player who joins is blessed for a minute:

```luau
-- A kind of this mod's own, from 0 to 1.
local luck = enshwick.stats.define_stat("you.blessings:luck", {
    ty = "Additive", default = 0, min = 0, max = 1, format = "Number",
})
-- A built-in kind the platform applies to the game.
local health = enshwick.stats.open_stat("enshwick:health_max")
local stats = enshwick.stats.stats()

enshwick.on("PlayerJoined", function(player)
    local minute = { For = 60 }
    stats:set(player.entity, "bless", { stat = health, op = "Add", value = 50 }, minute)
    stats:set(player.entity, "bless", { stat = luck, op = "Add", value = 0.1 }, minute)
end)
```

## Kinds

A stat kind has a [content id](concepts/ids.md) and a `StatDef`:

| Field | Means |
| --- | --- |
| `ty` | `Additive`: points, 0 by default. `Multiplicative`: a factor, 1 by default. `Flag`: on or off. |
| `default` | the value without modifiers; `starting_at(v)` sets it |
| `min`, `max` | the range the value stays in; `range(min, max)` sets it |
| `format` | how the value reads, `Number` or `Signed`. A third format is `Percent`. |

The text that names a kind is the derived part `<name>.name` in its
owner's [texts](content/texts.md), as content derives its name. The kind
`<mod>:luck` reads `luck.name`. A stat kind, an affix and an item of one
mod with the same name share that text, so give them different names.

`load.define_stat(id!("luck"), &def)` defines a kind of the mod's own during
`init`, and a malformed name fails the build. Its full id is `<mod>:luck`, see
[how ids are namespaced](concepts/ids.md). `load.open_stat(id)` opens a
built-in kind or another mod's: `deps::<alias>::stat("luck")` names a kind of
a dependency. The dependency's
[`exports`](concepts/loader-and-abi.md#what-other-mods-may-use) must list
`stat:` with the kind's name. The name it is a part of also works:
`stat:charm` covers `charm.fire`. Otherwise `open_stat` refuses it, and the
loader names what the dependency exports. `deps::<alias>::id` names content, which `content:`
exports. Every mod that
opens one id shares its values. A kind no mod defines reads 0, and the
loader logs it. `stats.info(stat)` gives a kind's id, text key and
definition, for a menu or a tooltip.

Item stats store a kind by its key, the game id of the GUID its id turns
into, as for any [content id](concepts/ids.md). Two ids with one key
would read as one kind. So the loader turns off the mod that opens the
second, and names both ids and both mods. Affixes are checked the same way.

## Modifiers

A `Modifier` is a stat, an `Op` and a value. They come from three sources,
summed in this order:

1. **Worn items**: an item keeps its `ItemStats` as [item data](item-and-entity-data.md)
   of the platform's kind `enshwick:stats`. On the server, `stats.item(item)`
   reads it and `stats.set_item(item, &stats)` writes it. `set_item` is
   unstable, as item data is. The items an
   entity wears are its equipment: armour, rings, the ranged weapon and the
   shield. Clients that run Enshwick get every item's stats.
2. **Buffs**: `load.add_buff_modifiers(buff, &modifiers)` gives every entity
   that has the buff type the modifiers while it has it.
3. **Mods**: `stats.set(entity, key, modifier, lasting)` keeps a modifier
   under the mod's own key; the same key and stat replace the one before,
   and `stats.remove(entity, key, stat)` takes it away. `Lasting` is
   one of these:
   - `UntilRemoved`;
   - `For(duration)`, of real time;
   - `Saved`: kept with the world for the player's account and given back
     each time the player joins.

The operations apply in a fixed order, whatever order the sources give them
in:

1. every `Add`, to the default;
2. then every `Multiply`;
3. then the greatest `Max`, as a floor.

An `Override` replaces the result, and the last one in
source order wins. The kind's range applies last, and a `Flag` reads 1 for
any value but 0. `aggregate(&def, modifiers)` is the same sum, for a mod's
own use and its tests:

```rust
use enshwick::stats::{Op, StatDef, aggregate};

let health = StatDef::additive().range(-100.0, 1000.0);
// (0 + 50 + 30) * 1.5, raised to at least 100.
let value = aggregate(&health, [(Op::Add, 50.0), (Op::Multiply, 1.5), (Op::Add, 30.0), (Op::Max, 100.0)]);
assert_eq!(value, 120.0);
```

`stats.sources(entity, Some(stat))` lists the modifiers that count toward a
stat, in the order they apply. Each comes with its `Origin`. The origins are
`Item` and `Buff`. A mod's modifier has `Mod`.

## Values

`stats.value(entity, stat)` reads a value, and `stats.is_on(entity, stat)` a
flag. The server works an entity's values out after each step in which one
of its sources changed. A source changes when:

- a mod sets or removes a modifier;
- a timed one runs out;
- an item's stats change;
- what the entity wears or its buffs differ from the last look.

The server looks once a second, and at once when a player's equipment
changes. The values go out as one snapshot, so a read takes no lock and costs
a hash lookup. Reads work from any thread: a damage filter may read them. A
client reads defaults; what a player sees of a built-in kind comes through
the game itself.

The server watches players and every entity a mod keeps a modifier on. It
also watches the entities a mod asks it to watch with
`stats.watch(entity, true)`. An example is an enemy whose curse buff should
count. Others keep their defaults.

`load.on_stats_changed(|change| ...)` runs on the server's game thread after
the step that moved a value, with the entity, the stat and the values before
and after.

## Built-in kinds

The platform defines these kinds in the `enshwick` namespace and applies
them to the game on the server, the dedicated one or a hosting client:

| Kinds | Type | What the game does with them |
| --- | --- | --- |
| `health_max` | points | added to the entity's maximum as gear adds to it; clients see the new maximum |
| `stamina_max` | points | as `health_max` |
| `mana_max` | points | as `health_max` |
| `damage`, `damage_<type>` | factor | multiply the hits the entity deals, every type or one |
| `resistance`, `resistance_<type>` | part | the part of a hit's damage the entity takes less: 0.25 takes a quarter less, 1 takes none, a negative part takes more |

The values of `<type>` are these:

- `cut`
- `pierce`
- `blunt`
- `fire`
- `ice`
- `shroud`
- `lightning`
- `poison`

Damage and resistance act through the same site
as [damage filters](damage.md), before them: on hits only, after the game's
own resistances and before its armour. The attacker of a hit is the root of
what hit, so an archer's damage factors count for its arrows. Health,
stamina and mana maxima go through the game's own adders, which perks, food
and gear change the same way. So they stay as the game keeps them. The
current value follows the game's own rules.

The game's other attributes, such as movement speed, critical chance or
armour points, are not built-in kinds. Their sets work alike.

## Tooltips

What an item gives is item data, so a client that runs Enshwick knows every
item's `ItemStats`. [`affixes.lines(item)`](affixes.md#what-an-item-rolled)
lists it a modifier per line. `stats.info(stat)` names each line's text
key and format. Per-item tooltip lines need a client hook of their own. A mod
item that always gives the same can show it with
[tooltip lines](content/items.md#tooltip-lines) on its type.

## Testing

The [fake loader](testing.md) answers stats as the server does, working
values out on every read. `loader.wear(entity, item)` puts an item on an
entity. `loader.set_buffs` and the buff calls give buffs.
`loader.settle_stats()` runs the change handlers as a step would.
