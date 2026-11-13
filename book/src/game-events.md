<!--
type: how-to
description: The game's systems tell each other what happens through ECS events. A mod observes them, typed for the common ones and by reflected name for any other.
-->

# Game events

The game's own systems tell each other what happens through ECS events.
[`game_events`](https://github.com/enshwick/enshwick/blob/main/crates/internal/base/src/game_events.rs)
lets a mod observe them, typed for the common ones and by reflected name for
any other.

## Typed events

An event says that a hit resolved, an entity died, a player levelled up or a
workshop finished a recipe, among others.

[`load.on::<E>(|event| ...)`](https://github.com/enshwick/enshwick/blob/main/crates/internal/core/src/load.rs)
runs a handler on every event of kind `E`:

```rust
use enshwick::game_events::{EntityDiedEvent, LevelUpEvent};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let world = load.world()?;
    load.on::<EntityDiedEvent>(move |died| {
        if let Some(player) = died.killer_player.and_then(|p| world.player(p)) {
            enshwick::info!("{} killed entity {}", player.name, died.dead_entity_id.0);
        }
    })?;
    load.on::<LevelUpEvent>(|up| enshwick::info!("entity {} reached level {}", up.player_entity_id.0, up.level))
}
```

| Kind | Fields |
| --- | --- |
| `HitEvent` | `root_source_id`, who the hit is credited to |
| | `source_id` and `target_id` |
| | `root_target_id` and `weapon_pide_id` |
| | `health_change`, below 0 for damage |
| | `stamina_change` and `damage_types` |
| | `attack_category` and `target_faction` |
| `EntityDiedEvent` | `dead_entity_id` |
| | `killer`, enshwick's own: the last hit's attacker |
| | `killer_player`, enshwick's own: the player credited with the kill |
| `LevelUpEvent` | `player_entity_id` and `level` |
| `XpGainedEvent` | `player_entity_id` and `amount` |
| | `source`. Its values include `Combat` and `Quest`. `Mining` is another. |
| `CraftRecipeEvent` | `crafting_operator_id` and `crafting_station_id` |
| | `recipe_id` and `recipe_amount` |
| `CraftRecipeSuccessEvent` | `workshop_id` and `recipe_id` |
| `AcceptOfferedInteractionEvent` | `guest_entity_id` and `host_entity_id` |
| | `offer_id` |
| `BuildingPlaceEvent` | `block` and `tracking_item_id` |
| `BuildingTearDownEvent` | `block` |
| `BuildingPickUpEvent` | `block` and `picked_entity_id` |
| `FlameAltarEvent` | `player_entity_id` and `base_entity_id` |
| | `r#type` |
| | its value `Abandon` or `Reclaim` |
| | its value `RemoveWater` or `SetAsPreferredSpawnAltar` |
| `LootInteractionEvent` | `guest_id` and `host_id` |
| | `loot_all` |
| `HandleEquipmentSlotChangedEvent` | `player_entity_id` and `equipment_slot` |
| `RevivedPlayerEvent` | `target_entity_id` and `source_entity_id` |

The `block` of a building event holds four values:

- `owner_id`;
- `position`;
- the volume;
- `material`.

Each kind and field takes the game's own name in Rust casing. `Block` is
enshwick's own type. So are the fields `killer` and `killer_player`. All three
need the `unstable` feature.

The loader maps each kind and its fields to the running build. So a game
update that renames or moves them changes the loader, not your mod.

Entities that may be missing are `Option`s. Kill credit comes from the hits
the dead entity kept and the player the game credits. Both are read while the
entity is still in the world. An entity that kept no history has `None`
there. Not all damage is a
hit: falls and the shroud change health without one.

## Any event by name

Every event type the game reflects can be observed by its engine name. That
is the low level: a game update that renames the type or a field turns the
mod's subscription off until the mod follows. Resolve the type and the fields
you read once, at init; each `Field` reads its value for the running build:

```rust
use enshwick::Entity;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let pushed = load.game_event_type("keen::ecs::ApplyPushBackEvent")?;
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
- `event.get(field)` is `None` for a field of another type.
  `event.read::<T>(offset)` reads what reflection does not name, such as an
  array element. `event.bytes()` is the whole event.
- Field types are the integers, `f32` and `f64`. `bool` and `Entity` are
  field types too.

`load.on_every_raw_game_event(|event| ...)` receives every event of every type,
for finding out what the game raises. `load.game_event_types()` lists every
event type the running build reflects, with its name, hash and size. The
[probe](https://github.com/enshwick/mods/tree/main/devtools/enshwick-probe)
uses both to count events by name. Each event is copied for the mod, so leave
it to development builds.

```rust
use std::collections::HashMap;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let names: HashMap<u32, &str> = load.game_event_types()?
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
moves the streams into its event queues. The loader copies the events a mod
asked for there and hands them over right after. So a handler:

- runs on the server's game thread, where [`World`](world.md) reads work, and
  sees the entities an event names still in the world;
- runs a step after the event happened, about 1/60 s with players online;
- keeps the game waiting: keep it short and leave slow work to a thread of
  your own.

Subscribe during `init`. The loader links the engine's move only when a mod
subscribed to a game event, so unused events cost nothing. With
subscribers, the move costs one hash lookup per event. On a client the events
come from the built-in server while it hosts a game, as server events do.

## Testing

[`FakeHost`](testing.md) records the types a mod asked for in
`game_event_types()`. `raise_game_event(type_hash, bytes)` hands an event
to the mod's handlers. `game_event_type(name, hash, size)` gives the fake
build an event type. `load.game_event_types()` lists that type, and
`load.game_event_type` resolves it. Give the fake type the offsets the mod
resolves with `field_offset`.

## From Luau

`enshwick.on` takes the name of a typed game event. Examples are `HitEvent`
and `EntityDiedEvent`. The handler gets the event's fields as
`game_events::HitEvent` has them, a mask as the list of its bit names, a game
value as its name. A value the API does not name arrives as
`Other(<number>)`, a name of ours with the game's number in it:

```luau
enshwick.on("HitEvent", function(hit)
    if hit.health_change < 0 and hit.root_source_id then
        enshwick.log.info(`{hit.root_source_id} hit {hit.target_id}`)
    end
end)
```

Any other game event is read by its reflected type and the fields the script
names. Each field comes with how to read it, one of these:

- `u8` or `i8`
- `u16` or `i16`
- `u32` or `i32`
- `u64` or `i64`
- `f32` or `f64`
- `bool` or `entity`

The 64-bit ones arrive as strings.
`enshwick.game_event_types()` lists every type of the running build with
its size. A field the type lacks on this build leaves the mod off at start,
with the reason in the log.

```luau
enshwick.on_game_type("keen::ecs::EntityDiedEvent", { deadEntityId = "entity" }, function(event)
    enshwick.log.info(`entity {event.deadEntityId} died`)
end)
```

Game events run on the server's game thread before the step's systems, as
for a Rust mod. On a client they run only while it hosts a game.

## Events of the client's own world

Unstable: needs the `unstable` feature and may change in any release
([Stability](stability.md)).

A client's own world raises game events of its own, of the same reflected
types, such as `keen::ecs::OpenHudScreenEvent`. The handler of
`load.on_client_raw_game_event(ty, handler)` runs on each event of type `ty`
in that world. The handler of `load.on_every_client_raw_game_event(handler)`
runs on every one. The `typed` feature adds `EventType::<T>::on_client`,
which reads them as the generated type:

```rust
fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let opened = load.game_event_type("keen::ecs::OpenHudScreenEvent")?;
    load.on_client_raw_game_event(opened, |event| {
        enshwick::debug!("a screen opened: {} bytes", event.bytes().len());
    })
}
```

A handler of the client's own events:

- runs on the thread that steps the client's world, right after the engine
  moved that world's events;
- gets only that world's events, also while the client hosts a game, and a
  server handler never gets them;
- runs with no filter, veto or item action on the events;
- never runs on a dedicated server, where the subscription is kept without
  an error, so one `both` mod serves both sides.

While a hosted game starts, the client's own events are not handed over.
Until the server's first step, the loader cannot tell its world from the
client's.

One move of the client's world reads at most 256 streams and 65,536 events.
It hands the handlers at most 256 events and 64 KiB of them. The rest of
that move's events are dropped, and the log says so once.

From Luau, `enshwick.on_client_game_type(type, fields, handler)` reads them
as `enshwick.on_game_type` reads a game event:

```luau
enshwick.on_client_game_type("keen::ecs::OpenHudScreenEvent", { screen = "u8" }, function(event)
    enshwick.log.info(`screen {event.screen} opened`)
end)
```

In tests, the [`FakeHost`](testing.md) of a client hands an event of the
client's own world to the mod's handlers with
`raise_client_game_event(type_hash, bytes)`.
