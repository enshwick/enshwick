# Time of day

[`load.clock()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/daytime.rs)
returns the
[`Clock`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/daytime.rs)
of the server's world: the time of day, and setting it.
[`load.clock_speed()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/daytime.rs)
returns the mod's `ClockSpeed`, its say in how fast the clock runs. Both are
`Copy`; get them during `init` and move them into handlers, tasks and
commands.

The game's clock runs in game hours. On the 1.0 game the day lasts from 06:00 to
22:00 and takes 30 real minutes, the night 12 by default; the server's settings change
both lengths. While players sleep at night, the game runs the clock up to 60
times faster.

```rust
fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let clock = load.clock();
    load.add_command("time", "shows the time, or sets it: /time [hours]", move |_ctx, args| {
        if let Some(hours) = args.first().and_then(|a| a.parse::<f32>().ok()) {
            return format!("{:?}", clock.set_hours(hours)).into();
        }
        match clock.now() {
            Some(now) => format!("{:05.2} h, {}", now.hours, if now.is_day { "day" } else { "night" }).into(),
            None => "no world runs".into(),
        }
    })
}
```

## Reading and setting the time

`now()` returns a `TimeOfDay`: the time in hours from 0 up to 24, whether it is
day, the hours the day and the night start at, how long each lasts in real
time, and the speed the clock runs at now. It is `None` off the game thread
or while no world runs.

`set_hours(hours)` sets the time, taken modulo 24, through the game's own
setter: every client jumps to it after the next step and runs on from there,
as after an administrator's change of the time. The world moves forward to
the next time the clock shows that hour, as if the hours between had passed,
so what the game counts in game time, such as the weather, goes on from
there; the game's own setter would send it back to the first day. It fails
with a [`WorldError`](world.md#writing-components): `WrongThread` off the
game thread, `Refused` for an hour that is not a finite number, `Missing` on
a build without the clock.

## Speed

The game sets the clock's speed itself every step: 1 by day, faster at night
while players sleep. A mod sets a factor on top with `ClockSpeed::set`: 2 makes
days pass twice as fast, 0.5 half as fast, 0 stops the clock; up to
`ClockSpeed::MAX`. The clock runs at the game's speed times every mod's
factor, so mods compose, and sleeping still speeds the night up. The loader
sends a new speed to the clients only when it changes.

```rust
use enshwick::events::{DayStarted, NightStarted};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let speed = load.clock_speed()?;
    // Short nights: the night runs three times as fast, the day as usual.
    load.on::<NightStarted>(move |()| {
        let _set = speed.set(3.0);
    })?;
    load.on::<DayStarted>(move |()| {
        let _set = speed.set(1.0);
    })
}
```

`clock_speed()` belongs in `init`: the loader follows the game's speed for
the mods that asked from the start.

## Day and night

[`DayStarted`](events.md) and `NightStarted` are raised on the game thread
after the step in which the day or the night began, whether the clock got
there or a mod set the time across the turn. They carry no data; `now()`
reads the clock.

Every call works on the server's game thread, from a game event, a
[scheduler](events.md#the-scheduler) task or a [command](commands.md); on a
client, while it hosts a game.
