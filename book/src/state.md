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

[`load.data_dir()`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-api/src/load.rs)
returns the mod's own folder, `mod-data/<id>`, and creates it when missing.
The path is plain `std::path::PathBuf`; keep it and use it from any thread.

```rust
fn init(load: &mut enshwick_api::Load<'_>) -> Result<(), enshwick_api::Off> {
    let dir = load.data_dir()?;
    enshwick_api::info!("data in {}", dir.display());
    Ok(())
}
```

Write files from a thread of your own or a loader thread, not from a hook or a
game event handler: the game thread waits on the disk otherwise.

## Store

[`state::Store<T>`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-api/src/state.rs)
keeps one serde value as `<dir>/<name>.toml`:

- `load()` returns the stored value, or `T::default()` while nothing is stored;
  a file that does not parse is an error;
- `save(&value)` writes to a temporary file first and renames it over the old
  one, so a crash never leaves half a file.

```rust
use enshwick_api::state::Store;

#[derive(Default, serde::Deserialize, serde::Serialize)]
struct Progress {
    highest_xp: u64,
}

fn init(load: &mut enshwick_api::Load<'_>) -> Result<(), enshwick_api::Off> {
    let store: Store<Progress> = Store::new(&load.data_dir()?, "progress");
    let mut progress = store.load()?;
    progress.highest_xp = progress.highest_xp.max(42);
    store.save(&progress)
}
```

Errors come back as `Off`, so `?` in `init` turns the mod off with the path
and the reason. Outside `init`, log them instead.

## Holds

A mod whose changes let a world grow past what the vanilla game opens safely,
such as more altars than the game groups into bases, sets a
[`state::Hold`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-api/src/state.rs)
while that is so, and clears it when it is no longer so.

```rust
use enshwick_api::state::Hold;

fn init(load: &mut enshwick_api::Load<'_>) -> Result<(), enshwick_api::Off> {
    let hold = Hold::new(&load.data_dir()?);
    hold.set("the world has 20 altar zones; the vanilla game groups only 16")?;
    // Later, once the world is back within vanilla limits:
    hold.clear()
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

## The loader's own data

The loader keeps its data in `mod-data/enshwick/`. `clones.toml` there records
every template and item a mod cloned, with its vanilla source. When no mod
serves a cloned template any more, the game reads the vanilla template under
the clone's GUID, so a placed object of a removed mod loads as the vanilla
object it was cloned from. See [Content](content.md).
