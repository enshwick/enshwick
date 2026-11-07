<!--
type: how-to
description: enshwick::test is a fake loader for a mod's unit tests: it answers as the game would, starts the mod through its exports and shows what the mod asked for.
-->

# Testing

[`enshwick::test`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/test.rs) is a
fake loader for a mod's unit tests.

## Turning it on

The fake is part of `enshwick`, behind the `test` feature. Turn the
feature on for tests only: list the crate again under `[dev-dependencies]`,
at the same version as under `[dependencies]`. A crate from
`cargo enshwick new` has it there already:

```toml
[dev-dependencies]
enshwick = { workspace = true, features = ["test"] }
```

## The shape of a test

You tell the fake what the game and the loader would answer. You start the
mod through the same exports the real loader calls. Then you check what the
mod asked for and what its hooks and handlers do.

1. Build a [`FakeHost`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/test.rs)
   with what the game would answer: symbols, settings text, the side.
2. Call `install()`. You get an
   [`Installed`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/test.rs)
   fake, which is the loader of the process until it drops.
3. Start the mod with `start(enshwick_mod_init, enshwick_mod_ready)`, the two
   exports [`declare_mod!`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/declare.rs)
   writes. `start` runs `init` first. Then it runs `ready`, but only when
   the first returns `Status::OK`.
4. Check the outcome: the status, the logs, the hooks it linked. Call a detour
   or raise an event as the game would.

A full test of a mod with a hook, settings and a settings change:

```rust,standalone_crate
use std::sync::atomic::{AtomicU32, Ordering};

use enshwick::abi::{Level, Status};
use enshwick::{Load, Mod, Next, Off, events, hook};

/// Bonus settings.
#[enshwick::config]
#[derive(Default)]
struct Settings {
    /// Added to every quest reward.
    bonus: u32,
}

static BONUS: AtomicU32 = AtomicU32::new(0);

struct Bonus;

#[hook]
fn reward(next: Next<'_, extern "system" fn(u32) -> u32>, base: u32) -> u32 {
    next.call(base) + BONUS.load(Ordering::Relaxed)
}

impl Mod for Bonus {
    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        load.on_settings_change(|settings: &Settings| {
            BONUS.store(settings.bonus, Ordering::Relaxed);
        })?;
        let reward = load.symbol("quest.reward")?;
        // SAFETY: the game's reward function has the type `REWARD` declares.
        unsafe { load.hook(&REWARD, reward) }?;
        Ok(Self)
    }
}

enshwick::declare_mod!(Bonus);

// Stands in for the game's function: the hook's chain ends here.
extern "system" fn vanilla(base: u32) -> u32 {
    base
}

let loader = enshwick::test::FakeHost::new()
    .symbol("quest.reward", enshwick::test::address(vanilla as extern "system" fn(u32) -> u32))
    .settings("bonus = 5\n")
    .install();
// SAFETY: the exports of `declare_mod!` above.
let status = unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
assert_eq!(status, Status::OK);
assert_eq!(loader.hooks().len(), 1);

let reward = loader.detour(&REWARD).ok_or("not linked")?;
assert_eq!(reward(10), 10, "the defaults until a world loads");

// A world loads: the loader raises SettingsReplaced with its settings.
assert_eq!(loader.raise::<events::SettingsReplaced>(&()), 1);
assert_eq!(reward(10), 15);

// A player edits the world's file; the loader then raises SettingsChanged.
loader.set_settings("settings", "bonus = 7\n");
assert_eq!(loader.raise::<events::SettingsChanged>(&()), 1);
assert_eq!(reward(10), 17);
assert!(loader.logs().iter().all(|(level, _)| *level != Level::WARN));
assert!(loader.faults().is_empty());
# Ok::<(), &str>(())
```

A hook's chain continues to its target address. Give the target as
[`address`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/test.rs)
of a plain `extern "system"` function with the game function's signature, and
that function plays the original.

## What the fake answers

Lookups you did not configure answer `Status::MISSING`, as a game build without
them would. A world's settings files are empty unless `settings` or
`settings_file` gives their text. So the mod reads its own defaults. The side
defaults to `Side::SERVER`, and the log level to `Level::INFO`. Raise
`SettingsReplaced` for a world that loads, `SettingsChanged` for an edit while
it runs.

The fake refuses what the loader refuses, with the same status. It refuses
these:

- a second hook of the mod on one function;
- one hook slot on two functions;
- a code patch outside the game's code or over another patch;
- a point not named after the mod's id;
- a rate clients count from a mod without `join: true`.

Hooks, code patches, points and rates asked for after `init` returns come too
late.

A test of the path where the game build lacks what the mod needs:

```rust,standalone_crate
use enshwick::abi::{Level, Status};
use enshwick::{Load, Mod, Off};

struct Picky;

impl Mod for Picky {
    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        load.symbol("quest.reward")?;
        Ok(Self)
    }
}

enshwick::declare_mod!(Picky);

let loader = enshwick::test::FakeHost::new().install();
// SAFETY: the exports of `declare_mod!` above.
let status = unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
assert_eq!(status, Status::REFUSED);
assert_eq!(
    loader.logs(),
    [(Level::ERROR, "off: symbol quest.reward: missing on this game build".to_owned())]
);
```

## Engine facts by area

A preset sets every type hash, size and offset one area of the API resolves,
such as `items()` for items and the item registry. Its values agree with each
other and claim nothing about a game build. `FakeHost` answers them through
`EngineFacts`, so a test lays out its game data by the layout the mod resolves,
never by offsets of its own:

