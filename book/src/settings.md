# Settings

A mod's settings belong to each world, beside its save in
`<stem>-mod-settings/<id>/`, one file per settings type. The loader writes the
files from the mod's types as the world loads, and the mod gets the values
then.

## Declare settings

Put `#[enshwick::config]` on a struct with named fields that implements
`Default`. The attribute derives serde's `Deserialize` and `Serialize`
through the API, so the mod needs no serde of its own, and derives
`enshwick::Config`. Every field has its default: a key the file lacks reads
from `Default`.

```rust
use enshwick::config::Config;

/// Furniture comfort settings.
#[enshwick::config]
#[derive(Debug)]
struct Settings {
    /// Comfort each piece of furniture gives inside a base, times this factor.
    #[config(range = 0.0..=100.0)]
    multiplier: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { multiplier: 2.0 }
    }
}

assert_eq!(Settings::FILE, "settings");
assert!(Settings::template().contains("multiplier = 2.0"));
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

A key the struct does not name is refused by the console and the Mods screen
with its name, so a misspelt setting never passes unread.

## Several files

A mod with more than one settings type names a file for each with
`#[enshwick::config(file = "name")]`: lowercase letters, digits, `-` and `_`,
without `.toml`. A type without `file` writes `settings.toml`. Two types of
one mod with the same file fail its build, the error naming the file and
pointing at both types.

```rust
use enshwick::config::Config;

/// Altar rules.
#[enshwick::config(file = "altars")]
#[derive(Default)]
struct Altars {
    /// Altars allowed at each Flame level.
    #[config(range = 1..=64)]
    per_level: Vec<u8>,
}

assert_eq!(Altars::FILE, "altars");
```

The console and the Mods screen show a mod's files as one: the fields of
`settings.toml` by their keys, those of another file under its name, such as
`altars.per_level`.

## Field attributes

| Attribute        | Meaning                                                                                                                          |
| ---------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| `range = a..=b`  | Numbers from `a` to `b`, each element of a list of numbers too; `a..` and `..=b` leave one end open. Constants work              |
| `choices = [..]` | One of these values; any type serde writes, such as an enum's variants                                                           |
| `text = "key"`   | The key of the field's label in the mod's [texts](content/texts.md), `config.<field>` by default; its description is `<key>.doc` |
| `first = n`      | For a list, the number its first element shows with on the Mods screen, 1 by default                                             |
| `secret`         | A value such as a password: never shown, sent to a client, changed in game or logged                                            |
| `group`          | On a field that is a `Config` of its own: a table of the file; see [Groups](#groups)                                             |

On the struct, `#[enshwick::config(text = "key")]` names the key of the
set's title, `config` by default, and `validate = path` names a
`fn(&Self) -> Result<(), E>` for checks of your own, `E` any `Display` such
as a `String`. The field's doc comment is its English description wherever no
translation exists.

```rust
use enshwick::config::Config;

/// Locker settings.
#[enshwick::config]
struct Settings {
    /// The locker costs the wooden chest's ingredients times this.
    #[config(range = 1..=100)]
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
assert_eq!(cost.accepts(), "1 to 100");
# Ok::<(), &str>(())
```

## Groups

A field marked `group` holds settings of their own, written as a table of the
file and shown together on the Mods screen, each row's label after the
group's title. Its fields keep their own ranges and texts; the console and
the Mods screen name them by dotted keys, such as `comfort.multiplier`, and
their text keys are `config.comfort.multiplier` by default. The group's own
title is `config.comfort`. The group's type is declared
`#[enshwick::config(group)]`: it writes no file of its own.

```rust
use enshwick::config::Config;

/// Comfort rules.
#[enshwick::config(group)]
#[derive(Default)]
struct Comfort {
    /// Comfort per piece of furniture, times this factor.
    #[config(range = 0.0..=10.0)]
    multiplier: f32,
}

/// Rates of the game.
#[enshwick::config]
#[derive(Default)]
struct Rates {
    /// Recipes cost this many times their price.
    #[config(range = 1..=5)]
    cost: u32,
    /// How furniture adds comfort.
    #[config(group)]
    comfort: Comfort,
}

assert!(Rates::template().contains("# How furniture adds comfort.\n[comfort]\n"));
assert!(Rates::schema().field("comfort.multiplier").is_some());
```

## When a mod gets its settings

[`load.on_settings(|settings: &Settings| ...)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/load/settings.rs)
calls the handler with the defaults at once, during `init`, and again with
the world's values each time a world loads, before
[`WorldLoaded`](events.md). When the world stops, the handler gets the
defaults again. A mod has one code path: it runs on whatever the handler
last stored.

```rust
use std::sync::atomic::{AtomicU32, Ordering};

/// Comfort settings.
#[enshwick::config]
#[derive(Default)]
struct Settings {
    /// Comfort per piece of furniture, times this factor.
    multiplier: f32,
}

/// Read by the hook on every call, as `f32` bits.
static MULTIPLIER: AtomicU32 = AtomicU32::new(0);

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.on_settings(|settings: &Settings| {
        MULTIPLIER.store(settings.multiplier.to_bits(), Ordering::Relaxed);
    })
}
```

A change to a world's settings applies at the next load of the world: on a
dedicated server, when the server starts again; on a client, when the world
loads again. A mod whose values may change while the world runs registers
[`load.on_settings_change`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/load/settings.rs)
instead. Its handler also runs about a second after a person saves the file,
at once after the console or the Mods screen changes a value, and when the
server of another's world sends new values. Its thread is not fixed.

Settings never change what a mod registers in `init`. Install hooks, add
content and change game data there whatever the settings say, and let the
handler set the values they read: a staff's charges applied to each item,
not baked into the item; a hook that checks a flag, not a hook linked or
not. This holds in single player, in a hosted game and on a dedicated
server, and it is why a client that registered its content before it knew
the server's values agrees with the server.

A hook can run on a game thread at the moment the handler replaces a value,
so keep what hooks read in atomics, or swapped whole in an
[`ArcSwap`](https://docs.rs/arc-swap). Never hold a lock a hook also takes.
A table too large for an atomic goes into an `ArcSwap` in the mod's state, as
[`enshwick-better-altars`](https://github.com/enshwick/mods/blob/main/better-altars/src/count.rs)
does with its altar table.

## How a world's file is kept

As each world loads, for every settings type of every mod that runs:

- a missing file is written from the defaults with the doc comments;
- a file that exists gets the keys the type added, at their defaults with
  their comments, and loses the keys the type no longer has. Before the
  first change the loader copies the file to `<file>.toml.bak`, one backup
  replaced each time. Each key added or removed is one line in the mod's log,
  such as `settings.toml: removed colour, which you.lockers 1.2.0 does not
  have`;
- the comment lines right above each key are refreshed from the type's doc
  comments. Values, comments at the end of a line, and comment lines a blank
  line apart from the key stay.

The loader never renames a key. A mod that renames a key gets the new key at
its default and loses the old one, with both lines in the log.

## How values read

A file a person wrote keeps working:

- a value that does not read as its field's type, such as text for a number
  or an unknown enum value, takes the default for this run, with a line in
  the log naming the key and the default. The file keeps what the person
  wrote;
- a number outside its range, a list's element too, is held to the range,
  with a line in the log;
- a value outside its choices takes the default, with a line in the log;
- when `validate` refuses the values, the mod keeps the values it had, the
  defaults as a world loads, with a line naming the reason;
- a file that is no TOML reads as the defaults, with the parser's reason.

Settings never turn a mod off. No log line holds the value of a `secret`.

## On a joined server

On a client in a dedicated server's world or another player's hosted game,
the server sends every setting of its mods of side `both`, without a `secret`, once
the client joined, and again after each change. The client's mods read those
values for the world they play in, and the client never writes them to its
disk. When the client leaves, its mods run on their defaults again.

## The schema and the console

[`Config::schema()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/config.rs)
describes every field: key, type, default, range, choices, doc comment and
text key. `on_settings` hands it to the loader with the file's template; it
is what the loader writes the file from, and what it checks values against
before it writes them.

