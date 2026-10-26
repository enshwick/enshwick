# Migrations

A new version of a mod sometimes changes what older versions left behind: a
setting gets a better name or another unit, a world store moves, an item gets
a new GUID. A **migration** says how to carry the old over. The mod declares
every migration it ever made, oldest first, and the loader runs each one
once per world and records it, as Factorio does with its saves.

## Declaring migrations

[`load.migrate`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/migrations.rs)
takes a
[`Migrations`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/migrations.rs)
list during `init`, before `load.config()`:

```rust
use enshwick::content::{Guid, Id};
use enshwick::migrations::Migrations;

/// The GUID the locker had before it got a content id.
const OLD_LOCKER: Guid = Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31");
const LOCKER: Id = Id::new("you.lockers:locker");

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.migrate(
        Migrations::new()
            .add("2", |m| {
                m.rename_setting("cost", "cost_multiplier")
                    .remove_setting("legacy")
                    .rename_content(OLD_LOCKER, LOCKER.guid())
            })
            .add("3", |m| {
                m.map_setting("limit", |old| {
                    let limit = old.as_integer().ok_or("limit is a number")?;
                    Ok(toml::Value::Integer(limit * 2))
                })
                .rename_store("owners", "keepers")
                .map_store("keepers", |table| {
                    table.remove("legacy");
                    Ok(())
                })
            }),
    )
}
```

- Each migration has an id, unique in the mod and never reused: name it
  after the release that brought it, such as `"2"` or `"0.5.0"`. Migrations
  run in the order they are added.
- Keep every migration in the list forever. A player may skip versions, and
  each world runs the ones it has not seen.

| Step                         | What it does                                                                             |
| ---------------------------- | ---------------------------------------------------------------------------------------- |
| `rename_setting(from, to)`   | moves a setting to a new key, value and comments kept; nothing when `to` is taken        |
| `remove_setting(key)`        | removes a setting                                                                        |
| `map_setting(key, f)`        | changes a setting's value: `f` takes the old `toml::Value` and returns the new one       |
| `take_setting(mod, key, to)` | sets `to` from the setting `key` of another mod; see [below](#settings-from-another-mod) |
| `rename_store(from, to)`     | moves a [world store](state.md#per-world-state) to a new name                            |
| `map_store(name, f)`         | changes a world store: `f` edits its `toml::Table` in place                              |
| `rename_content(from, to)`   | reads content saved under the GUID `from` as the content now under `to`                  |

A step whose setting or store is missing does nothing. A map returns
`Result<_, MigrationError>`; a `&str` or `String` turns into one with `?`, as
`ok_or("limit is a number")?` does, and an `Err` fails its migration with that
reason.

## When migrations run

- **The settings file**, `mod-configs/<id>.toml`, during `load.migrate`: the
  settings steps of every migration it has not seen. A failure turns the mod
  off with the reason.
- **Each world**, when it loads on a dedicated server or in a hosted game,
  before the mod reads the world's settings or opens its stores: the
  settings steps on the world's settings, and the store steps on its
  stores. A failure turns the mod off for the session, with the reason; the
  migration runs again at the next load.
- **Content renames** hold while the mod runs: whenever the game reads a
  resource under an old GUID that nobody serves, it gets the one under the
  new GUID, so placed objects saved with the old GUID load as the new ones.
  An item the mod adds under the new GUID is also added under the old one,
  so stacks saved with the old item stay in inventories and look and work
  the same.

A file or world the mod has never seen starts current: its files come from
the mod as it is now, so the loader records every migration as done and
runs only the `take_setting` steps. This holds for a world created after the update and for a world
the mod joins late.

`mod-data/enshwick/migrations.toml` records what ran, per mod, for the
settings files and for each world by its [key](world.md). Delete a mod's
line to run its migrations again.

## Settings from another mod

A mod that folds another one in lists it under `replaces` in its
[manifest](concepts/loader-and-abi.md#the-manifest) and takes the settings it
keeps with `take_setting`, so players keep their values:

```rust
use enshwick::migrations::Migrations;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.migrate(Migrations::new().add("2", |m| {
        m.take_setting("comfort", "multiplier", "comfort.multiplier")
            .take_setting("comfort", "enabled", "comfort.enabled")
    }))
}
```

The step reads the other mod's settings file for the settings file, and its
copy for each world for that world. Either key may be a dotted path into a
table. Unlike other steps, it runs on a settings file or a world the mod has
never seen too, since that is where a folded mod's values arrive. Where the
other mod has no value, the setting keeps its default.

A mod that only changes its id lists the old id under `was` instead: the
loader copies the old id's files whole, and no step is needed.

## Items under their old ids

An item renamed with `rename_content` keeps its old stacks: the game holds
them under the old item's game id, which the loader serves as a copy of the
new item. Code that asks "is this my item" compares through
[`migrations::current_id`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/migrations.rs),
which turns an old id into the id it was renamed to and leaves every other
id as it is:

```rust
use enshwick::content::{Guid, Id};
use enshwick::migrations::current_id;

const LOCKER: Id = Id::new("you.lockers:locker");

fn is_locker(item: u32) -> bool {
    current_id(item) == LOCKER.id()
}

assert!(is_locker(LOCKER.id()));
assert!(!is_locker(Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d").id()));
```

Moving a mod's items from hand-picked GUIDs to [content ids](content.md#content-ids)
is such a rename: keep each old GUID as the `from` of a `rename_content`.

## Checking before it runs

Before it changes anything the loader logs the plan under the mod's id:

```text
info  [you.lockers] migrations on world world0-d3a23c4a: you.lockers 3: change setting limit, rename store owners to keepers, change store keepers
```

With `migrations_dry_run = true` in `mod-configs/enshwick.toml` the loader
only logs the plan, adds `dry run, nothing changed`, and records nothing:
try a release against a copy of a server's data before it goes live.

## Testing migrations

The [fake loader](testing.md) keeps the migrations the mod declared and runs
them on text, the way the loader runs them on files:

```rust,standalone_crate
use enshwick::migrations::Migrations;

struct Lockers;

impl enshwick::Mod for Lockers {
    fn manifest() -> &'static enshwick::abi::Manifest {
        enshwick::manifest!(id: "you.lockers", side: SERVER)
    }

    fn init(load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
        load.migrate(Migrations::new().add("2", |m| m.rename_setting("cost", "cost_multiplier")))?;
        Ok(Self)
    }
}

enshwick::declare_mod!(Lockers);

let loader = enshwick::test::FakeHost::new().install();
// SAFETY: the exports of `declare_mod!` above.
let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
assert!(status.is_ok());
assert_eq!(loader.migrations(), ["2"]);
assert_eq!(loader.migrate_settings("cost = 3\n"), Ok("cost_multiplier = 3\n".to_owned()));
```

`migrate_store(name, text)` does the same for a world store and returns the
name it ends with.
