//! The world domain's namespaces.

use super::Namespace;

pub(super) const NAMESPACES: &[Namespace] = &[
    #[cfg(feature = "unstable")]
    Namespace::of::<crate::world::World>(),
    #[cfg(feature = "unstable")]
    Namespace::starting(
        "world",
        "",
        &[
            crate::Load::SCRIPT_WORLD_RENAME,
            crate::Load::SCRIPT_WRITABLE_TYPE,
        ],
    ),
    Namespace::of::<crate::buffs::Buffs>(),
    Namespace::of::<crate::game_settings::GameSettings>(),
    Namespace::of::<crate::weather::Weather>(),
    Namespace::starting(
        "weather",
        "",
        &[
            crate::Load::SCRIPT_WEATHER_FRONTS,
            #[cfg(feature = "typed")]
            crate::Load::SCRIPT_WEATHER_PHENOMENA,
        ],
    ),
    #[cfg(feature = "unstable")]
    Namespace::of::<crate::map::Map>(),
    #[cfg(feature = "unstable")]
    Namespace::starting("map", "", &[crate::Load::SCRIPT_MAP_EVENTS]),
    #[cfg(feature = "unstable")]
    Namespace::on_request::<crate::map::FogOfWar>("fog_of_war"),
    Namespace::of::<crate::daytime::Clock>(),
    Namespace::starting("daytime", "", &[crate::Load::SCRIPT_DAYTIME]),
    Namespace::on_request::<crate::altars::Altars>("altars"),
    Namespace::starting(
        "altars",
        "",
        &[
            crate::Load::SCRIPT_ALTARS,
            crate::Load::SCRIPT_ALTAR_LIMIT,
            crate::Load::SCRIPT_ALTAR_LIMITS,
            crate::Load::SCRIPT_FLAME_ALTARS,
        ],
    ),
    Namespace::of::<crate::voxels::Voxels>(),
    Namespace::starting("voxels", "", &[crate::Load::SCRIPT_VOXEL_EDITS]),
    Namespace::of::<crate::terrain::Terrain>(),
];