```rust
use enshwick::content::items::ItemLayout;
use enshwick::content::{EngineFacts, types};

let host = enshwick::test::FakeHost::new().items();
let layout = ItemLayout::resolve(&host)?;
let mut chest = vec![0u8; host.size(types::ITEM_INFO.id())?];
chest[layout.id..layout.id + 4].copy_from_slice(&7u32.to_le_bytes());
let item_hash = host.hash(types::ITEM_INFO.id())?;
# let _ = item_hash;
# Ok::<(), enshwick::Off>(())
```

| Preset            | What it sets                                                                 |
| ----------------- | ---------------------------------------------------------------------------- |
| `items()`         | `ItemLayout`, with the item and item registry types.                         |
| `recipes()`       | `RecipeLayout`, with the recipe registry type.                               |
| `workshops()`     | `WorkshopLayout` beside `items()`: workshops, templates, crafting components. |
| `crafting_menu()` | `MenuLayout`, with the UI bundle type.                                       |
| `ui_images()`     | `ImageLayout`, with the UI image type.                                       |
| `world()`         | The current transform and used item types `World` needs, and the transform's fields. |
| `inventory()`     | What `Inventories` reads: setups, sub-inventories, stacks, durability, fuel, owner. |
| `game_events()`   | The hit, death and equipment events, the hit history and kill credit, damage bits and a slot. |
| `map_waypoints()` | Each player's waypoint beside the map's markers.                             |
| `effect_events()` | The NPC command, threat and ping events `Effects` raises.                    |
| `loot()`          | The components loot from destroyed things checks; the loot slot stays the test's. |
| `meters()`        | The `FuelSetup` component, the template and the template collection, which an item meter is made from. |
| `container_kinds()` | The markers `ContainerKinds` tells holders apart by.                       |

Where no layout type is public, `test::facts` names each fact a test lays its
blobs out by:

```rust
use enshwick::content::EngineFacts;
use enshwick::test::facts;

let host = enshwick::test::FakeHost::new().inventory();
let stack = host.size(facts::ITEM_STACK.id())?;
let count = host.offset(facts::ITEM_STACK_COUNT.id())?;
let mut stacks = vec![0u8; stack * 8];
stacks[stack + count..stack + count + 4].copy_from_slice(&5u32.to_le_bytes());
# Ok::<(), enshwick::Off>(())
```

A test of a build without one fact starts from the preset and drops it with
`without_fact(facts::WORLD_TRANSFORM_ORIENTATION)`. A test may still set one
fact by hand where it tests that fact.

## Builder methods

[`FakeHost`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/test.rs)
is a builder. Each method sets one thing the game would answer, such as the
side, a symbol, a component, a player or another mod's service.
`install()` ends it. Its rustdoc lists every method.

## What the installed fake shows

`install()` returns an
[`Installed`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/test.rs).
Its methods fall into three groups, each listed in its rustdoc:

- Starting the mod. These methods are `unsafe`: pass the exports of one
  `declare_mod!`.
  - `start`
  - `init`
  - `ready`
- Reading back what the mod asked for, in order:
  - `logs`
  - `hooks`
  - `chat` and the rest
- Playing the game to it:
  - `detour`
  - `raise`
  - `tick` and the rest

Mid-function hooks run on registers you set up:

```rust
let loader = enshwick::test::FakeHost::new().install();
let mut registers = enshwick::abi::Registers { rcx: 7, ..Default::default() };
assert_eq!(loader.run_at(0x1_4000_1000, &mut registers), 0);
```

An inventory request stays open until the test finishes it. `outcome` is an
`InventoryResult` constant. `ops` holds one result per operation. The mod's
callback runs inside the call, as on the game thread:

```rust
use enshwick::abi::{InventoryOpResult, InventoryResult};

let loader = enshwick::test::FakeHost::new().item(enshwick::ItemId::new(0x5e01), 20, 0).install();
assert!(loader.inventory_requests().is_empty());
let done = [InventoryOpResult { error: 0, remaining: 0 }];
assert!(!loader.finish_inventory(InventoryResult::COMMITTED, &done));
```

## One mod life per install

Every `install` starts a new mod life, as a fresh load of the DLL would in
the game. The mod's `init` runs again, and the platform's own state starts
empty. Under `cargo test` the tests of one binary share a process. So the
fake serves one test at a time. `install` waits while another test holds an
installed fake, and panics when the same test still holds one.

State a mod keeps in a `static` outside its own type survives into the next
test. A `ModLocal` is the exception: it reads as empty again in every mod
life:

```rust
use enshwick::state::ModLocal;

static COPY_LIMIT: ModLocal<u32> = ModLocal::new();

assert_eq!(COPY_LIMIT.get_or_init(|| 2), &2);
```

Dropping the `Installed` fake uninstalls it; calls the mod makes afterwards
answer `Status::MISSING` and are not recorded.

## On a test server

The fake answers what you tell it; only the game says whether an engine
fact holds on the running build. Run a dedicated server of your own, never
one people play on, with a copy of a world:

- the server's [console window](chat.md#the-servers-console-window) runs
  each line typed into it, so a mod's actions run without anyone joining;
- [`enshwick-probe`](https://github.com/enshwick/mods/tree/main/devtools/enshwick-probe),
  a development mod that is never released, exercises the platform's world
  API and game events. It logs each check to
  `logs/enshwick.probe_<date>_<time>.log`. A passed check logs `PASS`, a
  failed one `FAIL`. A skipped one logs `SKIP`;
- [`enshwick-research`](https://github.com/enshwick/mods/tree/main/devtools/enshwick-research),
  another development mod, logs what a running client hands its item
  tooltip and what a refused join leaves behind, to
  `logs/enshwick.research_<date>_<time>.log`.

What needs a player, such as hits, levels or a character's stats, waits for
a client that joins the server or hosts the game.
