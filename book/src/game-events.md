# Engine events

The game's own systems tell each other what happens through ECS events: a
hit resolved, an entity died, a player levelled up, a workshop finished a
recipe. [`game_events`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/game_events.rs)
lets a mod observe them, typed for the common ones and by reflected name for
any other.

## Typed events

[`load.on_game::<E>(|event| ...)`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/load.rs)
runs a handler on every event of kind `E`:

```rust
use enshwick_loader_api::game_events::{Died, LevelUp};

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    let world = load.world()?;
    load.on_game::<Died>(move |died| {
        if let Some(player) = died.killer_player.and_then(|p| world.player(p)) {
            enshwick_loader_api::info!("{} killed entity {}", player.name, died.entity.0);
        }
    })?;
    load.on_game::<LevelUp>(|up| enshwick_loader_api::info!("entity {} reached level {}", up.player.0, up.level))
}
```

| Kind | Engine type | Fields |
| --- | --- | --- |
| `Hit` | `HitEvent` | `attacker`, `source`, `target`, `root_target`, `weapon`, `health_change` (below 0 for damage), `stamina_change`, `damage_types`, `attack`, `target_faction` |
| `Died` | `EntityDiedEvent` | `entity`, `killer` (the last hit's attacker), `killer_player` (the player credited with the kill) |
| `LevelUp` | `LevelUpEvent` | `player`, `level` |
| `XpGained` | `XPGainedEvent` | `player`, `amount`, `source` (`Combat`, `Quest`, `Mining` and others) |
| `CraftRequested` | `CraftRecipeEvent` | `player`, `station`, `recipe`, `amount` |
| `Crafted` | `CraftRecipeSuccessEvent` | `workshop`, `recipe` |
| `Interacted` | `AcceptOfferedInteractionEvent` | `player`, `target`, `offer` |
| `Built`, `TornDown`, `PickedUp` | `BuildingPlaceEvent`, `BuildingTearDownEvent`, `BuildingPickUpEvent` | `block` (`player`, `position`, volume, `material`); `item` for `Built`, `entity` for `PickedUp` |
| `AltarAction` | `FlameAltarEvent` | `player`, `altar`, `kind` (`Abandon`, `Reclaim`, `RemoveWater`, `SetSpawn`) |
| `LootOpened` | `LootInteractionEvent` | `player`, `container`, `take_all` |
| `EquipmentChanged` | `HandleEquipmentSlotChangedEvent` | `player`, `slot` |
| `Revived` | `RevivedPlayerEvent` | `player`, `by` |

Entities that may be missing are `Option`s. Kill credit comes from the dead
entity's `HitHistory` and `ExperienceSource`, read while it is still in the
world; an entity that kept no history has `None` there. Not all damage is a
hit: falls and the shroud change health without one.

## Any event by name

Every event type the game reflects can be observed. Resolve the type and the
fields you read once, at init; each `Field` reads its value for the running
build:

```rust
use enshwick_loader_api::ecs::Entity;

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    let pushed = load.event_type("keen::ecs::ApplyPushBackEvent")?;
    let target = pushed.field::<Entity>(load, "targetId")?;
    load.on_game_event(pushed, move |event| {
        if let Some(target) = event.get(target) {
            enshwick_loader_api::debug!("entity {} pushed back", target.0);
        }
    })
}
```

- `field` reads a field the type declares; `inherited_field` one of a base
  type, such as `rootSourceId` of `keen::ecs::BaseHitEvent` in
  `keen::ecs::HitEvent`, since reflection lists a field under the type that
  declares it.
- `event.get(field)` is `None` for a field of another type, and
  `event.read::<T>(offset)` reads what reflection does not name, such as an
  array element. `event.bytes()` is the whole event.
- Field types are the integers, `f32`, `f64`, `bool` and `Entity`.

`load.on_every_game_event(|event| ...)` receives every event of every type,
for finding out what the game raises; the
[probe](https://github.com/themaiby/enshwick/tree/main/mods/enshwick-probe)
uses it to count them. Each event is copied for the mod, so leave it to
development builds.

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
subscribed to an engine event, so unused events cost nothing; with
subscribers, the move costs one hash lookup per event. On a client the events
come from the built-in server while it hosts a game, as server events do.
Engine events need ABI minor 20; under an older loader `on_game` and
`on_game_event` turn the mod off with
`engine events need a newer loader (ABI minor 20)`.

## Testing

[`FakeHost`](testing.md) records the types a mod asked for in
`game_event_types()`, and `raise_game_event(type_hash, bytes)` hands an event
to the mod's handlers. Give the fake the reflected names, sizes and offsets
the mod resolves with `type_hash`, `type_size` and `field_offset`.
