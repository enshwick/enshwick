# Events and the scheduler

The loader tells mods what happens in the game through events, and runs work
on the game thread for them through the scheduler.

## Subscribing

[`load.on::<E>(|data| ...)`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/load.rs)
runs a handler each time the loader raises event `E`, with its typed data:

```rust
use enshwick_loader_api::events::{PlayerJoined, WorldSaved};

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    load.on::<PlayerJoined>(|player| enshwick_loader_api::info!("entity {} joined", player.entity))?;
    load.on::<WorldSaved>(|()| enshwick_loader_api::info!("saved"))
}
```

- Subscribe during `init`. Once every mod is initialised the loader closes
  subscriptions; a later call fails.
- The handler runs on the thread that raises the event, until the mod is off.
- A panic in the handler turns the mod off.
- The loader links the game functions behind an event only when some mod
  subscribed to it, so an unused event costs nothing.

[`load.subscribe(event, |raw| ...)`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/load.rs)
is the untyped form: it takes an
[`abi::Event`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/abi.rs)
and gives the handler the raw data pointer, null for events without data.

## Event kinds

The types are in
[`enshwick_loader_api::events`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/events.rs).

| Event | Data | When | Thread |
| --- | --- | --- | --- |
| `Tick` | `TickData` (`step`) | after each simulation step: about 20 a second with nobody online, 60 with players; from before `WorldLoaded` | game |
| `WorldLoaded` | none | the world finished loading and the server runs | game |
| `WorldSaved` | none | a save of the world finished writing to disk | game |
| `PlayerJoined` | `PlayerData` (`slot`, `entity`, `key`) | a player's character entered the world | game |
| `PlayerLeft` | `PlayerData` | a player who joined is leaving; the character is still in the world | game |
| `Shutdown` | none | an orderly stop begins, with the world and players alive; the final save follows | game |
| `BuildingPlaced` | `BuildingData` | a player placed a building piece or prop; raised after the step that placed it | game |
| `BuildingRemoved` | `BuildingData` | a player picked up or tore down a placed piece; the entity no longer exists | game |
| `SettingsChanged` | none | the mod's settings file changed, or a client's join values changed | loader |
| `LoaderSettingsChanged` | none | `mod-configs/enshwick.toml` changed | loader |
| `JoinMissing` | none | the join check kept this client out of a server | game |

The data:

- `TickData::step` counts steps since the loader started counting, from 1.
- `PlayerData` has the server's player `slot` (0 to 15), the character's
  `entity` and `key`, the same number for the same player account in every
  session. [`World::player`](world.md) gives the rest, such as the name.
- `BuildingData` has the `entity`, the acting `player`'s character entity, the
  `item` placed, `kind` (`BuildingData::PLACED`, `PICKED_UP` or `TORN_DOWN`),
  `replay` (1 when undo or redo replayed the action), the `template` GUID and
  the `position` in metres. A removal has `item`, `template` and `position`
  zeroed.

`on_config_change` wraps `SettingsChanged` for typed settings; see
[Settings](settings.md). The loader re-reads log levels on
`LoaderSettingsChanged` for every mod by itself.

`JoinMissing` is raised on the client, once the player closed the loader's
note about mods the server runs and the client lacks or runs in another
version.
[`mods::join_missing()`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/mods.rs)
lists them with the server's versions and update sources. The handler runs on
the game thread, so hand downloads to a thread of your own, as
[`enshwick-updater`](https://github.com/themaiby/enshwick/tree/main/mods/enshwick-updater)
does.

## Engine events

Beyond these, a mod observes the game's own ECS events, such as hits, deaths
and levels gained; see [Engine events](game-events.md).

## Server-only game events

Every event from `Tick` to `BuildingRemoved` comes from the server, on its
game thread. Keep handlers short: the game waits for them. Leave slow work,
such as file or network access, to a thread of your own.

On a client they come from the built-in server while the client hosts a game,
solo play included, and from nothing otherwise. Each hosted game starts with
`WorldLoaded` and ends with `Shutdown`. The next game has a new server and new
entities, so reset state tied to one game, such as entity ids, on `Shutdown`.
See [Sides and hosting](concepts/sides-and-hosting.md).

A killed process raises no `Shutdown`.

## The scheduler

[`load.scheduler()`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/load.rs)
returns a
[`Scheduler`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/schedule.rs),
a `Copy` handle that puts work on the server's game thread after a simulation
step:

- `next_tick(f)` runs `f` after the next step;
- `after(delay, f)` runs `f` after the first step at least `delay` from now;
- `every(period, f)` runs `f` about every `period`, the first time a period
  from now, while `f` returns `ControlFlow::Continue(())`; `Break(())` stops it.

```rust
use std::ops::ControlFlow;
use std::time::Duration;

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    let scheduler = load.scheduler()?;
    scheduler.after(Duration::from_secs(5), || enshwick_loader_api::info!("five seconds in"));
    scheduler.every(Duration::from_secs(60), || {
        enshwick_loader_api::info!("a minute passed");
        ControlFlow::Continue(())
    });
    Ok(())
}
```

- Get the scheduler during `init`: it subscribes to `Tick` for you.
- Then copy it anywhere and schedule from any thread, a hook or a background
  thread included.
- Tasks run where the game's server code runs, so they may call into the game,
  [read the world](world.md) and [run commands](commands.md).
- Tasks run only while steps run: on a dedicated server, or on a client while
  it hosts a game. Timing follows steps, so a task is late by up to one step.
- An idle step costs one atomic load.
