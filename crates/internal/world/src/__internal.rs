//! What core calls inside this crate and a mod must not: the parts of
//! `Load`'s methods that join this domain with one above it, whose bodies
//! stay in core.

#[cfg(feature = "unstable")]
use crate::abi::RecipeId;
#[cfg(feature = "unstable")]
use crate::content::Guid;
#[cfg(feature = "unstable")]
use crate::{Entity, Host, Off};

/// The handles of the typed API a script holds, which the scripts' start
/// functions on `Load` return.
#[cfg(feature = "typed")]
pub mod typed_script {
    pub use crate::typed::runtime::script::{ComponentType, EventType, ResourceType};
}

/// The ids of the vanilla altar upgrade recipes that raise an altar from
/// `max_level` or above, read from the game's recipe registry.
///
/// # Errors
///
/// [`Off`] when the registry does not read or holds no altar claim recipe.
#[cfg(feature = "unstable")]
pub fn altar_upgrades_from(host: Host, max_level: u8) -> Result<Vec<RecipeId>, Off> {
    crate::altars::kinds::upgrades_in_registry(host, max_level)
}

/// The template of the altar `entity` stands for; `None` for another
/// entity or one that does not read.
#[cfg(feature = "unstable")]
#[must_use]
pub fn altar_template_of(host: Host, entity: Entity) -> Option<Guid> {
    crate::altars::template_of(host, entity)
}

/// The players connected now, from the loader.
#[cfg(feature = "unstable")]
#[must_use]
pub fn online(host: Host) -> Vec<crate::world::Player> {
    crate::world::online(host)
}

/// The map marker type `info` describes, as the loader lists them.
#[cfg(feature = "unstable")]
#[must_use]
pub fn map_marker_type(info: &crate::abi::MapMarkerTypeInfo) -> crate::map::MapMarkerType {
    crate::map::MapMarkerType::from_info(info)
}

/// What a frame of the interface runs for `Load::on_fog_uncovered`: it
/// looks at `fog` four times a second and calls `handler` with each window
/// of new ground.
#[cfg(feature = "unstable")]
pub fn fog_uncovered<F>(fog: crate::map::FogOfWar, handler: F) -> impl FnMut() + Send + 'static
where
    F: Fn(&crate::map::FogOfWarUncovered) + Send + Sync + 'static,
{
    crate::map::events::fog_uncovered(fog, handler)
}
