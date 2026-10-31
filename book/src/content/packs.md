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

A mod can carry a pack too, see [Packs in a mod](#packs-in-a-mod). A pack
with a `main.luau` is a [Luau mod](../luau.md).

The repository's `examples/` folder holds three packs to start from:
`you.material-stacks`, `you.recipe-cost-and-time` and `you.storage-slots`.

`cargo enshwick new --data you.material-stacks` starts a pack: the folder
`you.material-stacks/` with every file above, each showing its first change
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
market = { id = "you.trading-post", version = ">=0.5, <1.0" }

[after]
lockers = "you.lockers"
```

| Field | Meaning |
| --- | --- |
| `id` | `<author>.<name>` like a mod id, the folder's name; the namespace of the pack's content |
| `version` | the pack's SemVer version |
| `name`, `description`, `authors`, `license`, `homepage` | what players read, as a mod's [manifest](../releasing.md) says it |
| `side` | `both` (the default), `server` or `client` |
| `join` | whether server and players must run the same pack to play together; on unless `false`, and required for a pack that adds or changes anything players' games read (everything but loot) |
| `requires`, `after`, `recommends`, `breaks` | the mods and packs it relates to, as a Rust mod's [manifest](../concepts/loader-and-abi.md#the-manifest) names them: tables keyed by an alias of `[a-z][a-z0-9_]*`, each a mod id or `{ id, version }`; `after` takes no version. An alias is unique across the four tables, and neither `vanilla` nor a Luau keyword |
| `replaces` | the ids of mods the pack takes the place of |
| `exports` | the local names of the events and other names its scripts offer other mods, such as `["greeted"]` for `<id>:greeted`; the loader refuses other mods the rest (see [what other mods may use](../concepts/loader-and-abi.md#what-other-mods-may-use)) |
| `enshwick` | the Enshwick releases the pack is made for, as a version requirement such as `"=1.0.0-alpha.12"`; a loader of another release skips the pack and logs both versions. Empty for any |
| `[[setting]]` | a setting of a [Luau mod](../luau.md#settings), one table each |

## Names and references

Every name a pack defines lives in its namespace: the item `lantern` of the
pack `you.lanterns` is the [content id](../content.md#content-ids)
`you.lanterns:lantern`, with the GUID `Guid::named` derives. Names are 1 to 128
of `[a-z0-9_./-]` and unique across the pack's files. A reference is one of:

- a name of the pack: `lantern`;
- a required mod's name after the alias `requires` gives it: with
  `lockers = "you.lockers"` in `[requires]`, `lockers:locker` names
  `you.lockers:locker`. A mod of `after` or `recommends` may not run, so its
  alias in a reference is a problem at its line;
- another mod's content id in full: `you.lockers:locker`;
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
error [you.lanterns] items.toml:7:1: no item "Tool_Torch1" in this pack, and no vanilla item "Tool_Torch1"; near: Tool_Torch01
```

A change is different. When a change names an item or recipe that the game
no longer has, and no name looks like it, the loader skips that part of the
change and logs a warning. The rest of the pack still applies, so a game
update that removes one item does not turn off the whole pack:

```text
warn  [you.stacks] items.toml:3:1: change 1: no vanilla item "Weapon_Old_Sword" in this game build; skipped
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
items = ["Material_T1_Wood_Logs", "Build_TerrainMaterial_T1_Stone", "6943053a-32c1-4718-ae22-e61427ddf54d"]
max_stack = 999

[[change]]
items = ["Material_T1_Resin"]
max_stack = { times = 4 }
```

`items` names the items as [references](#names-and-references) do. A number
sets the value; `{ times = x }` multiplies it, with `x` from 0 to 100.

| Field | Meaning | Values |
| --- | --- | --- |
| `max_stack` | how many of the item one inventory slot holds | 1 to 65535 |
| `slots` | how many slots the container the item places holds, or the backpack it is, never fewer than it has | 8 to 128, in steps of 8 |
| `damage_mod` | a weapon's damage factor | 0 to 100 |
| `speed_mod` | a weapon's attack speed factor | 0 to 10 |
| `armor` | every armour value of the item, scaled only: `{ times = x }` | a factor |
| `burn_time` | how long the item burns as fuel, in seconds | 0 to 86400 |
| `energy_output` | the energy the item gives as fuel | 0 to 1000000 |
| `comfort_amount` | the comfort the placed item gives | 0 to 1000 |

A change that names no item, changes nothing, gives a value out of range or
changes one field of one item a second time is a problem at its line, like
every other.

A stack size or container slots can also change for every vanilla item a
selection picks, read from the game's data when the game starts:

```toml
[[change]]
category = ["Materials", "Ammunition"]
max_stack = { times = 4 }

[[change]]
items = "stackable"
max_stack = { times = 2 }

[[change]]
items = "containers"
slots = { times = 2 }

[[change]]
items = ["Material_T1_Wood_Logs"]
max_stack = 999
```

| Selection | Picks |
| --- | --- |
| `category = [...]` | every item of these categories, by the game's names: `Customization`, `Equipment`, `Weapons`, `Tools`, `Instrument`, `FishingRod`, `BuildTools`, `Consumables`, `Ammunition`, `Materials`, `Blueprints`, `Currency`, `Collectibles`, `AnimalFood`, `PetFood`, `WeaponGem`, `ColorPalette` |
| `items = "stackable"` | every item one slot already holds more than one of |
| `items = "containers"` | every item that places a container |
| `items = "backpacks"` | every backpack, an item worn in the game's `Backpack` slot |

A selection changes only `max_stack` and `slots`. A stack size reaches only
items that stack and keep no state of their own, such as durability, so
weapons and armour stay at one; slots reach only items that place a
container or are backpacks. A backpack's size reaches only backpacks made
after the change: one a player has keeps the size it was made with, which
the save holds. An item the pack also names in `items` keeps that change alone,
so the last table above sets Wood Logs to 999 while other materials take
four times their size. A change gives `items` or `category`, never both. The
pack's log says how many items each selection reached.

The stats change only on items that have them: a `damage_mod` on an item
that deals no damage is left, and the pack's log says so. A weapon deals the
changed damage once the game sets it up again; the number its tooltip shows
was written when the item was made and stays.

`slots` changes the chests placed from then on, and chests upgraded to the
item, since an upgrade places a new chest: a chest placed before keeps the
size it was placed with, which the save holds. Removing the pack leaves every
chest as it is. A container only grows, as an upgraded chest moves every item
it holds; an item that places no container is a problem at its line. The
change reaches every item that places the same container, and the log names
them; when such items ask for different sizes, the last in load order counts
and the log says so. The largest vanilla chest holds 72 slots.

### Recipes

`recipes.toml` changes vanilla recipes the same way:

```toml
[[change]]
recipes = ["Tool_Torch01_T0_PK"]
output_count = 3

[[change]]
output = ["Material_T2_Wood_Planks"]
cost = { times = 0.5 }
time = { times = 0.5 }

[[change]]
output = ["Tool_Torch01"]
ingredients = { Material_T1_Wood_Logs = 1, Material_T1_Resin = 0, Build_TerrainMaterial_T1_Stone = 2 }
```

`recipes` names vanilla recipes by GUID or by debug name. `output` names
items and picks every vanilla recipe that makes one of them. A change gives
at least one of the two. A recipe a mod or the pack adds gets its values
where it is defined, so naming one here is a problem.

| Field | Meaning | Values |
| --- | --- | --- |
| `output_count` | how many of each item the recipe makes | 1 to 65535 |
| `cost` | how many of each ingredient it takes | 1 to 65535 |
| `time` | how long it takes, in seconds; 0 is at once | 0 to 86400 |
| `ingredients` | how many of each named ingredient it takes: 0 takes it out, and one the recipe lacks is added | 0 to 65535 each |
| `station` | the crafting station it is made at, named by the station's item | a vanilla station |

`cost` applies first, then `ingredients`, so a named ingredient wins over the
cost. A scaled count of 1 or more stays at least 1. An ingredient that takes
any item of a kind keeps its count. Most workbench recipes
take no time; production stations such as the kiln do. A changed time
reaches a production that runs already from its next step on.

A change can also pick every vanilla recipe of some kinds, by what it
makes, or every vanilla recipe made at some crafting stations, named by
their items:

```toml
[[change]]
kind = ["building-block", "roof"]
cost = { times = 0.5 }

[[change]]
workshop = ["Prop_Production_T2_Kiln", "Prop_Production_T2_Smelter"]
time = { times = 0.5 }
```

| Selection | Picks |
| --- | --- |
| `kind = [...]` | every recipe whose item is of these [kinds](recipes.md#recipe-kinds): `building-block`, `roof`, `overgrowth`, `terrain`, `decorative`, `comfort-furniture` or `other` |
| `workshop = [...]` | every recipe made at these stations; naming one needs the game's data at hand |

A selection changes `output_count`, `cost`, `time` and `station`, never
`ingredients`.
A change gives one of `recipes` or `output`, `kind` and `workshop`. A recipe
the pack also names in `recipes` or `output` keeps that change alone, and
the pack's log says how many recipes each selection reached.

`station` moves a recipe to another crafting station, such as charcoal
from the kiln to the smelter:

```toml
[[change]]
recipes = ["Material_T1_Factory_03_Charcoal"]
station = "Prop_Production_T2_Smelter"
```

The player's crafting menu then lists the recipe at the new station alone.
A move takes effect when the game starts, also in a development reload, and
needs the game's data at hand to find the station. Two packs that move one
recipe are a conflict like two sets: the later in load order wins.

The server crafts by its own recipes and each player's crafting menu shows
the player's own, so every side runs the same pack, which the join check
proves. [Game rates](../rates.md) multiply what a pack sets.

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
warn  [you.stacks] max_stack of 3f4093ae-75ed-4e5d-bd0b-784ba83b88f5 is 999 from you.stacks items.toml:2 (Material_T1_Wood_Logs), which overrides 500 from ann.bags items.toml:7 (Material_T1_Wood_Logs)
```

A Rust mod declares the same changes in its code; see
[Vanilla value changes from a mod](limits.md).

### Stack sizes and saved worlds

Unstable: stack sizes and slots may change in any release; a mod changing them
needs the `unstable` feature ([Stability](../stability.md)).

A world saved with a pack that raised a stack size depends on that pack: the
pack holds that world, and the loader keeps the raised size;
[Stack sizes and saved worlds](limits.md#stack-sizes-and-saved-worlds) tells
both and how they meet. To remove such a pack safely, split every stack larger
than the game's size first, then delete
`mod-data/<id>/worlds/<key>/hold.txt`, the file the log names.

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
`cost` times its ingredients. The texts of `name` and `description` take
the tags `<id>.name` and `<id>.description`, while an item's texts keep the
tag `<pack>:text/<key>` of their key. `like` names the vanilla station cloned:
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

`loot.toml` also changes how the game's own loot gives items that exist
already, with a `[[change]]` table:

```toml
[[change]]
items = ["Material_T1_Wood_Logs", "Material_T1_Resin"]
stack_size = { times = 2 }
count = { times = 2 }
```

| Field | Meaning | Values |
| --- | --- | --- |
| `stack_size` | how many of the item random loot gives in one stack, at least and at most alike | 1 to 65535 |
| `count` | how many of the item a container's fill table gives, at least and at most alike | 0 to 65535 |
| `spawn_probability` | the chance a fill table gives the item | 0 to 1 |
| `weight` | the item's weight among the ones a fill table picks from | 0 to 1000000 |

Random loot fills world chests, corpses and animals by the item's labels;
fill tables are the fixed lists a container names. A change reaches every
fill table that lists the item. Only the server rolls loot, so a pack of
loot changes alone may run `side = "server"` and turn `join` off.

`table` narrows a change to the fill tables of some containers, named by
their templates, and needs the game's data at hand:

```toml
[[change]]
items = ["Material_T1_Resin"]
table = ["LootContainer_Enemy_Corpse_Vukah"]
count = { times = 2 }
```

Containers often share one table: the Vukah corpse and the Vukah warrior
roll the same one, so the change above reaches both, and the change's
source in the conflict report and the logs says how many containers share
it. A change of one table applies after the item's changes of every table.
`stack_size` belongs to random loot, which has no tables.

A pack changes the table itself, before any roll, and each factor after it
applies once. For items the game scales, the world's resource drop setting
times the [resource drop rate](../rates.md) scales the pack's least and
largest stack, up to a ceiling that `stack_size` keeps in proportion; the
loot rate then scales the rolled stack, held to the item's stack size, which
an `items.toml` change may have raised.

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
error [you.lanterns] items.toml:7:1: no item "torch" in this pack; name a required mod's as <alias>:<name>, or a vanilla one by its GUID or, where the game's data is at hand, its debug name
```

`cargo enshwick pack check mods/you.lanterns --game <game folder>` gives the
same lines before you ship, reading vanilla names from that game's data
(`ENSHROUDED_DIR` when `--game` is left out). Without a game folder it
checks everything else, and a debug name or a station other than the
workbench is a problem that asks for the game's data. `cargo enshwick pack schema` writes the JSON Schema of
every file but `texts.toml`; name one at the top of a file, `#:schema ./items.schema.json`, and an
editor with TOML support completes and checks fields as you type.

## Changes while the game runs

The dedicated server watches a pack's files. When they change it reads them
again and logs any problem with its line; with none, it applies the new
`loot.toml` at once: chances, counts and items of drops change for the next
roll. Items, recipes, workshops, texts and changes of values are built into
the game's data at start, on the server and on every player's client, so a
change to them waits for the next start, and the log says so:

```text
info  [you.lanterns] data pack reloaded: 2 loot drops
warn  [you.lanterns] data pack changed beyond its loot: changes of values change at the next start, or at once with dev_reload = true
```

Loot of a kind no pack or mod had at start, such as the first drop from
destroyed objects, also waits for a restart.

### While you make a pack

With `dev_reload = true` in `mod-configs/enshwick.toml` (see [reloading a mod](../testing.md#reloading-a-mod-while-the-game-runs)),
the server and the client both watch the packs' files, and a change that
only moves values applies at once: stack sizes, recipe counts and times, and
chest slots. The loader merges every pack's changes again, reports
overridden values as at start, and builds each changed resource again in
place:

```text
info  [you.stacks] data pack reloaded: 3 changes of values, 4 resources built again
```

Some changes still take a restart, and the log names them: the first change
of an item or recipe that no change touched at start, and an ingredient taken
out or added, which changes the recipe list's length. Chest slots apply to
chests placed after the reload. On a server, players who joined before the
change keep the old values until they join again, so reload on a test server
of your own.

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
It has no `pack.toml`, so it names another mod's content by its full id.
