# Testing

[`enshwick_loader_api::test`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/test.rs) is a
fake loader for a mod's unit tests. You tell it what the game and the loader
would answer, start the mod through the same exports the real loader calls,
then check what the mod asked for and what its hooks and handlers do.

It is part of `enshwick-loader-api`, behind the `test` feature. Turn the
feature on for tests only: list the crate again under `[dev-dependencies]`,
at the same tag `cargo enshwick new` pins under `[dependencies]`:

```toml
[dev-dependencies]
enshwick-loader-api = { git = "https://github.com/themaiby/enshwick", tag = "enshwick-loader-api-v<version>", features = ["test"] }
```

## The shape of a test

1. Build a [`FakeHost`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/test.rs)
   with what the game would answer: symbols, settings text, the side.
2. Call `install()`. You get an
   [`Installed`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/test.rs)
   fake, which is the loader of the process until it drops.
3. Start the mod with `start(enshwick_mod_init, enshwick_mod_ready)`, the two
   exports [`declare_mod!`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/declare.rs)
   writes. `start` runs `init`, and runs `ready` only when `init` returns
   `Status::OK`.
4. Check the outcome: the status, the logs, the hooks it linked. Call a detour
   or raise an event as the game would.

A full test of a mod with a hook, settings and a settings change:

```rust
use std::sync::atomic::{AtomicU32, Ordering};

use enshwick_loader_api::abi::{Level, Status};
use enshwick_loader_api::{Load, Mod, Next, Off, events, hook};

/// Bonus settings.
#[derive(Default, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
struct Settings {
    /// Added to every quest reward.
    bonus: u32,
}

static BONUS: AtomicU32 = AtomicU32::new(0);

struct Bonus;

#[hook]
fn reward(next: Next<extern "system" fn(u32) -> u32>, base: u32) -> u32 {
    next.get()(base) + BONUS.load(Ordering::Relaxed)
}

impl Mod for Bonus {
    fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
        enshwick_loader_api::manifest!(id: "bonus", side: SERVER)
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = load.config()?;
        BONUS.store(settings.bonus, Ordering::Relaxed);
        load.on_config_change(|settings: Settings| {
            BONUS.store(settings.bonus, Ordering::Relaxed);
        })?;
        load.hook(&REWARD, load.symbol("quest.reward")?)?;
        Ok(Self)
    }
}

enshwick_loader_api::declare_mod!(Bonus);

// Stands in for the game's function: the hook's chain ends here.
extern "system" fn vanilla(base: u32) -> u32 {
    base
}

let loader = enshwick_loader_api::test::FakeHost::new()
    .symbol("quest.reward", enshwick_loader_api::test::address(vanilla as extern "system" fn(u32) -> u32))
    .settings("bonus = 5\n")
    .install();
// SAFETY: the exports of `declare_mod!` above.
let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
assert_eq!(status, Status::OK);
assert_eq!(loader.hooks().len(), 1);

let reward = loader.detour(&REWARD).ok_or("not linked")?;
assert_eq!(reward(10), 15);

// A player edits the settings file; the loader then raises SettingsChanged.
loader.set_settings("bonus = 7\n");
assert_eq!(loader.raise::<events::SettingsChanged>(&()), 1);
assert_eq!(reward(10), 17);
assert_eq!(loader.logs(), [(Level::INFO, "settings reloaded".to_owned())]);
assert!(loader.faults().is_empty());
# Ok::<(), &str>(())
```

A hook's chain continues to its target address. Give the target as
[`address`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/test.rs)
of a plain `extern "system"` function with the game function's signature, and
that function plays the original.

## What the fake answers

Lookups you did not configure answer `Status::MISSING`, as a game build without
them would. Settings default to the mod's own defaults, as on first start; the
side defaults to `Side::SERVER` and the log level to `Level::INFO`.

A test of the path where the game build lacks what the mod needs:

