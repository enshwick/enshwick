# Terrain

[`load.terrain()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/terrain.rs)
returns the
[`Terrain`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/terrain.rs)
of the server's world: the terrain materials, and adding, digging and
flattening ground as the game's terraforming does. It is `Copy`; get it
during `init` and move it into handlers, tasks and actions.

```rust
use enshwick::actions::{Action, Outcome};
use enshwick::terrain::Shape;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let terrain = load.terrain();
    let world = load.world()?;
    let actions = load.actions(enshwick::i18n::Texts::default())?;
    actions.add(Action::new("level", "Flatten the ground around you").admins(), move |request| {
        let here = world.position(request.player.entity);
        let (Some(here), Some(fill)) = (here, terrain.materials().first().map(|m| m.terrain_material)) else {
            return Outcome::refused("You are nowhere");
        };
        Outcome::done(format!("{:?}", terrain.flatten(here, 6.0, fill, Shape::Round)))
    })
}
```

## Ground and materials

The ground is made of voxels half a metre a side, each of a material: air,
a terrain material with an id from 1 to 127, or a building material. Built
pieces are never touched by these edits. `materials()` lists the terrain
materials of the running game: the voxel id, the item digging it yields (as
an item id, see [items](content/items.md)), the game's hardness category,
a voxel's health against tools, and whether a player's tool can flatten it.
A mod that wants dirt or stone finds the material whose item is the dirt or
stone item.

## Edits

Every edit changes the ground at once: clients see it, and the save keeps
it. Positions are in metres and no axis may be below 0; a radius or a height
is above 0 and up to `MAX_RADIUS` (16 metres), and smaller edits, up to
about 8 metres, keep the game's frame short. `Shape::Round` is a disc or a
sphere, `Shape::Square` a square or a cube.

| Call | What it does | Returns |
| --- | --- | --- |
| `add(at, radius, height, material, shape)` | adds a mound of the material, `radius` across and `height` high at its middle, as a shovel does | the volume added, m³ |
| `dig(at, radius, shape, protect)` | digs out the terrain in the sphere or cube; a material in `protect` stays, and the dig stops at the first half-metre shell that meets it, as a pickaxe stops at rock it cannot break | the volume removed, m³ |
| `flatten(at, radius, material, shape)` | levels the ground to the height of `at`: ground above goes, gaps below fill with the material | whether anything changed |

`terrain.dry()` gives the same terrain for edits that only count: each
returns what it would change and changes nothing, to see whether there is
ground somewhere or how much an edit would take.

Each call fails with a [`WorldError`](world.md#writing-components):
`Refused` for a radius, height, position or material out of range,
`OutsideWorld` for an edit that reaches past the world's edge or above
1,024 metres, `NotLoaded` while part of what it can reach is not loaded,
`WrongThread` off the game thread, `Missing` while no world runs. The
server holds only some parts of the world in memory; an edit elsewhere
writes nothing, since the game would leave it floating there with no
ground under it. Asking starts loading the area, so the same edit works
again a second or so later.

A player's shovel and pickaxe also check where the player may build, take
the cost, give the loot and experience and keep an undo; these edits do
none of that, so a mod adds what it needs, for example with
[altar zones](altars.md) and [inventory transactions](inventories.md). For
an undo, write the edits as a transaction.

## Transactions and undo

`world.transaction()` starts a `Transaction`: edits of the world the loader
writes as one and can take back as one. `terrain_add`, `terrain_dig` and
`terrain_flatten` take the same arguments as the `Terrain` calls; `commit`
hands the edits to the loader and returns a `TransactionId`. Any thread may
commit.

```rust
use std::sync::Mutex;

use enshwick::actions::{Action, Outcome};
use enshwick::terrain::Shape;
use enshwick::world::TransactionId;

