# Upgrading from 0.3

The API is still before 1.0, so a minor release may rename things. The ABI
between a mod and the loader did not change: a mod built against 0.3 keeps
running on the new loader, and only its source needs the new names when it is
built again.

## One naming scheme

The release after 0.3 names the API by a few rules:

- An event is named for what happened, in the past tense: `PlayerJoined`,
  `LevelGained`, `ShutdownStarted`.
- The game's own ECS events are "game events" everywhere. Each event module
  has a sealed `Kind` trait: `events::Kind` for the loader's events and
  `game_events::Kind` for the typed game events. `load.on::<E>()` and
  `load.on_game::<E>()` take them; the forms that read an event by its
  reflected name carry `raw` in their names.
- Registering content or a provider is `add_*`; a callback that may answer or
  refuse the game is `on_*`.
- A callback type of the ABI is named after the struct it takes, with `Fn`.
- Experience is spelled out, never `Xp`.

| 0.3                              | Now                                  |
| -------------------------------- | ------------------------------------ |
| `events::Tick`                   | `events::Ticked`                     |
| `events::Shutdown`               | `events::ShutdownStarted`            |
| `events::JoinMissing`            | `events::JoinRefused`                |
| `game_events::Typed`             | `game_events::Kind`                  |
| `game_events::GameEvent`         | `game_events::RawGameEvent`          |
| `game_events::LevelUp`           | `game_events::LevelGained`           |
| `game_events::XpGained`          | `game_events::ExperienceGained`      |
| `game_events::XpSource`          | `game_events::ExperienceSource`      |
| `game_events::AltarAction`       | `game_events::AltarUsed`             |
| `AltarAction` field `kind`       | `AltarUsed` field `action`           |
| `game_events::AltarActionKind`   | `game_events::AltarAction`           |
| `load.on_game_event(ty, ...)`    | `load.on_raw_game_event(ty, ...)`    |
| `load.on_every_game_event(...)`  | `load.on_every_raw_game_event(...)`  |
| `load.command(...)`              | `load.add_command(...)`              |
| `load.status(...)`               | `load.add_status(...)`               |
| `load.file_summon_item(...)`     | `load.add_summon_item(...)`          |
| `Purpose::Uniqueness { me, .. }` | `Purpose::Uniqueness { entity, .. }` |
| `Purpose::Roaming { me, .. }`    | `Purpose::Roaming { entity, .. }`    |
| `abi::SkillFn`                   | `abi::SkillCheckFn`                  |
| `abi::ExperienceFn`              | `abi::ExperiencePassFn`              |
| `abi::CommandFn`                 | `abi::CommandCallFn`                 |
| `abi::RenameFn`                  | `abi::RenameCallFn`                  |
| `abi::InventoryDoneFn`           | `abi::InventoryResultFn`             |

The raw constants of `abi::Event`, such as `Event::TICK`, keep their names.

## Errors at run time

`Off` is the reason a mod turns itself off during `init`. Calls a mod makes
while the game runs now return their own errors:

| Call                                                     | Error                   |
| -------------------------------------------------------- | ----------------------- |
| `Store::load`, `Store::save`, `Hold::set`, `Hold::clear` | `state::StoreError`     |
| `WorldStore::flush`                                      | `state::StoreError`     |
| `ZoneControl::set`                                       | `altars::SetSizesError` |

`StoreError` converts into `Off`, so `?` inside `init` needs no change:

```rust
use enshwick_loader_api::state::Store;

#[derive(Default, serde::Deserialize, serde::Serialize)]
struct Seen {
    count: u64,
}

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    let store: Store<Seen> = Store::new(&load.data_dir()?, "seen");
    let seen = store.load()?;
    enshwick_loader_api::info!("seen {} times", seen.count);
    Ok(())
}
```

## Named answers

Values that used to be bare numbers have names: a skill override answers
`SkillCheck::VANILLA`, `SkillCheck::UNLOCKED` or `SkillCheck::LOCKED`, and a
survivor status merge marks its answer with `abi::SURVIVOR_STATUS_SET`. The
safe layer, `on_skill` and `on_survivor_status`, hides both.
