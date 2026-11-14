<!--
type: how-to
description: The World handle reads and writes entities and their components in the server's world, finds them, destroys them, and reads players, build rights and labels.
-->

# The world

Unstable: everything in this chapter needs the `unstable` feature and may
change in any release ([Stability](stability.md)).

[`load.world()`](https://github.com/enshwick/enshwick/blob/main/crates/internal/world/src/world.rs)
returns a
[`World`](https://github.com/enshwick/enshwick/blob/main/crates/internal/world/src/world.rs),
the mod's way into the server's world.

## What it offers

A `World` offers reads of the server's world, component writes, character stats, entity
queries, build rights, labels, chat lines, destroy requests and saves. It is
`Copy`; get it during `init` and move it into handlers and tasks.

```rust
use enshwick::Entity;
use enshwick::events::PlayerJoined;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let world = load.world()?;
    load.on::<PlayerJoined>(move |joined| {
        if let Some(player) = world.player(joined.entity) {
            enshwick::info!("{} joined, admin: {}", player.name, player.is_admin());
        }
    })
}
```

```luau
enshwick.on("PlayerJoined", function(joined)
    local player = enshwick.world.player(joined.entity)
    if player then
        enshwick.log.info(`{player.name} joined, permissions {player.permissions}`)
    end
end)
```

In Luau, `enshwick.world` is the `World`. Its functions take the same
arguments, and a value such as a `Player` is a table of its fields.

`load.world()` turns the mod off on a game build without the component types the reads need.

## Where reads work

The reads serve the server's world from its game thread:

- in a game event handler (see [Events](events.md));
- in a [scheduler](events.md#the-scheduler) task;
- in an [action](actions.md) handler or a rename filter.

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
| `component(entity, ty)` | a read-only pointer to the entity's component of that type |

The pointer from `component` stays valid until the game thread returns to the
game; read what you need at once. Its type is an `ecs::ComponentType`.
Resolve it at init with `ComponentType::resolve`. That call turns the mod
off when the type is not reflected on the running build. The same type is
what
[`ecs::Components`](https://github.com/enshwick/enshwick/blob/main/crates/internal/base/src/ecs.rs)
takes inside systems ([ECS](ecs.md)).

```rust
use enshwick::Entity;
use enshwick::events::BuildingPlaced;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let world = load.world()?;
    load.on::<BuildingPlaced>(move |placed| {
        let entity = placed.entity;
        if let (Some(at), Some(item)) = (world.position(entity), world.used_item(entity)) {
            enshwick::debug!("item {item} placed at {at:?}");
        }
    })
}
```

## Stats

`vitals(entity)` returns an entity's health, stamina and mana, buffs and
gear included. Each is a `Meter`. A meter has `current` and `max`.
`progress(entity)` returns a character's level and experience:

- `level`
- `experience` toward the next level
- `experience_needed` for it

Health is there for every entity that has vitals. Stamina and mana are there
only for those that have them, such as characters.

```rust
use enshwick::events::PlayerJoined;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let world = load.world()?;
    load.on::<PlayerJoined>(move |joined| {
        let entity = joined.entity;
        if let (Some(vitals), Some(progress)) = (world.vitals(entity), world.progress(entity)) {
            enshwick::info!("level {} with {} health", progress.level, vitals.health.current);
        }
    })
}
```

Both read the entity's attribute sets as the game keeps them. On a game build
without the layout they need, they return `None`, and `load.world()` still
works.

Three calls set the current value and return the `Meter` the entity now has:

- `set_health(entity, value)`
- `set_stamina(entity, value)`
- `set_mana(entity, value)`

`heal(entity, amount)` adds to its health, and takes from it
for a negative amount. The value stays between a floor and the maximum, which
the game derives from gear and buffs and the writes leave alone:

| Stat | Floor | Refused when |
| --- | --- | --- |
| Health | 1: a write never kills | the entity is dead (`WorldError::Refused`): `revive` stands it up |
| Stamina | 0 | the entity has no stamina (`WorldError::Lacks`) |
| Mana | 0 | the entity has no mana (`WorldError::Lacks`) |

```rust
use enshwick::game_events::LevelUpEvent;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let world = load.world()?;
    // A level up restores the character's health.
    load.on::<LevelUpEvent>(move |gained| {
        let max = world.vitals(gained.player_entity_id).map_or(0, |v| v.health.max);
        if let Err(why) = world.set_health(gained.player_entity_id, max) {
            enshwick::debug!("no heal: {why}");
        }
    })
}
```

The game sends the new value to the clients and regenerates from it as from
any other.

`revive(entity, health)` stands a dead character up where it lies, as a
friend's revive does, with no respawn at a bed or flame. The health stays
between 1 and the maximum, and the call returns the `Meter` the character now
has. A character the game counts as dead has health below 1; the game's death
step sees it alive from the next step on. A living character is refused
(`WorldError::Refused`).

Three calls write a player's level and experience. Each queues the write; the
server's next level step applies it, where the game raises levels itself:

| Call | What the step does |
| --- | --- |
| `set_level(entity, level)` | sets the level, kept between 1 and the game's level cap. The level starts empty and needs what the game asks of it, as after a level-up, and a gain not applied yet is dropped. No level-up event |
| `set_experience(entity, experience)` | sets the experience inside the level, kept below what the level needs, and drops a gain not applied yet |
| `add_experience(entity, amount)` | adds a gain, as a kill does; the game levels up from it, one level a step, with its own level-up events. Experience filters do not see it |

Skill points follow the level, as the game counts them from it. Experience
filters (`Load::on_experience`) do not see these writes. Each call is refused
(`WorldError::Refused`) for an entity that is not a player's character in the
game, and while 256 writes wait for the step. `progress(entity)` reads the
result after the step. The `ProgressWritten` event tells the mod that wrote,
after the step, what each write left or why it was dropped
([events](events.md)).

```rust
use enshwick::actions::{Action, Outcome};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let world = load.world()?;
    let actions = load.actions(enshwick::i18n::Texts::default())?;
    actions.add(Action::new("level-ten", "Set your level to 10").admins(), move |request| {
        match world.set_level(request.player.entity, 10) {
            Ok(()) => Outcome::done("Level 10 from the next step"),
            Err(why) => Outcome::refused(format!("{why}")),
        }
    })
}
```

`item_meter(pide)` reads the [meter](content/items.md#meters) of an item's
own entity. It returns a `Meter` with the count and its maximum.
`change_item_meter(pide, change)` sets the meter to what `change` makes of
the count, kept between 0 and the maximum. The game saves it with the world
and shows it on the item's slot.

`shroud_time(entity)` reads how long, in seconds, a character holds out in
the shroud before it drains health. It returns a `Meter` with the most the
character holds. `change_shroud_time(entity, change)` sets it, kept between
0 and that most; clients see it from the next step. The game refills it
outside the shroud and saves none of it. `in_shroud(entity)` tells whether a character
stands in the shroud where it drains them, outside the altars' reach.

`skills(entity)` lists the skill nodes a character unlocked, each a `Skill`.
A `Skill` has the node's id in the game's skill tree. For nodes that level
up, it also has the level. Unlocking and unlearning stay the player's: the game resets a
whole tree it finds unconnected or over its points when the character loads.

## Writing components

A write goes to the component the server owns. The loader marks it changed
the way the game's own writes do. So the game sends it to clients and saves
it with the world. The game keeps copies for clients, the `Network*` and
`Client*` components. These are never written: the game overwrites them
from the server's own every step. So `WritableType::resolve` refuses them.

| Call | Does |
| --- | --- |
| `WritableType::resolve(load, name)` | a component type to write, such as `keen::ecs::CombatFlags`; call at init |
| `update(entity, ty, \|bytes\| ..)` | hands the component's reflected bytes to the closure, marked changed |
| `ty.field::<T>(load, name)` | a typed field of the type, resolved by name for the running build |
| `get(entity, field)` | the field's value, `None` when the entity lacks the component |
| `set(entity, field, value)` | writes the field, marked changed |

```rust
use enshwick::actions::{Action, Outcome};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let world = load.world()?;
    let flags = enshwick::world::WritableType::resolve(load, "keen::ecs::CombatFlags")?;
    let immune = flags.field::<bool>(load, "immune")?;
    let actions = load.actions(enshwick::i18n::Texts::default())?;
    actions.add(Action::new("immune", "Toggle your immunity").admins(), move |request| {
        let entity = request.player.entity;
        let now = !world.get(entity, immune).unwrap_or(false);
        match world.set(entity, immune, now) {
            Ok(()) => Outcome::done(format!("Immune: {now}")),
            Err(why) => Outcome::refused(format!("{why}")),
        }
    })
}
```

Writes work on the game thread between steps, in a game event, a scheduled
task or an action. The game thread runs ECS systems itself while it steps.
So a write from a filter or a hook on a system is refused even there. Writes
fail with a `WorldError`:

| Error | When |
| --- | --- |
| `Gone` | the entity is not in the world |
| `Lacks` | the entity lacks the component |
| `Refused` | the loader will not write the type |
| `WrongThread` | the write runs off the game thread or inside a step |
| `Missing` | no server runs, as on a client in another player's game, or the build has no writes |

A field of an engine type means what the engine makes of it. The engine may
not expect a write, such as a value its systems keep elsewhere. Such a write
may be overwritten or do nothing, so check a new write on a test server
first.

## Finding entities

`entities(ty)` returns every entity with a component of type `ty` and a place
in the world. Each comes as `Placed { entity, position }`, position in
metres. `near(point, radius, ty)` returns those near `point`, nearest first. They
lie within `radius` metres of it. `transform_type()` is the component every placed
entity has. So `near(point, radius, world.transform_type())` finds
everything there.

`containers(kinds)` lists the entities that hold items, by kind:

- `ContainerKinds::default()` is what players build, such as storage chests.
- The other constants of `ContainerKinds` name the rest, combined with `|`.
- `ContainerKinds::ALL` takes every kind.

It finds nothing on a build that lacks a component it tells the kinds apart
by. The engine walks every archetype with the component. So call them
now and then, from an action or a timer, not on every step.

```rust
use enshwick::actions::{Action, Outcome};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let world = load.world()?;
    let beds = enshwick::ecs::ComponentType::resolve(load, "keen::ecs::NpcBed")?;
    let actions = load.actions(enshwick::i18n::Texts::default())?;
    actions.add(Action::new("beds", "Count the beds"), move |_request| {
        Outcome::done(format!("{} beds", world.entities(beds).len()))
    })
}
```

## Destroying entities and saving

`destroy(entity)` asks the server to remove an entity after the systems of
the next step. It removes it the way the game removes things itself: no loot
drops and no death is raised. It refuses a player's character with
`WorldError::Refused`. It refuses an entity not in the world with
`WorldError::Gone`. It fails with `WorldError::Missing` where no server runs
or on a build without the command.

`save()` asks the server to save the world now, as its timed saves do;
[`WorldSaved`](events.md) follows once the save is on disk. A save already
under way counts as asked. Before the world has loaded it is refused.

`key()` names the world the server runs: its save file's name without
`.ksd`. An example is `world0`, the same in every session. The game gives a
deleted world's name to its next new world, so a key names a slot.
`data_dir()` is the mod's folder in that world, `<stem>-mod-data/<id>`
beside the save. There a [world store](state.md#per-world-state) keeps the
mod's state. The loader sets the folder aside when a new world takes the
name ([State](state.md#when-a-new-world-takes-a-saves-name)).

Like the reads, all three need the game thread; `key()` and `data_dir()`
work from any thread.

[`WorldSaving`](events.md) is raised after the step in which a save got
under way, before [`WorldSaved`](events.md). What the mod writes then
belongs to that save: its world store and files in its data folder.
`save_info()` reads the save on disk:

- the world's GUID, which stays the same across saves
- when it was written
- its size
- how many backups the game keeps beside it

It is `None` before the first save of a new world.

`hold_saves(true)` holds the world's saves and `hold_saves(false)` lets
them go, for a mod that saves only while players are in safe places. The
game saves only while no running mod holds its saves: the timed saves wait
and `save()` is refused meanwhile. Each mod holds its own. The log names each mod
that holds or lets go. A mod that turns off lets go, and every hold ends
with its hosted game. Once saves stay held for 10 minutes, the log warns and each
administrator online gets a notice, again every 10 minutes while they stay
held.

When a dedicated server or a hosted game shuts down, every hold lets go and
the final save runs. The log names each mod that held saves and for how
long. `ShutdownStarted` says that the next save is the final one. A mod that
calls `hold_saves(true)` inside its handler for that event refuses the final
save, and the log names it. A hold asked for after that handler is not
taken, so a mod that keeps calling `hold_saves(true)` does not stop the final
save.

```rust
use enshwick::events::ShutdownStarted;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let world = load.world()?;
    // This world must stay as it was loaded: no final save.
    load.on::<ShutdownStarted>(move |()| {
        if let Err(why) = world.hold_saves(true) {
            enshwick::warn!("the final save runs: {why}");
        }
    })?;
    Ok(())
}
```

```luau
-- This world must stay as it was loaded: no final save.
enshwick.on("ShutdownStarted", function()
    enshwick.world.hold_saves(true)
end)
```

`stop()` stops a dedicated server the way Ctrl+C in its console does, for
a timed restart. After the current step the server shuts down, and
`ShutdownStarted` follows as the holds let go. Then the final save runs,
and the process exits with code 0. The host's panel or service manager
then starts it again. The call returns at once, from any thread. The countdown,
the warnings to players and a save before it are the mod's.

A second `stop()` while the server stops fails with `WorldError::Refused`,
as does one after a person pressed Ctrl+C or before the game has started.
In single player and in a hosted game, which have no restart, it fails with
`WorldError::Missing`. A server
under Wine or Proton has not been tried.

```rust
fn countdown_over(world: &enshwick::world::World) {
    if let Err(why) = world.stop() {
        enshwick::warn!("server not stopped: {why}");
    }
}
```

```luau
local ok, why = pcall(enshwick.world.stop)
if not ok then
    enshwick.log.warn(`server not stopped: {why}`)
end
```

An entity spawned with `Spawn::new().do_not_save()` stays out of the save,
as the game's own passing animals do: it is gone once the world loads again.
`set_saved(entity, false)` leaves out an entity a mod spawned or placed
since the world loaded, from the step after its spawn.
`set_saved(entity, true)` puts it back in. After the next step the game
gives the entity its `DoNotSave` component, or takes it away. The next save
that starts after that leaves the entity out or keeps it.
Vanilla entities are refused, so no mod drops the game's own data from a
save. An entity not in the world fails with `WorldError::Gone`; where no
server runs, or on a build without the commands, the call fails with
`WorldError::Missing`.

```rust
use enshwick::events::WorldSaving;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let world = load.world()?;
    load.on::<WorldSaving>(move |()| {
        if let Some(info) = world.save_info() {
            enshwick::info!("saving over a save of {} bytes", info.size);
        }
    })?;
    // Hold saves while a boss fight runs, and let them go after it.
    world.hold_saves(true)?;
    Ok(world.hold_saves(false)?)
}
```

```luau
enshwick.on("WorldSaving", function()
    local info = enshwick.world.save_info()
    if info then
        enshwick.log.info(`saving over a save of {info.size} bytes`)
    end
end)
-- Hold saves while a boss fight runs, and let them go after it.
enshwick.world.hold_saves(true)
enshwick.world.hold_saves(false)
```

From Luau, a spawn the save leaves out is
`enshwick.effects.spawn(template, position, { do_not_save = true })`, and a
live entity is left out with `enshwick.world.set_saved(entity, false)`.

## Players

`player(entity)` returns the connected player whose character is `entity`.
`players()` returns every connected player with a character in the world. A
[`Player`](https://github.com/enshwick/enshwick/blob/main/crates/internal/world/src/world.rs)
has:

- `entity`, the character's entity;
- `slot`, the server's player slot;
- `key`, the same number for the same account in every session. Use it to
  remember a player between sessions;
- `permissions`, the server's permission bits. They are `PlayerInfo::CAN_*` in
  [`abi`](https://github.com/enshwick/enshwick/blob/main/crates/internal/abi/src/abi.rs).
  `is_admin()` tells whether the player may kick and ban, the server's
  administrators;
- `name`, the name the client sent. It is untrusted: a client picks it, and two
  players may share it. Show it, but decide by `key` or `permissions`.

`player_by_key(key)` finds the connected player with an account key.
`players_named(name)` finds the connected players with a name, ignoring
case, as the console finds a player a line names. Since names repeat, code that wants
one player refuses when it gets several.

### Kicking and banning

`account_action(action, key)` asks the game for one of its account
actions, as its administrators do; the mod decides who may ask:

- `AccountAction::Kick` removes a player online;
- `AccountAction::Ban` puts the account of a player online on the ban list
  and removes the player;
- `AccountAction::UnBan` takes the account off the list, online or not.

A dedicated server keeps the list in `bannedAccounts` of
`enshrouded_server.json`; a game a client hosts keeps it until the game
ends. The game never removes an administrator online. So a kick or a ban of one
returns `WorldError::Refused`. So does a kick or a ban of a hosted game's
host, and of an account not online. A ban of an account on
the list already succeeds. Call it on the server's game thread between steps.

### Chat mutes

`set_muted(key, true)` mutes an account in chat, and
`set_muted(key, false)` lifts the mute. `muted(key)` tells whether it is
muted. The server drops every line the muted player types before any other
player reads it. It tells the player why with one of the game's notices, at
most every 30 seconds. A client without Enshwick reads the same as a chat
line. Lines mods send and mod messages pass.

Mutes belong to the world. The loader keeps them beside its save in
`<save>-mod-data/enshwick/muted.txt`, one account per line, with `#` notes.
It reads the file as the world loads. So a mute lasts until it is lifted
and travels with the save. An edit of the file by hand applies at the next
world load.

An administrator online and a hosted game's host are never muted.
`set_muted` returns `WorldError::Refused` for them when it can tell, and
their lines pass even when their account was muted before. Both calls work
on any thread while a world runs, on a dedicated server and in a game a
client hosts. Elsewhere `set_muted` returns `WorldError::Missing`. The mod decides who may ask, such as an
[action](actions.md) only administrators see.

```rust
use enshwick::WorldError;
use enshwick::world::World;

fn toggle_mute(world: &World, key: enshwick::PlayerKey) -> Result<bool, WorldError> {
    let muted = !world.muted(key)?;
    world.set_muted(key, muted)?;
    Ok(muted)
}
```

### Join rules

[`load.on_join(|join| ...)`](https://github.com/enshwick/enshwick/blob/main/crates/internal/world/src/world/join.rs)
runs for each account that joins the server, before its player enters the
world. It gets a
[`Join`](https://github.com/enshwick/enshwick/blob/main/crates/internal/world/src/world/join.rs)
with the account's `player` key, the same key its player and the ban list
carry. The key is `None` for an account the game gives no key.

Returning `false` refuses the account. The server answers its game as it
answers an account on the ban list. The player reads that the host denies
access, and no world loads. The ban list stays as it was. Rules run in load
order, and the first one that refuses decides. A mod that is off is not
asked.

The rule runs on the thread the server reads joins on, so it answers at
once from what the mod holds. A panic in the rule turns the mod off and lets
that account in. A dedicated server and a game a client hosts both ask it.

```rust
use enshwick::PlayerKey;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    // Only the accounts on this mod's list join.
    let allowed = [PlayerKey::new(0x5c86_aa43_5230_614e)];
    load.on_join(move |join| join.player.is_some_and(|key| allowed.contains(&key)))
}
```

In Luau, `enshwick.world.on_join(function(join) ... end)` takes the same
rule. There `join.player` is the key as a decimal string, or `nil` for an
account without one.

## Build rights

`world.can_build(&player, a, b)` tells whether the server lets a player edit
a box. The box lies between the corners `a` and `b`, in metres. A mod that
digs, fills or pastes for a player asks before it changes the world. The
server decides, so a client cannot talk its way past the check. The permissions follow the
game's terrain tools:

- every edit needs the `CanEditWorld` permission;
- an edit that reaches into a Flame Altar's building zone also needs
  `CanEditBase`. A box that only shares a face with a zone stays outside it.

Enshwick adds two tests the game's building cursor runs for items, which its
terrain tools skip:

- both corners lie inside the world's building area;
- the box stays out of strict no-build zones the world places. Lenient ones
  and the zones creatures carry around them do not refuse an edit.

Permissions are the server's user groups and hold in every base alike: bases
have no owner. A player no longer in game has none. An item's other rules,
such as one that needs a building zone, are not part of the check.

`Err` carries a
[`BuildRefusal`](https://github.com/enshwick/enshwick/blob/main/crates/internal/world/src/world/build.rs)
named after the game's own answer:

- `NoPermission` with the missing `PlayerPermissions` bit
- `CursorOutsideBuildingZone`
- `CursorInsideStrictBuildInhibitorZone`

The check runs on the server's game thread at a quiet moment only and is
refused inside a step. Sometimes the server cannot tell: off the game
thread, inside a step, before a world is loaded or on a game build without
the checks. Then the answer is `Unchecked`, and the edit is refused.

```rust
use enshwick::Entity;
use enshwick::world::BuildRefusal;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let world = load.world()?;
    load.on::<enshwick::events::PlayerJoined>(move |joined| {
        let Some(player) = world.player(joined.entity) else {
            return;
        };
        let Some([x, y, z]) = world.position(player.entity) else {
            return;
        };
        match world.can_build(&player, [x - 4.0, y - 4.0, z - 4.0], [x + 4.0, y + 4.0, z + 4.0]) {
            Ok(()) => enshwick::info!("{} may build where they stand", player.name),
            Err(BuildRefusal::NoPermission(missing)) => {
                enshwick::info!("{} lacks {missing:?} here", player.name);
            }
            Err(why) => enshwick::debug!("no building here: {why}"),
        }
    })
}
```

## Labels

`world.set_label(entity, text)` names an entity as a player's rename does.
The game shows the name on hover and sends it to every client, vanilla ones
included. It saves the name with the world. The game keeps 255 bytes; an empty text removes
the name. It works on the game thread only, and returns `false` elsewhere or
when the entity is gone.
`world.has_label(entity)` tells whether an entity already carries such a
name, or `None` when the world cannot tell.

```rust
use enshwick::Entity;
use enshwick::events::BuildingPlaced;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let world = load.world()?;
    load.on::<BuildingPlaced>(move |placed| {
        if let Some(player) = world.player(placed.player) {
            let label = format!("Built by {}", player.name);
            let _labelled = world.set_label(placed.entity, &label);
        }
    })
}
```

The game checks no permission when a player renames something.
[`load.on_rename(|rename| ...)`](https://github.com/enshwick/enshwick/blob/main/crates/internal/world/src/world/rename.rs)
runs before every rename on the server, on its game thread. It gets a
[`Rename`](https://github.com/enshwick/enshwick/blob/main/crates/internal/world/src/world/rename.rs)
with:

- the `entity`
- the `player`, `None` when the server cannot tell
- the new `text`

Returning `false` refuses the rename, and the entity keeps its name.

```rust
fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    // Only administrators rename anything.
    load.on_rename(|rename| rename.player.as_ref().is_some_and(|p| p.is_admin()))
}
```

A panic in the filter turns the mod off and lets that rename through.
[`enshwick-personal-safes`](https://github.com/enshwick/mods/tree/main/personal-safes)
uses labels and a rename filter for its Safes.

## Chat lines

`world.broadcast(text)` and `world.tell(&player, text)` send chat lines; see
[Chat lines](chat.md#chat-lines). `world.language_of(player.key)` gives the
language a player reads the game in; see
[A text in each player's language](chat.md#a-text-in-each-players-language).
