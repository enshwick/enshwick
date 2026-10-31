# Item and entity data

Unstable: everything in this chapter needs the `unstable` feature and may
change in any release ([Stability](stability.md)).

Item and entity data is what a mod attaches to one thing of the world: a
ring's rolled bonus, the name engraved on a sword, the player who owns a placed
chest, the nickname of a survivor. Each piece belongs to a kind with an id
`<mod>:<name>` and a value type of the mod's choosing, kept per world beside
the game's save, never inside it. The
[`item_data`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/item_data.rs)
module attaches to items, the
[`entity_data`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/entity_data.rs)
module to any entity the save keeps in place.

```rust
use enshwick::Entity;
use enshwick::entity_data::Replication;
use enshwick::inventory::ItemInstance;

#[derive(serde::Serialize, serde::Deserialize)]
struct Bonus {
    fire: i16,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Owner {
    player: u64,
}

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    // `<this mod>:bonus`; another mod opens it as "<this mod>:bonus".
    let bonus = load.item_data::<Bonus>("bonus")?;
    let ring = ItemInstance { pide: Entity(900), item: enshwick::ItemId::new(0x5e01) };
    if bonus.get(ring).is_none() {
        let _set = bonus.set(ring, &Bonus { fire: 12 });
    }

    // `<this mod>:owner`, kept on the server alone.
    let owners = load.entity_data::<Owner>("owner", Replication::Server)?;
    let chest = Entity(4_200);
    if owners.get(chest).is_none() {
        let _set = owners.set(chest, &Owner { player: 7 });
    }
    Ok(())
}
```

## What carries data

- **An item** with an entity of its own, its PIDE: weapons, armour and other
  items with durability or a level, and items with a
  [meter](content/items.md#meters). An
  [`ItemInstance`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/item_data.rs)
  is that entity and the item id it holds; `ItemInstance::of(&slot)` takes it
  from an [inventory](inventories.md) slot, or gives `None` for an empty slot
  or an item without an entity of its own.
- **An entity the save keeps in place**: a placed prop, an altar, a
  container, a survivor. A player's character is not one, so entity data
  refuses it with `SetError::NotKept`; keep player data by `PlayerKey`.

## Kinds

`load.item_data::<T>(id)` and `load.entity_data::<T>(id, replication)` open
a kind during `init`, on both sides:

- `name` is the mod's own kind, `<mod>:<name>`;
- `mod:name` is any mod's kind. Every mod that opens one id shares its
  values, which is how mods share data without depending on each other. The
  value types must match; a value that does not decode as `T` reads as
  `None`.

The mod side of an id is a mod id; the name side is 1 to 64 of
`[a-z0-9_-]`. One id is either an item kind or an entity kind; opening it as
both turns the second mod off. A value is encoded with `postcard` and takes
at most 512 bytes.

Item data goes to every client that runs Enshwick. Entity data goes where
its `Replication` says:

| `Replication` | Who gets the values |
| --- | --- |
| `Server` | the server alone |
| `Clients` | the server, and every client that runs Enshwick: all values on joining, each change after |

An entity kind is sent once any mod opens it with `Clients`.

## Reads and writes

| Call | Does |
| --- | --- |
| `get(target)` | The value, or `None`. Any thread. |
| `set(target, &value)` | Sets it. |
| `remove(target)` | Removes it. Where `set` works. |

Item data is set on the server, or on a client while it hosts, once the
world has loaded. Entity data is set on the server's game thread, once the
world has loaded.

`set` and `remove` return a `SetError`: the value does not encode or is too
long, the call came from the wrong place (a client alone for item data, off
the server's game thread for entity data), or the world has not loaded. For
entity data also: the entity does not exist, or no load finds the entity
again (`NotKept`).

## How a load finds the owner again

An entity id lasts one session. Each load of a world numbers its entities,
item entities included, anew in the order of the save, so adding or
removing one thing moves the ids of everything saved after it. `Entity`
therefore has no serde form, and a mod keeps nothing by entity id in its own
files. The server keeps each value under its owner's place instead.

**Items.** When the world saves, the server keeps each value under the
item's place: the placed container, which of its inventories, and the slot.
After the next load the value finds the item in that slot again, if it holds
the same item. An item in a player's inventory has no such place: its value
lasts while its player plays. When the player leaves, or the server stops
while they play, the value goes aside in the world's file, kept but attached
to nothing. Within a session the value stays with the item across moves
between inventories and trades.

**Entities.** The server keeps each value under the entity's template and
position, as the save keeps it, to a sixteen-thousandth of a metre. After the
first step of a loaded world, every value finds its entity again by that
place. A value whose place names no entity for certain goes aside: its place
holds no entity, or two entities of the template share it, such as two chests
a mod spawned at one point, even turned apart. A value aside stays in the
world's file, attached to nothing and never matched again, since a later
entity may take its place. The log says how many went aside. Nothing is given
to another entity on a guess.

When the game destroys the owner, as a craft that uses an item up does, the
server drops its values after that step and tells the clients. When the id
names another thing:

- an item's value reads as missing once the entity holds another item; an
  entity's value reads as missing when a read on the server's game thread
  finds its template changed. The server keeps, with each entity value, the
  template its entity had when the value was set;
- the server drops every such value when the world saves.

An entity the world no longer holds when it saves, gone without a destroy the
server saw, has its values set aside. Reads of entity data on other threads
answer from what the server keeps, until that next check.

## Saved and sent

The server keeps item and entity data in one file,
`mod-data/enshwick/worlds/<key>/attachments.bin`, written when the world
saves and when the server stops, never in the game's save: removing the mods
leaves the world vanilla. A client that joins gets every value it is sent,
and each change after, so `get` on a client answers from what its server
sent. A vanilla client gets nothing.

A client and the server number entities apart: the client's own world gives
a replicated entity another id. A sent kind reaches clients under the
server's ids, so a client reads a value by an id the server named, such as
one in a [message](messages.md) or the item entities of inventory slots.

What a player should see on an item itself belongs in its
[meter](content/items.md#meters), which the game draws, or in its
[tooltip lines](content/items.md#tooltip-lines), which are the same for
every item of its type.
