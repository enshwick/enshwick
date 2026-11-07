<!--
type: how-to
description: Mods add enemies to the game's random spawners, by spawner or where a vanilla enemy spawns, and change how many a spawner spawns and how often.
-->

# Spawners

A random spawner is a template that lists the enemies it spawns, each with a
weight. A mod adds an entry to the game's spawners and changes how many they
spawn and how often.

## An enemy in the game's spawners

`load.add_spawn_entry(SpawnEntry)` adds an entry to the game's spawners, after
their own entries. The entry spawns `template` in the spawners `spawner` picks. A spawner picks an entry by its weight
against the others', which are 1 in most of the game's spawners. A
`SpawnerPick` is one of:

- `Spawner(guid)`: one vanilla spawner template, such as one
  `vanilla::template(name)` names;
- `Listing(guid)`: every vanilla spawner that lists the template `guid`, so a
  mod spawns its enemy where the vanilla one spawns.

`group` puts the entry in a spawn group and `conditions` limits when it
spawns, such as at night; both are optional.

## Counts and timeouts

`load.change_spawner(pick, SpawnerChange)` changes these values of the
spawners a pick names:

- the fewest and most spawned at once;
- the most alive at a time;
- the most in all;
- the shortest and longest wait between spawns, in seconds.

A value the
change leaves `None` keeps the spawner's.

Both calls are [template changes](templates.md): in memory, in load order,
each mod's on the one before's result. A pick that names no vanilla spawner
returns `Err(Off)` from the call. Spawns are the server's, so a mod that adds
only vanilla enemies to spawners runs on the server alone. One that spawns
its own [enemy](enemies.md) runs on both sides.

```rust
use enshwick::content::Guid;
use enshwick::content::spawns::{SpawnEntry, SpawnerChange, SpawnerPick};

fn init(load: &mut enshwick::Load<'_>, wolf: Guid, elder: Guid, nest: Guid) -> Result<(), enshwick::Off> {
    load.add_spawn_entry(SpawnEntry { weight: 0.25, ..SpawnEntry::new(SpawnerPick::Listing(wolf), elder) })?;
    load.change_spawner(
        SpawnerPick::Spawner(nest),
        SpawnerChange { max_entities_alive: Some(8), ..SpawnerChange::default() },
    )
}
```

These calls need the `typed` feature, which gives the spawn conditions.

## From Luau

The same calls are `enshwick.content.add_spawn_entry{...}` and
`enshwick.content.change_spawner(pick, {...})`. They run at the top level of
`main.luau`. A pick is a table of one key, `{ Spawner = guid }` or
`{ Listing = guid }`.

```luau
enshwick.content.add_spawn_entry({
    spawner = { Listing = "<the GUID of a vanilla wolf template>" },
    template = "<the GUID of the mod's enemy>",
    weight = 0.25,
})
enshwick.content.change_spawner({ Spawner = "<the GUID of a spawner>" }, { max_entities_alive = 8 })
```
