# Items

[`Load::add_item`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/content/items.rs)
adds an item to the game. The new item is a clone of a vanilla one: it looks
and behaves like it, under a GUID of its own, with its own name and
description if you give them.

```rust
use enshwick_loader_api::content::Guid;
use enshwick_loader_api::content::items::NewItem;

/// The vanilla wooden chest.
const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
/// The new item, the mod's own GUID.
const SAFE: Guid = Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31");
/// The tags of its name and description.
const SAFE_NAME: Guid = Guid::parse("b7c1d2e3-4f50-4a61-8b72-c3d4e5f60718");
const SAFE_DESCRIPTION: Guid = Guid::parse("c8d2e3f4-5061-4b72-9c83-d4e5f6071829");

const TEXTS: &str = r#"
[en]
"safe.name" = "Safe"
"safe.description" = "A chest only its owner can open."
"#;

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    let texts = load.texts(TEXTS)?;
    load.add_texts(&texts, &[
        ("safe.name", SAFE_NAME),
        ("safe.description", SAFE_DESCRIPTION),
    ])?;
    load.add_item(NewItem {
        name: Some(SAFE_NAME),
        description: Some(SAFE_DESCRIPTION),
        ..NewItem::new(SAFE, CHEST, "Safe")
    })
}
```

A real mod keeps the texts in a file and embeds it with
`include_str!("texts.toml")`; see [Texts](texts.md).

## `NewItem`

