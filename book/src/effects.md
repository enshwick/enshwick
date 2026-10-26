# Effects

[`load.effects()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/effects.rs)
returns the mod's
[`Effects`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/effects.rs):
game events it raises in the server's world, teleports and spawns. Like
[`World`](world.md), it is `Copy`; get it during `init` and move it into
handlers, tasks and actions.

```rust
use enshwick::actions::{Action, Outcome};
use enshwick::effects::Teleport;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let effects = load.effects();
    let actions = load.actions(enshwick::i18n::Texts::default())?;
    actions.add(Action::new("home", "Go to the world's centre"), move |request| {
        let me = request.player.entity;
        match effects.teleport(me, [0.0, 100.0, 0.0], Teleport::new().best_spawn()) {
            Ok(()) => Outcome::done("On your way"),
            Err(why) => Outcome::refused(format!("No teleport: {why}")),
        }
    })
}
```

## Raising game events

An event a mod raises goes where the game's own go: the loader puts it into
the event stream the engine moves at the start of the next step, so the
systems that read its type act on it as on one the game raised, and
[game event](game-events.md) handlers see it too. The loader stamps it with
the game time.

Only types the server's own systems raise are allowed, each with a typed
call:

| Call | Event | What the game does with it |
| --- | --- | --- |
| `npc_command(npc, NpcOrder::GoTo(at))` | `NpcCommandEvent` | the survivor with that survivor id walks to `at` |
| `npc_command(npc, NpcOrder::Reset)` | `NpcCommandEvent` | the survivor drops the order and goes back to its routine |
| `set_toggle(sender, signal, on)` | `SetToggleStateEvent` | the logic signal switches, as a lever drives a door |
| `add_threat(attacker, target, threat)` | `AddThreatTargetEvent` | the enemy takes notice of the target |
| `ping(sender, at)` | `UiPingEvent` | a ping on the map and in the world, from the sender |

`emit_raw(ty, bytes)` raises an event from its whole reflected layout, for
the same allowed types; the loader refuses any other type, and a size that
is not the type's. Each call fails with a
[`WorldError`](world.md#writing-components): `WrongThread` off the game
thread, `Refused` for a type the loader does not allow or when the step has
no room for more events, `Missing` on a build without the event.

```rust
use enshwick::actions::{Action, Arg, Outcome, Value};
use enshwick::effects::NpcOrder;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let effects = load.effects();
    let world = load.world()?;
    let actions = load.actions(enshwick::i18n::Texts::default())?;
    let come = Action::new("come", "Call a survivor to you")
        .arg(Arg::number("Survivor id", 0.0..=f64::from(u32::MAX), 1.0));
    actions.add(come, move |request| {
        let (Some(at), [Value::Number(id)]) = (world.position(request.player.entity), request.args) else {
            return Outcome::refused("You are nowhere");
        };
        Outcome::done(format!("{:?}", effects.npc_command(*id as u32, NpcOrder::GoTo(at))))
    })
}
```

## Teleports

`teleport(entity, position, how)` moves an entity as the game's admin
teleport does: after the next step the entity jumps to `position`, in metres,
and clients see it arrive. `Teleport::new()` keeps the entity's orientation;
`.best_spawn()` lets the game look for a free spot nearby to stand on, as it
does for respawns, and `.facing_yaw(radians)` turns the entity on arrival.
It fails with `Gone` for an entity not in the world or without a place, and
with `Refused` for one the game's teleport does not move: it moves actors and
characters, not props.

## Spawns

`spawn(template, position, how)` spawns an entity of a template, as the
game's admin spawn does: an enemy, an animal, a survivor or a prop. The
template is a GUID as `World::template` gives it, so a mod can spawn copies of
what it finds in the world. The new entity's id comes back at once; the
entity is in the world after the next step, and a template the game does not
know spawns nothing. `Spawn::new()` takes the template as it is;
`.level(n)` scales the entity as the game scales enemies,
`.despawn_after(duration)` lets the game remove it after that much game time,
and `.facing_yaw(radians)` turns it.

```rust
use core::time::Duration;
use enshwick::actions::{Action, Arg, Outcome, Value};
use enshwick::effects::Spawn;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let effects = load.effects();
    let world = load.world()?;
    let actions = load.actions(enshwick::i18n::Texts::default())?;
    let decoy = Action::new("decoy", "A copy of an entity beside you for a minute")
        .admins()
        .arg(Arg::number("Entity", 0.0..=f64::from(u32::MAX), 1.0));
    actions.add(decoy, move |request| {
        let [Value::Number(id)] = request.args else {
            return Outcome::refused("Name an entity");
        };
        let source = enshwick::ecs::Entity(*id as u32);
        let (Some(template), Some([x, y, z])) = (world.template(source), world.position(request.player.entity)) else {
            return Outcome::refused("Nothing to copy, or you are nowhere");
        };
        let how = Spawn::new().despawn_after(Duration::from_secs(60));
        Outcome::done(format!("{:?}", effects.spawn(template, [x + 2.0, y, z], how)))
    })
}
```

Items lying in the world are entities too, but a dropped stack carries the
item and its count in components the spawn does not fill; to hand a player
items, use [inventory transactions](inventories.md).

Every call works on the server's game thread, from a game event, a
[scheduler](events.md#the-scheduler) task or an [action](actions.md); on a
client, while it hosts a game.
