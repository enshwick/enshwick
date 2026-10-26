# Game events

The game's own systems tell each other what happens through ECS events: a
hit resolved, an entity died, a player levelled up, a workshop finished a
recipe. [`game_events`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/game_events.rs)
lets a mod observe them, typed for the common ones and by reflected name for
any other.

## Typed events

[`load.on_game::<E>(|event| ...)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/load.rs)
runs a handler on every event of kind `E`:

```rust
use enshwick::game_events::{Died, LevelGained};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let world = load.world()?;
    load.on_game::<Died>(move |died| {
        if let Some(player) = died.killer_player.and_then(|p| world.player(p)) {
            enshwick::info!("{} killed entity {}", player.name, died.entity.0);
        }
    })?;
    load.on_game::<LevelGained>(|up| enshwick::info!("entity {} reached level {}", up.player.0, up.level))
}
```

| Kind | Fields |
| --- | --- |
| `Hit` | `attacker`, `source`, `target`, `root_target`, `weapon`, `health_change` (below 0 for damage), `stamina_change`, `damage_types`, `attack`, `target_faction` |
| `Died` | `entity`, `killer` (the last hit's attacker), `killer_player` (the player credited with the kill) |
| `LevelGained` | `player`, `level` |
| `ExperienceGained` | `player`, `amount`, `source` (`Combat`, `Quest`, `Mining` and others) |
| `CraftRequested` | `player`, `station`, `recipe`, `amount` |
| `Crafted` | `workshop`, `recipe` |
| `Interacted` | `player`, `target`, `offer` |
| `Built`, `TornDown`, `PickedUp` | `block` (`player`, `position`, volume, `material`); `item` for `Built`, `entity` for `PickedUp` |
| `AltarUsed` | `player`, `altar`, `action` (`Abandon`, `Reclaim`, `RemoveWater`, `SetSpawn`) |
| `LootOpened` | `player`, `container`, `take_all` |
| `EquipmentChanged` | `player`, `slot` |
| `Revived` | `player`, `by` |

The loader maps each kind and its fields to the running build, so a game
update that renames or moves them changes the loader, not your mod.

Entities that may be missing are `Option`s. Kill credit comes from the hits
the dead entity kept and the player the game credits, read while it is still
in the world; an entity that kept no history has `None` there. Not all damage is a
hit: falls and the shroud change health without one.

## Any event by name

Every event type the game reflects can be observed by its engine name. That
is the low level: a game update that renames the type or a field turns the
mod's subscription off until the mod follows. Resolve the type and the fields
you read once, at init; each `Field` reads its value for the running build:

```rust
use enshwick::ecs::Entity;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let pushed = load.event_type("keen::ecs::ApplyPushBackEvent")?;
    let target = pushed.field::<Entity>(load, "targetId")?;
    load.on_raw_game_event(pushed, move |event| {
        if let Some(target) = event.get(target) {
            enshwick::debug!("entity {} pushed back", target.0);
        }
    })
}
```

- `field` reads a field the type declares; `inherited_field` one of a base
  type, since reflection lists a field under the type that declares it.
- `event.get(field)` is `None` for a field of another type, and
  `event.read::<T>(offset)` reads what reflection does not name, such as an
  array element. `event.bytes()` is the whole event.
- Field types are the integers, `f32`, `f64`, `bool` and `Entity`.

`load.on_every_raw_game_event(|event| ...)` receives every event of every type,
for finding out what the game raises, and `load.event_types()` lists every
event type the running build reflects, with its name, hash and size; the
[probe](https://github.com/enshwick/devtools-mods/tree/main/enshwick-probe)
uses both to count events by name. Each event is copied for the mod, so leave
it to development builds.

```rust
use std::collections::HashMap;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let names: HashMap<u32, &str> = load
        .event_types()?
        .into_iter()
        .map(|ty| (ty.hash(), ty.name()))
        .collect();
    load.on_every_raw_game_event(move |event| {
        let name = names.get(&event.type_hash()).copied().unwrap_or("unknown");
        enshwick::debug!("{name}: {} bytes", event.bytes().len());
    })
}
```

## When and where

The server's systems write events into streams while they run. At the start
of the next step, on the game thread and before any system runs, the engine
moves the streams into its event queues; the loader copies the events a mod
asked for there and hands them over right after. So a handler:

- runs on the server's game thread, where [`World`](world.md) reads work, and
  sees the entities an event names still in the world;
- runs a step after the event happened, about 1/60 s with players online;
- keeps the game waiting: keep it short and leave slow work to a thread of
  your own.

Subscribe during `init`. The loader links the engine's move only when a mod
subscribed to a game event, so unused events cost nothing; with
subscribers, the move costs one hash lookup per event. On a client the events
come from the built-in server while it hosts a game, as server events do.

## Testing

[`FakeHost`](testing.md) records the types a mod asked for in
`game_event_types()`, and `raise_game_event(type_hash, bytes)` hands an event
to the mod's handlers. `event_type(name, hash, size)` gives the fake build an
event type that `load.event_types()` lists and `load.event_type` resolves;
give it the offsets the mod resolves with `field_offset`.