```rust
use enshwick_loader_api::abi::{Level, Status};
use enshwick_loader_api::{Load, Mod, Off};

struct Picky;

impl Mod for Picky {
    fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
        enshwick_loader_api::manifest!(id: "picky", side: SERVER)
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        load.symbol("quest.reward")?;
        Ok(Self)
    }
}

enshwick_loader_api::declare_mod!(Picky);

let loader = enshwick_loader_api::test::FakeHost::new().install();
// SAFETY: the exports of `declare_mod!` above.
let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
assert_eq!(status, Status::REFUSED);
assert_eq!(
    loader.logs(),
    [(Level::WARN, "off: symbol quest.reward: missing on this game build".to_owned())]
);
```

## Builder methods

Every method of `FakeHost` takes and returns the builder.

| Method                               | What it sets                                                                          |
| ------------------------------------ | ------------------------------------------------------------------------------------- |
| `side(side)`                         | The side the game runs as; `Side::SERVER` by default.                                 |
| `hosting(bool)`                      | Whether a client hosts a game now.                                                    |
| `server_world(world)`                | The entity world of the running server; 0 for none.                                   |
| `base(address)`                      | The game's image base.                                                                |
| `fingerprint(fingerprint)`           | The game build's fingerprint.                                                         |
| `handle(handle)`                     | The handle the mod receives; `ModHandle(1)` by default.                               |
| `symbol(name, address)`              | What `Load::symbol(name)` finds.                                                      |
| `system(name, address)`              | What `Load::system(name)` finds.                                                      |
| `constant(name, value)`              | What `Load::constant(name)` finds.                                                    |
| `field_offset(type, field, offset)`  | What `Load::field_offset` finds.                                                      |
| `type_size(type, size)`              | What `Load::type_size` finds.                                                         |
| `type_hash(type, hash)`              | The ECS type hash of a component type.                                                |
| `settings(text)`                     | The text of the mod's settings file.                                                  |
| `data_dir(path)`                     | The path `Load::data_dir` returns; `mod-data/test` by default. Nothing is created.    |
| `log_level(level)`                   | The most detailed level kept for the mod's lines.                                     |
| `caller_frame(frame)`                | The caller's context a hook with `HookFlags::CALLER_FRAME` reads.                     |
| `component(entity, type_hash, data)` | A component of an entity, by address; it must stay valid while the fake is installed. |
| `template(entity, guid)`             | The template GUID of an entity.                                                       |
| `placed(type_hash, entity, at)`      | An entity with a component and a place, for `World::entities` and `World::near`.      |
| `world_key(key)`                     | The key of the running world, for `World::key` and world stores.                      |
| `player(info)`                       | A connected player with a character in the world.                                     |
| `loaded(info)`                       | A loaded mod that `enshwick_loader_api::mods()` lists.                                |
| `join_missing(info)`                 | A server mod that `enshwick_loader_api::mods::join_missing()` lists.                  |
| `replaced(info)`                     | A mod another mod replaces, which `enshwick_loader_api::mods::replaced()` lists.      |
| `foreign_command(name, reply)`       | The reply of a command another mod or the loader answers, for `commands::run`.        |
| `service(&TABLE)`                    | A service another mod publishes, typed by its `#[service]` declaration.               |
| `refuse_hook(target)`                | Refuses hooks on a target, as when another mod holds it exclusively.                  |
| `code(bytes)`                        | The game's code section, for pattern searches.                                        |
| `workshop_recipes(limit)`            | The most recipes one workshop takes; 512 by default, `u32::MAX` for lifted.           |
| `item(item, max_stack, flags)`       | The static data of an item, for `item_info`.                                          |
| `without_inventories()`              | A build without inventory transactions: `inventory_open` answers `Status::MISSING`.   |

`install()` ends the builder.

## What the installed fake shows

Starting the mod:

| Method               | What it does                                                     |
| -------------------- | ---------------------------------------------------------------- |
| `start(init, ready)` | Runs `init`, then `ready` when `init` returned `Status::OK`.     |
| `init(init)`         | Runs `init` alone; returns its status.                           |
| `ready(ready)`       | Runs `ready` alone, after a successful `init`.                   |
| `api()`, `handle()`  | The table and handle the mod is given, for calls to the raw ABI. |

All three start methods are `unsafe`: pass the exports of one `declare_mod!`.

What the mod asked for, each in order:

