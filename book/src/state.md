# State and data folders

The loader gives every mod two places beside the game:

| Folder | Holds | Who edits it |
| --- | --- | --- |
| `mod-configs/` | `<id>.toml`, the mod's [settings](settings.md); `<id>.texts.toml`, a server owner's [text overrides](content/texts.md) | the player |
| `mod-data/<id>/` | what the mod keeps between sessions | the mod |

Settings are what a player chooses; data is what the mod remembers. Keep them
apart: never write to the settings file from the mod, and never ask a player
to edit `mod-data`.

## The data folder

[`load.data_dir()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/load.rs)
returns the mod's own folder, `mod-data/<id>`, and creates it when missing.
The path is plain `std::path::PathBuf`; keep it and use it from any thread.

```rust
fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let dir = load.data_dir()?;
    enshwick::info!("data in {}", dir.display());
    Ok(())
}
```

Write files from a thread of your own or a loader thread, not from a hook or a
game event handler: the game thread waits on the disk otherwise.

## Per-world state

[`load.world_store::<T>(name)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/load.rs)
keeps a value per world, in `mod-data/<id>/worlds/<key>/<name>.toml`, never
inside the game's save: removing the mod leaves the world as vanilla left it.
The key is [`World::key`](world.md#destroying-entities-and-saving).

```rust
#[derive(Default, serde::Deserialize, serde::Serialize)]
struct Bounties {
    paid: u64,
}

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let bounties = load.world_store::<Bounties>("bounties")?;
    load.on_game::<enshwick::game_events::EntityDiedEvent>(move |died| {
        if died.killer_player.is_some() {
            let _paid = bounties.update(|b| b.paid += 1);
        }
    })
}
```

- The store opens the running world's file when the world has loaded and
  writes it, when changed, as the world saves (on a thread of its own) and at
  shutdown. The mod's state and the world's save move together: a crash
  loses both since the last save.
- `with(|value| ...)` reads and `update(|value| ...)` changes the value; both
  return `None` while no world is open, before the world loads or after a
  hosted game stops. `flush()` writes now.
- A file that does not parse keeps the store shut and stays as it is, so a
  fix by hand loses nothing.
- `T` is a TOML table, such as a struct, with a `Default` for a new world.

## Store

[`state::Store<T>`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/state.rs)
keeps one serde value as `<dir>/<name>.toml`:

- `load()` returns the stored value, or `T::default()` while nothing is stored;
  a file that does not parse is an error;
- `save(&value)` writes to a temporary file first and renames it over the old
  one, so a crash never leaves half a file.

```rust
use enshwick::state::Store;

#[derive(Default, serde::Deserialize, serde::Serialize)]
struct Progress {
    highest_experience: u64,
}

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let store: Store<Progress> = Store::new(&load.data_dir()?, "progress");
    let mut progress = store.load()?;
    progress.highest_experience = progress.highest_experience.max(42);
    store.save(&progress)?;
    Ok(())
}
```

Errors come back as a `StoreError` naming the file and the reason. It
converts into `Off`, so `?` in `init` turns the mod off with that text; outside
`init`, log it instead. `Hold` and `WorldStore::flush` return the same error.

## Holds

A mod whose changes let a world grow past what the vanilla game opens safely,
such as more altars than the game groups into bases, sets a
[`state::Hold`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/state.rs)
while that is so, and clears it when it is no longer so.

```rust
use enshwick::state::Hold;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let hold = Hold::new(&load.data_dir()?);
    hold.set("the world has 20 altar zones; the vanilla game groups only 16")?;
    // Later, once the world is back within vanilla limits:
    hold.clear()?;
    Ok(())
}
```

The hold is a file, `mod-data/<id>/hold.txt`, holding the reason on one line.
While it is set, the loader checks it at start and stops the game before the
world loads whenever the mod would not run:

- on a game build the loader does not know, where every mod stays off;
- when the mod is missing, skipped or turned off.

The log names the mod and its reason. A player who accepts the risk deletes
`mod-data/<id>/hold.txt` to open the world anyway. `hold.reason()` reads the
reason back while the hold is set.

A hold that concerns one world only takes that world's key, from
[`World::key`](world.md):

```rust
use enshwick::state::Hold;

fn hold_world(load: &mut enshwick::Load<'_>, world: &str) -> Result<(), enshwick::Off> {
    Hold::for_world(&load.data_dir()?, world).set("the world has 20 altar zones")?;
    Ok(())
}
```

It is the file `mod-data/<id>/worlds/<key>/hold.txt`. While the loader runs,
it stops the game only as that world starts to load without the mod, before
the save is read; other worlds open as usual. On a game build the loader
does not know, it cannot tell which world loads, so every world's hold stops
the game at start.

## The loader's own data

The loader keeps its data in `mod-data/enshwick/`. `migrations.toml` there
records the [migrations](migrations.md) that ran on each mod's settings file
and on each world. `clones.toml` there records
every template and item a mod cloned, with its vanilla source. When no mod
serves a cloned template any more, the game reads the vanilla template under
the clone's GUID, so a placed object of a removed mod loads as the vanilla
object it was cloned from, and an item of a removed mod stays as a stand-in
until the mod is back. See [Content](content.md#when-a-mod-is-removed).
