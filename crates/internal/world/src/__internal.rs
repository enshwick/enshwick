//! What core calls inside this crate and a mod must not: the parts of
//! `Load`'s methods that join this domain with one above it, whose bodies
//! stay in core, and the crate-private reads the domains above build on,
//! such as a world call's status as its outcome and a player as the loader
//! hands them over.

use crate::abi::RecipeId;
use crate::content::Guid;
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
pub fn altar_upgrades_from(host: Host, max_level: u8) -> Result<Vec<RecipeId>, Off> {
    crate::altars::kinds::upgrades_in_registry(host, max_level)
}

/// The template of the altar `entity` stands for; `None` for another
/// entity or one that does not read.
#[must_use]
pub fn altar_template_of(host: Host, entity: Entity) -> Option<Guid> {
    crate::altars::template_of(host, entity)
}

/// Whether the loader takes the terrain edit `edit`, a check the loader and
/// the fake host share.
///
/// It takes a known kind and shape, a radius above 0 and up to
/// [`MAX_RADIUS`](crate::terrain::MAX_RADIUS), a position at 0 or more on
/// every axis, and for an add or a flatten a terrain material, 1 to 127,
/// for an add a height above 0 and up to the same limit.
#[must_use]
pub fn terrain_edit_valid(edit: &crate::abi::TerrainEdit) -> bool {
    use crate::abi::TerrainEdit;
    let reach = |metres: f32| metres > 0.0 && metres <= crate::terrain::MAX_RADIUS;
    let material = (1..=0x7f).contains(&edit.material);
    reach(edit.radius)
        && edit
            .position
            .iter()
            .all(|axis| axis.is_finite() && *axis >= 0.0)
        && edit.shape <= TerrainEdit::SQUARE
        && match edit.kind {
            TerrainEdit::ADD => material && reach(edit.height),
            TerrainEdit::DIG => true,
            TerrainEdit::FLATTEN => material,
            _ => false,
        }
}

/// Whether the loader takes the changes of the terrain material `material`.
///
/// The loader and the fake host share it: changes it knows, a health of 1
/// or more and a hardness the game names when it sets them.
#[must_use]
pub fn terrain_material_changes_valid(material: &crate::abi::TerrainMaterialAdd) -> bool {
    use crate::abi::TerrainMaterialAdd;
    let sets = |change| material.changes & change != 0;
    material.changes & !TerrainMaterialAdd::ALL_CHANGES == 0
        && (!sets(TerrainMaterialAdd::HEALTH) || material.health >= 1)
        && (!sets(TerrainMaterialAdd::HARDNESS) || material.hardness <= HARDEST)
}

/// The buff collection patch the loader and the fake host build: each mod
/// buff type's edit, its check against the vanilla collection, and the
/// build of the collection with every mod's types.
#[cfg(feature = "unstable")]
pub mod buff_types {
    pub use crate::buffs::new::{Edit, Refusal, build, check};
}

/// The hardest of the game's hardness categories, `Unbreakable`
/// (`HardnessCategory`, reflection 26430625).
pub const HARDEST: u8 = 5;

/// `status` of a world call as its outcome.
///
/// # Errors
///
/// The [`WorldError`](crate::WorldError) `status` stands for.
pub fn world_status(status: crate::abi::Status) -> Result<(), crate::WorldError> {
    crate::WorldError::from_status(status)
}

/// `status` of a world call as its outcome, with `missing` for a thing the
/// call did not find.
///
/// # Errors
///
/// The [`WorldError`](crate::WorldError) `status` stands for.
pub fn world_check(
    status: crate::abi::Status,
    missing: crate::WorldError,
) -> Result<(), crate::WorldError> {
    crate::WorldError::check(status, missing)
}

/// The player `info` describes, as the loader hands them over.
#[must_use]
pub fn player(info: &crate::abi::PlayerInfo) -> crate::world::Player {
    crate::world::Player::from_info(info)
}

/// The players connected now, from the loader.
#[must_use]
pub fn online(host: Host) -> Vec<crate::world::Player> {
    crate::world::online(host)
}

/// The account of `player`, found among the connected players when only
/// the character was named.
///
/// # Errors
///
/// [`crate::WorldError::Gone`] when no connected player has the character.
pub fn player_key(
    host: Host,
    player: crate::world::PlayerRef,
) -> Result<crate::PlayerKey, crate::WorldError> {
    player.key_in(host)
}

/// The character of `player`, found among the connected players when only
/// the account was named.
///
/// # Errors
///
/// [`crate::WorldError::Gone`] when no connected player has the account.
pub fn player_entity(
    host: Host,
    player: crate::world::PlayerRef,
) -> Result<Entity, crate::WorldError> {
    player.entity_in(host)
}

/// The players connected now, or the loader's status when it cannot list
/// them.
///
/// # Errors
///
/// The loader's status of its players list.
pub fn listed(host: Host) -> Result<Vec<crate::world::Player>, crate::abi::Status> {
    crate::world::listed(host)
}

/// The characters of the connected players `keys`, from one list of the
/// players, as the world finds them.
///
/// # Errors
///
/// [`WorldError::Gone`](crate::WorldError::Gone) for a key not connected or
/// without a character; another [`WorldError`](crate::WorldError) when the
/// loader cannot list the players.
pub fn characters<const N: usize>(
    host: Host,
    keys: [crate::PlayerKey; N],
) -> Result<[Entity; N], crate::WorldError> {
    crate::world::characters(host, keys)
}

/// The `CurrentTransform` component type on the host's build, which
/// [`position`] reads; only this crate makes one, so a position read always
/// reads that component.
#[derive(Clone, Copy, Debug)]
pub struct Transform(pub(crate) crate::ecs::ComponentType);

impl Transform {
    /// Resolves `CurrentTransform` on the host's build.
    ///
    /// # Errors
    ///
    /// [`Off`] when the build does not reflect it.
    pub fn named(host: Host) -> Result<Self, Off> {
        crate::ecs::ComponentType::named(host, crate::engine_ids::CURRENT_TRANSFORM).map(Self)
    }
}

/// Where `entity` stands, in metres, from its `CurrentTransform`, as
/// [`World::position`](crate::world::World::position) reads it.
#[must_use]
pub fn position(transform: Transform, host: Host, entity: Entity) -> Option<[f64; 3]> {
    crate::world::position(transform, host, entity)
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
