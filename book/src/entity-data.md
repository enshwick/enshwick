# Entity data

Entity data is what a mod attaches to any entity of the world: the player
who owns a placed chest, the nickname of a survivor, the day a gate was
built. It works as [item data](item-data.md) does, for every entity instead
of items alone: kinds with ids `<mod>:<name>`, a value type of the mod's
choosing, values kept per world beside the game's save, never inside it.
The [`entity_data`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/entity_data.rs)
module reads and writes it.

```rust
use enshwick::ecs::Entity;
use enshwick::entity_data::Replication;

#[derive(serde::Serialize, serde::Deserialize)]
struct Owner {
    player: u64,
}

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    // `<this mod>:owner`, kept on the server alone.
    let owners = load.entity_data::<Owner>("owner", Replication::Server)?;
    let chest = Entity(4_200);
    if owners.get(chest).is_none() {
        let _set = owners.set(chest, &Owner { player: 7 });
    }
    Ok(())
}
```

## Kinds

`load.entity_data::<T>(id, replication)` opens a kind during `init`, on
both sides. As for item data, `name` is the mod's own kind and `mod:name`
any mod's: every mod that opens one id shares its values, which is how mods
share entity data without depending on each other. One id is either an item
kind or an entity kind; opening it as both turns the second mod off. A value
is encoded with `postcard` and takes at most 512 bytes.

| `Replication` | Who gets the values |
| --- | --- |
| `Server` | the server alone |
| `Clients` | the server, and every client that runs Enshwick: all values on joining, each change after |

A kind is sent once any mod opens it with `Clients`.

## Reads and writes

| Call | Does |
| --- | --- |
| `get(entity)` | The entity's value, or `None`. Any thread. |
| `set(entity, &value)` | Sets it. On the server's game thread, once the world has loaded. |
| `remove(entity)` | Removes it. As for `set`. |

`set` and `remove` return a `SetError`: the value does not encode or is too
long, the call came off the server's game thread, the world has not loaded,
or the entity does not exist.

## The entity and its value

The game saves an entity under its id and creates it with the same id when
the world loads, so a value stays with its entity across saves. The server
keeps, with each value, the template its entity had when the value was set.
When the game destroys the entity, the server drops its values after that
step and tells the clients. When its id names another thing:

- a read on the server's game thread finds it out and the value reads as
  missing;
- the server drops every such value when the world saves.

An entity in an area that is not loaded keeps its values.

Reads on other threads answer from what the server keeps, until that next
check.

## Saved and sent

The server keeps item and entity data in one file,
`mod-data/enshwick/worlds/<key>/attachments.bin`, written when the world
saves and when the server stops. Removing the mods leaves the world vanilla.

A client and the server number entities apart: the client's own world gives
a replicated entity another id. A sent kind reaches clients under the
server's ids, so a client reads a value by an id the server named, such as
one in a [message](messages.md) or the item entities of inventory slots.
