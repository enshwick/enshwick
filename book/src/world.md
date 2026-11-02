# The world

Unstable: everything in this chapter needs the `unstable` feature and may
change in any release ([Stability](stability.md)).

[`load.world()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/load.rs)
returns a
[`World`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/world.rs),
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
game; read what you need at once. Its type is an `ecs::ComponentType`,
resolved at init with `ComponentType::resolve`, which turns the mod off when
the type is not reflected on the running build; the same type is what
[`ecs::Components`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/ecs.rs)
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

`vitals(entity)` returns its health, stamina and mana, each a `Meter` with
`current` and `max`, buffs and gear included; `progress(entity)` a
character's `level`, `experience` toward the next level and
`experience_needed` for it. Health is there for every entity that has
vitals; stamina and mana only for those that have them, such as characters.

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
them from the server's own every step, so `WritableType::resolve` refuses them.

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
task or an action. The game thread runs ECS
systems itself while it steps, so a write from a filter or a hook on a system
is refused even there. They fail with a `WorldError`: `Gone` for an entity not
in the world, `Lacks` when it lacks the component, `Refused` for a type the
loader will not write, `WrongThread` off the game thread or inside a step, and
`Missing` on a build without writes. A field
of an engine type means what the engine makes of it: a write the engine does
not expect, such as a value its systems keep elsewhere, may be overwritten or
do nothing, so check a new write on a test server first.

## Finding entities

`entities(ty)` returns every entity with a component of type `ty` and a place
in the world, as `Placed { entity, position }`, position in metres;
`near(point, radius, ty)` those within `radius` metres of `point`, nearest
first. `transform_type()` is the component every placed entity has, so
`near(point, radius, world.transform_type())` finds everything there.
`containers(kinds)` lists the entities that hold items, by kind:
`ContainerKinds::default()` is what players build, such as storage chests;
the other constants of `ContainerKinds` name the rest, combined with `|`, and
`ContainerKinds::ALL` takes every kind. It finds nothing on a build that lacks
a component it tells the kinds apart by. The
engine walks every archetype with the component, so call them now and then,
from an action or a timer, not on every step.

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
the next step, the way the game removes things itself: no loot drops and no
death is raised. It refuses a player's character (`WorldError::Refused`) and
an entity not in the world (`WorldError::Gone`).

`save()` asks the server to save the world now, as its timed saves do;
[`WorldSaved`](events.md) follows once the save is on disk. A save already
under way counts as asked. Before the world has loaded it is refused.

`key()` names the world the server runs: its save file's name and a hash of
its folder, such as `world0-1a2b3c4d`, the same in every session. A
[world store](state.md#per-world-state) files the mod's state under it. A
world whose save moves to another folder gets a new key, and the loader
moves its mods' data along ([Move a world to another folder](https://enshwick.com/guides/run-a-server-in-docker/#move-a-world-to-another-folder)).

Like the reads, all three need the game thread; `key()` works from any
thread.

## Players

`player(entity)` returns the connected player whose character is `entity`;
`players()` returns every connected player with a character in the world. A
[`Player`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/world.rs)
has:

- `entity`, the character's entity;
- `slot`, the server's player slot;
- `key`, the same number for the same account in every session. Use it to
  remember a player between sessions;
- `permissions`, the server's permission bits, `PlayerInfo::CAN_*` in
  [`abi`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/abi.rs);
  `is_admin()` tells whether the player may kick and ban, the server's
  administrators;
- `name`, the name the client sent. It is untrusted: a client picks it, and two
  players may share it. Show it, but decide by `key` or `permissions`.

`player_by_key(key)` finds the connected player with an account key, and
`players_named(name)` the connected players with a name, ignoring case, as
the console finds a player a line names. Since names repeat, code that wants
one player refuses when it gets several.

### Kicking and banning

`account_action(action, key)` asks the game for one of its account
actions, as its administrators do; the mod decides who may ask:

- `AccountAction::Kick` removes a player online;
- `AccountAction::Ban` puts the account on the ban list and removes its
  player when online. An account that is not online goes straight onto the
  list; until the server restarts, the loader removes and bans it when it
  joins, since the game reads its list for joins only at start;
- `AccountAction::UnBan` takes the account off the list.

A dedicated server keeps the list in `bannedAccounts` of
`enshrouded_server.json`; a game a client hosts keeps it until the game
ends. The game never removes an administrator online, so a kick or a ban of
one returns `WorldError::Refused`; so do a kick or a ban of a hosted game's
host, a kick of an account not online, and a ban the game's list does not
take. A ban of an account on the list already succeeds.
Call it on the server's game thread between steps.

### The allowlist

With `allowlist = true` in `mod-configs/enshwick.toml`, a dedicated server
lets in only the accounts its allowlist lists, and its administrators; the
loader removes any other player as the player enters the world, and a player
the game gives no account. The game has
no such list, so the loader keeps it in `mod-data/enshwick/allowlist.txt`,
one account per line, with `#` notes.

`set_allowlisted(key, true)` lists an account and `set_allowlisted(key,
false)` takes it off; `allowlisted(key)` tells whether the list holds it. Both
work on any thread and with the setting off, and a change applies from the
next join. Outside a dedicated server they return `WorldError::Missing`.

```rust
use enshwick::WorldError;
use enshwick::world::World;

fn invite(world: &World, key: enshwick::PlayerKey) -> Result<(), WorldError> {
    if !world.allowlisted(key)? {
        world.set_allowlisted(key, true)?;
    }
    Ok(())
}
```

### Chat mutes

`set_muted(key, true)` mutes an account in chat and `set_muted(key, false)`
lifts the mute; `muted(key)` tells whether it is muted. The server drops
every line the muted player types before any other player reads it, and
tells the player why with one of the game's notices, at most every 30
seconds; a client without Enshwick reads the same as a chat line. Lines mods
send and mod messages pass. The loader keeps the mutes in
`mod-data/enshwick/muted.txt`, one account per line, with `#` notes, and
reads it at every start, so a mute lasts until it is lifted; an edit of the
file by hand applies at the next start.

An administrator online and a hosted game's host are never muted:
`set_muted` returns `WorldError::Refused` for them when it can tell, and
their lines pass even when their account was muted before. Both calls work on any
thread, on a dedicated server and in a game a client hosts; elsewhere they
return `WorldError::Missing`. The mod decides who may ask, such as an
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

## Build rights

`world.can_build(&player, a, b)` tells whether the server lets a player edit
the box between the corners `a` and `b`, in metres. A mod that digs, fills or
pastes for a player asks before it changes the world; the server decides, so
a client cannot talk its way past the check. The permissions follow the
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
[`BuildRefusal`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/world/build.rs)
named after the game's own answer: `NoPermission` with the missing
`PlayerPermissions` bit, `CursorOutsideBuildingZone` or
`CursorInsideStrictBuildInhibitorZone`. The check runs on the server's game
thread at a quiet moment only and is refused inside a step. When the server
cannot tell, off the game thread, inside a step, before a world is loaded or
on a game build without the checks, the answer is `Unchecked`, and the edit
is refused.

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

`world.set_label(entity, text)` names an entity as a player's rename does: the
game shows the name on hover, sends it to every client, vanilla ones included,
and saves it with the world. The game keeps 255 bytes; an empty text removes
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
[`load.on_rename(|rename| ...)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/world/rename.rs)
runs before every rename on the server, on its game thread, with a
[`Rename`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/world/rename.rs):
the `entity`, the `player` (`None` when the server cannot tell) and the new
`text`. Returning `false` refuses the rename, and the entity keeps its name.

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
[Chat lines](chat.md#chat-lines).
