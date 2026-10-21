# The world

[`load.world()`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/load.rs)
returns a
[`World`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/world.rs):
reads of the server's world, component writes, character stats, entity
queries, labels, chat lines, destroy requests and saves. It is `Copy`; get
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

## Stats

`vitals(entity)` returns its health, stamina and mana, each a `Meter` with
`current` and `max`, buffs and gear included; `progress(entity)` a
character's `level`, `experience` toward the next level and
`experience_needed` for it. Health is there for every entity that has
vitals; stamina and mana only for those that have them, such as characters.

```rust
use enshwick_loader_api::events::PlayerJoined;

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    let world = load.world()?;
    load.on::<PlayerJoined>(move |joined| {
        let entity = enshwick_loader_api::ecs::Entity(joined.entity);
        if let (Some(vitals), Some(progress)) = (world.vitals(entity), world.progress(entity)) {
            enshwick_loader_api::info!("level {} with {} health", progress.level, vitals.health.current);
        }
    })
}
```

Both read the entity's attribute sets as the game keeps them. On a game build
without the layout they need, they return `None`, and `load.world()` still
works.

`set_health(entity, value)`, `set_stamina(entity, value)` and
`set_mana(entity, value)` set the current value and return the `Meter` the
entity now has; `heal(entity, amount)` adds to its health, and takes from it
for a negative amount. The value stays between a floor and the maximum, which
the game derives from gear and buffs and the writes leave alone:

| Stat | Floor | Refused when |
| --- | --- | --- |
| Health | 1: a write never kills | the entity is dead (`WorldError::Refused`): death and revival stay the game's |
| Stamina | 0 | the entity has no stamina (`WorldError::Lacks`) |
| Mana | 0 | the entity has no mana (`WorldError::Lacks`) |

```rust
use enshwick_loader_api::game_events::LevelGained;

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    let world = load.world()?;
    // A level up restores the character's health.
    load.on_game::<LevelGained>(move |gained| {
        let max = world.vitals(gained.player).map_or(0, |v| v.health.max);
        if let Err(why) = world.set_health(gained.player, max) {
            enshwick_loader_api::debug!("no heal: {why}");
        }
    })
}
```

The game sends the new value to the clients and regenerates from it as from
any other.

`item_meter(pide)` reads the [meter](content/items.md#meters) of an item's
own entity, a `Meter` with the count and its maximum, and
`change_item_meter(pide, change)` sets it to what `change` makes of the
count, kept between 0 and the maximum. The game saves it with the world and
shows it on the item's slot.

`shroud_time(entity)` reads how long a character holds out in the shroud
before it drains health, in seconds, as a `Meter` with the most it holds,
and `change_shroud_time(entity, change)` sets it, kept between 0 and that
most; clients see it from the next step. The game refills it outside the
shroud and saves none of it. `in_shroud(entity)` tells whether a character
stands in the shroud where it drains them, outside the altars' reach.

`skills(entity)` lists the skill nodes a character unlocked, each a `Skill`
with the node's id in the game's skill tree and its level for nodes that
level up. Unlocking and unlearning stay the player's: the game resets a
whole tree it finds unconnected or over its points when the character loads.

## Writing components

A write goes to the component the server owns, and the loader marks it
changed the way the game's own writes do, so the game sends it to clients and
saves it with the world. The copies the game keeps for clients, the
`Network*` and `Client*` components, are never written: the game overwrites
them from the server's own every step, so `writable_type` refuses them.

| Call | Does |
| --- | --- |
| `writable_type(load, name)` | a component type to write, such as `keen::ecs::CombatFlags`; call at init |
| `update(entity, ty, \|bytes\| ..)` | hands the component's reflected bytes to the closure, marked changed |
| `ty.field::<T>(load, name)` | a typed field of the type, resolved by name for the running build |
| `get(entity, field)` | the field's value, `None` when the entity lacks the component |
| `set(entity, field, value)` | writes the field, marked changed |

```rust
fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    let world = load.world()?;
    let flags = world.writable_type(load, "keen::ecs::CombatFlags")?;
    let immune = flags.field::<bool>(load, "immune")?;
    load.add_command("immune", "toggles your character's immunity", move |ctx, _args| {
        let entity = ctx.player.entity;
        let now = !world.get(entity, immune).unwrap_or(false);
        match world.set(entity, immune, now) {
            Ok(()) => format!("immune: {now}").into(),
            Err(why) => format!("{why}").into(),
        }
    })
}
```

Writes work on the game thread between steps, in a game event, a scheduled
task or a chat command, and need ABI minor 22. The game thread runs ECS
systems itself while it steps, so a write from a filter or a hook on a system
is refused even there. They fail with a `WorldError`: `Gone` for an entity not
in the world, `Lacks` when it lacks the component, `Refused` for a type the
loader will not write, `WrongThread` off the game thread or inside a step, and
`Missing` under an older loader. A field
of an engine type means what the engine makes of it: a write the engine does
not expect, such as a value its systems keep elsewhere, may be overwritten or
do nothing, so check a new write on a test server first.

## Finding entities

`entities(ty)` returns every entity with a component of type `ty` and a place
in the world, as `Placed { entity, position }`, position in metres;
`near(point, radius, ty)` those within `radius` metres of `point`, nearest
first. `transform_type()` is the component every placed entity has, so
`near(point, radius, world.transform_type())` finds everything there. The
engine walks every archetype with the component, so call them now and then,
from a command or a timer, not on every step.

```rust
fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    let world = load.world()?;
    let beds = world.component_type(load, "keen::ecs::NpcBed")?;
    load.add_command("beds", "counts the beds", move |_ctx, _args| {
        format!("{} beds", world.entities(beds).len()).into()
    })
}
```

## Destroying entities and saving

`destroy(entity)` asks the server to remove an entity after the systems of
the next step, the way the game removes things itself: no loot drops and no
death is raised. It refuses a player's character (`WorldError::Refused`) and
an entity not in the world (`WorldError::Gone`).

`save()` asks the server to save the world now, as its timed saves do;
[`WorldSaved`](events.md) follows once the save is on disk. A save already
under way counts as asked. Before the world has loaded it is refused.

`key()` names the world the server runs: its save file's name and a hash of
its folder, such as `world0-1a2b3c4d`, the same in every session. A
[world store](state.md#per-world-state) files the mod's state under it.

All three need ABI minor 21 and, like the reads, the game thread; `key()`
works from any thread.

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

`player_by_key(key)` finds the connected player with an account key, and
`players_named(name)` the connected players with a name, ignoring case, as a
command finds the player another typed. Since names repeat, a command that
wants one player refuses when it gets several.

## Labels

`world.set_label(entity, text)` names an entity as a player's rename does: the
game shows the name on hover, sends it to every client, vanilla ones included,
and saves it with the world. The game keeps 255 bytes; an empty text removes
the name. It works on the game thread only, and returns `false` elsewhere or
when the entity is gone.
`world.has_label(entity)` tells whether an entity already carries such a
name, or `None` when the world cannot tell.

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
