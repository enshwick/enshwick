# Content packs

A **content pack** adds items, recipes, workshops, loot and texts from TOML
files, with no code. It is a folder in `mods/` with a `pack.toml` and no DLL:

```text
mods/
  lanterns/
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
[the join check](../concepts/join-check.md), is updated by the updater, has
its log `logs/enshwick/mods/<id>.log`, and is turned off alone when it goes
wrong. What it adds goes through the same calls a mod makes (`add_item`,
`add_recipe`, `add_workshop`, `add_loot`, `add_texts`), beside vanilla
content: a pack never changes a vanilla item, recipe or table.

A mod can carry a pack too, see [Packs in a mod](#packs-in-a-mod).

## `pack.toml`

```toml
id = "lanterns"
version = "1.0.0"
name = "Lanterns"
description = "Coloured lanterns, crafted at a bench of their own."
authors = ["Ann"]
license = "MIT"
homepage = "https://example.com/lanterns"
updates = "github:ann/lanterns:lanterns"
signed_by = ["RWQ..."]

[requires]
safes = ">=0.5, <1.0"
```

| Field | Meaning |
| --- | --- |
| `id` | 1 to 64 of `[a-z0-9-]`, the folder's name; the namespace of the pack's content |
| `version` | the pack's SemVer version |
| `name`, `description`, `authors`, `license`, `homepage` | what players read, as a mod's [manifest](../releasing.md) says it |
| `side` | `both` (the default), `server` or `client` |
| `join` | whether server and players must run the same pack to play together; on unless `false` |
| `updates` | where releases come from: `github:<owner/name>:<component>` or an `https://` document, as a mod's `updates:` |
| `signed_by` | the minisign keys the author signs releases with, at most two |
| `requires` | mods and packs it needs, with a version requirement each |

## Names and references

Every name a pack defines lives in its namespace: the item `lantern` of the
pack `lanterns` is the [content id](../content.md#content-ids)
`lanterns:lantern`, with the GUID `Guid::named` derives. Names are 1 to 128
of `[a-z0-9_./-]` and unique across the pack's files. A reference is one of:

- a name of the pack: `lantern`;
- another mod's content id: `safes:safe`;
- a vanilla GUID: `6943053a-32c1-4718-ae22-e61427ddf54d`.

`cargo enshwick id lanterns:lantern` prints a name's GUID and game id.

## `items.toml`

```toml
[[item]]
id = "lantern"
like = "6943053a-32c1-4718-ae22-e61427ddf54d"
name = "lantern.name"
description = "lantern.description"
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
`cost` times its ingredients. `like` names the station cloned: `workbench`.

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
`workshop` names a workshop of the pack, or another by GUID or content id;
the copied recipe's when left out.

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
`meter` rolls the fill of items with a meter, all of the same maximum.

## `texts.toml`

The pack's texts, in the form of a mod's [texts file](texts.md): a table per
language, `en` the fallback. A server owner's `mod-configs/<id>.texts.toml`
overrides them as it does a mod's.

## Problems

The loader checks every file before the pack adds anything: unknown fields,
names defined twice, references to names the pack lacks, text keys without
text, icons the pack lacks, chances outside 0 to 1. Each problem is a line
in the pack's log with its file, line and column, and the pack stays off:

```text
error [lanterns] items.toml:7:1: no item "torch" in this pack; name another mod's as <mod>:<name> or a vanilla one by its GUID
```

`cargo enshwick pack check mods/lanterns` gives the same lines before you
ship. `cargo enshwick pack schema` writes the JSON Schema of each file;
name one at the top of a file, `#:schema ./items.schema.json`, and an
editor with TOML support completes and checks fields as you type.

## Changes while the server runs

The dedicated server watches a pack's files. When they change it reads them
again and logs any problem with its line; with none, it applies the new
`loot.toml` at once: chances, counts and items of drops change for the next
roll. Items, recipes, workshops and texts are built into the game's
registries at start, on the server and on every player's client, so a change
to them waits for the next start, and the log says so:

```text
info  [lanterns] content pack reloaded: 2 loot drops
warn  [lanterns] content pack changed beyond its loot: items, recipes, workshops and texts change at the next start
```

Loot of a kind no pack or mod had at start, such as the first drop from
destroyed objects, also waits for a restart.

## Releases and the join check

A pack is released like a mod: an archive `<component>.zip` holding the
pack's files under `mods/<id>/`, signed with minisign by a key `signed_by`
names. `cargo enshwick pack zip mods/lanterns --out lanterns.zip` checks the
pack and writes the archive; sign it and attach both to the release, as
[Releasing and updates](../releasing.md) describes. The updater installs a
pack's files into its folder only, never into another pack's or the
loader's.

In the join check a pack's version carries the hash of its files,
`lanterns@1.0.0+3f2a9c1e`: a server and a player with other files of one
version differ, and the updater offers the server's release.

## Packs in a mod

A mod adds a pack it embeds with
[`Load::add_pack`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/packs.rs);
the names live in the mod's namespace:

```rust
use enshwick_loader_api::packs::{Pack, PackFiles};

// In a mod: include_str!("pack/items.toml"), and so on.
const ITEMS: &str = r#"
[[item]]
id = "lantern"
like = "6943053a-32c1-4718-ae22-e61427ddf54d"
name = "lantern.name"
"#;
const TEXTS: &str = "[en]\n\"lantern.name\" = \"Lantern\"\n";

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    let files = PackFiles::new().with("items.toml", ITEMS).with("texts.toml", TEXTS);
    load.add_pack(&Pack::parse("my-mod", &files)?)
}
```

`Pack::parse` returns every problem with its file and line, which `?` turns
into the mod's off reason. A mod's pack is not watched: change it and rebuild.