| Method                 | Returns                                                                |
| ---------------------- | ---------------------------------------------------------------------- |
| `logs()`               | `(Level, String)` lines the mod logged, at the levels the loader kept. |
| `faults()`             | The faults the mod reported, such as a panic in a hook.                |
| `hooks()`              | The hooks linked, as `LinkedHook { target, detour, priority, flags }`. |
| `mid_hooks()`          | The addresses of mid-function hooks.                                   |
| `subscriptions()`      | The events subscribed to.                                              |
| `resources()`          | The resources served, as `ServedResource { key, base }`.               |
| `code_patches()`       | `(address, expected, replacement)` of each code patch.                 |
| `texts()`              | `(tag, language, text)` of each text added.                            |
| `commands()`           | `(name, help)` of each chat command registered.                        |
| `console()`            | The command lines run as the console with `commands::run`.             |
| `chat()`               | `(account key, text)` of each chat line sent; key 0 for everyone.      |
| `labels()`             | `(entity, text)` of each label set.                                    |
| `game_event_types()`   | The game event types asked for, by hash; 0 for every type.             |
| `destroyed()`          | The entities the mod asked the world to destroy.                       |
| `saves()`              | How many saves the mod asked for.                                      |
| `published()`          | `(name, major)` of each service published.                             |
| `defined_events()`     | The names of the mod events defined.                                   |
| `sent_events()`        | The names of the mod events sent.                                      |
| `migrations()`         | The ids of the migrations declared.                                    |
| `status()`             | The lines the mod's status sources give now.                           |
| `settings_defaults()`  | The defaults the mod passed each time it read its settings.            |
| `reported_synced()`    | The join values the mod reported each time it read its settings.       |
| `inventory_requests()` | The inventory requests not finished yet, as `SubmittedRequest`.        |

Playing the game:

| Method                            | What it does                                                                |
| --------------------------------- | --------------------------------------------------------------------------- |
| `detour(&HOOK)`                   | The hook's entry point when the mod linked it; call it as the game would.   |
| `run_at(address, &mut registers)` | Runs the mid-function hooks at an address; returns how many ran.            |
| `raise::<E>(&data)`               | Raises an event to the mod's handlers on this thread; returns how many ran. |
| `raise_raw(event, data)`          | The same for a raw `Event` id; `unsafe`.                                    |
| `raise_game_event(hash, bytes)`   | Hands a game event to the mod's handlers; returns how many ran.             |
| `send_mod_event(&event)`          | Sends another mod's event to the mod's handlers; returns how many ran.      |
| `migrate_settings(text)`          | Runs the mod's migrations' settings steps on a settings text.               |
| `migrate_store(name, text)`       | Runs their store steps on a world store; the name and text it ends with.    |
| `set_settings(text)`              | Replaces the settings file's text; raise `SettingsChanged` after it.        |
| `set_server_synced(values)`       | Sets the joined server's join values; raise `SettingsChanged` after it.     |
| `command(name, &player, args)`    | Runs the mod's chat command; its reply, or `None` without such a command.   |
| `rename(entity, player, text)`    | Asks the mod's rename filters; whether every filter lets it through.        |
| `build_resource(&key, base)`      | Builds a served resource from base bytes with the mod's patch.              |
| `finish_inventory(outcome, ops)`  | Finishes the oldest open inventory request; false when none is open.        |

Mid-function hooks run on registers you set up:

```rust
let loader = enshwick_loader_api::test::FakeHost::new().install();
let mut registers = enshwick_loader_api::abi::Registers { rcx: 7, ..Default::default() };
assert_eq!(loader.run_at(0x1_4000_1000, &mut registers), 0);
```

An inventory request stays open until the test finishes it. `outcome` is an
`InventoryResult` constant and `ops` holds one result per operation; the mod's
callback runs inside the call, as on the game thread:

```rust
use enshwick_loader_api::abi::{InventoryOpResult, InventoryResult};

let loader = enshwick_loader_api::test::FakeHost::new().item(0x5e01, 20, 0).install();
assert!(loader.inventory_requests().is_empty());
let done = [InventoryOpResult { error: 0, remaining: 0 }];
assert!(!loader.finish_inventory(InventoryResult::COMMITTED, &done));
```

