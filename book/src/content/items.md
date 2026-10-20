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
        guid: SAFE,
        like: CHEST,
        name: Some(SAFE_NAME),
        description: Some(SAFE_DESCRIPTION),
        debug_name: "Safe".into(),
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

`add_item` serves two resources: the new item's `ItemInfo`, built from the
vanilla one with the new id, GUID, name and description written in, and the
item registry with the new GUID listed after the others. Everything else in
the `ItemInfo` is the vanilla item's, so a cloned chest places the same
vanilla chest.

The item exists once the game reads it, but nobody has one yet. Players get
it from a [recipe](recipes.md), loot, or a mod's
[`give`](../inventories.md).

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
resources, when the game's reflection lacks a field the clone writes, or
when another mod serves the same item GUID from another vanilla item.
[`enshwick-safes`](https://github.com/themaiby/enshwick/tree/main/mods/enshwick-safes)
adds its Safe this way.
