# Settings

A mod's settings live in `mod-configs/<id>.toml` beside the game. The loader
writes the file from the mod's defaults on first start, and the mod reads it
during `init`. There are two ways to read it: a typed settings struct with
`#[derive(enshwick::Config)]`, or plain serde over a defaults string.

## Typed settings

Derive `enshwick::Config` on a struct with named fields that also derives
serde's `Deserialize` and `Serialize` and implements `Default`, and call
[`load.config()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/load.rs):

```rust
/// Furniture comfort settings.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick::Config)]
#[serde(deny_unknown_fields)]
struct Settings {
    /// Comfort each piece of furniture gives inside a base, times this factor.
    #[config(runtime, range = 0.0..=100.0)]
    multiplier: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { multiplier: 2.0 }
    }
}

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let settings: Settings = load.config()?;
    enshwick::info!("furniture comfort x{}", settings.multiplier);
    Ok(())
}
```

The file a player sees is written from the type:

- the struct's doc comment heads the file;
- each field's doc comment precedes its key, as `#` comment lines;
- each value comes from `Default`. A value TOML cannot hold, such as `None`,
  is written as a comment saying no default could be written.

For the struct above the file reads:

```toml
# Furniture comfort settings.

# Comfort each piece of furniture gives inside a base, times this factor.
multiplier = 2.0
```

`Settings::template()` returns that text, so a unit test can check that the
defaults parse.

Every read checks each field's `range` and `choices` (below), then
`#[config(validate = path)]`, a `fn(&Self) -> Result<(), E>` for checks of
your own, `E` any `Display` such as a `String`. A refusal is a `ConfigError`:
`Parse` for text that does not fit the type, `Value` for a field out of its
range or choices, `Invalid` for your own check. At init it turns the mod off
with the message, such as `settings: multiplier is 0 to 100`; at run time it
skips the changed file.

## Stages, ranges and texts

Each field says when a change takes effect, which values it takes and how a
settings menu names it:

| Attribute        | Meaning                                                                                                                          |
| ---------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| `startup`        | Read when a world loads; `on_config_change` gets the world's values, and a change waits for the next load                        |
| `restart`        | Read once as the game starts, for every world; a change waits for the next start. For what only `init` can set up                |
| `runtime`        | The default: applied while the game runs, through `on_config_change`                                                             |
| `client`         | Each player's own: never sent to or taken from a server; the console of a dedicated server leaves it alone                       |
| `sync`           | The server's value: a joined client runs with it ([Join values](#join-values))                                                   |
| `startup, sync`  | Read when a world loads, as `startup`; a joined client runs with the server's value, the same while the world runs               |
| `range = a..=b`  | Numbers from `a` to `b`, each element of a list of numbers too; `a..` and `..=b` leave one end open. Constants work              |
| `choices = [..]` | One of these values; any type serde writes, such as an enum's variants                                                           |
| `text = "key"`   | The key of the field's label in the mod's [texts](content/texts.md), `config.<field>` by default; its description is `<key>.doc` |
| `secret`         | A value such as a password: never sent to a client, and changed in the file only; never with `sync`                              |
| `group`          | On a field that is a `Config` of its own: a table of the file; see [Groups](#groups)                                             |

A number outside its range in the settings file, a list's element too, is
held to the range when the file is read, with a warning naming the key and
the value in the mod's log; the console and the Mods screen refuse it.

A field takes one stage, or the pair `startup, sync`; any other two fail the
build. On the struct,
`#[config(text = "key")]` names the key of the set's title, `config` by
default. The field's doc comment is its English description wherever no
translation exists.

```rust
use enshwick::config::{Config, Stage};

/// Locker settings.
#[derive(serde::Deserialize, serde::Serialize, enshwick::Config)]
struct Settings {
    /// The locker costs the wooden chest's ingredients times this.
    #[config(startup, range = 1..=100)]
    cost_multiplier: u32,
    /// When the locker opens for administrators.
    #[config(choices = ["never", "always"])]
    admins: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self { cost_multiplier: 1, admins: "never".into() }
    }
}

let schema = Settings::schema();
let cost = schema.field("cost_multiplier").ok_or("no field")?;
assert_eq!(cost.stage, Stage::Startup);
assert_eq!(cost.accepts(), "1 to 100");
assert!(Settings::template().contains("# Takes effect when the world loads."));
# Ok::<(), &str>(())
```

## Groups

A field marked `group` holds settings of their own, written as a table of the
file and shown together on the Mods screen, each row's label after the
group's title. Its fields keep their own stages, ranges and texts; the
console and the Mods screen name them by dotted keys, such as
`comfort.multiplier`, and their text keys are `config.comfort.multiplier`
by default. The group's own title is `config.comfort`.

```rust
use enshwick::config::Config;

/// Comfort rules.
#[derive(Default, serde::Deserialize, serde::Serialize, enshwick::Config)]
#[serde(default)]
struct Comfort {
    /// Comfort per piece of furniture, times this factor.
    #[config(range = 0.0..=10.0)]
    multiplier: f32,
}

/// Rates of the game.
#[derive(Default, serde::Deserialize, serde::Serialize, enshwick::Config)]
#[serde(default)]
struct Rates {
    /// Recipes cost this many times their price.
    #[config(startup, range = 1..=5)]
    cost: u32,
    /// How furniture adds comfort.
    #[config(group)]
    comfort: Comfort,
}

assert!(Rates::template().contains("# How furniture adds comfort.\n[comfort]\n"));
assert!(Rates::schema().field("comfort.multiplier").is_some());
```

A mod that used to keep a setting at the top of the file moves it into a
group with a [migration](migrations.md): `rename_setting("cost",
"recipes.cost")`, and `take_setting` brings another mod's flat key into a
dotted one.

## Settings per world

A setting lives in one of two places, its scope, which its stage decides and
the schema states (`Field::scope`, `Scope`):

| Scope   | Stages                       | Where                                                                     |
| ------- | ---------------------------- | ------------------------------------------------------------------------- |
| `user`  | `client`, `restart`          | the settings file `mod-configs/<id>.toml`, every world                    |
| `world` | `startup`, `runtime`, `sync` | `mod-data/<id>/worlds/<key>/settings.toml`, with the world                |

The settings file is also the template: a world that loads for the first time
in single player, hosted or on a dedicated server gets a copy of it as its
own settings, never inside the game's save. While a world runs in the
process, a mod reads that world's settings with its own `client` settings
over them; with no world, as in the main menu, the template. A mod reads its
settings as before: when a world loads or stops, the loader raises
[`Event::SETTINGS_REPLACED`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/abi.rs),
and `on_config_change` gets the world's settings, its `startup` values too:
they apply when the world loads, with no restart of the game, so a mod applies
them in its handler: a recipe added with `add_recipe_with` and rebuilt, drops
replaced with `loot::replace_drops`, a flag behind a hook linked whatever the
value. A value only `init` can use, such as hooks it links or not, or content
whose shape it fixes, is a `restart` value instead: the handler always gets
the one the game started with, and the settings file holds it for every
world.

A newer version of a mod that adds a setting finds it at its template value
in each world. Changes to world settings made before any world runs, as on
the screen that creates or starts a world, wait in
`mod-data/<id>/worlds/next.toml` and go into the next world that loads.

While a world runs, the loader watches its settings file as it watches the
settings file, and an edit reaches `on_config_change` about a second after it
is saved. An edit of the settings file then changes only the `client`
settings the mod reads, and the mod's log says where the world's settings
are.

## The schema and the console

[`Config::schema()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/config.rs)
describes every field: key, stage, scope, type, default, range, choices, doc
comment and text key. `load.config()` hands it to the loader, which keeps it
for the process; it is the description the loader's screens build on, and
what the loader checks values against before it writes them.

Players and administrators change settings on the Mods screen. The server's
[console](chat.md#the-console), such as the server's
[command file](chat.md#the-command-file), reads and
changes them too:

| Line                                  | Answer                                                                           |
| ------------------------------------- | -------------------------------------------------------------------------------- |
| `enshwick config`                     | the mods that describe their settings                                            |
| `enshwick config <mod>`               | each setting with its value, what it takes and where and when a change applies   |
| `enshwick config <mod> <key>`         | one setting with its default and description                                     |
| `enshwick config <mod> <key> <value>` | checks the value and writes it to the running world's settings, comments kept    |

On a dedicated server, `enshwick config` edits the world it runs; before its
world loads, the change waits for it. A key names one element of a list with
its index, `per_level[2]`, and a grouped setting with its dotted key,
`comfort.multiplier`.

The world settings, when a player creates or starts a world and in the pause
menu's world settings, are where a world's settings are edited: under their
last tab, every mod's world settings show as the game's own rows, built from
the schema: a toggle for `bool`, a slider for a number with a range (in stops
for whole numbers up to 32 values), a slider per element for a list of up to
16 numbers with a range (numbered from 1, or from the field's `first = n`,
and labelled by the mod's text `<key>.element`, `{n}` standing for the
number, such as `Flame level {n}`), a dropdown for `choices`, and the value with the
file's name for the rest (text, longer lists, tables, numbers without a
range). A list's length changes in the file. They are read-only where the game makes its own rows read-only, for a
player who is no host; joined to another's game, the rows show the server's
`sync` values only.

Rows speak the player's language through the mod's
[texts](content/texts.md), which `load.texts` hands the loader: the label is
the field's text key (`config.<field>` or its `text`), the description under
it `<key>.doc`, a choice `<key>.<choice>`; without a text, the label is the
key in words and the description the doc comment. `startup` rows say "when the
world loads": they apply when the world loads again; `restart` rows say "when
the game starts again". A change is
written at once, as the game applies its own settings.

The Mods screen, "Mods" in the title screen's main menu and in the pause menu
beside Settings, lists the mods with their names, versions, descriptions,
authors and status, and edits each player's own (`client`) settings. Over
the pause menu while hosting, its "World settings" button opens the game's
world settings.

On the Mods screen a text setting shows the game's own text field. Click
it, type, and press Enter: the value goes through the same checks as any
other change, and a refusal shows above the list. Only Enter sets the
value: typing that ends any other way sets nothing. A text longer than 256
bytes, and any text on a game build where the loader cannot draw the text
field, shows as a line with the file's name instead. A `secret` shows only
its mark, never its value.

On a client joined to a dedicated server, the screen also offers "Server
settings": the server's mods with settings, and each one's values as the
server's world holds them, asked over the loader's own messages. An
administrator changes them there; the server checks the permission on its
own record of the player, runs the same checks as the console, writes its
world's settings, and answers with the outcome and the new values. Other
players see the join values only, without changes. Fields marked `secret`
never leave the server, and a player changes at most a few settings a
second.

Before anything writes, the loader runs the mod's own check: `load.config()`
hands it over, and it parses what the mod would read and runs the type's
`validate`; a refusal keeps the file and shows its reason. A `validate` that
returns `ConfigError::text("config.too-few", &[("min", "2")])` names a text
of the mod's [texts](content/texts.md): each player reads it in their own
language, with `{min}` filled in. A `String` reason shows as written.

A value is TOML (`5`, `true`, `"text"`, `[1, 2]`); text settings also take
plain words. After a write the loader raises the mod's settings event at once:
`runtime` and `sync` values apply through `on_config_change` (joined players
get `sync` values at once), `startup` values apply when the world loads
again, and `restart` values, written to the settings file, when the game
starts again. `client` values are refused on a dedicated server.

When a new version renames or changes a setting, declare a
[migration](migrations.md): the loader carries old files and every world's
settings over once.

A file that does not parse turns the mod off at init with serde's message. Add
`#[serde(deny_unknown_fields)]` so a misspelt key does the same instead of
being ignored. Add `#[serde(default)]` if a file written by an older version of
the mod, without a newer key, should still load.

[`enshwick-game-rates`](https://github.com/enshwick/mods/tree/main/game-rates)
reads this setting as `multiplier` in its `[comfort]` group, beside its recipe rates.

## Changes while the game runs

[`load.on_config_change(|settings: Settings| ...)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/load.rs)
applies an edited file without a restart:

- the loader looks at every settings file once a second, so the handler runs
  about a second after the file changes;
- it also runs at once after the console or the Mods screen changes a setting,
  and as a world loads or stops;
- its thread is not fixed: the settings watcher for an edited file, the thread
  that made the change for the console or the Mods screen, the game thread as a
  world loads or stops;
- the file is parsed and validated first. A file that no longer parses or
  validates is logged as skipped, the handler is not called, and the running
  values stay;
- after the handler returns, the mod's log says `settings reloaded`.

Register the handler during `init`, like every subscription.

A hook can run on a game thread at the moment the handler replaces a value,
so keep what hooks read in atomics, or swapped whole in an
[`ArcSwap`](https://docs.rs/arc-swap). Never hold a lock a hook also takes.

```rust
use std::sync::atomic::{AtomicU32, Ordering};

/// Comfort settings.
#[derive(Default, serde::Deserialize, serde::Serialize, enshwick::Config)]
struct Settings {
    /// Comfort per piece of furniture, times this factor.
    multiplier: f32,
}

/// Read by the hook on every call, as `f32` bits.
static MULTIPLIER: AtomicU32 = AtomicU32::new(0);

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.apply_config(|settings: &Settings| {
        MULTIPLIER.store(settings.multiplier.to_bits(), Ordering::Relaxed);
    })?;
    Ok(())
}
```

`apply_config` runs the closure now and after every change, then returns
the settings it read. The mod writes those lines once instead of after
`config` and again in `on_config_change`.

A table too large for an atomic goes into an `ArcSwap` in the mod's state, as
[`enshwick-better-altars`](https://github.com/enshwick/mods/blob/main/better-altars/src/count.rs)
does with its altar table.

## Join values

Some values must be equal on the server and its players, such as a table the
client's interface also reads. Mark such a field `#[config(sync)]`:

```rust
use enshwick::config::Config;

/// Altar settings.
#[derive(Default, serde::Deserialize, serde::Serialize, enshwick::Config)]
struct Settings {
    /// Altars allowed at each Flame level. A joined player runs with the
    /// server's table.
    #[config(sync)]
    altars: Vec<u8>,
    /// Log every placed altar; this side's own business.
    verbose: bool,
}

let settings = Settings { altars: vec![2, 4], verbose: true };
assert_eq!(settings.synced().as_deref(), Some("{ altars = [2, 4] }"));
```

Join values only work for a mod with `join: true` in its manifest, the mods
that count in the [join check](concepts/join-check.md). Then:

- **On the server.** Each time the mod reads its settings, the loader takes
  the marked fields as an inline TOML table. It advertises them in the Steam
  rules `enshwick.sync.<id>` (and `.1`, `.2`, ... for long values), and they
  count in the mod set's fingerprint. Values that need more than 8 rules of
  127 bytes are left out of both, with a warning.
- **On a client that joins.** The join check reads the server's join values.
  When they change what the mod runs with, the loader calls the mod's
  `on_config_change` handler with its settings file and the server's join
  values over it, validated again. The file stays as it is. Joining a server
  without join values for the mod calls the handler with the file's values
  again. Server values that do not fit the fields or do not validate are
  refused with a warning, and the file's values apply. The server sends its
  join values again over the loader's messages once the client's game has
  connected, so a value that changed since the client read the rules
  arrives too.
- **On a client that joins a hosted game.** A hosting client has no rules;
  the joining client learns from the game's Steam lobby that the host runs
  Enshwick, and the host sends its join values once the game has connected
  (see [Games a client hosts](concepts/join-check.md#games-a-client-hosts)).

Apply join values in `on_config_change`. A mod without a handler runs with its
file's values until the next start.

A rule of the world that its players must share and that must not change
under a running world, such as whether survivors keep a copy per altar, is
`#[config(startup, sync)]`. The server applies it when the world loads and
advertises the value in force: a change to its file waits for the next load
and reaches no player before then. A joined client runs with the server's
value from the join on, whatever its own file says, and with its own again
after it leaves. The console and the rows say "when the world loads, joined
players get it", and on a joined client the row shows the server's value,
locked.

A server's edited join values are advertised once the mod has read them, that
is, once its `on_config_change` handler has run, and pushed at the same moment
to every joined player whose client runs Enshwick: their mod's
`on_config_change` runs with the new values, without joining again.

[`enshwick-better-altars`](https://github.com/enshwick/mods/tree/main/better-altars)
keeps its altar table as a join value: the client's building interface asks
the same function as the server's check.
