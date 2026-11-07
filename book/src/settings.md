<!--
type: how-to
description: A mod's settings belong to each world and lie beside its save. The loader writes them from the mod's settings types as the world loads, and the mod reads them.
-->

# Settings

Settings let an admin tune a mod for each world. The loader keeps them in
files beside the world's save, one per settings type, and hands the mod the values as the world
loads.

## Declare settings

A settings type is a struct with named fields and a `Default`. Mark it with
`#[enshwick::config]`:

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

The attribute derives `enshwick::Config` and serde's traits through the
API, so the mod needs no serde of its own. A key the file lacks takes its
value from `Default`.

The loader writes each world's file from the type. The struct's doc comment
heads the file, and each field's doc comment goes before its key:

```toml
# Furniture comfort settings.

# Comfort each piece of furniture gives inside a base, times this factor.
multiplier = 2.0
```

A value TOML cannot hold, such as `None`, is written as a comment that says
so. The console and the Mods screen refuse a key the struct does not name,
so a misspelt setting never passes unread.

## Several files

A mod with more than one settings type gives each its own file:

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

The name is lowercase letters, digits, hyphens and underscores, without the
extension. A
type without `file` writes `settings.toml`. When two types of one mod name
the same file, the build fails and points at both.

The console and the Mods screen show a mod's files as one. The fields of
`settings.toml` go by their keys, and those of another file by the file's
name and the key, such as `altars.per_level`.

## Check and label each field

Field attributes set what a value accepts and how the Mods screen labels
it:

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

