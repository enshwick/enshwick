<!--
type: how-to
description: The game keeps one entity per survivor. A mod that keeps several entities of one survivor answers the game's lookups for sleep, roaming, beds and summoning.
-->

# Survivors

The game keeps one entity per survivor and asks for it from many of its
systems. It asks when a survivor sleeps, roams, gets a bed, or comes to a
summoning staff.

## Lookups

A mod that keeps several entities of one survivor answers those
questions itself through
[`load.on_survivor_lookup`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/survivors.rs).
The platform tells the askers apart and reads what each works on. The mod
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
answers:

- A `Uniqueness` answer may keep another entity than the game's. After it,
  the survivor's record stays on the game's own entity.
- After a `Spawn` answer, the move command reaches only the answered entity
  for a moment.

```rust
use enshwick::survivors::{Answer, Purpose};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.on_survivor_lookup(|lookup| match lookup.purpose {
        // A staff always spawns a new entity instead of moving the old one.
        Purpose::Spawn { .. } => Answer::Nobody,
        _ => Answer::Vanilla,
    })
}
```

```luau
enshwick.survivors.on_survivor_lookup(function(lookup)
    -- A staff always spawns a new entity instead of moving the old one.
    local purpose = lookup:purpose()
    if type(purpose) == "table" and purpose.Spawn then
        return "Nobody"
    end
    return "Vanilla"
end)
```

One mod answers at a time; a second is refused. The lookup runs on the
server's worker threads, in the dedicated server or a hosting client. For
`Cursor` it runs on a client alone too. Keep it quick, never block, and read
[altars](altars.md) through a handle taken at init.

## Beds

`load.on_survivor_bed_check` runs after a survivor's own check kept its bed.
Returning `false` makes the game clear it. A `BedCheck`, a name of Enshwick's
own, has these fields:

- `npc_id`, the survivor's id;
- `entity`, the survivor's entity;
- `bed_id`;
- `home`, the survivor's home cell;
- `bed_at`, the bed's cell.

## Summoning staffs

`load.add_summon_item(item, survivor)` files a mod item as a summoning staff
of the survivor, on both sides, so the game treats it as that survivor's
staff. `load.on_summon` runs before a staff spawns or moves a survivor. It
returns a `SummonVerdict`:

- `Take` makes the use take the item, in the same transaction and only when
  the spawn or move works.
- `Refuse` refuses the use, and nothing spawns, moves or is taken.
- `Allow` leaves it to the game.

`load.on_summoned` runs after a use that worked.

A `Summon` names the used item:

- its id;
- `slot`, its slot;
- `pide`, its own entity;
- `meter`, for an item with a [meter](content/items.md#meters), as clients
  see it;
- `consumed`, after the use, whether the game took it.

A staff with charges takes the item on its last charge from `on_summon`. It
spends one in `on_summoned` otherwise. Both run inside the game's step, on
the thread of the use: hand world writes to the [scheduler](events.md#the-scheduler).

`survivors::WORKSHOP_SURVIVORS` lists the game's workshop survivors. Each comes
with its id, its vanilla staff and the template the staff places.
`WorkshopSurvivor::find("blacksmith")` picks one by its short name.

```rust
use enshwick::survivors::{SummonVerdict, WorkshopSurvivor};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    const MY_STAFF: enshwick::ItemId = enshwick::ItemId::new(0x5e01);
    let smith = WorkshopSurvivor::find("blacksmith")
        .ok_or_else(|| enshwick::Off::new("no blacksmith"))?;
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
        if let (MY_STAFF, false, Some(staff)) = (summon.item, summon.consumed, summon.pide) {
            scheduler.next_tick(move || {
                let _spent = world.change_item_meter(staff, |charges| charges - 1);
            });
        }
    })
}
```

## The survivor menu

The client's survivor menu keeps one record per survivor. With several
entities of one survivor, `load.on_survivor_status` decides what it shows. It
gets the survivor and the status byte of each entity. It returns the byte to
show, or `None` for the game's. Client only; one mod at a time.

## Worked example

[`enshwick-survivors-per-altar`](https://github.com/enshwick/mods/tree/main/survivors-per-altar)
answers the game's survivor lookups per altar in
[`policy.rs`](https://github.com/enshwick/mods/blob/main/survivors-per-altar/src/policy.rs).
Its `on_survivor_lookup` is fed by the altars it reads on `AltarsChanged`. It
answers `Answer::Vanilla` while its setting is off.
