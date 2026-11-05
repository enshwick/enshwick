# Weather phenomena

The phenomena a weather layer picks from are data in the game's weather
resource. With the `typed` feature, mods add phenomena of their own and
change the game's.

## Adding and changing one

During `init`, `load.add_weather_phenomenon(NewPhenomenon)` adds a copy of one of the
game's phenomena under a content id, with its own probability, duration,
radius, strength and effect, to the layer of the phenomenon it copies or to a
layer named by its GUID. `load.change_weather_phenomenon` changes the values
of the game's phenomenon or of one a mod added before. Mods that add or
change phenomena apply in load order, each on the one before's result.

## What a copy keeps

A copy keeps the clouds and look of the phenomenon it copies, and its effect
is clouds, rain or a blizzard: the game knows no other, and a new look needs
client assets. Clients draw weather from their own copy of the resource, so a
mod that adds a phenomenon runs on both sides. The resource holds at most 255
phenomena, for the game and every mod together.

```rust
use core::time::Duration;
use enshwick::content::{Guid, Id};
use enshwick::weather::{NewPhenomenon, PhenomenonChange, WeatherPhenomenonEffect};

fn init(load: &mut enshwick::Load<'_>, rain: Guid) -> Result<(), enshwick::Off> {
    let values = PhenomenonChange::new()
        .probability(0.5)
        .duration(Duration::from_mins(2), Duration::from_mins(5))
        .effect(WeatherPhenomenonEffect::Rain);
    let id = Id::new("you.drizzle:drizzle");
    load.add_weather_phenomenon(NewPhenomenon::new(id, rain).values(values))?;
    load.change_weather_phenomenon(rain, PhenomenonChange::new().probability(0.1))
}
```

## From Luau

The same calls are `enshwick.weather.add_weather_phenomenon{...}` and
`enshwick.weather.change_weather_phenomenon(guid, {...})`, at the top level
of `main.luau`, with the fields of `NewPhenomenon` and `PhenomenonChange`;
a duration is in seconds of game time.

```luau
local rain = "<the GUID of one of the game's rain phenomena>"
enshwick.weather.add_weather_phenomenon({
    id = "you.drizzle:drizzle",
    like = rain,
    values = { probability = 0.5, duration_min = 120, duration_max = 300, effect = "Rain" },
})
enshwick.weather.change_weather_phenomenon(rain, { probability = 0.1 })
```
