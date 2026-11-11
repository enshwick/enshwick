//! The world domain's namespaces, and how `Load` opens each of its
//! handles.

use super::{Bound, Function, Namespace};
use crate::altars::Altars;
use crate::buffs::Buffs;
use crate::daytime::Clock;
use crate::game_settings::GameSettings;
#[cfg(feature = "unstable")]
use crate::map::{FogOfWar, Map};
use crate::terrain::Terrain;
use crate::voxels::Voxels;
use crate::weather::Weather;
#[cfg(feature = "unstable")]
use crate::world::World;
use crate::{Load, Off};

pub(super) const NAMESPACES: &[Namespace] = &[
    #[cfg(feature = "typed")]
    Namespace::starting(
        "typed",
        "The game's components, events and resources as the typed API generates them from its reflection.",
        &[Load::SCRIPT_TYPED],
    ),
    #[cfg(feature = "unstable")]
    Namespace::of::<World>(),
    #[cfg(feature = "unstable")]
    Namespace::starting(
        "world",
        "",
        &[Load::SCRIPT_WORLD_RENAME, Load::SCRIPT_WRITABLE_TYPE],
    ),
    Namespace::of::<Buffs>(),
    Namespace::of::<GameSettings>(),
    Namespace::of::<Weather>(),
    Namespace::starting(
        "weather",
        "",
        &[
            Load::SCRIPT_WEATHER_FRONTS,
            #[cfg(feature = "typed")]
            Load::SCRIPT_WEATHER_PHENOMENA,
        ],
    ),
    #[cfg(feature = "unstable")]
    Namespace::of::<Map>(),
    #[cfg(feature = "unstable")]
    Namespace::starting("map", "", &[Load::SCRIPT_MAP_EVENTS]),
    #[cfg(feature = "unstable")]
    Namespace::on_request::<FogOfWar>("fog_of_war"),
    Namespace::of::<Clock>(),
    Namespace::starting("daytime", "", &[Load::SCRIPT_DAYTIME]),
    Namespace::on_request::<Altars>("altars"),
    Namespace::starting(
        "altars",
        "",
        &[
            Load::SCRIPT_ALTARS,
            Load::SCRIPT_ALTAR_LIMIT,
            Load::SCRIPT_ALTAR_LIMITS,
            Load::SCRIPT_FLAME_ALTARS,
        ],
    ),
    Namespace::of::<Voxels>(),
    Namespace::starting("voxels", "", &[Load::SCRIPT_VOXEL_EDITS]),
    Namespace::of::<Terrain>(),
];

#[cfg(feature = "unstable")]
impl Bound for World {
    const NAME: &'static str = "world";
    const DOC: &'static str = "The server's world: players, accounts, entities, props, vitals and edits in transactions. Unstable: may change in any release.";
    const PARTS: &'static [&'static [Function<Self>]] = &[
        Self::SCRIPT_PLAYERS,
        Self::SCRIPT_THINGS,
        Self::SCRIPT_ACCOUNTS,
        Self::SCRIPT_BUILD,
        Self::SCRIPT_ENTITIES,
        Self::SCRIPT_PROPS,
        Self::SCRIPT_TRANSACTIONS,
        Self::SCRIPT_VITALS,
        Self::SCRIPT_METERS,
        Self::SCRIPT_FIELDS,
        Self::SCRIPT_CAPTURE,
        Self::SCRIPT_SAVES,
    ];

    fn resolve(load: &mut Load<'_>) -> Result<Self, Off> {
        load.world()
    }
}

impl Bound for Buffs {
    const NAME: &'static str = "buffs";
    const DOC: &'static str =
        "Buffs: their types, and the buffs entities have, applied and removed.";
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_BUFFS];

    fn resolve(load: &mut Load<'_>) -> Result<Self, Off> {
        Ok(load.buffs())
    }
}

impl Bound for GameSettings {
    const NAME: &'static str = "game_settings";
    const DOC: &'static str =
        "The server's game settings: read them, change them for now, or keep a change.";
    const PARTS: &'static [&'static [Function<Self>]] =
        &[Self::SCRIPT_SETTINGS, Self::SCRIPT_READS];

    fn resolve(load: &mut Load<'_>) -> Result<Self, Off> {
        load.game_settings()
    }
}

impl Bound for Weather {
    const NAME: &'static str = "weather";
    const DOC: &'static str =
        "The weather of the server's world: what it is where, its fronts, and setting it.";
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_SKY];

    fn resolve(load: &mut Load<'_>) -> Result<Self, Off> {
        Ok(load.weather())
    }
}

#[cfg(feature = "unstable")]
impl Bound for Map {
    const NAME: &'static str = "map";
    const DOC: &'static str =
        "The world's map on the server: its markers, its frame, waypoints and snapshots.";
    const PARTS: &'static [&'static [Function<Self>]] =
        &[Self::SCRIPT_MARKERS, Self::SCRIPT_SNAPSHOT];

    fn resolve(load: &mut Load<'_>) -> Result<Self, Off> {
        load.map()
    }
}

#[cfg(feature = "unstable")]
impl Bound for FogOfWar {
    const NAME: &'static str = "map";
    const DOC: &'static str =
        "The world's map on the server: its markers, its frame, waypoints and snapshots.";
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_FOG];

    fn resolve(load: &mut Load<'_>) -> Result<Self, Off> {
        load.fog_of_war()
    }
}

impl Bound for Clock {
    const NAME: &'static str = "daytime";
    const DOC: &'static str =
        "The clock of the server's world: the time of day and how fast it runs.";
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_CLOCK];

    fn resolve(load: &mut Load<'_>) -> Result<Self, Off> {
        Ok(load.clock())
    }
}

impl Bound for Altars {
    const NAME: &'static str = "altars";
    const DOC: &'static str = "Flame Altars: their list, their bases, zones and limits.";
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_LIST, Self::SCRIPT_HELPERS];

    fn resolve(load: &mut Load<'_>) -> Result<Self, Off> {
        load.altars()
    }
}

impl Bound for Voxels {
    const NAME: &'static str = "voxels";
    const DOC: &'static str = "The world's voxels, read a box or one at a time.";
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_READS];

    fn resolve(load: &mut Load<'_>) -> Result<Self, Off> {
        Ok(load.voxels())
    }
}

impl Bound for Terrain {
    const NAME: &'static str = "terrain";
    const DOC: &'static str =
        "The world's terrain: add, dig and flatten ground, and its materials.";
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_EDITS];

    fn resolve(load: &mut Load<'_>) -> Result<Self, Off> {
        Ok(load.terrain())
    }
}
