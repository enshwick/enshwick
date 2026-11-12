//! The world domain: bases, altars, terrain and voxels, weather, time of day,
//! the map, buffs and game settings.

#[cfg(windows)]
pub(crate) mod altars;
#[cfg_attr(
    not(windows),
    expect(dead_code, reason = "only the running game groups altar zones")
)]
pub(crate) mod base_groups;
#[cfg(windows)]
pub(crate) mod bases;
#[cfg(windows)]
pub(crate) mod buffs;
#[cfg(windows)]
pub(crate) mod daytime;
#[cfg(windows)]
pub(crate) mod entities;
#[cfg(windows)]
pub(crate) mod fog;
pub mod fog_image;
#[cfg(windows)]
pub(crate) mod game_settings;
#[cfg(windows)]
pub(crate) mod label;
pub mod map_frame;
pub mod map_marker_types;
pub mod prop_holds;
#[cfg_attr(
    not(windows),
    expect(dead_code, reason = "only the Windows build asks the game's zones")
)]
pub(crate) mod rights;
#[cfg(windows)]
pub(crate) mod scene;
pub mod scene_markers;
#[cfg(windows)]
pub(crate) mod terrain;
pub mod terrain_pages;
#[cfg_attr(
    not(windows),
    expect(dead_code, reason = "only the Windows build writes the game's voxels")
)]
pub(crate) mod transactions;
#[cfg_attr(
    not(windows),
    expect(dead_code, reason = "only the Windows build reads the game's voxels")
)]
pub(crate) mod voxels;
#[cfg(windows)]
pub(crate) mod weather;
#[cfg(windows)]
pub(crate) mod writes;
