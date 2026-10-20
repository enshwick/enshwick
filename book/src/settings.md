# Settings

A mod's settings live in `mod-configs/<id>.toml` beside the game. The loader
writes the file from the mod's defaults on first start, and the mod reads it
during `init`. There are two ways to read it: a typed settings struct with
`#[derive(enshwick_api::Config)]`, or plain serde over a defaults string.

## Typed settings

Derive `enshwick_api::Config` on a struct with named fields that also derives
serde's `Deserialize` and `Serialize` and implements `Default`, and call
[`load.config()`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-api/src/load.rs):

```rust
/// Furniture comfort settings.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick_api::Config)]
#[serde(deny_unknown_fields)]
#[config(validate = Settings::check)]
struct Settings {
    /// Comfort each piece of furniture gives inside a base, times this factor.
    multiplier: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { multiplier: 2.0 }
    }
}

impl Settings {
    fn check(&self) -> Result<(), String> {
        if self.multiplier.is_finite() && self.multiplier >= 0.0 {
            Ok(())
        } else {
            Err("multiplier must be a number of zero or more".into())
        }
    }
}

fn init(load: &mut enshwick_api::Load<'_>) -> Result<(), enshwick_api::Off> {
    let settings: Settings = load.config()?;
    enshwick_api::info!("furniture comfort x{}", settings.multiplier);
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

`#[config(validate = path)]` names a `fn(&Self) -> Result<(), String>`. At
init, an error turns the mod off with the message; at run time it skips the
changed file.

A file that does not parse turns the mod off at init with serde's message. Add
`#[serde(deny_unknown_fields)]` so a misspelt key does the same instead of
being ignored. Add `#[serde(default)]` if a file written by an older version of
the mod, without a newer key, should still load.

[`enshwick-furniture-comfort`](https://github.com/themaiby/enshwick/tree/main/mods/enshwick-furniture-comfort)
is the example above in full.

## Untyped settings

[`load.settings(DEFAULTS)`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-api/src/load.rs)
reads the same file into any `serde::Deserialize` type. When the file is
missing, the loader writes `DEFAULTS` there as is, so write `DEFAULTS` as the
commented file a player should see, and make sure your type accepts it.

```rust
const DEFAULTS: &str = "\
# Altars allowed at each Flame level.
altars_per_flame_level = [2, 4, 8]
";

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    altars_per_flame_level: Vec<u8>,
}

fn init(load: &mut enshwick_api::Load<'_>) -> Result<(), enshwick_api::Off> {
    let settings: Settings = load.settings(DEFAULTS)?;
    enshwick_api::info!("table {:?}", settings.altars_per_flame_level);
    Ok(())
}
```

Untyped settings have no validation hook, no join values and no
`on_config_change`; subscribe to
[`events::SettingsChanged`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-api/src/events.rs)
and read the file again yourself if you need changes at run time.
[`enshwick-flame-altar-zones`](https://github.com/themaiby/enshwick/tree/main/mods/enshwick-flame-altar-zones)
reads its settings this way.

## Changes while the game runs

[`load.on_config_change(|settings: Settings| ...)`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-api/src/load.rs)
applies an edited file without a restart:

- the loader looks at every settings file once a second, so the handler runs
  about a second after the file changes;
- it runs on the loader's settings thread, not the game thread;
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
#[derive(Default, serde::Deserialize, serde::Serialize, enshwick_api::Config)]
struct Settings {
    /// Comfort per piece of furniture, times this factor.
    multiplier: f32,
}

/// Read by the hook on every call, as `f32` bits.
static MULTIPLIER: AtomicU32 = AtomicU32::new(0);

fn init(load: &mut enshwick_api::Load<'_>) -> Result<(), enshwick_api::Off> {
    let settings: Settings = load.config()?;
    MULTIPLIER.store(settings.multiplier.to_bits(), Ordering::Relaxed);
    load.on_config_change(|settings: Settings| {
        MULTIPLIER.store(settings.multiplier.to_bits(), Ordering::Relaxed);
    })
}
```

A table too large for an atomic goes into an `ArcSwap` in the mod's state, as
[`enshwick-flame-altar-count`](https://github.com/themaiby/enshwick/blob/main/mods/enshwick-flame-altar-count/src/lib.rs)
does with its altar table.

## Join values

Some values must be equal on the server and its players, such as a table the
client's interface also reads. Mark such a field `#[config(sync)]`:

```rust
use enshwick_api::config::Config;

/// Altar settings.
#[derive(Default, serde::Deserialize, serde::Serialize, enshwick_api::Config)]
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

Join values only work for a mod with `join = true` in its manifest, the mods
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
  refused with a warning, and the file's values apply.

Apply join values in `on_config_change`. A mod without a handler runs with its
file's values until the next start.

A server's edited join values are advertised once the mod has read them, that
is, once its `on_config_change` handler has run. Players get them the next
time they join.

[`enshwick-flame-altar-count`](https://github.com/themaiby/enshwick/tree/main/mods/enshwick-flame-altar-count)
keeps its altar table as a join value: the client's building interface asks
the same function as the server's check.
