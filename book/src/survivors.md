# Survivors

The game keeps one entity per survivor and asks for it from many of its
systems: when a survivor sleeps, roams, gets a bed, or comes to a summoning
staff. A mod that keeps several entities of one survivor, such as one per
base, answers those questions itself through
[`load.on_survivor_lookup`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/survivors.rs).
The platform tells the askers apart and reads what each works on; the mod
sees a `Lookup` with the survivor, the game's own answer and a `Purpose`:

| Purpose            | Who asks                                     | What the mod answers                           |
| ------------------ | -------------------------------------------- | ---------------------------------------------- |
| `Uniqueness`       | The pass that keeps one entity per survivor  | `entity` to keep it, another one to remove it  |
| `Spawn`            | A summoning staff used at `target`           | The entity to move there, or `Nobody` to spawn |
| `BedOwner`         | A bed at `bed_at` binding its sleeper        | The entity that may sleep there                |
| `AssignBed`        | A player giving a bed with the staff         | The entity that gets it                        |
| `PreviousOccupant` | The same assignment, asking who slept there  | That entity; `Lookup::bed_of` tells            |
| `Roaming`          | A survivor's idle behaviour                  | The entity whose base it keeps to              |
| `BedBinding`       | The first binding of beds after a load       | Usually `Nobody` while several entities exist  |
| `Cursor`           | The cursor's distance check before assigning | `Nobody` skips the measure                     |

`Answer::Vanilla` keeps the game's answer. Two engine rules come with the
answers: after a `Uniqueness` answer that keeps another entity than the
game's, the survivor's record stays on the game's own entity, and after a
`Spawn` answer the move command reaches only the answered entity for a
moment.

```rust
use enshwick_loader_api::survivors::{Answer, Purpose};

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    load.on_survivor_lookup(|lookup| match lookup.purpose {
        // A staff always spawns a new entity instead of moving the old one.
        Purpose::Spawn { .. } => Answer::Nobody,
        _ => Answer::Vanilla,
    })
}
```

One mod answers at a time; a second is refused. The lookup runs on the
server's worker threads, in the dedicated server or a hosting client, and for
`Cursor` on a client alone too: keep it quick, never block, and read
[altars](altars.md) through a handle taken at init.

## Beds

`load.on_survivor_bed_check` runs after a survivor's own check kept its bed;
returning `false` makes the game clear it. A `BedCheck` has the survivor, its
entity, the bed, the survivor's home cell and the bed's cell.

## Summoning staffs

`load.add_summon_item(item, survivor)` files a mod item as a summoning staff
of the survivor, on both sides, so the game treats it as that survivor's
staff. `load.on_summon` runs before a staff spawns or moves a survivor;
returning `true` makes the use take the item, in the same transaction and only
when the spawn or move works.

```rust
use enshwick_loader_api::survivors::SurvivorId;

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    const MY_STAFF: u32 = 0x5e01;
    load.add_summon_item(MY_STAFF, SurvivorId(1_201_801_546))?;
    load.on_summon(|summon| summon.item == MY_STAFF && summon.ammo == 0)
}
```

## The survivor menu

The client's survivor menu keeps one record per survivor. With several
entities of one survivor, `load.on_survivor_status` decides what it shows: it
gets the survivor and the status byte of each entity, and returns the byte to
show, or `None` for the game's. Client only; one mod at a time.
