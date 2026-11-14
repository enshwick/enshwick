//! The world domain's namespaces.

use super::Namespace;
use crate::Load;
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
        &[
            Load::SCRIPT_WORLD_JOIN,
            Load::SCRIPT_WORLD_RENAME,
            Load::SCRIPT_WRITABLE_TYPE,
        ],
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
