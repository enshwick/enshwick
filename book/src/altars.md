<!--
type: how-to
description: A mod reads the Flame Altars of a world through a handle it asks for in init: each altar's entity, level and build zone, and an event when the altars change.
-->

# Altars and bases

Flame Altars are game data, so the platform reads them for every mod. A mod
calls [`load.altars()`](https://github.com/enshwick/enshwick/blob/main/crates/internal/world/src/altars.rs) in `init`.
The call returns a `Copy` handle to keep in its state.

## Reading the altars

```rust
use enshwick::events::AltarsChanged;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let altars = load.altars()?;
    load.on::<AltarsChanged>(move |()| {
        for altar in altars.list() {
            enshwick::info!(
                "altar {} level {} zone {:?}",
                altar.entity.0,
                altar.level,
                altar.zone
            );
        }
    })
}
```

```luau
local altars = enshwick.altars.altars()
enshwick.on("AltarsChanged", function()
    for _, altar in altars:list() do
        local low, high = altar.zone.min, altar.zone.max
        enshwick.log.info(`altar {altar.entity} level {altar.level} zone {low[1]},{low[2]},{low[3]} to {high[1]},{high[2]},{high[3]}`)
    end
end)
```

The loader follows the altars in the game server: the dedicated server, or a
client while it hosts a game. A client that only joins a server sees none.

## Cells and zones

Zones are boxes of whole 0.5 m cells, half-open (`min <= p < max`) like the
engine's own point test, with axis 1 up. A `CellBox` has these methods:

- `contains`
- `touches`
- `footprint_area`

Two functions turn a position into its cell: `cell_of_fixed` and
`cell_of_metres`.

Each `Altar` has:

- `entity` and `level`;
- `position`, in metres;
- `zone`: the zone as the engine has built it now, growth by any mod
  included;
- `levels`: the vanilla zone at every level up to the current one, each inside
  the next;
- `tilted`: an altar tilted off the vertical has no known zone.

`list` works from any thread and returns an empty list while no world runs.
The loader raises `AltarsChanged` on the game thread after a step that
changed an altar. An altar changes when it is placed, removed, raised a level
or has its zone built anew. A removed altar leaves the list a few seconds and
some 60 server steps after the game last showed it. This holds for the last
altar too, and then the list turns empty. A server that stalls keeps its
altars.

## Bases

The engine groups altars whose zones touch or overlap into one base.
`altars.bases()` returns those groups. Each holds its altars, their zones and
the bounds of all of them. `Base::contains` tells whether a cell lies in one
of its zones. `bases_of` does the same for a list a mod already holds.

```rust
fn based(altars: &enshwick::altars::Altars, cell: [i64; 3]) -> bool {
    altars.bases().iter().any(|base| base.contains(cell))
}
```

## Sizing the zones

Any mod may size the zones:
[`load.on_altar_zone(|zone| ...)`](https://github.com/enshwick/enshwick/blob/main/crates/internal/world/src/altars.rs)
adds its filter to a chain. When altars change, the loader asks the filters
about every altar, in load order. Each filter sees the zone so far, the
vanilla one first. It may grow the zone by `grow` cells on every horizontal
side or make it `height` cells tall. The loader turns the result into the
engine's own shape for the altar's turn. It writes that shape on the server's
next zone pass, while the altar stays at that level.

```rust
fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    // Every altar four cells wider on each side, on top of what mods before
    // this one gave it.
    let sizes = load.on_altar_zone(|zone| zone.grow += 4)?;
    let _ = sizes;
    Ok(())
}
```

Filters run on the server's game thread, once per altar each time the
altars change. A mod that solves sizes at length does it on a thread of
its own. It keeps the answers where its filter reads them. It calls
`sizes.changed()` from any thread to have the zones sized again on the next
step. A size changes the altar's built zone, so it raises `AltarsChanged`
again. Before solving again, compare the input you solve from, the `levels`,
not the built zones.

## Altar kinds with zones of their own

`load.add_flame_altar(NewFlameAltar)` adds a kind of Flame Altar. The kind is a
[prop](content/props.md) copied from the vanilla altar whose item is
`item.like`. It has its item, its template and, with `recipe_cost`, its
recipe. Its altars take `zones`, one `KindZone { grow, height }` per level
from 1. Each entry gives the cells added on each side of the vanilla zone and
the height, in the terms of a zone filter. A level past the list takes the last entry. The kind sizes its
altars through a zone filter of the mod, so filters that run after it see the
kind's size. Each altar's `template` names its kind.

On a server copy, a level-1 altar of a kind with `grow = 20` and
`height = 80` had a zone 121 cells wide and 80 high. The game's level-1 altar
had 80 by 80. The kind looks like the vanilla altar: a new look needs client
files.

```rust
use enshwick::altars::{KindZone, NewFlameAltar};
use enshwick::content::{Guid, id};
use enshwick::content::items::NewItem;

fn init(load: &mut enshwick::Load<'_>, altar: Guid) -> Result<(), enshwick::Off> {
    load.add_flame_altar(NewFlameAltar {
        item: NewItem::new(id!("wide-altar"), altar),
        zones: vec![KindZone { grow: 20, height: 80 }],
        recipe_cost: Some(2),
    })?;
    Ok(())
}
```

In Luau the call is `enshwick.altars.add_flame_altar{...}`, at the top level
of `main.luau`, with the same fields.

```luau
local altar = "<the GUID of the game's Flame Altar item>"
local wide = enshwick.altars.add_flame_altar({
    item = { id = "you.altars:wide-altar", like = altar },
    zones = { { grow = 20, height = 80 } },
    recipe_cost = 2,
})
enshwick.log.info(`wide altars are {wide.template}`)
```

### A maximum level for a kind

`load.limit_altar_level(kind, max_level)` keeps a kind's altars at
`max_level` or below. It is unstable: it needs the `unstable` feature. An
altar's level rises by an upgrade recipe crafted at the altar. On the server,
a craft filter of the mod refuses the vanilla altar upgrades from
`max_level` up at an altar of the kind. The filter runs before
the crafting system reads the request, so the altar keeps its level and the
player keeps the items. A `max_level` of 0 refuses even the claim. The
Flame's own upgrades, upgrade recipes that mods add, other kinds and the
vanilla altar keep the game's limit. The upgrade path was read from the code
of server 26430253 and the recipes from the data of client 26430625.

```rust
use enshwick::altars::NewFlameAltar;
use enshwick::content::{Guid, id};
use enshwick::content::items::NewItem;

fn init(load: &mut enshwick::Load<'_>, altar: Guid) -> Result<(), enshwick::Off> {
    let small = load.add_flame_altar(NewFlameAltar {
        item: NewItem::new(id!("small-altar"), altar),
        zones: Vec::new(),
        recipe_cost: None,
    })?;
    load.limit_altar_level(small, 2)
}
```

In Luau: `enshwick.altars.limit_altar_level(small, 2)`.

## Altars per level and bases past 16

This mod lets a server admin set how many Flame Altars every Flame level
allows. Its `Cargo.toml` says `side = "both"`, since each player's interface
asks the same limit:

<!-- altar-count -->
```rust,standalone_crate
use std::sync::atomic::{AtomicU8, Ordering};

use enshwick::{Load, Mod, Off};

/// Flame Altar settings.
#[enshwick::config]
struct Settings {
    /// Altars allowed at every Flame level, 1 to 16.
    altars: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self { altars: 16 }
    }
}

/// What the limit answers; replaced as each world loads.
static ALTARS: AtomicU8 = AtomicU8::new(16);

struct AltarCount;

impl Mod for AltarCount {
    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        load.on_settings(|settings: &Settings| {
            ALTARS.store(settings.altars.clamp(1, 16), Ordering::Relaxed);
        })?;
        load.on_altar_limit(|limit| limit.allowed = ALTARS.load(Ordering::Relaxed))?;
        Ok(Self)
    }
}

enshwick::declare_mod!(AltarCount);
```

```luau
local altars = 16
enshwick.on_settings(function(settings)
    altars = math.clamp(settings.altars, 1, 16)
end)

enshwick.altars.on_altar_limit(function(limit)
    limit.allowed = altars
    return limit
end)
```

The Luau mod has `side = "both"` in its `pack.toml`. The same file
holds the setting `altars` ([Settings](settings.md#settings-of-a-luau-mod)). The Rust limit reads an
atomic, so the settings handler can change it while the game runs.

`load.on_altar_limit(|limit| ...)` adds a filter to the chain that decides
how many altars each Flame level allows, on both sides. Each filter runs in
load order. It sees the level, the game's own count and the count so far, and
sets `allowed`. The loader keeps the answer within the zones the game groups.

The game groups only 16 zones into its 16 bases. `load.raise_base_limit()`
lets it group every zone it lists and allows up to 32 altars a level. With it,
the game refuses an altar that would start a 17th base with its own "too many
altars".

`load.request_base_limits(zones, bases)` needs the `unstable` feature
([Stability](stability.md)). It allows up to `zones` altars a level, at most
255. The game then groups every zone it lists, in up to `bases` bases, at most
179. The largest request of every mod holds. A value past
`base_limit().max_zones` or `max_bases` turns the mod off.

Both return `false` on a game build whose code the platform does not know,
where the game keeps 16. The loader's hooks go in together or not at all: when
one cannot be armed, the game keeps 16. Bases past 16 need more of the game's
code than the zones do. Where the loader cannot arm all of it, the game keeps
16 bases with the zones raised. The log then says what was missing, and
`max_bases` reads 16.

A server holds the bases its mods asked for. A joined client with bases past
16 armed groups every base the server does. A client whose bases past 16 could
not arm groups 16, and its log says so.

Settings arrive only as a world loads, so a mod whose bases count is a setting
asks for its most at init. It then calls `altars::set_base_cap(bases)` from
its settings handler, and the server refuses an altar that would start a base
past the cap. Each call replaces the mod's own cap. The lowest cap of every
mod holds. A cap at or past the request leaves the request, and a cap below 16
is refused.

`altars::grouping()` tells how many zones and bases the last grouping found.
`altars::base_limit()` tells the most the game groups now.
`BaseLimit::past_vanilla` tells whether a world needs the raised limit to
open.

## Building across zones

Vanilla accepts a placed object only when both corners of its box lie in one
zone.
[`load.on_building_zone_check`](https://github.com/enshwick/enshwick/blob/main/crates/internal/world/src/altars.rs)
runs a filter when the game refuses an object for that reason. The filter gets
the zones, the object's box and the margin in 32.32 fixed-point metres.
Returning `true` accepts the object after all.

```rust
fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.on_building_zone_check(|check| {
        check.zones.iter().any(|zone| {
            (0..3).all(|a| zone.min[a] - check.margin <= check.object.min[a] && check.object.max[a] < zone.max[a] + check.margin)
        })
    })
}
```

The check runs on both sides, since a client predicts placement. It also runs
while a player aims a piece, so keep it cheap and never block in it. A mod that changes
placement this way counts in the join check, so every player runs it.

## Worked example

[`enshwick-better-altars`](https://github.com/enshwick/mods/tree/main/better-altars)
is a whole mod on these calls. Its
[`lib.rs`](https://github.com/enshwick/mods/blob/main/better-altars/src/lib.rs)
answers `on_altar_limit` per Flame level from its settings. Its
[`zones.rs`](https://github.com/enshwick/mods/blob/main/better-altars/src/zones.rs)
sizes the zones with `on_altar_zone`. It sizes them again on `AltarsChanged`.
