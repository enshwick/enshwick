# Data packs

A **data pack** adds items, recipes, workshops, loot and texts from TOML
files, with no code, and changes vanilla values such as how many of an item
one slot holds. It is a folder in `mods/` with a `pack.toml` and no DLL:

```text
mods/
  you.lanterns/
    pack.toml
    items.toml
    recipes.toml
    workshops.toml
    loot.toml
    texts.toml
    icons/
      lantern.png
```

Every file but `pack.toml` is optional. The loader runs a pack as it runs a
mod: it is in [`mods.toml`](../concepts/loader-and-abi.md) and the lock, counts in
[the join check](../concepts/join-check.md), has its log `logs/enshwick/mods/<id>.log`, and is turned off alone when it goes
wrong. What it adds goes through the same calls a mod makes (`add_item`,
`add_recipe`, `add_workshop`, `add_loot`, `add_texts`), beside vanilla
content. A pack changes vanilla values only through
[changes](#changes-of-vanilla-values), in memory: it never removes a vanilla
item, recipe or table, never gives one another id, and never writes the
game's files or saves.

A mod can carry a pack too, see [Packs in a mod](#packs-in-a-mod).

`cargo enshwick new --data you.bigger-stacks` starts a pack: the folder
`you.bigger-stacks/` with every file above, each showing its first change
commented out, the schema files that let an editor check them, and a README
with the steps to check, install and release it. `--side` sets where it runs,
both sides when left out.

## `pack.toml`

```toml
id = "you.lanterns"
version = "1.0.0"
name = "Lanterns"
description = "Coloured lanterns, crafted at a bench of their own."
authors = ["Ann"]
license = "MIT"
homepage = "https://example.com/lanterns"

[requires]
"you.trading-post" = ">=0.5, <1.0"
```

| Field | Meaning |
| --- | --- |
| `id` | `<author>.<name>` like a mod id, the folder's name; the namespace of the pack's content |
| `version` | the pack's SemVer version |
| `name`, `description`, `authors`, `license`, `homepage` | what players read, as a mod's [manifest](../releasing.md) says it |
| `side` | `both` (the default), `server` or `client` |
| `join` | whether server and players must run the same pack to play together; on unless `false`, and required for a pack that adds or changes anything players' games read (everything but loot) |
| `requires` | mods and packs it needs, with a version requirement each |

## Names and references

Every name a pack defines lives in its namespace: the item `lantern` of the
pack `you.lanterns` is the [content id](../content.md#content-ids)
`you.lockers:locker`, with the GUID `Guid::named` derives. Names are 1 to 128
of `[a-z0-9_./-]` and unique across the pack's files. A reference is one of:

- a name of the pack: `lantern`;
- another mod's content id: `you.lockers:locker`;
- a vanilla GUID: `6943053a-32c1-4718-ae22-e61427ddf54d`;
- a vanilla item's debug name, as the game's item registry lists it:
  `Craftingstation_Workbench`. Where a recipe names a workshop, a crafting
  station's debug name names the station's workshop:
  `workshop = "Prop_Production_T1_Forge"`.

The pack's own names come first, so a debug name is looked up only when the
pack defines no such name. The loader reads the debug names from the game's
archive when it loads the pack, on the server and the client alike; a name
the game lacks, or that several items carry, is a problem at its line, with
the names that look like it:

```text
error [you.lanterns] items.toml:7:1: no item "Torchh" in this pack, and no vanilla item "Torchh"; near: Torch
```

`cargo enshwick id you.lockers:locker` prints a name's GUID and game id.

## `items.toml`

```toml
[[item]]
id = "lantern"
like = "6943053a-32c1-4718-ae22-e61427ddf54d"
name = "locker.name"
description = "locker.description"
meter = { max = 3, start = 3 }
stats = [{ label = "lantern.light", value = 5, format = "percent" }]
icon = { tint = [255, 170, 60], amount = 60 }
```

Each item is a clone of `like`, as [`add_item`](items.md) makes it: `name`,
`description` and each stat's `label` are keys of `texts.toml`; `meter` gives
each item a count of its own (`start` is `max` when left out); `format` is
`number`, `signed` or `percent`. An `icon` either recolours a vanilla icon
(`tint`, `amount` in percent, `like` the icon's item, the item's `like` when
left out) or names a PNG of the pack: `icon = { png = "icons/lantern.png" }`.

## Changes of vanilla values

`items.toml` also changes values of items that exist already, vanilla or
another mod's, with a `[[change]]` table:

```toml
[[change]]
items = ["Wood_Log", "Stone_Raw", "6943053a-32c1-4718-ae22-e61427ddf54d"]
max_stack = 999

[[change]]
items = ["Resin"]
max_stack = { times = 4 }
```

`items` names the items as [references](#names-and-references) do. A number
sets the value; `{ times = x }` multiplies it, with `x` from 0 to 100.

| Field | Meaning | Values |
| --- | --- | --- |
| `max_stack` | how many of the item one inventory slot holds | 1 to 65535 |

A change that names no item, changes nothing, gives a value out of range or
changes one field of one item a second time is a problem at its line, like
every other.

### Several packs on one value

The loader takes every pack's and mod's changes of one value in load order.
The last set wins; then every factor multiplies it. A whole number rounds
half away from zero and stays in the field's range, and a factor never takes
it below the smallest value. Two factors never conflict, since their order
does not matter. A set that overrides another mod's set is a warning in both
logs and in the conflict report, naming both values and both lines; when the
later pack [requires](#packtoml) the other, it overrides it on purpose and
the line is information:

```text
warn  [you.stacks] max_stack of 6943053a-32c1-4718-ae22-e61427ddf54d is 999 from you.stacks items.toml:2 (Wood_Log), which overrides 500 from ann.bags items.toml:7 (Wood_Log)
```

A Rust mod declares the same changes with
[`Load::change`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content/changes.rs),
during `init`.

### Stack sizes and saved worlds

The game cuts every stack above an item's stack size when a world loads and
deletes the rest. So a world saved with a pack that raised a stack size
depends on that pack. The first time such a world loads, the pack sets a
[hold](../state.md#holds): if the world is opened again without the pack, the game
stops before the world loads and the log says which pack and items:

```text
error you.stacks holds this game (it lets Wood_Log and 12 more items stack past the game's own size (20 to 999); ...) and the mod does not run: the game stops before the world loads
```

To remove such a pack safely, split every stack larger than the game's size
first, then delete `mod-data/<id>/hold.txt`. When several packs raise one
item, each of them holds. A pack that only lowers stack sizes sets no hold;
while it runs, the game cuts larger stacks to its value.

## `workshops.toml`

```toml
[[workshop]]
id = "lantern-bench"
like = "workbench"
name = "lantern-bench.name"
recipe = { cost = 2 }
```

A station with a workshop of its own, as [`add_workshop`](workshops.md) makes
it: the station item is `<id>`, its template `<id>.template`, its workshop
`<id>.workshop`, and with `recipe` it is crafted like the vanilla station for
`cost` times its ingredients. `like` names the vanilla station cloned:
`workbench`, or any crafting station by its item's debug name or GUID, such
as `Prop_Production_T1_Forge`, `Prop_Production_T2_Kiln` or
`Prop_Production_T3_Alchemy_Station`. The loader checks that the game lists
a workshop of its own for the item, that it is no survivor's, and that the
template the item places has the crafting components; any other item is a
problem at its line.

## `recipes.toml`

```toml
[[recipe]]
id = "lantern.recipe"
like = "6943053a-32c1-4718-ae22-e61427ddf54d"
output = "lantern"
count = 1
cost = 2
workshop = "lantern-bench"
```

A copy of the recipe that makes `like`, making `count` of `output` for `cost`
times the copied ingredients, as [`add_recipe`](recipes.md) makes it.
`workshop` names a workshop of the pack, another by GUID or content id, or a
vanilla station's by the station item's debug name; the copied recipe's when
left out.

## `loot.toml`

```toml
[[drop]]
items = ["lantern"]
chance = 0.05
from = "locked-chests"
count = [1, 2]
meter = [1, 3]
```

Each drop gives one of `items` at `chance` per roll, as
[`add_loot`](random-loot.md) adds it. `from` is `bosses`, `locked-chests`,
`corpses`, `chests`, `containers` or `destroyed`, or a list of the last four.
`meter` rolls the fill of items with a meter: every item of the drop must be
one of the pack's own with a meter of the same maximum, and the range must not
pass that maximum.

## `texts.toml`

The pack's texts, in the form of a mod's [texts file](texts.md): a table per
language, `en` the fallback. A server owner's `mod-configs/<id>.texts.toml`
overrides them as it does a mod's.

## Problems

The loader checks every file before the pack adds anything: unknown fields,
names defined twice, references to names neither the pack nor the game has,
stations that cannot be cloned, text keys without text, icons the pack
lacks, chances outside 0 to 1, changes out of range or made twice, and
`join = false` on a pack that players' games read. Each problem is a line
in the pack's log with its file, line and column, and the pack stays off:

```text
error [you.lanterns] items.toml:7:1: no item "torch" in this pack; name another mod's as <mod>:<name>, or a vanilla one by its GUID or, where the game's data is at hand, its debug name
```

`cargo enshwick pack check mods/you.lanterns --game <game folder>` gives the
same lines before you ship, reading vanilla names from that game's data
(`ENSHROUDED_DIR` when `--game` is left out). Without a game folder it
checks everything else, and a debug name or a station other than the
workbench is a problem that asks for the game's data. `cargo enshwick pack schema` writes the JSON Schema of
every file but `texts.toml`; name one at the top of a file, `#:schema ./items.schema.json`, and an
editor with TOML support completes and checks fields as you type.

## Changes while the server runs

The dedicated server watches a pack's files. When they change it reads them
again and logs any problem with its line; with none, it applies the new
`loot.toml` at once: chances, counts and items of drops change for the next
roll. Items, recipes, workshops, texts and changes of values are built into
the game's registries at start, on the server and on every player's client,
so a change to them waits for the next start, and the log says so:

```text
info  [you.lanterns] data pack reloaded: 2 loot drops
warn  [you.lanterns] data pack changed beyond its loot: items, recipes, workshops, texts and changes of values change at the next start
```

Loot of a kind no pack or mod had at start, such as the first drop from
destroyed objects, also waits for a restart.

## Releases and the join check

A pack is released like a mod: an archive `<component>.zip` holding the
pack's files under `mods/<id>/`. `cargo enshwick pack zip mods/you.lanterns --out
lanterns.zip` checks the pack and writes the archive; attach it to the
release, as [Releasing mods](../releasing.md) describes.

In the join check a pack's version carries the hash of its files,
`lanterns@1.0.0+3f2a9c1e`: a server and a player with other files of one
version differ, and the player's game stays out of the server.

## Packs in a mod

A mod adds a pack it embeds with
[`Load::add_pack`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/packs.rs);
the names live in the mod's namespace:

```rust
use enshwick::packs::{Pack, PackFiles};

// In a mod: include_str!("pack/items.toml"), and so on.
const ITEMS: &str = r#"
[[item]]
id = "lantern"
like = "6943053a-32c1-4718-ae22-e61427ddf54d"
name = "locker.name"
"#;
const TEXTS: &str = "[en]\n\"locker.name\" = \"Locker\"\n";

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let files = PackFiles::new().with("items.toml", ITEMS).with("texts.toml", TEXTS);
    load.add_pack(&Pack::parse("you.my-mod", &files)?)
}
```

`Pack::parse` returns every problem with its file and line, which `?` turns
into the mod's off reason. A mod's pack is not watched: change it and rebuild.