static LAST: Mutex<Option<TransactionId>> = Mutex::new(None);

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let world = load.world()?;
    let actions = load.actions(enshwick::i18n::Texts::default())?;
    actions.add(Action::new("moat", "Dig a moat around you").admins(), move |request| {
        let Some([x, y, z]) = world.position(request.player.entity) else {
            return Outcome::refused("You are nowhere");
        };
        let moat = [[8.0, 0.0], [-8.0, 0.0], [0.0, 8.0], [0.0, -8.0]]
            .into_iter()
            .fold(world.transaction(), |moat, [dx, dz]| {
                moat.terrain_dig([x + dx, y - 2.0, z + dz], 3.0, Shape::Square, &[])
            });
        match moat.commit() {
            Ok(id) => {
                *LAST.lock().unwrap_or_else(|e| e.into_inner()) = Some(id);
                Outcome::done("Digging")
            }
            Err(why) => Outcome::refused(format!("{why}")),
        }
    })?;
    actions.add(Action::new("unmoat", "Fill the last moat in").admins(), move |_| {
        let last = LAST.lock().unwrap_or_else(|e| e.into_inner()).take();
        match last.map(|id| world.undo(id)) {
            Some(Ok(())) => Outcome::done("Filling"),
            Some(Err(why)) => Outcome::refused(format!("{why}")),
            None => Outcome::refused("No moat to fill"),
        }
    })
}
```

The loader writes the edits in order on the server's game thread between
steps, about 64 bricks of 8³ voxels (4 metres a side) a step, so a large
transaction spreads over several steps instead of weighing on one; an edit
is never split, and one larger than the budget takes a step of its own.
On a dedicated server with no player, one transaction of eight digs of
6 metres, 57,512 voxels, was written over 20 steps and undone over 19; a
world step took 5.9 ms on average (18 ms for the slowest twentieth), and
steps came every 51 ms against 48 ms idle.
Before an edit writes, the loader keeps every brick it can touch, 1 KiB a
brick. `world.transaction_status(id)` tells where a transaction stands:

| Status | Meaning |
| --- | --- |
| `Pending` | queued, or written in part |
| `Applied` | written whole; it can be undone |
| `Undoing` | its undo is queued or written in part |
| `Undone` | undone, or dropped before it wrote anything |
| `Failed(why)` | stopped at an edit it could not write or read back; what it wrote stays and can be undone |
| `Expired` | its undo data was dropped; it can no longer be undone |
| `UndoRefused(why)` | the last undo stopped, keeping what it put back; ask again later |

A transaction checks that its whole region lies inside the world and is
loaded before it writes anything: if not, it fails with `OutsideWorld` or
`NotLoaded`, and the world is as it was; the check starts loading the
area, so the same transaction committed again a second or so later can
pass. Once the loader has met the
running world, a commit with an edit past its edge is refused at once
with `OutsideWorld`. A region can still unload while a large transaction is under way;
it then stops before the next edit, and an undo takes back what it wrote.

`world.undo(id)` drops a queued transaction and takes back one written in
whole or in part, putting every brick back as it was, newest edit first,
again at 64 bricks a step. Clients see the undo, and the save keeps it, as
any edit. Before it writes, the undo checks that every brick still holds
what the transaction left there, and it checks each brick again right
before writing it back. If a player, another mod or a later transaction
has changed one, the undo stops there and the status turns `UndoRefused`
with `Refused`: a check before the first write means nothing was written,
a later one keeps what was put back so far. Part of the region not loaded
refuses the undo before it writes; a column that unloads during the undo
makes it wait, and after about 3,600 steps it stops with `NotLoaded`. An
undo stopped this way may be asked again. Undo overlapping transactions
newest first.

The loader keeps up to 64 MiB of undo data, 65,536 bricks, over all
transactions; past that, the oldest written transactions lose theirs and
turn `Expired`. A transaction holds up to 1,024 edits and may touch no more
bricks than the cap; a commit, or an undo, while queued work holds the
cap is refused. The loader remembers the newest 4,096 transactions; the status of
an older one is `None`. When the world ends, queued work fails with
`Missing` and written transactions turn `Expired`.

## Reading voxels

[`load.voxels()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/voxels.rs)
returns the `Voxels` of the server's world, which read the ground and the
building blocks voxel by voxel without changing them. A `VoxelPos` is a
voxel's index on each axis from the world's corner, half a metre a step
with y up; `VoxelPos::from_metres` finds the voxel holding a world
position.

```rust
use std::ops::ControlFlow;
use std::time::Duration;

use enshwick::voxels::{VoxelMaterial, VoxelPos};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let voxels = load.voxels();
    let corner = VoxelPos::from_metres([2048.0, 60.0, 2048.0])
        .ok_or_else(|| enshwick::Off::new("the landmark lies outside the world"))?;
    load.scheduler()?.every(Duration::from_secs(60), move || {
        if let Ok(block) = voxels.read(corner, [8, 8, 8]) {
            let built = block
                .iter()
                .filter(|(_, voxel)| matches!(voxel.material(), VoxelMaterial::Building(_)))
                .count();
            enshwick::info!("{built} voxels built at the landmark");
        }
        ControlFlow::Continue(())
    });
    Ok(())
}
```

