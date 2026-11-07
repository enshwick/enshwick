<!--
type: how-to
description: Read and change the difficulty and gameplay values of a world while it runs: those of a dedicated server's configuration or of the host's world menu, typed.
-->

# Game settings

[`load.game_settings()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/game_settings.rs)
returns the server's
[`GameSettings`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/game_settings.rs).
They are the difficulty and gameplay values of its configuration or the
host's world menu. `enshrouded_server.json` keeps them in
`gameSettings`. A mod reads them typed and changes them while the world runs.

## Using the settings

`GameSettings` is `Copy`. Get it during `init` and move it into handlers,
tasks and actions.

```rust
use enshwick::actions::{Action, Arg, Outcome, Value};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let settings = load.game_settings()?;
    let actions = load.actions(enshwick::i18n::Texts::default())?;
    let durability = Action::new("durability", "Item wear").admins().arg(Arg::toggle("On"));
    actions.add(durability, move |request| {
        let [Value::Toggle(on)] = request.args else {
            return Outcome::refused("On or off");
        };
        match settings.change(|s| s.enable_durability = *on) {
            Ok(_) => Outcome::done(format!("Durability {}", if *on { "on" } else { "off" })),
            Err(why) => Outcome::refused(format!("Unchanged: {why}")),
        }
    })
}
```

In Luau, `change` and `keep` take a function that changes the settings it
gets and returns them, and return the settings in force:

```luau
enshwick.on("WorldLoaded", function()
    local ok, why = pcall(enshwick.game_settings.change, function(settings)
        settings.enable_durability = false
        return settings
    end)
    if not ok then
        enshwick.log.warn(`durability unchanged: {why}`)
    end
end)
```

## Reading

`read()` returns the settings in force as a `Settings`. With them comes the
`GameSettingsPreset` they came from. It returns `None` off the game thread and
while no world runs. `Settings` has a field for each of the game's values:

- factors, where 1 is the game's normal;
- switches;
- the day and night lengths and hunger's time, as `Duration`s;
- enums for the choices, such as `WeatherFrequency` or `TombstoneMode`.

An enum value this API does not name reads as `Other(byte)`. It is written
back as it was.

## Changing

`change(|s| ...)` lets a closure edit the settings in force. It applies them
as the game's admin command does:

- the server's systems read them from the next step;
- every client gets them;
- the preset becomes `GameSettingsPreset::Custom`.

It returns the settings now in force, or a
[`WorldError`](world.md#writing-components):

- `Refused` for a negative or not finite factor, or a day or night length of
  zero;
- `WrongThread` off the game thread;
- `Missing` while no world runs.

Most values act at once. Some act on what comes next:

- enemy health, damage and stamina on enemies spawned after the change;
- food buffs on food eaten after it;
- the tombstone mode on the next death;
- the weather frequency on the next change of weather.

A new day or night length keeps the clock's hour and only changes its pace
([time of day](daytime.md)).

A change lasts until the server stops, and the configuration's values return
with the next start, unless the game saves its configuration first. A
dedicated server writes `gameSettings` back to `enshrouded_server.json`
whenever its configuration changes. A ban, an unban or an admin's settings
change does so, and a change made here may then reach the file.

`keep(|s| ...)` changes the settings the same way. It also keeps the fields
whose value it changed with the running world. The loader applies them again
each time that world loads, by the end of its first step, over what the
configuration says. The loader keeps them by field name in
`<stem>-mod-data/enshwick/game-settings.toml`. `forget_kept()` forgets
every kept field of the world, any mod's, and puts back the values the world
loaded with.

```rust
use enshwick::actions::{Action, Outcome};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let settings = load.game_settings()?;
    let actions = load.actions(enshwick::i18n::Texts::default())?;
    let lush = Action::new("lush", "Plants grow twice as fast in this world").admins();
    actions.add(lush, move |_request| match settings.keep(|s| s.plant_growth_speed_factor = 2.0) {
        Ok(_) => Outcome::done("Kept for this world"),
        Err(why) => Outcome::refused(format!("Unchanged: {why}")),
    })
}
```

Several mods changing settings compose as their changes do. Each `change`
or `keep` edits the values in force, so mods that touch different values
never meet. The last change to one value wins, and kept fields add up the
same way.

Where each call works is in
[What may be called where](concepts/threads.md#what-may-be-called-where).
