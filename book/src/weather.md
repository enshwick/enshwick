# Weather and chunks

[`load.weather()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/weather.rs)
returns the
[`Weather`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/weather.rs)
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

## How the game makes weather

The weather comes in fronts: the world has a few weather layers, and each
has a current phenomenon at a place with a radius (rain, a blizzard, or
clouds without either) that lasts its time, after which the game picks the
next. Rain falls as snow where it is cold. How often rain and storms come is
the server's `weatherFrequency` game setting.

`at(position)` weighs a place, in metres, as the game's own systems do (fires
going out, characters getting wet): a `WeatherAt` with weights for clear,
rain, snow and blizzard that add up to 1, and `state()` names the strongest.
`fronts()` lists the current fronts: layer, kind (`Clouds`, `Rain`,
`Blizzard`), place, radius and the game time left.

## Starting new weather

`set(forecast, lasting)` starts new weather in every layer at once, as the
game's own state configurations do: `Forecast::Rain` or `Forecast::Blizzard`
start that kind of front in every layer that has one and clear skies in the
others, `Forecast::Clear` clears every layer, and `Forecast::Any` lets the
game pick anew. The game places the fronts itself, so a front may lie away
from the players. `lasting` is how long, in game time, or the game's own
for `None`; then the game goes on as usual. Clients follow, and the next save
keeps it.

## Chunks

The server loads the world in square chunks of 256 metres around the
players. [`ChunkLoaded`](events.md) is raised after the step in which a
chunk's entities came to exist, as a player came within about 240 metres of
it; [`ChunkUnloaded`](events.md) after its entities are gone, once no player
was within about 260 metres. The `ChunkData` names the chunk by its column
`x` and row `z`: it spans `x * size` to `(x + 1) * size` metres on one axis
and `z * size` to `(z + 1) * size` on the other, at any height. A mod that
works on what lies in an area reads it once its chunk loaded.

Where each call works is in
[What may be called where](concepts/threads.md#what-may-be-called-where).
