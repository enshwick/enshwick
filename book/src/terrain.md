# Terrain

[`load.terrain()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/terrain.rs)
returns the
[`Terrain`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/terrain.rs)
of the server's world: the terrain materials, and adding, digging and
flattening ground as the game's terraforming does. It is `Copy`; get it
during `init` and move it into handlers, tasks and commands.

```rust
use enshwick::terrain::Shape;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let terrain = load.terrain();
    let world = load.world()?;
    load.add_command("level", "flattens the ground around you", move |ctx, _args| {
        let Some(me) = ctx.character() else {
            return "the console has no character".into();
        };
        let (Some(here), Some(fill)) = (world.position(me), terrain.materials().first().map(|m| m.id)) else {
            return "nowhere".into();
        };
        format!("{:?}", terrain.flatten(here, 6.0, fill, Shape::Round)).into()
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
`WrongThread` off the game thread, `Missing` while no world runs.

A player's shovel and pickaxe also check where the player may build, take
the cost, give the loot and experience and keep an undo; these edits do
none of that, so a mod adds what it needs, for example with
[altar zones](altars.md) and [inventory transactions](inventories.md).

Every call works on the server's game thread, from a game event, a
[scheduler](events.md#the-scheduler) task or a [command](commands.md); on a
client, while it hosts a game.
