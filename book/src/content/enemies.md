<!--
type: how-to
description: Mods add enemies copied from vanilla ones with their own level, health, damage, faction and arsenal, and arsenals of vanilla attacks.
-->

# Enemies and arsenals

A mod adds an enemy copied from a vanilla one, with its own level, health,
damage, faction and arsenal. An arsenal is what an enemy attacks with: a pick
of the game's attacks and behaviours.

## An enemy of the mod's own

`load.add_enemy(NewEnemy)` copies the vanilla enemy template `like` under the
mod's id, as [`add_template`](templates.md) does, and sets what `NewEnemy`
gives:

| Field | What it sets |
| --- | --- |
| `level` | the enemy's level, from 1 |
| `health` | its health at each level, about this times the vanilla enemy's |
| `damage` | the damage it deals, this times the vanilla enemy's |
| `faction` | its faction, which decides whom it fights |
| `arsenal` | its arsenal, a vanilla one or one the mod adds |
| `boss` | whether it is a boss |
| `changes` | further [component changes](templates.md#what-a-change-does) |

Health follows the level as in the game: on a server copy, a vanilla wolf of
level 20 had 567 health and its copy with `health = 2` had 1135. The enemy
spawns from its template, as spawners and `effects::spawn` spawn others. An
enemy is read on the server and the client, so a mod that adds one runs on
both sides. Its model, animations and attacks are the game's: the client has
to have them.

`vanilla::enemy(name)` names a vanilla enemy by its template's name, with its
faction and arsenal; `vanilla::template(name)` names any template. The first
of these calls reads the names of all the game's templates once, 139 ms on a
dedicated server: a Luau mod makes it at the top level of `main.luau`, where
no per-second budget applies.

## Arsenals

`load.add_arsenal(NewArsenal)` copies the vanilla arsenal `like` under the
mod's id and lists it beside the game's. `load.change_arsenal(guid, change)`
changes a vanilla arsenal; mods that change one change it in load order, each
on the one before's result. An `ArsenalChange` holds:

- `attacks` and `behaviors`: the attacks and behaviours, by GUID, each from
  any vanilla arsenal, in place of the arsenal's own;
- `tune`: new selection chances, threat and ranges of attacks it holds.

An attack or behaviour no vanilla arsenal holds returns `Err(Off)` from the
call.

```rust
use enshwick::content::{Guid, id};
use enshwick::content::enemies::{ArsenalChange, NewArsenal, NewEnemy};
use enshwick::typed::FactionType;

fn init(load: &mut enshwick::Load<'_>, wolf: Guid, wolf_arsenal: Guid, bite: Guid) -> Result<(), enshwick::Off> {
    let biter = load.add_arsenal(NewArsenal {
        id: id!("biter"),
        like: wolf_arsenal,
        change: ArsenalChange { attacks: Some(vec![bite]), ..ArsenalChange::default() },
    })?;
    load.add_enemy(NewEnemy {
        level: Some(20),
        health: Some(2.0),
        faction: Some(FactionType::Vukah),
        arsenal: Some(biter),
        ..NewEnemy::new(id!("elder-wolf"), wolf)
    })?;
    Ok(())
}
```

These calls need the `typed` feature, which gives the factions.

## From Luau

The same calls are `enshwick.content.add_enemy{...}`,
`enshwick.content.add_arsenal{...}` and
`enshwick.content.change_arsenal(guid, {...})`, at the top level of
`main.luau`; `enshwick.content.enemy(name)` and
`enshwick.content.template(name)` name vanilla ones.

```luau
local biter = enshwick.content.add_arsenal({
    id = "you.wolves:biter",
    like = "<the GUID of a vanilla arsenal>",
    change = { attacks = { "<the GUID of one of its attacks>" } },
})
enshwick.content.add_enemy({
    id = "you.wolves:elder-wolf",
    like = "<the GUID of a vanilla wolf template>",
    level = 20,
    health = 2,
    faction = "Vukah",
    arsenal = biter,
})
```

## When the mod is removed

The loader serves the vanilla template under the copy's GUID, so saved
enemies load as the vanilla one: see
[Content](../content.md#when-a-mod-is-removed).
