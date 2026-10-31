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
| `Ticked` | `TickData` (`step`, `elapsed_steps`, `elapsed_nanos`, `step_nanos`, `cost_nanos`, `game`) | after a server step, every 100 ms by default; see [Ticks](#ticks); from before `WorldLoaded` | game |
| `WorldLoaded` | none | the world finished loading and the server runs | game |
| `WorldSaved` | none | a save of the world finished writing to disk | game |
| `PlayerJoined` | `PlayerData` (`slot`, `entity`, `key`) | a player's character entered the world | game |
| `PlayerLeft` | `PlayerData` | a player who joined is leaving; the character is still in the world | game |
| `ShutdownStarted` | none | an orderly stop begins, with the world and players alive; the final save follows | game |
| `BuildingPlaced` | `BuildingData` | a player placed a building piece or prop; raised after the step that placed it | game |
| `BuildingRemoved` | `BuildingData` | a player picked up or tore down a placed piece; the entity no longer exists | game |
| `SettingsChanged` | none | the mod's settings file changed, the console or the Mods screen changed a setting, or a client's join values changed | not fixed: loader or game |
| `LoaderSettingsChanged` | none | `mod-configs/enshwick.toml` changed | loader |
| `JoinRefused` | none | the join check kept this client out of a server | game |
| `AltarsChanged` | none | a Flame Altar of the server's world was placed, removed, moved, raised a level or had its zone built anew, for a mod that called `load.altars()` ([altars](altars.md)) | game |
| `SettingsReplaced` | none | a world loaded in this process, so its own settings apply, or stopped, so the settings file applies again | game |
| `DayStarted` | none | the day began in the server's world, by the clock or a time a mod set ([time of day](daytime.md)) | game |
| `NightStarted` | none | the night began in the server's world | game |
| `KnowledgeChanged` | `KnowledgeData` (`entity`, `id`, `value`) | a knowledge value of a player (or the world, entity 0) changed ([knowledge](knowledge.md)) | game |
| `QuestCompleted` | `QuestData` (`entity`, `quest`) | a player completed a quest and got its rewards | game |
| `ChunkLoaded` | `ChunkData` (`index`, `x`, `z`, `size`) | a 256 m chunk of the server's world loaded as players came near; its entities exist now ([weather and the world's chunks](weather.md)) | game |
| `ChunkUnloaded` | `ChunkData` | a chunk unloaded once no player was near; its entities are gone | game |
| `PlayerDied` | `PlayerDeathData` (`entity`, `cause`, `key`, `position`, `cause_guid`, `was_deadly_fog`, `drowned`, `killer_entity`) | a player died from a hit, a fall, the shroud or drowning, with the cause; for a hit, the killer's template and the attacker of the last hit; raised after the step; freezing and an administrator's kill raise none; any other entity's death is the game event `Died`; unstable | game |

The data:

- `TickData::step` counts steps since the loader started counting, from 1.
  The other fields are in [Ticks](#ticks).
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

When and how `JoinRefused` is raised, and `mods::join_missing()`, which lists
the mods the player lacks, are in
[A server's pages and the join check](releasing.md#a-servers-pages-and-the-join-check).

## Ticks

On dedicated server build 399 the simulation runs fixed ticks of 1/60 s, and
one step runs 1 to 3 of them (read from the server's code). With nobody
online the server made 20 steps a second over 229 minutes of its own
statistics. With players its loop waits only 1 ms, which allows up to 60
steps a second; that rate is not yet measured with a real player. Steps
are not time, so a tick handler runs on a period of wall time:

- `load.on::<Ticked>(f)` runs `f` after the first step, then after the first
  step at least `Ticked::DEFAULT_EVERY` (100 ms) after its last call.
- `load.on_tick(every, f)` picks the period; `Duration::ZERO` runs `f` on
  every step.
- `load.on_every_step(f)` runs `f` on every step, always. Use it for state
  that changes each step, input or the camera.

A call costs about 1 µs, most of it the processor fetching the mod's code and
data afresh (measured on a server copy with 11 mods, build 399). Steps a handler skips never enter the mod and cost it nothing.

`TickData` says what passed since the handler's last call:

- `elapsed_steps`: the server steps, 1 for an every-step handler;
- `elapsed()`: the wall time, for a first call the time since the step
  before;
- `step_interval()`: the average time between steps over about the last 16
  steps;
- `cost()`: what one call of the mod's event handlers cost on average since
  the last call, when the loader's `diagnostics` setting times callbacks;
  `None` otherwise and for every-step handlers.

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

`on_tick` returns a `TickPeriod`, a `Copy` handle to keep anywhere, the
handler included. Its `set_every(period)` takes effect on the next step. The
next call then comes a period after the last one, or at once if that time has
passed. The every-step handler of `on_every_step` has no period
to change.

### An adaptive tick

A handler that need not keep pace with the server can follow its load. This
one runs every 100 ms to 2 s: it slows down while steps come late or its own
calls cost much, and speeds up again when there is room.

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
[`Scheduler`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/schedule.rs),
a `Copy` handle that puts work on the server's game thread after a simulation
step. The scheduler looks for due tasks on a tick of the default period, so
its precision is `Ticked::DEFAULT_EVERY` (100 ms):

- `next_tick(f)` runs `f` on the scheduler's next pass, within the tick
  period;
- `after(delay, f)` runs `f` on the first pass at least `delay` from now;
- `every(period, f)` runs `f` about every `period`, the first time a period
  from now, while `f` returns `ControlFlow::Continue(())`; `Break(())` stops it.

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
while the server stands still, such as a hold on a container or a grace
period after a player's action, counts in the server's steps instead:
[`load.step_clock()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/time.rs)
returns the mod's
[`StepClock`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/time.rs),
which its `Ticked` handler moves on by the time the tick carries, at most
`StepClock::MAX_STEP` (250 ms) for each step in it. A stalled server, a world
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
  `advance(elapsed, steps)`, or ticks the fake host with
  `loader.tick(elapsed)` ([testing](testing.md)).