| Field | Meaning |
| --- | --- |
| `guid` | The new item's GUID. Its game id is `guid.id()`. |
| `like` | The vanilla item it is cloned from: what it looks like and does. |
| `name` | The tag of its name; `None` keeps the vanilla item's name. |
| `description` | The tag of its description; `None` keeps the vanilla one. |
| `debug_name` | The name in the item registry's debug list. |
| `meter` | A count each item keeps, see [Meters](#meters); `None` for none. |
| `stats` | Lines the tooltip shows after the vanilla item's, see [Tooltip lines](#tooltip-lines). |
| `icon` | An icon of its own, see [Icons](#icons); `None` keeps the vanilla icon. |

`NewItem::new(guid, like, debug_name)` makes one with no edits but its
identity; set the other fields with struct update syntax.

`add_item` serves two resources: the new item's `ItemInfo`, built from the
vanilla one with the new id, GUID, name, description, meter and tooltip lines
written in, and the item registry with the new GUID listed after the others;
with a meter also the entity template and the template collection with it.
With an icon it serves the icon's UI image too. Everything else in the
`ItemInfo` is the vanilla item's, so a cloned chest places the same vanilla
chest.

An item whose GUID changed keeps the stacks saved with the old one through a
[migration](../migrations.md): `rename_content(old, new)` adds it under both.

The item exists once the game reads it, but nobody has one yet. Players get
it from a [recipe](recipes.md), loot, or a mod's
[`give`](../inventories.md).

## Meters

A meter is a count each item keeps on its own, from 0 to a maximum, such as
the charges of a staff. The game keeps it as it keeps the water pouch's
water: on the item's own entity, saved with the world, sent to players and
drawn as a bar on the item's slot.

```rust
use enshwick_loader_api::content::Guid;
use enshwick_loader_api::content::items::{Meter, NewItem};

const STAFF: Guid = Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31");
const VANILLA_STAFF: Guid = Guid::parse("505ccfbc-440d-4d63-a3d6-d689d3d55e78");
/// The staff entity's template: the mod's own GUID, fixed forever.
const STAFF_ENTITY: Guid = Guid::parse("0b6f3c1e-2d4a-4e5f-8a9b-1c2d3e4f5a6b");

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    load.add_item(NewItem {
        meter: Some(Meter { template: STAFF_ENTITY, max: 3, start: 3 }),
        ..NewItem::new(STAFF, VANILLA_STAFF, "Charged staff")
    })
}
```

| `Meter` field | Meaning |
| --- | --- |
| `template` | The GUID of the item entity's template, a clone of the water pouch's; saves hold it. Items with the same meter may share one. |
| `max` | The most it holds, 1 or more. |
| `start` | What a new item holds when nothing sets its fill. |

Each new item starts at `start`, or at the fill the give names: an
[`Op::Give`](../inventories.md) with `fill: Some(Fill::of(count, max))`, or a
[drop](random-loot.md) with `.meter(min, max, of)` for a count rolled as it
drops. [`World::item_meter`](../world.md) reads it and
`change_item_meter` changes it, on the server's game thread;
[`Reader::meter`](../inventories.md) reads it as clients see it, and so does
a summoning staff's [`Summon`](../survivors.md).

The item gets an entity of its own (the game's `HasPIDE`) and keeps its
count as fuel (`UsesFuel`), which is what draws the bar. An item that has an
entity already, such as a weapon with durability, cannot take a meter:
`add_item` turns the mod off.

## Tooltip lines

`stats` adds lines to the item's tooltip, after the vanilla item's, as the
game lists an armour's bonuses: each a [text tag](texts.md) and a number.

```rust
use enshwick_loader_api::content::Guid;
use enshwick_loader_api::content::items::{NewItem, Stat, StatFormat};

const RING: Guid = Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31");
const VANILLA_RING: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
const LUCK: Guid = Guid::parse("c8d2e3f4-5061-4b72-9c83-d4e5f6071829");

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    load.add_item(NewItem {
        stats: vec![Stat { label: LUCK, value: 5, format: StatFormat::Percent }],
        ..NewItem::new(RING, VANILLA_RING, "Lucky ring")
    })
}
```

`StatFormat::Number` reads `5`, `Signed` reads `+5` and `Percent` reads
`5%`. The lines are the same for every item of the type; what one item holds
of its own shows through its meter.

## Icons

The game's item icons are pictures it ships ready-made: UI images, named by
GUID, whose pixels it reads from its archive. An
[`Icon`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/content/icons.rs)
is a UI image the mod serves under a GUID of its own, made when the client
first shows it:

- `Pixels::Tint(Tint { colour, amount })` recolours the vanilla icon
  `like`: every pixel moves `amount` percent toward `colour` at its own
  brightness, so the shading stays. The pixels come from the player's own
  game files, so no game data ships with the mod;
- `Pixels::Png(include_bytes!("icon.png"))` draws the mod's own picture,
  best square, such as 256 by 256.

```rust
use enshwick_loader_api::content::Guid;
use enshwick_loader_api::content::icons::{Icon, Pixels, Tint};
use enshwick_loader_api::content::items::NewItem;

/// The Blacksmith's summoning staff; its icon is a portrait.
const VANILLA_STAFF: Guid = Guid::parse("505ccfbc-440d-4d63-a3d6-d689d3d55e78");
const STAFF: Guid = Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31");
const STAFF_ICON: Guid = Guid::parse("3c1f2a9e-7d4b-4e8a-9f60-1a2b3c4d5e6f");

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    load.add_item(NewItem {
        icon: Some(Icon {
            guid: STAFF_ICON,
            like: VANILLA_STAFF,
            pixels: Pixels::Tint(Tint { colour: [120, 170, 255], amount: 60 }),
        }),
        ..NewItem::new(STAFF, VANILLA_STAFF, "Spectral staff")
    })
}
```

`like` is the vanilla icon's GUID: an item's own GUID for an item whose icon
the game draws from its model, most items, or the GUID in its `iconImage`.

An icon shows on a client whose loader knows the build's texture code: the
loader serves the pixels, lists the icon with the game's UI images and
waits until the UI knows it before an item names it, since a UI image the
UI does not know ends the game. Anywhere else, a client of another build or
the dedicated server, the item keeps its vanilla icon. `Load::add_icon`
serves an icon alone, for an item a mod serves itself.

## GUIDs and item ids

[`Guid::parse`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/code.rs)
reads the textual form `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx` and stores the
bytes in the order the game keeps them in memory. It is a `const fn`: in a
constant, a malformed GUID fails the build.

The game numbers items, recipes and workshops by a 32-bit id, which
[`Guid::id`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/content.rs)
computes from the GUID. The id is what an inventory slot holds and what the
[inventory](../inventories.md) calls take:

```rust
use enshwick_loader_api::content::Guid;

const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
assert_eq!(CHEST.id(), 1_302_084_846);
assert_eq!(CHEST.to_string(), "6943053a-32c1-4718-ae22-e61427ddf54d");
```

Saves keep item ids, so a GUID is fixed for good once players have the item:
see [Content](../content.md#guids-are-fixed-forever).

## Errors

`add_item` returns `Err(Off)` when the loader is too old for served
resources, when the game's reflection lacks a field the clone writes, when
another mod serves the same item GUID from another vanilla item, or when the
meter's maximum is 0. A meter on an item that has an entity already turns
the mod off when the game reads the item.
[`enshwick-safes`](https://github.com/themaiby/enshwick/tree/main/mods/enshwick-safes)
adds its Safe this way.