The server's [console](chat.md#the-console) reads and changes the running
world's settings:

| Line                                  | Answer                                                                    |
| ------------------------------------- | ------------------------------------------------------------------------- |
| `enshwick config`                     | the mods that describe their settings                                     |
| `enshwick config <mod>`               | each setting with its value and what it takes                             |
| `enshwick config <mod> <key>`         | one setting with its default and description                              |
| `enshwick config <mod> <key> <value>` | checks the value and writes it to the running world's file, comments kept |

With no world running, a change is refused: settings belong to each world. A
key names one element of a list with its index, `per_level[2]`, and a grouped
setting with its dotted key, `comfort.multiplier`. A value is TOML (`5`,
`true`, `"text"`, `[1, 2]`); text settings also take plain words.

## The Mods screen

In a world the process runs, single player or hosted, a mod's Settings
button opens that world's settings of the mod. In the main menu there is no
button, and the screen says once that mod settings belong to each world. The
game's own world settings show no mod rows.

Every row is the game's own, built from the schema: a toggle for `bool`, a
slider for a number with a range (in stops for whole numbers up to 32
values), a slider per element for a list of up to 16 numbers with a range
(numbered from 1, or from the field's `first = n`, and labelled by the mod's
text `<key>.element`, `{n}` standing for the number, such as
`Flame level {n}`), a dropdown for `choices`, a text field for text up to 256
bytes, and the value with the file's name for the rest (longer text, longer
lists, tables, numbers without a range). A list's length changes in the
file. A `secret` shows only its mark, never its value.

Rows speak the player's language through the mod's
[texts](content/texts.md), which `load.texts` hands the loader: the label is
the field's text key (`config.<field>` or its `text`), the description under
it `<key>.doc`, a choice `<key>.<choice>`; without a text, the label is the
key in words and the description the doc comment.

A change is written at once, with the file's comments kept, and the screen
says it applies when the world loads again. Before it writes, the loader runs
the mod's own check: it parses the file with the change and runs the type's
`validate`; a refusal keeps the file and shows its reason. A `validate` that
returns `ConfigError::text("config.too-few", &[("min", "2")])` names a text
of the mod's [texts](content/texts.md): each player reads it in their own
language, with `{min}` filled in. A `String` reason shows as written.

In another's world the page shows the values the server sent, read-only.
When the server runs Enshwick, its "Server settings" button opens the
server's settings of the mod, which only its administrators change. The
server checks the permission on its own record of the player, runs the same
checks as the console, writes its world's file, and answers with the outcome
and the new values. Fields marked `secret` never leave the server, and a
player changes at most a few settings a second. What a player clicks for
each of these is in
[Change a mod's settings](https://enshwick.com/guides/change-mod-settings/).

[`enshwick-game-rates`](https://github.com/enshwick/mods/tree/main/game-rates)
reads its comfort setting as `multiplier` in its `[comfort]` group, beside
its recipe rates.
