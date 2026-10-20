# Altars and bases

Flame Altars are game data, so the platform reads them for every mod. A mod
asks for them during `init` with
[`load.altars()`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/altars.rs)
and gets a `Copy` handle to keep in its state:

```rust
use enshwick_loader_api::events::AltarsChanged;

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    let altars = load.altars()?;
    load.on::<AltarsChanged>(move |()| {
        for altar in altars.list() {
            enshwick_loader_api::info!(
                "altar {} level {} zone {:?}",
                altar.entity.0,
                altar.level,
                altar.zone
            );
        }
    })
}
```

The loader follows the altars in the game server: the dedicated server, or a
client while it hosts a game. A client that only joins a server sees none.

## Cells and zones

Zones are boxes of whole 0.5 m cells, half-open (`min <= p < max`) like the
engine's own point test, with axis 1 up. `CellBox` has `contains`, `touches`
and `footprint_area`; `cell_of_fixed` and `cell_of_metres` turn a position
into its cell.

Each `Altar` has:

- `entity`, `level` and `position` in metres;
- `zone`: the zone as the engine has built it now, growth by any mod
  included;
- `levels`: the vanilla zone at every level up to the current one, each inside
  the next;
- `tilted`: an altar tilted off the vertical has no known zone.

`list` works from any thread and returns an empty list while no world runs.
`AltarsChanged` is raised on the game thread after a step in which an altar
was placed, removed, raised a level or had its zone built anew.

## Bases

The engine groups altars whose zones touch or overlap into one base.
`altars.bases()` returns those groups, each with its altars, their zones and
the bounds of all of them; `Base::contains` tells whether a cell lies in one
of its zones. `bases_of` does the same for a list a mod already holds.

```rust
fn based(altars: &enshwick_loader_api::altars::Altars, cell: [i64; 3]) -> bool {
    altars.bases().iter().any(|base| base.contains(cell))
}
```

## Sizing the zones

One mod may size the zones:
[`load.control_altar_zones()`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/altars.rs)
makes it the one, and a second mod asking is refused. It hands the loader a
`ZoneSize` per altar: the vanilla zone of the altar's level grown by `grow`
cells on every horizontal side and made `height` cells tall. The loader turns
that into the engine's own shape for the altar's turn and writes it on the
server's next zone pass, while the altar stays at that level; an altar without
a size, or at another level, keeps its vanilla zone.

```rust
use enshwick_loader_api::altars::ZoneSize;
use enshwick_loader_api::events::AltarsChanged;

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    let control = load.control_altar_zones()?;
    load.on::<AltarsChanged>(move |()| {
        let sizes: Vec<ZoneSize> = control
            .altars()
            .list()
            .iter()
            .filter_map(|altar| {
                let zone = altar.levels.last()?;
                Some(ZoneSize { altar: altar.entity, level: altar.level, grow: 4, height: zone.max[1] - zone.min[1] })
            })
            .collect();
        let _ = control.set(&sizes);
    })
}
```

`set` works from any thread, so a mod that solves sizes at length does it on
a thread of its own. A size changes the altar's built zone and so raises
`AltarsChanged` again: compare the input you solve from, the `levels`, not the
built zones, before solving again.

## Altars per level and bases past 16

`load.on_altar_limit(|flame_level| count)` decides how many altars each Flame
level allows, on both sides; one mod at a time. The game groups only 16 zones
into its 16 bases; `load.raise_base_limit()` lets it group 32, and refuses an
altar that would start a 17th base with the game's own "too many altars". It
returns `false` on a game build whose code the platform does not know, where
the game keeps 16. The code patches and the refusal of a 17th base go in
together or not at all: when a patch or one of the loader's hooks cannot be
armed, none of them is, the mod is turned off, and the game keeps 16.
`altars::grouping()` tells how many zones and bases the last grouping found.
`altars::VANILLA_ALTARS_PER_FLAME_LEVEL` and `altars::vanilla_altars(level)`
give the game's own table, and `VANILLA_ALTAR_ZONES` and `RAISED_ALTAR_ZONES`
the two zone limits.

## Building across zones

Vanilla accepts a placed object only when both corners of its box lie in one
zone.
[`load.on_zone_check`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/altars.rs)
runs a filter when the game refuses one for that reason, with the zones, the
object's box and the margin in 32.32 fixed-point metres; returning `true`
accepts it after all.

```rust
fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    load.on_zone_check(|check| {
        check.zones.iter().any(|zone| {
            (0..3).all(|a| zone.min[a] - check.margin <= check.object.min[a] && check.object.max[a] < zone.max[a] + check.margin)
        })
    })
}
```

The check runs on both sides, since a client predicts placement, and while a
player aims a piece: keep it cheap and never block in it. A mod that changes
placement this way counts in the join check, so every player runs it.
