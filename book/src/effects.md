<!--
type: how-to
description: The Effects handle of a mod raises game events in the server's world, teleports entities and spawns them, from event handlers, tasks and player actions.
-->

# Effects

[`load.effects()`](https://github.com/enshwick/enshwick/blob/main/crates/internal/core/src/effects.rs)
returns the mod's
[`Effects`](https://github.com/enshwick/enshwick/blob/main/crates/internal/core/src/effects.rs):
game events it raises in the server's world, teleports and spawns. Like
[`World`](world.md), it is `Copy`. Get it during `init` and move it into
handlers, tasks and actions.

```rust
use enshwick::actions::{Action, Outcome};
use enshwick::effects::Teleport;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let effects = load.effects();
    let actions = load.actions(enshwick::i18n::Texts::default())?;
    actions.add(Action::new("home", "Go to the world's centre"), move |request| {
        let me = request.player.entity;
        match effects.teleport(me, [0.0, 100.0, 0.0], Teleport::new().search_best_spawn_position()) {
            Ok(()) => Outcome::done("On your way"),
            Err(why) => Outcome::refused(format!("No teleport: {why}")),
        }
    })
}
```

## From Luau

A Luau mod reaches the same calls through `enshwick.effects`; here each
player who joins starts at the world's centre:

```luau
enshwick.on("PlayerJoined", function(player)
    local ok, why = pcall(enshwick.effects.teleport, player.entity, vector.create(0, 100, 0), {
        search_best_spawn_position = true,
    })
    if not ok then
        enshwick.log.warn(`no teleport: {why}`)
    end
end)
```

## Raising game events

An event a mod raises goes where the game's own go. The loader puts it into
the event stream the engine moves at the start of the next step. So the
systems that read its type act on it as on one the game raised.
[Game event](game-events.md) handlers see it too. The loader stamps it with
the game time.

Only types the server's own systems raise are allowed, each with a typed
call:

| Call | Event | What the game does with it |
| --- | --- | --- |
| `npc_command(npc, NpcCommandType::GoToPosition(at))` | `NpcCommandEvent` | the survivor with that survivor id walks to `at` |
| `npc_command(npc, NpcCommandType::Reset)` | `NpcCommandEvent` | the survivor drops the order and goes back to its routine |
| `set_toggle(sender, signal, on)` | `SetToggleStateEvent` | the logic signal switches, as a lever drives a door |
| `add_threat(attacker, target, threat)` | `AddThreatTargetEvent` | the enemy takes notice of the target |
| `ping(sender, at)` | `UiPingEvent` | a ping on the map and in the world, from the sender; unstable, needs the `unstable` feature |

`emit_raw(ty, bytes)` raises an event from its whole reflected layout, for
the same allowed types. The loader refuses any other type, and a size that
is not the type's. Each call fails with a
[`WorldError`](world.md#writing-components):

- `WrongThread` off the game thread;
- `Refused` for a type the loader does not allow, or when the step has no
  room for more events;
- `Missing` on a build without the event.

```rust
use enshwick::actions::{Action, Arg, Outcome, Value};
use enshwick::effects::NpcCommandType;

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
        Outcome::done(format!("{:?}", effects.npc_command(*id as u32, NpcCommandType::GoToPosition(at))))
    })
}
```

## Teleports

`teleport(entity, position, how)` moves an entity as the game's admin
teleport does. After the next step the entity jumps to `position`, in metres,
and clients see it arrive. `Teleport::new()` keeps the entity's orientation.
Two methods change the teleport:

- `.search_best_spawn_position()` lets the game look for a free spot nearby
  to stand on, as it does for respawns;
- `.facing_yaw(radians)` turns the entity on arrival.

It fails with `Gone` for an entity not in the world or without a place. It
fails with `Refused` for one the game's teleport does not move: it moves
actors and characters, not props. It fails with `Missing` where no server
runs or on a build without teleports.

### Teleport a player

`teleport_player(player, position, how)` moves a connected player's
character, by the player's account key. `teleport_player_to(player, to,
how)` moves it to where another connected player stands. Beside another
player the game always looks for a free spot, so the two never stand in one
place. Elsewhere, pass `.search_best_spawn_position()` so the player never
lands inside terrain or a wall.

Either call fails with `Gone` for a player who is not connected or has no
character in the world. Moving to another player also fails with `Gone`
when that player has no place in the world. A teleported player arrived
where the server put them, live, in a hosted game (client 26430625) and on
a dedicated server (26430253).

```rust
use enshwick::actions::{Action, Arg, Outcome, Value};
use enshwick::effects::Teleport;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let effects = load.effects();
    let actions = load.actions(enshwick::i18n::Texts::default())?;
    let to = Action::new("to", "Go to a player").admins().arg(Arg::player("Player"));
    actions.add(to, move |request| {
        let [Value::Player { key, name }] = request.args else {
            return Outcome::refused("Name a player");
        };
        match effects.teleport_player_to(request.player.key, *key, Teleport::new()) {
            Ok(()) => Outcome::done(format!("On your way to {name}")),
            Err(why) => Outcome::refused(format!("No teleport: {why}")),
        }
    })
}
```

```luau
local actions = enshwick.actions.actions(enshwick.i18n.parse(""))
actions:add({
    id = "to",
    label = "Go to a player",
    admins = true,
    args = { enshwick.actions.player("Player") },
}, function(request)
    local target = request:args()[1].Player
    local ok, why = pcall(enshwick.effects.teleport_player_to, request:player().key, target.key, {})
    if not ok then
        return enshwick.actions.refused(`No teleport: {why}`)
    end
    return enshwick.actions.done(`On your way to {target.name}`)
end)
```

## Spawns

`spawn(template, position, how)` spawns an entity of a template, as the
game's admin spawn does: an enemy, an animal, a survivor or a prop. The
template is a GUID as `World::template` gives it, so a mod can spawn copies of
what it finds in the world. The new entity's id comes back at once. The
entity is in the world after the next step. A template the game does not
know spawns nothing. `Spawn::new()` takes the template as it is. Three
methods change the spawn:

- `.level(n)` scales the entity as the game scales enemies;
- `.despawn_after(duration)` lets the game remove it after that much game
  time;
- `.orientation_yaw(radians)` turns it.

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
        let source = enshwick::Entity(*id as u32);
        let (Some(template), Some([x, y, z])) = (world.template(source), world.position(request.player.entity)) else {
            return Outcome::refused("Nothing to copy, or you are nowhere");
        };
        let how = Spawn::new().despawn_after(Duration::from_secs(60));
        Outcome::done(format!("{:?}", effects.spawn(template, [x + 2.0, y, z], how)))
    })
}
```

## Drops

`drop_items(item, count, position, how)` drops a stack of new items on the
ground, as the game spills loot: one loose pickup any player can take. The
pickup's entity comes back at once and is in the world after the next step.
The game does not save it. `count` is 1 to 65535; a larger stack is two
drops. `ItemDrop::new()` drops a common item without a level. Two methods
change it:

- `.level(n)` gives the item a level, as the game levels weapons and armour;
- `.rarity(rarity)` gives it an `ItemRarity`.

The first drop reads the names of the game's templates once, about a tenth of
a second on a dedicated server. An item the game does not know is refused
(`WorldError::Refused`).

```rust
use enshwick::actions::{Action, Outcome};
use enshwick::effects::ItemDrop;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let effects = load.effects();
    let world = load.world()?;
    let actions = load.actions(enshwick::i18n::Texts::default())?;
    let wood = enshwick::ItemId::from_guid(enshwick::content::Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d"));
    actions.add(Action::new("wood", "Five logs at your feet").admins(), move |request| {
        let Some([x, y, z]) = world.position(request.player.entity) else {
            return Outcome::refused("You are nowhere");
        };
        match effects.drop_items(wood, 5, [x, y + 1.0, z], ItemDrop::new()) {
            Ok(_) => Outcome::done("Dropped"),
            Err(why) => Outcome::refused(format!("{why}")),
        }
    })
}
```

To put items straight into a player's inventory or a chest, use
[inventory transactions](inventories.md).

Where each call works is in
[What may be called where](concepts/threads.md#what-may-be-called-where).
