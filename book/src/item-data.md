# Item data

Item data is what a mod attaches to one item: a ring's rolled bonus, the
name engraved on a sword, the owner of a staff. Each piece belongs to a
kind with an id `<mod>:<name>` and a value type of the mod's choosing. The
[`item_data`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/item_data.rs)
module reads and writes it.

```rust
use enshwick::ecs::Entity;
use enshwick::item_data::ItemInstance;

#[derive(serde::Serialize, serde::Deserialize)]
struct Bonus {
    fire: i16,
}

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    // `<this mod>:bonus`; another mod opens it as "<this mod>:bonus".
    let bonus = load.item_data::<Bonus>("bonus")?;
    let ring = ItemInstance { pide: Entity(900), item: 0x5e01 };
    if bonus.get(ring).is_none() {
        let _set = bonus.set(ring, &Bonus { fire: 12 });
    }
    Ok(())
}
```

## Items

Only an item with an entity of its own, its PIDE, carries data: weapons,
armour and other items with durability or a level, and items with a
[meter](content/items.md#meters). An
[`ItemInstance`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/item_data.rs)
is that entity and the item id it holds; `ItemInstance::of(&slot)` takes it
from an [inventory](inventories.md) slot, or gives `None` for an empty slot or
an item without an entity of its own.

The game saves an item's entity under its id and creates it with the same
id on load, so the value stays with the item across saves, moves between
inventories and trades. Once the entity holds another item, the value reads
as missing, and the server drops it when the world saves. When the game
destroys the item, as a craft that uses it up does, the server drops its
values after that step and tells the clients. An item in an area that is not
loaded keeps its values.

## Kinds

`load.item_data::<T>(id)` opens a kind during `init`, on both sides:

- `name` is the mod's own kind, `<mod>:<name>`;
- `mod:name` is any mod's kind. Every mod that opens one id shares its
  values, which is how mods share item data without depending on each
  other. The value types must match; a value that does not decode as `T`
  reads as `None`.

The mod side of an id is a mod id; the name side is 1 to 64 of
`[a-z0-9_-]`. A value is encoded with
`postcard` and takes at most 512 bytes.

## Reads and writes

| Call | Does |
| --- | --- |
| `get(item)` | The item's value, or `None`. Any thread. |
| `set(item, &value)` | Sets the item's value. On the server, or a client while it hosts, once the world has loaded. |
| `remove(item)` | Removes it. As for `set`. |

`set` and `remove` return a `SetError`: the value does not encode or is too
long, the call came from a client alone, or the world has not loaded.

## Saved and sent

The server keeps the values in
`mod-data/enshwick/worlds/<key>/attachments.bin`, written when the world
saves and when the server stops, never in the game's save: removing the
mods leaves the world vanilla. Every change goes to the clients that run
Enshwick, and a client that joins gets every value, so `get` on a client
answers from what its server sent. A vanilla client gets nothing.

Data on entities other than items is [entity data](entity-data.md), kept in
the same file.

What a player should see on the item itself belongs in its
[meter](content/items.md#meters), which the game draws, or in its
[tooltip lines](content/items.md#tooltip-lines), which are the same for
every item of its type.
