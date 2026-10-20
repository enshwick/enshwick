# Actions

[`load.actions()`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/actions.rs)
returns the mod's
[`Actions`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/actions.rs):
game events it raises in the server's world, and teleports. Like
[`World`](world.md), it is `Copy`; get it during `init` and move it into
handlers, tasks and commands. It needs ABI minor 23.

```rust
use enshwick_loader_api::actions::Teleport;

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    let actions = load.actions()?;
    load.add_command("home", "takes you to the world's centre", move |ctx, _args| {
        match actions.teleport(ctx.player.entity, [0.0, 100.0, 0.0], Teleport::new().best_spawn()) {
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
use enshwick_loader_api::actions::NpcOrder;

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    let actions = load.actions()?;
    let world = load.world()?;
    load.add_command("come", "calls a survivor to you: /come <survivor id>", move |ctx, args| {
        let (Some(at), Some(id)) = (world.position(ctx.player.entity), args.first().and_then(|a| a.parse().ok())) else {
            return "/come <survivor id>".into();
        };
        format!("{:?}", actions.npc_command(id, NpcOrder::GoTo(at))).into()
    })
}
```

## Teleports

`teleport(entity, position, how)` moves an entity as the game's admin
teleport does: after the next step the entity jumps to `position`, in metres,
and clients see it arrive. `Teleport::new()` keeps the entity's orientation;
`.best_spawn()` lets the game look for a free spot nearby to stand on, as it
does for respawns, and `.facing_yaw(radians)` turns the entity on arrival.
It fails with `Gone` for an entity not in the world or without a place.

Every call works on the server's game thread, from a game event, a
[scheduler](events.md#the-scheduler) task or a [command](commands.md); on a
client, while it hosts a game.
