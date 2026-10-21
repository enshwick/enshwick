# Game settings

[`load.game_settings()`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/game_settings.rs)
returns the server's
[`GameSettings`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/game_settings.rs):
the difficulty and gameplay values of its configuration (`gameSettings` in
`enshrouded_server.json`) or of the host's world menu, read typed and changed
while the world runs. It is `Copy`; get it during `init` and move it into
handlers, tasks and commands. It needs ABI minor 29.

```rust
fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    let settings = load.game_settings()?;
    load.add_command("durability", "turns item wear on or off: /durability on|off", move |_ctx, args| {
        let on = match args.first() {
            Some(&"on") => true,
            Some(&"off") => false,
            _ => return "/durability on|off".into(),
        };
        match settings.change(|s| s.durability = on) {
            Ok(_) => format!("durability {}", if on { "on" } else { "off" }).into(),
            Err(why) => format!("unchanged: {why}").into(),
        }
    })
}
```

## Reading

`read()` returns the settings in force as a `Settings` and the `Preset` they
came from, or `None` off the game thread and while no world runs. `Settings`
has a field for each of the game's values: factors (1 is the game's normal),
switches, the day and night lengths and hunger's time as `Duration`s, and
enums for the choices such as `WeatherFrequency` or `TombstoneMode`. An enum
value this API does not name reads as `Other(byte)` and is written back as it
was.

## Changing

`change(|s| ...)` lets a closure edit the settings in force and applies them
as the game's admin command does: the server's systems read them from the
next step, every client gets them, and the preset becomes `Preset::Custom`.
It returns the settings now in force, or a
[`WorldError`](world.md#writing-components): `Refused` for a negative or not
finite factor or a day or night length of zero, `WrongThread` off the game
thread, `Missing` while no world runs.

Most values act at once. Some act on what comes next: enemy health, damage and
stamina on enemies spawned after the change, food buffs on food eaten after
it, the tombstone mode on the next death, the weather frequency on the next
change of weather. A new day or night length keeps the clock's hour and only
changes its pace ([time of day](daytime.md)).

A change lasts until the server stops: the game writes it nowhere, and the
configuration's values return with the next start. `keep(|s| ...)` (minor
57) changes the settings the same way and keeps the fields whose value it
changed with the running world: the loader applies them again each time that
world loads, by the end of its first step, over what the configuration says.
The loader keeps them by field name in
`mod-data/enshwick/worlds/<key>/game-settings.toml`. `forget_kept()` forgets
every kept field of the world, any mod's, and puts back the values the world
loaded with.

```rust
fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    let settings = load.game_settings()?;
    load.add_command("lush", "plants grow twice as fast in this world", move |_ctx, _args| {
        match settings.keep(|s| s.plant_growth = 2.0) {
            Ok(_) => "kept for this world".into(),
            Err(why) => format!("unchanged: {why}").into(),
        }
    })
}
```

Several mods changing settings compose as their changes do: each `change`
or `keep` edits the values in force, so mods that touch different values
never meet, and the last change to one value wins; kept fields add up the
same way.

Every call works on the server's game thread, from a game event, a
[scheduler](events.md#the-scheduler) task or a [command](commands.md); on a
client, while it hosts a game.
