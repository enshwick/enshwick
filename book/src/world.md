# The world

[`load.world()`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/load.rs)
returns a
[`World`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/world.rs):
reads of the server's world, entity labels and chat lines. It is `Copy`; get
it during `init` and move it into handlers and tasks.

```rust
use enshwick_loader_api::ecs::Entity;
use enshwick_loader_api::events::PlayerJoined;

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    let world = load.world()?;
    load.on::<PlayerJoined>(move |joined| {
        if let Some(player) = world.player(Entity(joined.entity)) {
            enshwick_loader_api::info!("{} joined, admin: {}", player.name, player.is_admin());
        }
    })
}
```

`load.world()` turns the mod off under a loader too old for world reads, or on
a game build without the component types the reads need.

## Where reads work

The reads serve the server's world from its game thread:

- in a game event handler (see [Events](events.md));
- in a [scheduler](events.md#the-scheduler) task;
- in a [command handler](commands.md) or a rename filter.

Elsewhere, such as on a background thread or in a hook on a worker thread,
they return `None` or nothing. On a client the server is the built-in one,
while the client hosts a game. Inside a hooked ECS system, read components
through the system's context instead; see [ECS inside systems](ecs.md).

## Entities and components

| Call | Returns |
| --- | --- |
| `position(entity)` | the entity's position in metres, `[f64; 3]` |
| `template(entity)` | its template GUID, 16 bytes in file order |
| `used_item(entity)` | the item a placed prop came from, which it keeps across saves |
| `component_type(load, name)` | a reflected component type, such as `keen::ecs::Name`; call at init |
| `component(entity, ty)` | a read-only pointer to the entity's component of that type |

The pointer from `component` stays valid until the game thread returns to the
game; read what you need at once. `component_type` turns the mod off when the
type is not reflected on the running build. Its `hash()` is the hash
[`ecs::Components`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/ecs.rs)
takes inside systems.

```rust
use enshwick_loader_api::ecs::Entity;
use enshwick_loader_api::events::BuildingPlaced;

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    let world = load.world()?;
    load.on::<BuildingPlaced>(move |placed| {
        let entity = Entity(placed.entity);
        if let (Some(at), Some(item)) = (world.position(entity), world.used_item(entity)) {
            enshwick_loader_api::debug!("item {item} placed at {at:?}");
        }
    })
}
```

## Players

`player(entity)` returns the connected player whose character is `entity`;
`players()` returns every connected player with a character in the world. A
[`Player`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/world.rs)
has:

- `entity`, the character's entity;
- `slot`, the server's player slot;
- `key`, the same number for the same account in every session. Use it to
  remember a player between sessions;
- `permissions`, the server's permission bits, `PlayerInfo::CAN_*` in
  [`abi`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/abi.rs);
  `is_admin()` tells whether the player may kick and ban, the server's
  administrators;
- `name`, the name the client sent. It is untrusted: a client picks it, and two
  players may share it. Show it, but decide by `key` or `permissions`.

## Labels

`world.set_label(entity, text)` names an entity as a player's rename does: the
game shows the name on hover, sends it to every client, vanilla ones included,
and saves it with the world. The game keeps 255 bytes; an empty text removes
the name. It works on the game thread only, and returns `false` elsewhere or
when the entity is gone.

```rust
use enshwick_loader_api::ecs::Entity;
use enshwick_loader_api::events::BuildingPlaced;

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    let world = load.world()?;
    load.on::<BuildingPlaced>(move |placed| {
        if let Some(player) = world.player(Entity(placed.player)) {
            let label = format!("Built by {}", player.name);
            let _labelled = world.set_label(Entity(placed.entity), &label);
        }
    })
}
```

The game checks no permission when a player renames something.
[`load.on_rename(|rename| ...)`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/commands.rs)
runs before every rename on the server, on its game thread, with a
[`Rename`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/commands.rs):
the `entity`, the `player` (`None` when the server cannot tell) and the new
`text`. Returning `false` refuses the rename, and the entity keeps its name.

```rust
fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    // Only administrators rename anything.
    load.on_rename(|rename| rename.player.as_ref().is_some_and(|p| p.is_admin()))
}
```

A panic in the filter turns the mod off and lets that rename through.
[`enshwick-safes`](https://github.com/themaiby/enshwick/tree/main/mods/enshwick-safes)
uses labels and a rename filter for its Safes.

## Chat lines

`world.broadcast(text)` sends `text` to every player's chat, and
`world.tell(&player, text)` to one player. Both work from any thread: the line
waits in a queue and the server posts it after the next step. Each line of
`text` becomes a message of its own. See [Commands and chat](commands.md).
