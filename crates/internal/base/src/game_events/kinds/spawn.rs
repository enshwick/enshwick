//! The typed event of a player's respawn point.

use super::on;
use crate::engine_ids as ids;
use crate::game_events::Field;
use crate::{BaseLoad, Off};

/// A player's respawn point moved to another base or save point.
///
/// The game raises it when the character stands in a spawn point, and
/// `World::set_respawn_point` raises it too; the server's player record and
/// the save follow it.
///
/// Unstable: needs the `unstable` feature and may change in any release.
///
/// # Examples
///
/// ```
/// use enshwick::game_events::PlayerSpawnInfoChangeEvent;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on::<PlayerSpawnInfoChangeEvent>(|moved| {
///         enshwick::info!("{} now respawns at base {:?}", moved.player_character_id, moved.base_id);
///     })
/// }
/// ```
#[cfg(feature = "unstable")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
#[expect(
    clippy::struct_field_names,
    reason = "the fields take the game's names, which all end in Id"
)]
pub struct PlayerSpawnInfoChangeEvent {
    /// The player's character, as the save names it.
    pub player_character_id: crate::content::Guid,
    /// The base of the Flame Altar the character respawns at, if any.
    pub base_id: Option<crate::content::Guid>,
    /// The save point's scene, for a respawn point outside a base.
    pub scene_instance_id: Option<UniqueSceneId>,
}

/// The game's id of a scene instance, which names a save point's scene in
/// [`PlayerSpawnInfoChangeEvent`]; never 0, which the game leaves for no
/// scene.
///
/// Unstable: needs the `unstable` feature and may change in any release.
///
/// # Examples
///
/// ```
/// use enshwick::game_events::{PlayerSpawnInfoChangeEvent, UniqueSceneId};
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on::<PlayerSpawnInfoChangeEvent>(|moved| {
///         if let Some(UniqueSceneId(scene)) = moved.scene_instance_id {
///             enshwick::info!("a save point in scene {scene:#x}");
///         }
///     })
/// }
/// ```
#[cfg(feature = "unstable")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
#[expect(
    clippy::exhaustive_structs,
    reason = "a newtype over one value, which never gains a field"
)]
pub struct UniqueSceneId(pub u32);

#[cfg(feature = "unstable")]
impl crate::events::Kind for PlayerSpawnInfoChangeEvent {
    type Data = Self;

    fn subscribe<F>(load: &mut BaseLoad, handler: F) -> Result<(), Off>
    where
        F: Fn(&Self) + Send + Sync + 'static,
    {
        use crate::content::Guid;

        let ty = load.game_event_type(ids::PLAYER_SPAWN_INFO_CHANGE_EVENT)?;
        let character: Field<Guid> =
            ty.field_by_id(load, ids::PLAYER_SPAWN_INFO_CHANGE_EVENT_CHARACTER)?;
        let base: Field<Guid> = ty.field_by_id(load, ids::PLAYER_SPAWN_INFO_CHANGE_EVENT_BASE)?;
        let scene: Field<u32> = ty.field_by_id(load, ids::PLAYER_SPAWN_INFO_CHANGE_EVENT_SCENE)?;
        on(
            load,
            ty,
            move |e| {
                Some(Self {
                    player_character_id: e.get(character)?,
                    base_id: Some(e.get(base)?).filter(|&id| id != Guid::NIL),
                    scene_instance_id: Some(e.get(scene)?).filter(|&id| id != 0).map(UniqueSceneId),
                })
            },
            handler,
        )
    }
}
