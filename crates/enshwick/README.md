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

```rust
/// Hello's settings.
#[derive(serde::Deserialize, serde::Serialize, enshwick::Config)]
struct Settings {
    /// Altars added at every Flame level.
    extra_altars: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self { extra_altars: 2 }
    }
}

struct Hello;

impl enshwick::Mod for Hello {
    fn manifest() -> &'static enshwick::abi::Manifest {
        enshwick::manifest!(id: "you.hello", side: BOTH, join: true)
    }

    fn init(load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
        let settings: Settings = load.config()?;
        let extra = settings.extra_altars;
        load.on_altar_limit(move |limit| limit.allowed = limit.allowed.saturating_add(extra))?;
        enshwick::info!("{extra} more altars per Flame level");
        Ok(Self)
    }
}

enshwick::declare_mod!(Hello);
```

Turn the `test` feature on under `[dev-dependencies]` only. Parts behind the
`unstable` feature can change in any release. The macros come from
`enshwick-macros`, an internal crate this one re-exports. See
[the modding book](https://enshwick.com/dev/).

Licensed under [MIT](https://github.com/enshwick/enshwick/blob/main/LICENSE).
