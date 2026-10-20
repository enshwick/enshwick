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
staff. `load.on_summon` runs before a staff spawns or moves a survivor and
returns a `SummonVerdict`: `Take` makes the use take the item, in the same
transaction and only when the spawn or move works; `Refuse` refuses the use,
and nothing spawns, moves or is taken; `Allow` leaves it to the game.
`load.on_summoned` runs after a use that worked.

A `Summon` names the used item: its id, its `slot`, its own entity `pide`
and, for an item with a [meter](content/items.md#meters), the `meter` as
clients see it; after the use, `consumed` says whether the game took it. A
staff with charges takes the item on its last charge from `on_summon` and
spends one in `on_summoned` otherwise. Both run inside the game's step, on
the thread of the use: hand world writes to the [scheduler](events.md#the-scheduler).

`survivors::WORKSHOP_SURVIVORS` lists the game's workshop survivors, each with
its id, its vanilla staff and the template the staff places;
`WorkshopSurvivor::find("blacksmith")` picks one by its short name.

```rust
use enshwick_loader_api::survivors::{SummonVerdict, WorkshopSurvivor};

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    const MY_STAFF: u32 = 0x5e01;
    let smith = WorkshopSurvivor::find("blacksmith")
        .ok_or_else(|| enshwick_loader_api::Off::new("no blacksmith"))?;
    load.add_summon_item(MY_STAFF, smith.id())?;
    // The last charge takes the staff; the others are spent after the use.
    load.on_summon(|summon| {
        let last = summon.meter.is_none_or(|charges| charges.current <= 1);
        if summon.item == MY_STAFF && last {
            SummonVerdict::Take
        } else {
            SummonVerdict::Allow
        }
    })?;
    let (world, scheduler) = (load.world()?, load.scheduler()?);
    load.on_summoned(move |summon| {
        if summon.item == MY_STAFF && !summon.consumed {
            let staff = summon.pide;
            scheduler.next_tick(move || {
                let _spent = world.change_item_meter(staff, |charges| charges - 1);
            });
        }
    })
}
```

## The survivor menu

The client's survivor menu keeps one record per survivor. With several
entities of one survivor, `load.on_survivor_status` decides what it shows: it
gets the survivor and the status byte of each entity, and returns the byte to
show, or `None` for the game's. Client only; one mod at a time.