## One mod life per process

A mod initialises once per process, as in the game: its state and the table it
talks to are process-wide. So write one test per mod life, and run the tests
with [`cargo nextest`](https://nexte.st), which gives each test its own
process:

```bash
cargo nextest run
```

Under `cargo test` the tests of one binary share a process. The fake then
serves one test at a time: `install` waits while another test holds an
installed fake. A second test in the same process still sees the mod's state
from the first, so it cannot start the mod again.

Dropping the `Installed` fake uninstalls it; calls the mod makes afterwards
answer `Status::MISSING` and are not recorded.

## On a test server

The fake answers what you tell it; only the game says whether an engine
fact holds on the running build. Run a dedicated server of your own, never
one people play on, with a copy of a world:

- [`enshwick-admin-tools`](https://github.com/themaiby/enshwick/tree/main/mods/enshwick-admin-tools)
  runs each line appended to its command file as the server console, so a
  mod's commands run without anyone joining;
- [`enshwick-probe`](https://github.com/themaiby/enshwick/tree/main/mods/enshwick-probe),
  a development mod that is never released, exercises the platform's world
  API and game events and logs each check as `PASS`, `FAIL` or `SKIP` to
  `logs/enshwick/mods/probe.log`;
- [`enshwick-research`](https://github.com/themaiby/enshwick/tree/main/mods/enshwick-research),
  another development mod, logs what a running client hands its item
  tooltip and what a refused join leaves behind, to
  `logs/enshwick/mods/research.log`.

What needs a player, such as hits, levels or a character's stats, waits for
a client that joins the server or hosts the game.

## Reloading a mod while the game runs

With `dev_reload = true` in `mod-configs/enshwick.toml`, a dedicated server
or a client loads every mod from a copy under `mods/enshwick/reload/`, so
its DLL in `mods/` stays writable, and watches those DLLs. When one changes and stays
the same for a second, as after `cargo enshwick install`, the loader reloads
its mod without a restart:

1. it wires the mod's hooks past it and stops handing it events;
2. it waits for calls already inside the mod to leave;
3. it runs the mod's `shutdown`, unloads the DLL and loads the new copy;
4. it runs the new copy's `init` with the same handle, links its hooks into
   the armed chains, and runs its `ready`.

A mod opts in with `reload = true` in its `manifest!` and keeps a contract:

- **Only what a reload can unlink.** Hooks on functions it hooked at start,
  events, lookups (symbols, constants, systems, fields, types), settings,
  logging, reads of the game's archive and, on a client, the interface's
  frame and HUD callbacks and map markers: the loader stops calling the old
  copy's callbacks, clears its markers and calls the new copy's. Everything
  else a mod can register (commands, content, served
  resources, services, mod events, code patches, mid-function hooks, filters,
  the interface, stats and the rest) stays with the old copy, so the loader
  refuses to reload a mod that used any of it, naming the calls, and the
  change waits for a restart.
- **No live state it cannot hand over.** The new copy starts with fresh
  statics. What it needs back, it reads from its settings or its world store.
- **Its own threads stop in `shutdown`.** A thread still running in the old
  DLL after it is unloaded crashes the server. The loader runs `shutdown`
  after the hooks and events are unlinked.
- **The same manifest.** A new copy whose side, flags, dependencies or build
  rows changed, or that hooks a function nothing hooked at start, is refused
  or turned off; its version may change.

The log says `<id> reloaded` or why not. The
[hot-reload example](https://github.com/themaiby/enshwick/tree/main/examples/hot-reload)
shows the contract: an event, a ticker thread stopped in `shutdown`, and a
greeting to change. Reloads are for development only, and nothing is
reloaded unless the setting is on.

On a client, a mod with a window or a HUD reloads the same way: change what
its frame callback draws, `cargo enshwick install`, and the next frame draws
the new copy. A mod that a server's join turned off stays off until the game
restarts, whatever its DLL does. A client that also hosts runs one loader for
both, so a reload there reloads the mod for the hosted game too. Texts,
dialogs, the Mods screen's settings rows and served content still need a
restart: the loader refuses the reload and names those calls.
