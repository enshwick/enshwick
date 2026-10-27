# The map

[`load.map()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/map.rs)
returns the `Map` of the game server, dedicated or hosted by a client: the
markers the world comes with and the waypoint each player sets. It is
`Copy`; get it during `init` and move it into handlers and tasks. Every call
belongs on the server's game thread, like the [world](world.md)'s.

```rust
use core::ops::ControlFlow;
use std::time::Duration;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let map = load.map()?;
    let world = load.world()?;
    load.scheduler()?.every(Duration::from_secs(30), move || {
        let markers = map.markers().map_or(0, |markers| markers.len());
        let waypoints = world
            .players()
            .iter()
            .filter(|player| matches!(map.waypoint(player.entity), Ok(Some(_))))
            .count();
        enshwick::info!("{markers} markers, {waypoints} waypoints set");
        ControlFlow::Continue(())
    });
    Ok(())
}
```

## Markers

`map.markers()` lists every entity with a `MapMarker`, by entity:
towns, ruins, dungeons, Flame shrines, quest areas and the like. Each
`MapMarker` has its entity, its position in metres, its type and the text
tag of its label. `map.markers_in(a, b)` keeps those in the box between two
corners, edges included. Both walk every marker, so call them now and then,
not every step. They fail with a [`WorldError`](world.md#writing-components):
`WrongThread` off the game thread, `Missing` while no world runs, and
`Refused` for a corner that is not finite.

A marker's type is a `MapMarkerTypeId`, the key of the game's marker type
registry. The registry is game data and grows with the game, so the API
has no list of types; compare ids, or keep your own table of the ones a mod
cares about. A marker without a type or a label has `None` there.

The label is a text tag. A client reads its text with
`enshwick::i18n::game_text(label.id())`; a dedicated server has no texts.

## Waypoints

`map.waypoint(player)` returns where a player's waypoint is, the one every
player in the game sees, in metres; `None` while it is off. The server keeps
it only while the player plays, never in the save. It fails like
`markers()`, and with `Missing` on a game build without waypoints.

## Snapshots for a map outside the game

`map.snapshot(altars)` takes what the map shows now as a `MapSnapshot`:
every player with position and waypoint, the Flame Altars of an `Altars`
from `load.altars()` with the base each belongs to, and every marker. It is
plain serde data in Enshwick's own format, for a process beside the server
such as a web map. `to_bytes()` writes `MAP_SNAPSHOT_FORMAT` in two bytes and
then the fields in postcard's encoding; `MapSnapshot::from_bytes` reads it
back and refuses any other format. A snapshot carries a marker's label as a
text id only, since a dedicated server has no texts. Take one every few
seconds, not every step: it walks every marker.

## The fog of war on the client

`load.fog_of_war()` asks the loader to copy the client's fog of war for
the mod, during `init`; on a dedicated server it fails, since only the
player's machine has the fog. The loader copies it as the game uncovers it,
so `FogOfWar` reads from any thread.

`fog.state()` returns a `FogState`: the image's size in pixels, the metres
one pixel covers, the world's z size, a `version` that grows whenever pixels
may have changed, and a `session` that changes with each world the client
enters. Once the client leaves the world, the copy is gone until it enters one
again. `state.pixel_of(position)` finds the pixel of a place in metres:
row 0 is the world's high z edge, and x grows with world x. `fog.read(min,
size)` copies a box of pixels as a `FogImage`, one byte each from 0 (hidden)
to 255 (uncovered), and `fog.coverage(position)` reads one place. While
the client is in no world, on a game build without the fog, or after the
loader stopped copying it, they fail with `FogError::Missing`.

## What a server does not have

The markers a player places by hand and the map's image live only on the
player's machine, and so does the fog of war. A server never sees them.
