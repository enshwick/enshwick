# Items

[`Load::add_item`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content/items.rs)
adds an item to the game. The new item is a clone of a vanilla one: it looks
and behaves like it, under a [content id](../content.md#content-ids) of its
own, with its own name and description if you give them.

```rust
use enshwick::content::{Guid, Id};
use enshwick::content::items::NewItem;

/// The vanilla wooden chest.
const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
/// The new item.
const LOCKER: Id = Id::new("you.lockers:locker");
/// The tags of its name and description.
const LOCKER_NAME: Id = Id::new("you.lockers:locker.name");
const LOCKER_DESCRIPTION: Id = Id::new("you.lockers:locker.description");

const TEXTS: &str = r#"
[en]
"locker.name" = "Locker"
"locker.description" = "A chest only its owner can open."
"#;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let texts = load.texts(TEXTS)?;
    load.add_texts(&texts, &[
        ("locker.name", LOCKER_NAME),
        ("locker.description", LOCKER_DESCRIPTION),
    ])?;
    load.add_item(NewItem {
        name: Some(LOCKER_NAME.guid()),
        description: Some(LOCKER_DESCRIPTION.guid()),
        ..NewItem::new(LOCKER, CHEST)
    })
}
```

A real mod keeps the texts in a file and embeds it with
`include_str!("texts.toml")`; see [Texts](texts.md).

## `NewItem`

| Field | Meaning |
| --- | --- |
| `id` | The new item's content id. Its game id is `id.id()`. |
| `like` | The vanilla item it is cloned from: what it looks like and does. |
| `name` | The tag of its name; `None` keeps the vanilla item's name. |
| `description` | The tag of its description; `None` keeps the vanilla one. |
| `debug_name` | The name in the item registry's debug list; the id unless set. |
| `meter` | A count each item keeps, see [Meters](#meters); `None` for none. |
| `stats` | Lines the tooltip shows after the vanilla item's, see [Tooltip lines](#tooltip-lines). |
| `icon` | An icon of its own, see [Icons](#icons); `None` keeps the vanilla icon. |

`NewItem::new(id, like)` makes one with no edits but its identity; set the
other fields with struct update syntax. `add_item` declares the item's id,
its meter template's and its icon's.

`add_item` serves two resources: the new item's data, built from the
vanilla one with the new id, GUID, name, description, meter and tooltip lines
written in, and the item registry with the new GUID listed after the others;
with a meter also the entity template and the template collection with it.
With an icon it serves the icon's UI image too. Everything else in the
item data is the vanilla item's, so a cloned chest places the same vanilla
chest.

An item whose GUID changed keeps the stacks saved with the old one through a
[migration](../migrations.md): `rename_content(old, new)` adds it under both.
Old stacks keep the old game id, so compare item ids through
[`migrations::current_id`](../migrations.md#items-under-their-old-ids).

The item exists once the game reads it, but nobody has one yet. Players get
it from a [recipe](recipes.md), loot, or a mod's
[`give`](../inventories.md).

## Meters

A meter is a count each item keeps on its own, from 0 to a maximum, such as
the charges of a staff. The game keeps it as it keeps the water pouch's
water: on the item's own entity, saved with the world, sent to players and
drawn as a bar on the item's slot.

```rust
use enshwick::content::{Guid, Id};
use enshwick::content::items::{Meter, NewItem};

const STAFF: Id = Id::new("my-mod:charged-staff");
const VANILLA_STAFF: Guid = Guid::parse("505ccfbc-440d-4d63-a3d6-d689d3d55e78");
/// The staff entity's template, fixed forever.
const STAFF_ENTITY: Id = Id::new("my-mod:charged-staff.entity");

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.add_item(NewItem {
        meter: Some(Meter { template: STAFF_ENTITY, max: 3, start: 3 }),
        ..NewItem::new(STAFF, VANILLA_STAFF)
    })
}
```

| `Meter` field | Meaning |
| --- | --- |
| `template` | The content id of the item entity's template, a clone of the water pouch's; saves hold its GUID. Items with the same meter may share one. |
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
use enshwick::content::{Guid, Id};
use enshwick::content::items::{NewItem, Stat, StatFormat};

const RING: Id = Id::new("my-mod:lucky-ring");
const VANILLA_RING: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
const LUCK: Id = Id::new("my-mod:lucky-ring.luck");

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let texts = load.texts("[en]\nluck = \"Luck\"\n")?;
    load.add_texts(&texts, &[("luck", LUCK)])?;
    load.add_item(NewItem {
        stats: vec![Stat { label: LUCK.guid(), value: 5, format: StatFormat::Percent }],
        ..NewItem::new(RING, VANILLA_RING)
    })
}
```

`StatFormat::Number` reads `5`, `Signed` reads `+5` and `Percent` reads
`5%`. The lines are the same for every item of the type; what one item holds
of its own shows through its meter.

## Icons

The game's item icons are pictures it ships ready-made: UI images, named by
GUID, whose pixels it reads from its archive. An
[`Icon`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content/icons.rs)
is a UI image the mod serves under a content id of its own, made when the
client first shows it:

- `Pixels::Tint(Tint { colour, amount })` recolours the vanilla icon
  `like`: every pixel moves `amount` percent toward `colour` at its own
  brightness, so the shading stays. The pixels come from the player's own
  game files, so no game data ships with the mod;
- `Pixels::Png(include_bytes!("icon.png"))` draws the mod's own picture,
  best square, such as 256 by 256.

```rust
use enshwick::content::{Guid, Id};
use enshwick::content::icons::{Icon, Pixels, Tint};
use enshwick::content::items::NewItem;

/// The Blacksmith's summoning staff; its icon is a portrait.
const VANILLA_STAFF: Guid = Guid::parse("505ccfbc-440d-4d63-a3d6-d689d3d55e78");
const STAFF: Id = Id::new("my-mod:spectral-staff");
const STAFF_ICON: Id = Id::new("my-mod:spectral-staff.icon");

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.add_item(NewItem {
        icon: Some(Icon {
            id: STAFF_ICON,
            like: VANILLA_STAFF,
            pixels: Pixels::Tint(Tint { colour: [120, 170, 255], amount: 60 }),
        }),
        ..NewItem::new(STAFF, VANILLA_STAFF)
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

[`Guid::parse`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/code.rs)
reads the textual form `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx` and stores the
bytes in the order the game keeps them in memory. It is a `const fn`: in a
constant, a malformed GUID fails the build.

The game numbers items, recipes and workshops by a 32-bit id, which
[`Guid::id`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content.rs)
computes from the GUID. The id is what an inventory slot holds and what the
[inventory](../inventories.md) calls take:

```rust
use enshwick::content::Guid;

const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
assert_eq!(CHEST.id(), 1_302_084_846);
assert_eq!(CHEST.to_string(), "6943053a-32c1-4718-ae22-e61427ddf54d");
```

Saves keep item ids, so a content id is fixed for good once players have the
item: see [Content ids](../content.md#content-ids).

## Errors

`add_item` returns `Err(Off)` when the game's reflection lacks a field the clone writes, when
another mod serves the same item GUID from another vanilla item, when the
meter's maximum is 0, or when the loader refuses one of its content ids. An
item whose game id a vanilla item has turns the mod off when the game first
reads the registry, with both items named in the mod's log. A meter on an item that has an entity already turns
the mod off when the game reads the item.
[`enshwick-safes`](https://github.com/enshwick/mod-personal-safes)
adds its Safe this way.
