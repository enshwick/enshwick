# enshwick

The modding API of [Enshwick](https://github.com/enshwick/enshwick) for
Enshrouded, and the one crate a Rust mod depends on. Enshwick is a modding API
platform. A mod maker gets:

- a typed Rust API for settings, events, the world, items and recipes, the
  game's interface and messages between server and client;
- [Luau scripts](https://github.com/enshwick/enshwick/blob/main/book/src/luau.md)
  and
  [data packs](https://github.com/enshwick/enshwick/blob/main/book/src/content/packs.md)
  without code, for mods that need no Rust;
- tests without the game: the `test` feature adds a fake host, so a mod's unit
  tests run on any computer;
- translations: a mod carries its texts in the 15 languages the game offers, and
  each player sees their own language.

The Enshwick loader comes with the platform and runs every mod, so players
install one thing.

A mod names itself once, in its `Cargo.toml`; the version and the rest come
from `[package]`:

```toml
[package.metadata.enshwick]
id = "you.hello"
side = "both"
```

```rust
/// Hello's settings.
#[enshwick::config]
struct Settings {
    /// Altars added at every Flame level.
    extra_altars: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self { extra_altars: 2 }
    }
}

static EXTRA: AtomicU8 = AtomicU8::new(0);

struct Hello;

impl enshwick::Mod for Hello {
    fn init(load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
        // Each world's settings arrive as it loads; the hook reads them.
        load.on_settings(|settings: &Settings| EXTRA.store(settings.extra_altars, Relaxed))?;
        load.on_altar_limit(|limit| {
            limit.allowed = limit.allowed.saturating_add(EXTRA.load(Relaxed));
        })?;
        Ok(Self)
    }
}

use std::sync::atomic::{AtomicU8, Ordering::Relaxed};

enshwick::declare_mod!(Hello);
```

Turn the `test` feature on under `[dev-dependencies]` only. Parts behind the
`unstable` feature can change in any release. The macros come from
`enshwick-macros`, an internal crate this one re-exports. See
[the modding book](https://enshwick.com/dev/).

Licensed under [MIT](https://github.com/enshwick/enshwick/blob/main/LICENSE).
