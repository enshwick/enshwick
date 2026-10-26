# Effects

[`load.effects()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/effects.rs)
returns the mod's
[`Effects`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/effects.rs):
game events it raises in the server's world, teleports and spawns. Like
[`World`](world.md), it is `Copy`; get it during `init` and move it into
handlers, tasks and commands.

```rust
use enshwick::effects::Teleport;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let effects = load.effects();
    load.add_command("home", "takes you to the world's centre", move |ctx, _args| {
        let Some(me) = ctx.character() else {
            return "the console has no character".into();
        };
        match effects.teleport(me, [0.0, 100.0, 0.0], Teleport::new().best_spawn()) {
            Ok(()) => "on your way".into(),
            Err(why) => format!("no teleport: {why}").into(),
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
use enshwick::effects::NpcOrder;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let effects = load.effects();
    let world = load.world()?;
    load.add_command("come", "calls a survivor to you: /come <survivor id>", move |ctx, args| {
        let Some(me) = ctx.character() else {
            return "the console has no character".into();
        };
        let (Some(at), Some(id)) = (world.position(me), args.first().and_then(|a| a.parse().ok())) else {
            return "/come <survivor id>".into();
        };
        format!("{:?}", effects.npc_command(id, NpcOrder::GoTo(at))).into()
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
use enshwick::effects::Spawn;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let effects = load.effects();
    let world = load.world()?;
    load.add_command("decoy", "spawns a copy of an entity beside you for a minute: /decoy <entity>", move |ctx, args| {
        let Some(me) = ctx.character() else {
            return "the console has no character".into();
        };
        let source = args.first().and_then(|a| a.parse().ok()).map(enshwick::ecs::Entity);
        let (Some(template), Some([x, y, z])) = (source.and_then(|e| world.template(e)), world.position(me)) else {
            return "/decoy <entity>".into();
        };
        let how = Spawn::new().despawn_after(Duration::from_secs(60));
        format!("{:?}", effects.spawn(template, [x + 2.0, y, z], how)).into()
    })
}
```

Items lying in the world are entities too, but a dropped stack carries the
item and its count in components the spawn does not fill; to hand a player
items, use [inventory transactions](inventories.md).

Every call works on the server's game thread, from a game event, a
[scheduler](events.md#the-scheduler) task or a [command](commands.md); on a
client, while it hosts a game.
