<!--
type: how-to
description: Read the weather at a place in the server's world and the fronts that make it, and start new weather, through the Weather handle a mod gets during init.
-->

# Weather and chunks

[`load.weather()`](https://github.com/enshwick/enshwick/blob/main/crates/internal/world/src/weather.rs)
returns the
[`Weather`](https://github.com/enshwick/enshwick/blob/main/crates/internal/world/src/weather.rs)
of the server's world: how it is at a place, the fronts that make it, and
starting new weather. It is `Copy`; get it during `init` and move it into
handlers, tasks and actions.

```rust
use core::time::Duration;
use enshwick::actions::{Action, Arg, Outcome, Value};
use enshwick::weather::Forecast;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let weather = load.weather();
    let world = load.world()?;
    let actions = load.actions(enshwick::i18n::Texts::default())?;
    actions.add(Action::new("weather", "The weather here"), move |request| {
        let here = world.position(request.player.entity).unwrap_or_default();
        Outcome::done(format!("{:?}", weather.at(here).map(|now| now.state())))
    })?;
    let kinds = [("clear", "Clear"), ("rain", "Rain"), ("storm", "Storm")];
    let new = Action::new("new-weather", "New weather").admins().arg(Arg::choice("Weather", &kinds));
    actions.add(new, move |request| {
        let forecast = match request.args {
            [Value::Choice(0)] => Forecast::Clear,
            [Value::Choice(1)] => Forecast::Rain,
            _ => Forecast::Blizzard,
        };
        Outcome::done(format!("{:?}", weather.set(forecast, Some(Duration::from_mins(15)))))
    })
}
```

## From Luau

A Luau mod reaches the same calls through `enshwick.weather`; here a task
ends a blizzard by clearing the skies for 15 minutes:

```luau
enshwick.schedule.every(60, function()
    for _, front in enshwick.weather.fronts() do
        if front.kind == "Blizzard" then
            enshwick.weather.set("Clear", 15 * 60)
            break
        end
    end
    return true
end)
```

## How the game makes weather

The weather comes in fronts. The world has a few weather layers. Each has a
current phenomenon, rain, a blizzard or clouds without either, at a place
with a radius. The phenomenon lasts its time, after which the game picks the
next. Rain falls as snow where it is cold. How often rain and storms come is
the server's `weatherFrequency` game setting.

`at(position)` weighs a place, in metres, as the game's own systems do, such
as for fires going out and characters getting wet. It returns a `WeatherAt`
with weights for clear, rain, snow and blizzard that add up to 1. Its
`state()` names the strongest.

`fronts()` lists the current fronts. For each it gives:

- the layer;
- the kind, one of these:
  - `Clouds`;
  - `Rain`;
  - `Blizzard`;
- the place and radius;
- the game time left;
- the GUID of the front's phenomenon.

`load.on_front(handler)` calls `handler` about a second after a front starts
and after it ends. It passes the front and whether it started. So a mod can
give the players inside its weather a buff.

```rust
use enshwick::weather::WeatherPhenomenonEffect;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.on_front(|change| {
        if change.started && change.front.kind == WeatherPhenomenonEffect::Blizzard {
            enshwick::info!("a blizzard of {} m starts", change.front.radius);
        }
    })
}
```

```luau
enshwick.weather.on_front(function(change)
    if change.started and change.front.kind == "Blizzard" then
        enshwick.log.info(`a blizzard of {change.front.radius} m starts`)
    end
end)
```

## Starting new weather

`set(forecast, lasting)` starts new weather in every layer at once, as the
game's own state configurations do:

- `Forecast::Rain` or `Forecast::Blizzard` starts that kind of front in every
  layer that has one, and clear skies in the others;
- `Forecast::Clear` clears every layer;
- `Forecast::Any` lets the game pick anew.

The game places the fronts itself, so a front may lie away from the players.
`lasting` is how long, in game time, or the game's own for `None`. Then the
game goes on as usual. Clients follow.

A save keeps a front only while it has at most 30 minutes left
(`MAX_RESTORED_REMAINING`). After a restart the game picks anew in a layer
whose front had more. So a longer `lasting` logs a warning and lasts only
until the world next loads.

## Adding and changing phenomena

With the `typed` feature, mods add weather phenomena of their own and change
the game's: see [Weather phenomena](weather-phenomena.md).

## Chunks

The server loads the world in square chunks of 256 metres around the
players. [`ChunkLoaded`](events.md) is raised after the step in which a
chunk's entities came to exist. That happens as a player came within about
240 metres of it. [`ChunkUnloaded`](events.md) is raised after its entities
are gone, once no player was within about 260 metres.

The `ChunkData` names the chunk by its column and row. Its column is `x` and
its row `z`. It spans from `x * size` to `(x + 1) * size` metres on one axis.
On the other it spans from `z * size` to `(z + 1) * size`, at any height. A mod that
works on what lies in an area reads it once its chunk loaded.

These chunks hold the world's entities, not its ground. The terrain's voxels
load and unload on their own, with or without a player, and raise no event.
On a dedicated server with no player, an edited spot's ground unloaded and
loaded three times, and neither event was raised (server 399). A
mod that works on the ground reads it as
[Terrain and voxels](terrain.md) says.

Where each call works is in
[What may be called where](concepts/threads.md#what-may-be-called-where).
