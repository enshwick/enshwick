# State and data folders

What a mod keeps for a world lies beside that world's save, so copying the
save folder moves the world with everything its mods keep for it. What
belongs to the mod's installation lies in the mod's own folder in `mods/`.

## Where things live

| Folder | Holds | Who edits it |
| --- | --- | --- |
| `<stem>-mod-settings/<id>/`, beside the save | the world's [settings](settings.md) of the mod | the player or admin |
| `<stem>-mod-data/<id>/`, beside the save | what the mod keeps for that world | the mod |
| `mods/<folder>/` | the mod's installation: its DLL or pack, and an installation [hold](#holds) | the installer |
| `mod-configs/` | `<id>.toml`, the template of the mod's settings; `<id>.texts.toml`, a server owner's [text overrides](content/texts.md) | the player |

`<stem>` is the save's file name without `.ksd`, such as `world0`. The game
keeps every world of a machine in one folder: a dedicated server's
`savegame/`, a client's Steam `remote/` or `Saved Games\Enshrouded\`. The
loader creates a world's folders when something first writes there.

Settings are what a player chooses; data is what the mod remembers. Keep them
apart: never write the settings file from the mod, and never ask a player to
edit `<stem>-mod-data`.

## When a new world takes a save's name

The stem is a slot, not the world's name: the game gives a deleted world's
`world2` to the next new world, and leaves the old world's folders where
they were (client 26320373). So the loader records the world's GUID from the
save's header in `<stem>-mod-data/enshwick/world.toml`. As a world loads,
when its save holds another GUID, or the recorded world's save is gone, the
loader renames both folders to `<name>.old-<date>-<time>`, logs one line,
and the world starts from each mod's defaults. Nothing is deleted. A copy of
a save keeps its GUID, so a copied world keeps its folders.

## The data folder

[`world.data_dir()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/world.rs)
returns the mod's folder in the running world, `<stem>-mod-data/<id>`, and
creates it when missing; `None` while no world runs, as in the main menu or
during `init`. [`load.data_dir()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/load.rs)
answers the same. The path is plain `std::path::PathBuf`; use it from any
thread while that world runs.

```rust
fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let world = load.world()?;
    load.on::<enshwick::events::WorldLoaded>(move |()| {
        if let Some(dir) = world.data_dir() {
            enshwick::info!("this world's data is in {}", dir.display());
        }
    })
}
```

Write files from a thread of your own or a loader thread, not from a hook or a
game event handler: the game thread waits on the disk otherwise.

## Per-world state

[`load.world_store::<T>(name)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/load.rs)
keeps a value per world, in `<stem>-mod-data/<id>/<name>.toml`, never inside
the game's save: removing the mod leaves the world as vanilla left it.

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

Errors come back as a `StoreError` naming the file and the reason. It
converts into `Off`, so `?` in `init` turns the mod off with that text;
outside `init`, log it instead. `Hold` and `WorldStore::flush` return the
same error.

## Files of your own

A mod that keeps a file in another format in its data folder uses
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
    let hold = Hold::new(&load.mod_dir()?);
    hold.set("the world has 20 altar zones; the vanilla game groups only 16")?;
    // Later, once the world is back within vanilla limits:
    hold.clear()?;
    Ok(())
}
```

A hold in the mod's folder, from
[`load.mod_dir()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/load.rs),
is the file `mods/<folder>/hold.txt`, holding the reason. While it is set,
the loader checks it at start and stops the game before any world loads
whenever the mod would not run:

- on a game build the loader does not know, where every mod stays off;
- when the mod is skipped or turned off.

The log names the mod and its reason. A player who accepts the risk deletes
`hold.txt` to open the world anyway; removing the mod's folder removes the
hold with it. `hold.reason()` reads the reason back while the hold is set.

`hold.set_text(reason, key, args)` also names a text of the mod's
[texts](content/texts.md), with each `{name}` filled from `args`. While the
mod's texts are loaded, the log shows that text in the reader's language. When
they are not, as when the mod is skipped, it shows `reason`. The file keeps
`reason` on its first line and the text on the second; a file of one line
reads as before.

```rust
use enshwick::state::Hold;

fn hold_altars(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    Hold::new(&load.mod_dir()?).set_text(
        "the world has 20 altar zones",
        "hold.altars",
        &[("altars", "20")],
    )?;
    Ok(())
}
```

A hold that concerns one world only lies in that world's data folder:

```rust
use enshwick::state::Hold;

fn hold_world(world: &enshwick::world::World) -> Result<(), enshwick::Off> {
    if let Some(dir) = world.data_dir() {
        Hold::new(&dir).set("the world has 20 altar zones")?;
    }
    Ok(())
}
```

It is the file `<stem>-mod-data/<id>/hold.txt`. While the loader runs, it
stops the game only as that world starts to load without the mod, before
the save is read; other worlds open as usual. On a game build the loader
does not know, it learns no world, so such a hold does not stop the game:
a mod that must hold on every build sets a hold in its own folder.

## The loader's own files

The loader keeps its installation's files in `mods/enshwick/`. `clones.toml`
there records every template and item a mod cloned, with its vanilla source.
When no mod serves a cloned template any more, the game reads the vanilla
template under the clone's GUID, so a placed object of a removed mod loads as
the vanilla object it was cloned from, and an item of a removed mod stays as a
stand-in until the mod is back. See
[Content](content.md#when-a-mod-is-removed).

For each world it keeps `<stem>-mod-data/enshwick/`: the world's GUID
(`world.toml`), the mods it last ran with (`mods.lock`), item and entity
data (`attachments.bin`), lasting stat modifiers (`stats.bin`), kept game
settings (`game-settings.toml`) and queued knowledge (`knowledge.toml`). The
recipes a world turns off are a setting, in
`<stem>-mod-settings/enshwick/recipes.toml`.

The loader no longer reads `mod-data/` beside the game; it logs one line when
the folder is there. Move a world's files from the old layout by hand, or
delete the folder.

## Worked examples

- [`enshwick-better-altars`](https://github.com/enshwick/mods/tree/main/better-altars)
  sets and clears its hold in
  [`hold.rs`](https://github.com/enshwick/mods/blob/main/better-altars/src/hold.rs).
- [`enshwick-personal-safes`](https://github.com/enshwick/mods/tree/main/personal-safes)
  and [`enshwick-shared-experience`](https://github.com/enshwick/mods/tree/main/shared-experience)
  keep their per-world state in a `WorldStore`, each in its `lib.rs`
  ([personal-safes](https://github.com/enshwick/mods/blob/main/personal-safes/src/lib.rs),
  [shared-experience](https://github.com/enshwick/mods/blob/main/shared-experience/src/lib.rs)).
