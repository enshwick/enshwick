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
            .filter(|player| map.waypoint(player.entity).is_some())
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
it only while the player plays, never in the save.

## What a server does not have

What a player has uncovered of the map, the markers a player places by hand
and the map's image live only on that player's machine. A server never sees
them.
