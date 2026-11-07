<!--
type: how-to
description: The Map of the game server holds the markers a world comes with, its frame and each player's waypoint, with snapshots for other maps and the fog of war.
-->

# The map

Unstable: everything in this chapter needs the `unstable` feature and may
change in any release ([Stability](stability.md)).

[`load.map()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/map.rs)
returns the `Map` of the game server, dedicated or hosted by a client.

## Getting the map

The `Map` holds the markers the world comes with, its frame and the waypoint
each player sets. It is `Copy`; get it during `init` and move it into handlers and tasks. Every call
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

```luau
enshwick.schedule.every(30, function()
    local ok, markers = pcall(enshwick.map.markers)
    local waypoints = 0
    for _, player in enshwick.world.players() do
        local set, at = pcall(enshwick.map.waypoint, player.entity)
        if set and at then
            waypoints += 1
        end
    end
    enshwick.log.info(`{ok and #markers or 0} markers, {waypoints} waypoints set`)
    return true
end)
```

## Markers

`map.markers()` lists every marker the scene places, in the scene's order:
towns, ruins, dungeons, Flame shrines, quest areas and the like. Each
`MapMarker` has its position in metres, its type and the text tag of its
label. The loader reads them from the game's data once per scene, apart
from the game thread. So they are there with no player near, and later calls
are cheap. Until that read is done, a second or two after the first call,
they fail with `NotLoaded`: ask again later.
`map.markers_in(a, b)` keeps those in the box between two corners, edges
included. They fail with a [`WorldError`](world.md#writing-components):

- `WrongThread` off the game thread;
- `Missing` while no world runs or when the scene does not read on the game
  build;
- `Refused` for a corner that is not finite.

A marker's type is a `MapMarkerTypeId`, the key of the game's marker type
registry. The registry is game data and grows with the game, so the API
has no fixed list of types. A client reads the registry with
`Ui::map_marker_types` ([Map markers](interface.md#map-markers)). Its
`MapMarkerType` entries carry the same keys with their group and flags. A
dedicated server's data has no registry. A marker without a type or a
label has `None` there.

The label is a text tag. A client reads its text with
`enshwick::i18n::game_text(label.id())`; a dedicated server has no texts.

## Waypoints

`map.waypoint(player)` returns where a player's waypoint is, the one every
player in the game sees, in metres; `None` while it is off. The server keeps
it only while the player plays, never in the save. It fails like
`markers()`, and with `Missing` on a game build without waypoints.

## Hearing waypoints and pings

`load.on_waypoint(handler)` calls the handler on the server's game thread
when a player sets, moves or clears their waypoint. The handler gets a
`WaypointChanged`: the player's account, their character and the new
`position`. The `position` is `None` once cleared. The waypoints are compared
on a tick every tenth of a second, so a change shows within that. The waypoint
a player has when they join counts as no change.

`load.on_ping(handler)` calls the handler for each ping, the game's
`UiPingEvent`. The handler gets a `MapPinged`: the pinging player's account,
the entity it comes from and where. The account is `None` for a ping a mod
raised for another entity. One sender is heard at most once a tenth of a
second.

Both come from what a client sends, so neither hands on a place outside
the [map's frame](#the-maps-frame). A waypoint there is named once in the
log. Both are unstable (see [Stability](stability.md)). A server copy showed a
ping a mod raises reaching `on_ping`. A player's own ping and waypoint are not
yet seen in the game.

```rust
fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.on_waypoint(|change| match change.position {
        Some(at) => enshwick::info!("{:?} marks {at:?}", change.player),
        None => enshwick::info!("{:?} clears the waypoint", change.player),
    })?;
    load.on_ping(|ping| enshwick::info!("{:?} pings {:?}", ping.player, ping.position))
}
```

```luau
enshwick.map.on_waypoint(function(change)
    enshwick.log.info(`{change.player} marks {change.position}`)
end)
enshwick.map.on_ping(function(ping)
    enshwick.log.info(`{ping.entity} pings {ping.position}`)
end)
```

## The map's frame

`map.frame()` returns a `MapFrame`: where the world's map starts, how far
it reaches and the size of the game's map cells. All three are in metres on
the world's x and z axes. A map drawn outside the game puts its corner at
`origin` and spans `size`. On the main scene of the current game the frame
starts at (0, 0). It spans 10 240 m each way in cells of 8 m.

```rust
use enshwick::map::Map;

fn place(map: Map, position: [f64; 3]) -> Option<[f64; 2]> {
    let frame = map.frame().ok()?;
    let across = |axis: usize, of: usize| (position[axis] - frame.origin[of]) / frame.size[of];
    Some([across(0, 0), across(2, 1)])
}
```

The loader reads it once per scene like the markers, so it fails like
`markers()`:

- `NotLoaded` until that read is done;
- `WrongThread` off the game thread;
- `Missing` while no world runs, for a scene without a map, or when the
  scene does not read on the game build.

## Snapshots for a map outside the game

`map.snapshot(altars)` takes what the map shows now as a `MapSnapshot`. It
holds:

- every player with position and waypoint;
- the Flame Altars of an `Altars` from `load.altars()`, with the base each
  belongs to;
- every marker;
- the map's frame, `None` for a scene without one.

It is plain serde data in Enshwick's own format, for a process beside the server
such as a web map. `to_bytes()` writes `MAP_SNAPSHOT_FORMAT` in two bytes and
then the fields in postcard's encoding. `MapSnapshot::from_bytes` reads it
back and refuses any other format. A snapshot carries a marker's label as a
text id only, since a dedicated server has no texts. Take one every few
seconds, not every step: it walks every marker.

## The fog of war on the client

`load.fog_of_war()` asks the loader to copy the client's fog of war for
the mod, during `init`. On a dedicated server it fails, since only the
player's machine has the fog. The loader copies it as the game uncovers it,
so `FogOfWar` reads from any thread.

`fog.state()` returns a `FogState` with these fields:

- the image's size in pixels;
- the metres one pixel covers;
- the world's z size;
- a `version` that grows whenever pixels may have changed;
- a `session` that changes with each world the client enters.

Once the client leaves the world, the copy is gone until it enters one
again. `state.pixel_of(position)` finds the pixel of a place in metres:
row 0 is the world's high z edge, and x grows with world x.
`fog.read(min, size)` copies a box of pixels as a `FogImage`, one byte each
from 0 (hidden) to 255 (uncovered). `fog.coverage(position)` reads one place. While
the client is in no world, on a game build without the fog, or after the
loader stopped copying it, they fail with `FogError::Missing`.

## What a server does not have

The markers a player places by hand and the map's image live only on the
player's machine, and so does the fog of war. A server never sees them.
