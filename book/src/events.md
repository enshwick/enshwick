<!--
type: how-to
description: The loader tells a mod what happens in the game through typed events, and its scheduler runs the mod's work on the game thread, from Rust and from Luau.
-->

# Events and the scheduler

The loader tells mods what happens in the game through events, and runs work
on the game thread for them through the scheduler.

## Subscribing

[`load.on::<E>(|data| ...)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/load.rs)
runs a handler each time the loader raises event `E`, with its typed data:

```rust
use enshwick::events::{PlayerJoined, WorldSaved};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.on::<PlayerJoined>(|player| enshwick::info!("entity {} joined", player.entity.0))?;
    load.on::<WorldSaved>(|()| enshwick::info!("saved"))
}
```

```luau
enshwick.on("PlayerJoined", function(player)
    enshwick.log.info(`entity {player.entity} joined`)
end)
enshwick.on("WorldSaved", function()
    enshwick.log.info("saved")
end)
```

- Subscribe during `init`. Once every mod is initialised the loader closes
  subscriptions; a later call fails.
- The handler runs on the thread that raises the event, until the mod is off.
- A panic in the handler turns the mod off.
- The loader links the game functions behind an event only when some mod
  subscribed to it, so an unused event costs nothing.

[`load.on_raw_event(event, |raw| ...)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/load.rs)
is the untyped form: it takes an
[`abi::Event`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/abi.rs)
and gives the handler the raw data pointer, null for events without data.

## Event kinds

The types are in
[`enshwick::events`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/events.rs).

| Event | Data | When | Thread |
| --- | --- | --- | --- |
| `Ticked` | `TickData` | After a server step, every 100 ms by default. See [Ticks](#ticks). It starts before `WorldLoaded`. | game |
| `WorldLoaded` | none | The world finished loading and the server runs. | game |
| `WorldSaved` | none | A save of the world finished writing to disk. | game |
| `PlayerJoined` | `PlayerData` | A player's character entered the world. | game |
| `PlayerLeft` | `PlayerData` | A player who joined is leaving. The character is still in the world. | game |
| `ShutdownStarted` | none | An orderly stop begins, with the world and players alive. The final save follows unless a mod holds saves inside its handler ([saves](world.md)). | game |
| `BuildingPlaced` | `BuildingData` | A player placed a building piece or prop. It is raised after the step that placed it. | game |
| `BuildingRemoved` | `BuildingData` | A player picked up or tore down a placed piece. The entity no longer exists. | game |
| `SettingsChanged` | none | A settings file of the mod in the running world changed, or the console or the Mods screen changed a setting. It is also raised when the server of another's world sent new values. | not fixed: loader or game |
| `LoaderSettingsChanged` | none | `enshwick.toml` in the game's folder changed. | loader |
| `JoinRefused` | none | The join check kept this client out of a server. | game |
| `AltarsChanged` | none | A Flame Altar of the server's world was placed, removed, moved, raised a level or had its zone built anew. Only a mod that called `load.altars()` gets it ([altars](altars.md)). | game |
| `SettingsReplaced` | none | A world loaded, so its settings apply, or stopped, so the defaults apply again. On a client, the server of another's world sent its settings, or the client left it. | game |
| `DayStarted` | none | The day began in the server's world, by the clock or a time a mod set ([time of day](daytime.md)). | game |
| `NightStarted` | none | The night began in the server's world. | game |
| `KnowledgeChanged` | `KnowledgeData` | A knowledge value of a player (or the world, entity 0) changed ([knowledge](knowledge.md)). | game |
| `QuestCompleted` | `QuestData` | A player completed a quest and got its rewards. | game |
| `ChunkLoaded` | `ChunkData` | A 256 m chunk of the server's world loaded as players came near. Its entities exist now ([weather and the world's chunks](weather.md)). | game |
| `ChunkUnloaded` | `ChunkData` | A chunk unloaded once no player was near. Its entities are gone. | game |
| `PlayerDied` | `PlayerDeathData` | A player died from a hit, a fall, the shroud or drowning, and the data names the cause. For a hit, it also names the killer's template and the attacker of the last hit. It is raised after the step. Freezing and an administrator's kill raise none. Any other entity's death is the game event `Died`. Unstable. | game |

The data types have these fields:

