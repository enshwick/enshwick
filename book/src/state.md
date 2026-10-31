# State and data folders

The loader gives every mod two places beside the game: one for its settings
and one for its data.

## Settings and data

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
    load.on::<enshwick::game_events::EntityDiedEvent>(move |died| {
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
- A file that does not read or parse keeps the store shut and stays as it
  is, so a fix by hand loses nothing. `shut_reason()` says why, and the
  loader's status summary and the Mods screen show it under the mod, as the
  feature `world store <name>` off. A mod that guards
  something with the value, such as who may open a chest, keeps it guarded
  while the store is shut.
- `T` is a TOML table, such as a struct, with a `Default` for a new world.
  A field the type gains in an update reads from `Default` while a file
  lacks it, in nested tables too, so no `#[serde(default)]` is needed.
  Arrays come from the file whole. A map whose `Default` holds entries gets
  each one back while the file lacks it, so keep such defaults empty.

## Store

[`state::Store<T>`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/state.rs)
keeps one serde value as `<dir>/<name>.toml`:

- `load()` returns the stored value, or `T::default()` while nothing is stored;
  keys the file lacks come from `T::default()` too, as for a world store; a
  file that does not parse is an error;
- `save(&value)` writes the file whole through
  [`files::write_atomically`](#files-of-your-own), so a crash never leaves
  half a file, and keeps the old one as `<name>.toml.bak`.

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

## Files of your own

A mod that keeps a file in another format uses
[`enshwick::files`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/files.rs),
the same calls the stores use:

- `read_if_present(path)` and `read_to_string_if_present(path)` return
  `Ok(None)` only when no file is there. Any other error, such as a file
  another program has locked, is an `Err`: never start empty on it, or the
  next write replaces the player's data with nothing.
- `write_atomically(path, contents)` writes to a temporary file of its own
  beside `path`, flushes it to disk and renames it over the old file, which
  stays as `backup_path(path)`, `<path>.bak`. A crash or a power loss leaves
  the old file or the new one, and two writers at once never share a
  temporary file.

```rust
use enshwick::files;

fn record_visit(dir: &std::path::Path) -> std::io::Result<()> {
    let path = dir.join("visits.txt");
    // A file that does not read stops here, before anything is written.
    let mut visits = files::read_to_string_if_present(&path)?.unwrap_or_default();
    visits.push_str("visit\n");
    files::write_atomically(&path, visits)
}
```

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

The hold is a file, `mod-data/<id>/hold.txt`, holding the reason.
While it is set, the loader checks it at start and stops the game before the
world loads whenever the mod would not run:

- on a game build the loader does not know, where every mod stays off;
- when the mod is missing, skipped or turned off.

The log names the mod and its reason. A player who accepts the risk deletes
`mod-data/<id>/hold.txt` to open the world anyway. `hold.reason()` reads the
reason back while the hold is set.

`hold.set_text(reason, key, args)` also names a text of the mod's
[texts](content/texts.md), with each `{name}` filled from `args`. While the
mod's texts are loaded, the log shows that text in the reader's language. When
they are not, as when the mod is missing, it shows `reason`. The file keeps `reason` on its first line and the text on the
second; a file of one line reads as before.

```rust
use enshwick::state::Hold;

fn hold_altars(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    Hold::new(&load.data_dir()?).set_text(
        "the world has 20 altar zones",
        "hold.altars",
        &[("altars", "20")],
    )?;
    Ok(())
}
```

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

## Worked examples

- [`enshwick-better-altars`](https://github.com/enshwick/mods/tree/main/better-altars)
  sets and clears its hold in
  [`hold.rs`](https://github.com/enshwick/mods/blob/main/better-altars/src/hold.rs).
- [`enshwick-personal-safes`](https://github.com/enshwick/mods/tree/main/personal-safes)
  and [`enshwick-shared-experience`](https://github.com/enshwick/mods/tree/main/shared-experience)
  keep their per-world state in a `WorldStore`, each in its `lib.rs`
  ([personal-safes](https://github.com/enshwick/mods/blob/main/personal-safes/src/lib.rs),
  [shared-experience](https://github.com/enshwick/mods/blob/main/shared-experience/src/lib.rs)).