| Attribute        | Meaning                                                                                                                          |
| ---------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| `range = a..=b`  | Numbers from `a` to `b`, each element of a list of numbers too. `a..` and `..=b` leave one end open. Constants work              |
| `choices = [..]` | One of these values; any type serde writes, such as an enum's variants                                                           |
| `text = "key"`   | The key of the field's label in the mod's [texts](content/texts.md), `config.<field>` by default; its description is `<key>.doc` |
| `first = n`      | For a list, the number its first element shows with on the Mods screen, 1 by default                                             |
| `secret`         | A value such as a password: never shown, sent to a client, changed in game or logged                                            |
| `group`          | On a field that is a `Config` of its own: a table of the file; see [Groups](#groups)                                             |

The field's doc comment is its English description wherever no translation
exists.

On the struct itself, `text = "key"` names the key of the set's title,
`config` by default. `validate = path` names a check of your
own. It is a `fn(&Self) -> Result<(), E>`, and its error type
implements `Display`.

## Groups

A field marked `group` holds settings of its own. The file writes them as a
table, and the Mods screen shows them together under the group's title:

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

The group's type is declared `#[enshwick::config(group)]` and writes no
file of its own. Its fields keep their own ranges and texts. The console
and the Mods screen name them by dotted keys, such as `comfort.multiplier`.
Their text keys are `config.comfort.multiplier` by default, and the group's
title is `config.comfort`.

## When a mod gets its settings

[`load.on_settings`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/load/settings.rs)
calls its handler with the defaults at once, during `init`. It calls it
again with the world's values each time a world loads, before
[`WorldLoaded`](events.md). When the world stops, the handler gets the
defaults again:

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

The mod has one code path: it runs on whatever the handler last stored.

A change to a world's settings applies the next time the world loads. On a
dedicated server, that is when the server starts again.

For values that change while the world runs, register
[`load.on_settings_change`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/load/settings.rs)
instead. Its handler runs when `on_settings`'s would, and also on every
change, on a thread that is not fixed:

- about a second after a person saves the file;
- at once after the console or the Mods screen changes a value;
- when the server of another's world sends new values.

## Register the same things whatever the settings say

Settings never change what a mod registers in `init`. Install hooks, add
content and change game data there in every case. Let the handler set the
values they read instead. A hook stays linked and checks a flag. A staff
sets its charges on each item from the settings, and the item's own data
stays the same.

This holds in single player, in a hosted game and on a dedicated server. It
is also why a client agrees with its server: the client registered its
content before it knew the server's values.

A hook can run on a game thread at the moment the handler replaces a value.
Keep what hooks read in atomics, or swapped whole in an
[`ArcSwap`](https://docs.rs/arc-swap), and never hold a lock a hook also
takes.
[`enshwick-better-altars`](https://github.com/enshwick/mods/blob/main/better-altars/src/count.rs)
keeps its altar table in an `ArcSwap` this way.

## How a world's file is kept

As each world loads, the loader brings the file of every settings type of
every running mod up to date:

- it writes a missing file from the defaults, with the doc comments;
- it adds the keys the type gained, at their defaults with their comments,
  and drops the keys the type lost;
- it refreshes the comment lines right above each key from the doc
  comments.

Values stay as they are. So do comments at the end of a line, and comment
lines a blank line away from their key.

Before its first change to a file, the loader copies it to
`<file>.toml.bak`, one backup replaced each time. Each key added or removed is one line in the mod's log,
such as `settings.toml: removed colour, which you.lockers 1.2.0 does not
have`.

The loader never renames a key. When a mod renames one, the new key starts
at its default, the old one goes, and the log has both lines.

## How values read

A file a person wrote keeps working, and settings never turn a mod off:

- a value that does not read as its field's type takes the default for
  this run, such as text for a number. The log names the key and the
  default, and the file keeps what the person wrote;
- a number outside its range is held to the range, each element of a list
  too, with a line in the log;
- a value outside its choices takes the default, with a line in the log;
- when `validate` refuses the values, the mod keeps the values it had, or
  the defaults as a world loads, with a line naming the reason;
- a file that is no TOML reads as the defaults, with the parser's reason.

No log line holds the value of a `secret`.

## On a joined server

A client in a dedicated server's world, or in another player's hosted
game, plays on the server's settings. The server sends every setting of its
mods of side `both` once the client joins, and again after each change. It
never sends a `secret`.

The client's mods read those values for the world they play in. The client
never writes them to its disk, and its mods run on their defaults again
when it leaves.

## The schema and the console

[`Config::schema()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/config.rs)
describes every field: key, type, default, range, choices, doc comment and
text key. `on_settings` hands it to the loader with the file's template.
The loader writes the file from it and checks values against it before it
writes them.

The server's [console](chat.md#the-console) reads and changes the running
world's settings:

| Line                                  | Answer                                                                    |
| ------------------------------------- | ------------------------------------------------------------------------- |
| `enshwick config`                     | the mods that describe their settings                                     |
| `enshwick config <mod>`               | each setting with its value and what it takes                             |
| `enshwick config <mod> <key>`         | one setting with its default and description                              |
| `enshwick config <mod> <key> <value>` | checks the value and writes it to the running world's file, comments kept |

With no world running, the console refuses a change, since settings belong
to each world. A key names one element of a list by its index, such as
`per_level[2]`, and a grouped setting by its dotted key. A value is TOML,
such as `5` or `"text"`. Text settings also take plain words.

## The Mods screen

In a world the process runs, single player or hosted, a mod's Settings
button opens that world's settings of the mod. In the main menu there is no
button, and the screen says once that mod settings belong to each world.
The game's own world settings show no mod rows.

The screen builds each row from the schema, with the game's own widgets:

| Field | Row |
| --- | --- |
| `bool` | a toggle |
| a number with a range | a slider, in stops for whole numbers of up to 32 values |
| a list of up to 16 numbers with a range | a slider per element, numbered from 1 or from `first = n` |
| `choices` | a dropdown |
| text up to 256 bytes | a text field |
| anything else, such as a table or a number without a range | the value, with the name of the file to change it in |

A list's length changes only in the file. A `secret` shows only its mark,
never its value.

Rows speak the player's language through the mod's
[texts](content/texts.md), which `load.texts` hands the loader:

| Text | Key |
| --- | --- |
| a row's label | the field's text key: `config.<field>` or its `text` |
| the description under it | `<key>.doc` |
| a choice | `<key>.<choice>` |
| a list element's label | `<key>.element`, with `{n}` for the number. For example, `Flame level {n}` |

Without a text, the label is the key in words and the description is the
doc comment.

A change is written at once, with the file's comments kept, and the screen
says it applies when the world loads again. Before it writes, the loader
parses the file with the change and runs the type's `validate`. A refusal
keeps the file and shows its reason.

The reason can be a text of the mod's own, which each player reads in their
language. Such a `validate` returns
`ConfigError::text("config.too-few", &[("min", "2")])`, and the text fills
in its value. A `String` reason shows as written.

In another's world the page shows the values the server sent, read-only.
When the server runs Enshwick, its "Server settings" button opens the
server's settings of the mod, which only its administrators change.

The server checks the permission on its own record of the player, then runs
the same checks as the console. It writes its world's file and answers with
the outcome and the new values. Fields marked `secret` never leave the
server, and a player changes at most a few settings a second. What a player
clicks is in
[Change a mod's settings](https://enshwick.com/guides/change-mod-settings/).

[`enshwick-game-rates`](https://github.com/enshwick/mods/tree/main/game-rates)
keeps its comfort setting in a `[comfort]` group, beside its recipe rates.

## Settings of a Luau mod

A [Luau mod](luau.md) declares its settings in `pack.toml`, one
`[[setting]]` table each, in the order the Mods screen shows them:

```toml
[[setting]]
key = "greeting"
default = "Welcome"
doc = "What a player who joins reads before their name."

[[setting]]
key = "announce_minutes"
default = 30
min = 1
max = 1440
doc = "How often the log counts the players online, in minutes."
```

| Field | Meaning |
| --- | --- |
| `key` | the key in each world's `<stem>-mod-settings/<id>/settings.toml`, of `[a-z0-9_]` |
| `default` | the value until a world sets another; its type is the setting's |
| `doc` | what it does, for the file's comments and the Mods screen |
| `min`, `max` | the range of a number |
| `choices` | the values it accepts |
| `text` | the key of its label in the pack's `texts.toml`; `config.<key>` unless given |

The loader treats these as it treats a Rust mod's settings. It writes the
file on first start, checks every value against its declaration, and shows
the settings behind the mod's gear on the Mods screen.

`enshwick.settings()` returns the values in force as a table. A
`SettingsChanged` handler runs after the new values are in place, so it
reads them again there:

```luau
enshwick.on("SettingsChanged", function()
    enshwick.log.info(`greeting now {enshwick.settings().greeting}`)
end)
```

`enshwick.on_settings(handler)` works as `load.on_settings` does: its
handler gets the values now, and again as each world loads and stops.
`enshwick.on_settings_change(handler)` also runs on every change in game.

## Settings of a data pack

A data pack declares its settings in `pack.toml`, as a Luau mod does. A
number in `loot.toml` or `rewards.toml` can name one; see
[Let an admin tune a pack per world](content/packs.md#let-an-admin-tune-a-pack-per-world).