- `TickData::step` counts steps since the loader started counting, from 1.
  The other fields of `TickData` are in [Ticks](#ticks):
  - `elapsed_steps`
  - `elapsed_nanos`
  - `step_nanos`
  - `cost_nanos`
  - `game`
- `PlayerData` has three fields. [`World::player`](world.md) gives the rest,
  such as the name.
  - `slot`: the server's player slot, 0 to 15.
  - `entity`: the character's entity.
  - `key`: the same number for the same player account in every session.
- `BuildingData` has seven fields. A removal has `item` and `template`
  zeroed. Its `position` is zeroed too.
  - `entity`
  - `player`: the acting player's character entity.
  - `item`: the item placed.
  - `kind`: one of these constants:
    - `BuildingData::PLACED`
    - `PICKED_UP`
    - `TORN_DOWN`
  - `replay`: 1 when undo or redo replayed the action.
  - `template`: the template GUID.
  - `position`: the position in metres.
- `KnowledgeData` has three fields:
  - `entity`
  - `id`
  - `value`
- `QuestData` has two fields:
  - `entity`
  - `quest`
- `ChunkData` has four fields:
  - `index`
  - `x`
  - `z`
  - `size`
- `PlayerDeathData` has eight fields:
  - `entity`
  - `cause`
  - `key`
  - `position`
  - `cause_guid`
  - `was_deadly_fog`
  - `drowned`
  - `killer_entity`

For typed settings, `on_settings` wraps `SettingsReplaced`.
`on_settings_change` wraps both events; see [Settings](settings.md). The loader re-reads log levels on
`LoaderSettingsChanged` for every mod by itself.

When and how `JoinRefused` is raised, and `mods::join_missing()`, which lists
the mods the player lacks, are in
[The join check and a refused join](releasing.md#the-join-check-and-a-refused-join).

## Ticks

On dedicated server build 399, the simulation runs fixed ticks of 1/60 s,
and one step runs 1 to 3 of them. Both were read from the server's code. With nobody
online the server made 20 steps a second over 229 minutes of its own
statistics. With players its loop waits only 1 ms, which allows up to 60
steps a second. A client hosting a world with one player in it made 60
steps a second over 20 minutes of the game's own statistics (client 426). A
dedicated server with a real player is not yet measured. Steps are not
time, so a tick handler runs on a period of wall time:

- `load.on::<Ticked>(f)` runs `f` after the first step. Then it runs again
  after the first step that comes at least `Ticked::DEFAULT_EVERY` (100 ms)
  after its last call.
- `load.on_tick(every, f)` picks the period. A period of `Duration::ZERO`
  runs `f` on every step.
- `load.on_every_step(f)` runs `f` on every step, always. Use it for state
  that changes each step, input or the camera.

A call costs about 1 µs (measured on a server copy with 11 mods, build 399).
Most of it is the processor fetching the mod's code and data afresh. Steps a handler skips never enter the mod and cost it nothing.

`TickData` says what passed since the handler's last call:

- `elapsed_steps`: the server steps, 1 for an every-step handler;
- `elapsed()`: the wall time, for a first call the time since the step
  before;
- `step_interval()`: the average time between steps over about the last 16
  steps;
- `cost()`: what one call of the mod's event handlers cost on average since
  the last call. It has a value when the loader's `diagnostics` setting
  times callbacks. Otherwise, and for every-step handlers, it is `None`.

```rust
use std::time::Duration;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.on::<enshwick::events::Ticked>(|tick| {
        enshwick::debug!("{} steps in {:?}", tick.elapsed_steps, tick.elapsed());
    })?;
    load.on_tick(Duration::from_secs(1), |_| enshwick::debug!("a second"))?;
    load.on_every_step(|tick| enshwick::debug!("step {}", tick.step))
}
```

`on_tick` returns a `TickPeriod`. It is a `Copy` handle to keep anywhere,
the handler included. Its `set_every(period)` takes effect on the next step. The
next call then comes a period after the last one, or at once if that time has
passed. The every-step handler of `on_every_step` has no period
to change.

### An adaptive tick

A handler that need not keep pace with the server can follow its load. This
one runs every 100 ms to 2 s. It slows down while steps come late or its own
calls cost much. It speeds up again when there is room.

```rust
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use enshwick::abi::TickData;
use enshwick::events::TickPeriod;

const FASTEST: Duration = Duration::from_millis(100);
const SLOWEST: Duration = Duration::from_secs(2);

static PERIOD: OnceLock<TickPeriod> = OnceLock::new();
static EVERY_MS: AtomicU64 = AtomicU64::new(100);

fn adapt(tick: &TickData) {
    let every = Duration::from_millis(EVERY_MS.load(Ordering::Relaxed));
    let late = tick.step_interval().is_some_and(|s| s > Duration::from_millis(60));
    let costly = tick.cost().is_some_and(|c| c > Duration::from_micros(50));
    let next = if late || costly {
        (every * 2).min(SLOWEST)
    } else {
        (every / 2).max(FASTEST)
    };
    if next != every && PERIOD.get().is_some_and(|p| p.set_every(next).is_ok()) {
        EVERY_MS.store(u64::try_from(next.as_millis()).unwrap_or(u64::MAX), Ordering::Relaxed);
    }
}

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let period = load.on_tick(FASTEST, |tick| {
        // The mod's own work goes here.
        adapt(tick);
    })?;
    let _first = PERIOD.set(period);
    Ok(())
}
```

## Game events

Beyond these, a mod observes the game's own ECS events, such as hits, deaths
and levels gained; see [Game events](game-events.md).

## Events from the server

Every event from `Ticked` to `BuildingRemoved` comes from the server, on its
game thread. Keep handlers short: the game waits for them. Leave slow work,
such as file or network access, to a thread of your own.

On a client they come from the built-in server while the client hosts a game,
solo play included, and from nothing otherwise. Where a hosted game starts and
ends, and the state to reset between games, is in
[Hosting](concepts/sides-and-hosting.md#hosting).

A killed process raises no `ShutdownStarted`.

## The scheduler

[`load.scheduler()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/load.rs)
returns a
[`Scheduler`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/schedule.rs).
It is a `Copy` handle that puts work on the server's game thread after a simulation
step. The scheduler looks for due tasks on a tick of the default period, so
its precision is `Ticked::DEFAULT_EVERY` (100 ms):

- `next_tick(f)` runs `f` on the scheduler's next pass, within the tick
  period.
- `after(delay, f)` runs `f` once. It runs on the first pass at least
  `delay` from now.
- `every(period, f)` runs `f` again and again, about once a period. The
  first run comes a period from now. It repeats while `f` returns
  `ControlFlow::Continue(())`. `Break(())` stops it.

```rust
use std::ops::ControlFlow;
use std::time::Duration;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let scheduler = load.scheduler()?;
    scheduler.after(Duration::from_secs(5), || enshwick::info!("five seconds in"));
    scheduler.every(Duration::from_secs(60), || {
        enshwick::info!("a minute passed");
        ControlFlow::Continue(())
    });
    Ok(())
}
```

- Get the scheduler during `init`: it subscribes to `Ticked` for you.
- Then copy it anywhere and schedule from any thread, a hook or a background
  thread included.
- Tasks run where the game's server code runs, so they may call into the game,
  [read the world](world.md) and [run console lines](chat.md#the-console).
- Tasks run only while steps run: on a dedicated server, or on a client while
  it hosts a game. A task is late by up to one tick period and one step.
- A task queued while a game runs belongs to that game. When a client
  leaves the game it hosts, the game's tasks end, so none runs in the next
  game with the last one's entities. Tasks queued during `init` run in every
  game.
- A pass with nothing due costs one atomic load, and the steps between
  passes cost nothing.

## The step clock

The scheduler's delays are wall-clock time. A wait that should not run out
while the server stands still counts in the server's steps instead. Such
waits include a hold on a container or a grace period after a player's
action. For them,
[`load.step_clock()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/time.rs)
returns the mod's
[`StepClock`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/time.rs).
Its `Ticked` handler moves the clock on by the time the tick carries. It
moves at most `StepClock::MAX_STEP` (250 ms) for each step in the tick. A stalled server, a world
loading or a debugger's pause takes no steps, so the clock stands still with
it. The first step after counts `MAX_STEP` at most. The clock moves on with
its tick, every 100 ms by default.

```rust
use std::time::Duration;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let clock = load.step_clock()?;
    let started = clock.now();
    load.on::<enshwick::events::Ticked>(move |_| {
        if clock.now().saturating_sub(started) > Duration::from_secs(10) {
            enshwick::info!("ten seconds of world time");
        }
    })
}
```

- Get it during `init`; every call returns the same clock, a
  `&'static StepClock` to copy anywhere.
- `now()` reads it from any thread without a lock.
- A unit test makes its own with `StepClock::new()` and moves it with
  `advance(elapsed, steps)`. A test may instead tick the fake host with
  `loader.tick(elapsed)` ([testing](testing.md)).

## Worked examples

- [`enshwick-optimizer`](https://github.com/enshwick/mods/tree/main/optimizer)
  runs work on every step with `on_every_step`. It does so in
  [`optimizer/src/lib.rs`](https://github.com/enshwick/mods/blob/main/optimizer/src/lib.rs).
- [`enshwick-shared-experience`](https://github.com/enshwick/mods/tree/main/shared-experience)
  runs work on every step with `on_every_step`. It does so in
  [`shared-experience/src/lib.rs`](https://github.com/enshwick/mods/blob/main/shared-experience/src/lib.rs).
- [`enshwick-area-loot`](https://github.com/enshwick/mods/tree/main/area-loot)
  keeps its step clock and resets per-game state on `ShutdownStarted`. It
  does so in
  [`take.rs`](https://github.com/enshwick/mods/blob/main/area-loot/src/take.rs).