`read(min, size)` returns a `VoxelBox` of up to `READ_MAX` voxels, a box
of 64 a side; `sample(at)` reads one voxel. Each voxel has a material,
`Air`, `Terrain(id)` with the id `materials()` lists or `Building(id)`,
and a fill from 0 to 255, which shapes the surface between voxels; the
game does not name it, so the name is enshwick's. A box is read in the
order x, then y, then z; `VoxelBox::get` finds a voxel by its place and
`iter` walks them all with their places.

The server holds only some parts of the world in memory. A read of a box
that is not wholly loaded fails with `NotLoaded` and starts loading it;
about a second later the same read returns the real ground. Every read can
pull an area into memory, so a mod reads where it needs to and never
sweeps the world. A box past the world's edge, or above 1,024 metres,
fails with `OutsideWorld`: the game itself reads such voxels as air, which
a mod could not tell from real air. The edge is the game's own, read from
the running world; the height is the loader's, above every place the game
builds, since the game keeps none. An empty box,
one larger than `READ_MAX` or one past 2³⁰ voxels on an axis is `Refused`.
A read costs one lock and one copy for each 4-metre brick it touches.
Reads, like edits, need a quiet moment: inside a step, from a filter or a
hook, they fail with `WrongThread`.

Every call works on the server's game thread, from a game event, a
[scheduler](events.md#the-scheduler) task or an [action](actions.md); on a
client, while it hosts a game.

## Capturing a region

`world.props_in(a, b)` lists the props whose position lies in the box
between two corners in metres: the things players placed from items, such
as chests, beds and torches. Each `Prop` has its entity, the `ItemId` it
was placed from (its `UsedItem`) and a `WorldTransform` with its position
in metres and its orientation, a quaternion x, y, z then w with y up.
A prop is told apart by its template, the one its item places, so the
notes and animals a world comes with are not props even where they carry
an item. The call walks every entity with a `UsedItem`, so call it now and
then, not every step. Before the game first ran its inventories, a box
holding things to check fails with `Missing`.

`world.capture(a, b)` takes the region between two voxels, both included,
as plain data: a `Capture` with its voxels and its props, both counted from
the region's lowest corner, its `origin`. It reads the voxels in tiles of
up to 64 voxels, 32 metres, a side, counted from that corner, and takes at
most `CAPTURE_TILES`, 16 tiles: such as a region of 128 by 128 metres and
32 high, or a cube of 64 metres. A larger region is `Refused` before any
read. A tile read took 2.4 to 3.1 milliseconds on a server copy under
Rosetta and Wine, so a capture holds the game thread for at most about 50
milliseconds. A mod captures a bigger region in parts, one per step or
scheduler task, and keeps them together itself. `NotLoaded` and
`OutsideWorld` come from the reads: a region not wholly loaded fails after
every tile has started loading, so a second try a moment later finds it.

```rust
use std::ops::ControlFlow;
use std::time::Duration;

use enshwick::voxels::VoxelPos;
use enshwick::world::{Capture, WorldError};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let world = load.world()?;
    let corner = VoxelPos::new(7_200, 1_680, 2_880);
    let far = VoxelPos::new(7_231, 1_699, 2_911);
    let mut tries = 0;
    load.scheduler()?.every(Duration::from_secs(1), move || {
        tries += 1;
        match world.capture(corner, far) {
            Ok(shed) => {
                let turned: Capture = shed.rotated(1);
                let next_door = turned.translated([40, 0, 0]);
                enshwick::info!(
                    "{} props, {:?} voxels, to paste at {:?}",
                    turned.props().len(),
                    turned.size(),
                    next_door.map(|capture| capture.origin()),
                );
                ControlFlow::Break(())
            }
            Err(WorldError::NotLoaded) if tries < 5 => ControlFlow::Continue(()),
            Err(why) => {
                enshwick::warn!("no capture: {why}");
                ControlFlow::Break(())
            }
        }
    });
    Ok(())
}
```

`rotated(quarter_turns)` turns a capture about the vertical, each quarter
turn taking the +x side to +z as seen from above, with negative turns the
other way. The region keeps its corner: a box 32 voxels wide and 20 deep
turns into one 20 wide and 32 deep. Voxels move to their turned places and
props turn their positions and orientations with them; a voxel holds no
direction of its own, so nothing else changes. A turned orientation comes
with w at or above zero, so four quarter turns give back the same numbers
within rounding. `translated(by)` moves the
origin by a number of voxels on each axis, and is `None` once the corner
would leave the world's coordinates. Both are pure: they change only the
capture, never the world. A `Capture` implements serde's `Serialize` and
`Deserialize`, so a mod can keep it in any format serde writes.
