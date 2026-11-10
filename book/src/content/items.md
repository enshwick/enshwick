<!--
type: how-to
description: Add an item to the game with Load::add_item: a clone of a vanilla item that looks and behaves like it, under a content id of its own, with its own name.
-->

# Items

[`Load::add_item`](https://github.com/enshwick/enshwick/blob/main/crates/internal/core/src/content/items.rs)
adds an item to the game. The item clones a vanilla one under a
[content id](../concepts/ids.md) of its own, and looks and behaves
like it. It takes
its own name and description if you give them.

## A first item

```rust
use enshwick::content::{Id, id, vanilla};
use enshwick::content::items::NewItem;

/// The new item.
const LOCKER: Id = id!("locker");
/// The tags of its name and description.
const LOCKER_NAME: Id = LOCKER.part("name");
const LOCKER_DESCRIPTION: Id = LOCKER.part("description");

const TEXTS: &str = r#"
[en]
"locker.name" = "Locker"
"locker.description" = "A chest only its owner can open."
"#;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    // The vanilla storage box, by the debug name the item registry gives it.
    let storage_box = vanilla::item("Prop_Storage_24_T1_Box")?;
    let texts = load.texts_from(TEXTS)?;
    load.add_texts(&texts, &[
        ("locker.name", LOCKER_NAME),
        ("locker.description", LOCKER_DESCRIPTION),
    ])?;
    load.add_item(NewItem {
        name: Some(LOCKER_NAME.guid()),
        description: Some(LOCKER_DESCRIPTION.guid()),
        ..NewItem::new(LOCKER, storage_box)
    })
}
```

```luau
local LOCKER = "you.lockers:locker"

local texts = enshwick.i18n.texts_from([[
[en]
"locker.name" = "Locker"
"locker.description" = "A chest only its owner can open."
]])
enshwick.i18n.add_texts(texts, {
    { "locker.name", `{LOCKER}.name` },
    { "locker.description", `{LOCKER}.description` },
})
enshwick.content.add_item({
    id = LOCKER,
    -- The vanilla storage box, by the debug name the item registry gives it.
    like = enshwick.content.item("Prop_Storage_24_T1_Box"),
    name = enshwick.content.guid(`{LOCKER}.name`),
    description = enshwick.content.guid(`{LOCKER}.description`),
})
```

A real mod keeps the texts in `src/texts.toml` and reads them with
`load.texts()`; see [Texts](texts.md).
[`vanilla::item`](https://github.com/enshwick/enshwick/blob/main/crates/internal/core/src/content/vanilla.rs)
names a vanilla item by its debug name.
[`deps!`](../concepts/loader-and-abi.md#the-manifest) offers it too, as
`deps::vanilla::item`. A name the game lacks turns the mod off, with the names
that look like it in the log.

### Find an item by the name the game shows

A player knows an item by the name the game shows, not by its debug name.
Three calls of `content::vanilla` serve a picker, a list or a typed name.
Luau has them under `enshwick.content`, by the same names.

| Call | Answers |
| --- | --- |
| `item_by_display_name("Glider")` | the item the game shows by that name, in any language it ships and whatever the case |
| `items()` | every vanilla item: its GUID, its debug name and the tag of its shown name |
| `recipes()` | every vanilla recipe: its GUID, its debug name and the item it makes |

An unknown name answers `Unknown`, with the names that look like it. A name
the game shows for several items answers `Ambiguous`, with those items. The
names come from the game's texts. Only a client's archive holds them: the
loader finds no text table in the archive of server 26430253. So the lookup answers on a client, in single player and in a
game a player hosts. On a dedicated server it answers `Unreadable`; there a
mod names an item by its debug name or GUID.

```rust
use enshwick::content::vanilla::{self, VanillaError};

fn find(name: &str) -> Option<enshwick::content::Guid> {
    match vanilla::item_by_display_name(name) {
        Ok(item) => Some(item),
        Err(VanillaError::Unknown { near, .. }) => {
            enshwick::info!("no item {name}; near: {}", near.join(", "));
            None
        }
        Err(_) => vanilla::item(name).ok(),
    }
}
```

## `NewItem`

| Field | Meaning |
| --- | --- |
| `id` | The new item's content id. Its game id is `ItemId::from_guid(id.guid())`. |
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

`add_item` serves two resources:

- the new item's data, built from the vanilla one with the new id, GUID,
  name, description, meter and tooltip lines written in;
- the item registry with the new GUID listed after the others.

With a meter it also serves the entity template and the template collection
with it. With an icon it serves the icon's UI image too. Everything else in the
item data is the vanilla item's, so a cloned chest places the same vanilla
chest.

An item whose GUID changed keeps the stacks saved with the old one through a
[content rename](../content.md#renamed-content), which adds it under both.
Old stacks keep the old game id, so compare item ids through
`content::renames::current_id`.

The item exists once the game reads it, but nobody has one yet. Players get
it from a [recipe](recipes.md), loot, or a mod's
[`give`](../inventories.md).

## Meters

A meter is a count each item keeps on its own, from 0 to a maximum, such as
the charges of a staff. The game keeps it as it keeps the water pouch's
water, on the item's own entity. The meter is saved with the world, sent to
players and drawn as a bar on the item's slot.

```rust
use enshwick::content::{Id, id, vanilla};
use enshwick::content::items::{FuelSetup, NewItem};

const STAFF: Id = id!("charged-staff");
/// The staff entity's template, fixed forever.
const STAFF_ENTITY: Id = STAFF.part("entity");

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    // The Blacksmith's summoning staff.
    let vanilla_staff = vanilla::item("NPC_Crafter_Blacksmith01")?;
    load.add_item(NewItem {
        meter: Some(FuelSetup { template: STAFF_ENTITY, max_fuel_charges: 3, start: 3 }),
        ..NewItem::new(STAFF, vanilla_staff)
    })
}
```

| `FuelSetup` field | Meaning |
| --- | --- |
| `template` | The content id of the item entity's template, a clone of the water pouch's; saves hold its GUID. Items with the same meter may share one. |
| `max` | The most it holds, 1 or more. |
| `start` | What a new item holds when nothing sets its fill. |

Each new item starts at `start`, or at the fill the give names:

- an [`Op::Give`](../inventories.md) with `fill: Some(Fill::of(count, max))`;
- a [drop](random-loot.md) with `.meter(min, max, of)`, for a count rolled as
  it drops.

[`World::item_meter`](../world.md) reads it and `change_item_meter` changes
it, on the server's game thread. [`Reader::meter`](../inventories.md) reads it
as clients see it. A summoning staff's [`Summon`](../survivors.md) reads it
the same way.

The item gets an entity of its own (the game's `HasPIDE`) and keeps its
count as fuel (`UsesFuel`), which is what draws the bar. An item that has an
entity already, such as a weapon with durability, cannot take a meter:
`add_item` turns the mod off.

## Tooltip lines

`stats` adds lines to the item's tooltip, after the vanilla item's, as the
game lists an armour's bonuses: each a [text tag](texts.md) and a number.

```rust
use enshwick::content::{Id, id, vanilla};
use enshwick::content::items::{NewItem, ImpactUiValue, StatFormat};

const BOX: Id = id!("lucky-box");
const LUCK: Id = BOX.part("luck");

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let storage_box = vanilla::item("Prop_Storage_24_T1_Box")?;
    let texts = load.texts_from("[en]\nluck = \"Luck\"\n")?;
    load.add_texts(&texts, &[("luck", LUCK)])?;
    load.add_item(NewItem {
        stats: vec![ImpactUiValue { loca_id: LUCK.guid(), value: 5, value_format: StatFormat::Percent }],
        ..NewItem::new(BOX, storage_box)
    })
}
```

Each `StatFormat` shows the value 5 its own way:

- `StatFormat::Number` reads `5`;
- `Signed` reads `+5`;
- `Percent` reads `5%`.

The lines are the same for every item of the type; what one item holds
of its own shows through its meter.

## Icons

The game's item icons are pictures it ships ready-made: UI images, named by
GUID, whose pixels it reads from its archive. An
[`Icon`](https://github.com/enshwick/enshwick/blob/main/crates/internal/core/src/content/icons.rs)
is a UI image the mod serves under a content id of its own, made when the
client first shows it:

- `Pixels::Tint(Tint { colour, amount })` recolours the vanilla icon
  `like`. Every pixel moves `amount` percent toward `colour` at its own
  brightness, so the shading stays. The pixels come from the player's own
  game files, so no game data ships with the mod;
- `Pixels::Png(include_bytes!("icon.png"))` draws the mod's own picture,
  best square, such as 256 by 256.

```rust
use enshwick::content::{Id, id, vanilla};
use enshwick::content::icons::{Icon, Pixels, Tint};
use enshwick::content::items::NewItem;

const STAFF: Id = id!("spectral-staff");
const STAFF_ICON: Id = STAFF.part("icon");

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    // The Blacksmith's summoning staff; its icon is a portrait.
    let vanilla_staff = vanilla::item("NPC_Crafter_Blacksmith01")?;
    load.add_item(NewItem {
        icon: Some(Icon {
            id: STAFF_ICON,
            like: vanilla_staff,
            pixels: Pixels::Tint(Tint { colour: [120, 170, 255], amount: 60 }),
        }),
        ..NewItem::new(STAFF, vanilla_staff)
    })
}
```

`like` is the vanilla icon's GUID. Most items have an icon the game draws from
their model, and for them it is the item's own GUID. Otherwise it is the GUID
in the item's `iconImage`.

An icon shows on a client whose loader knows the build's texture code. There
the loader serves the pixels and lists the icon with the game's UI images. It
waits until the UI knows the icon before an item names it, since a UI image
the UI does not know ends the game. Anywhere else, a client of another build or
the dedicated server, the item keeps its vanilla icon. `Load::add_icon`
serves an icon alone, for an item a mod serves itself.

## GUIDs and item ids

[`Guid::parse`](https://github.com/enshwick/enshwick/blob/main/crates/internal/core/src/code.rs)
reads the textual form `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx` and stores the
bytes in the order the game keeps them in memory. It is a `const fn`: in a
constant, a malformed GUID fails the build.

The game numbers items, recipes and workshops by a 32-bit id.
[`Guid::fnv1a`](https://github.com/enshwick/enshwick/blob/main/crates/internal/core/src/content.rs)
computes it from the GUID. `ItemId::from_guid` and `RecipeId::from_guid` give
it its type. The id is what an inventory slot holds and what the
[inventory](../inventories.md) calls take:

```rust
use enshwick::content::Guid;

const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
assert_eq!(enshwick::ItemId::from_guid(CHEST).get(), 1_302_084_846);
assert_eq!(CHEST.to_string(), "6943053a-32c1-4718-ae22-e61427ddf54d");
```

In Luau, `enshwick.content.item_id` and `enshwick.content.recipe_id` give the
id. They take a GUID, such as `enshwick.content.item` gives for a vanilla
item, or a content id in full for the mod's own:

```lua
local torch = enshwick.content.item_id(enshwick.content.item("Tool_Torch01"))
local tonic = enshwick.content.item_id("you.tonics:tonic")
```

Saves keep item ids, so a content id is fixed for good once players have the
item: see [Content ids](../concepts/ids.md).

## Errors

`add_item` returns `Err(Off)` in four cases:

- the game's reflection lacks a field the clone writes;
- another mod serves the same item GUID from another vanilla item;
- the meter's maximum is 0;
- the loader refuses one of its content ids.

An item whose game id a vanilla item has turns the mod off when the game
first reads the registry. The mod's log names both items. A meter on an item that has an entity already turns
the mod off when the game reads the item.
[`enshwick-personal-safes`](https://github.com/enshwick/mods/tree/main/personal-safes)
adds its Safe this way.
