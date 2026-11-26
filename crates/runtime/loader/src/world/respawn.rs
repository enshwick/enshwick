//! A player's respawn point: the spawn point entity the character's
//! `SelectedTargets` holds for `TargetType::SpawnPoint`, read and set as the
//! game's `player_update_important_locations` sets it.
//!
//! The game sets the target's entity when the character stands in a spawn
//! point and raises `PlayerSpawnInfoChangeEvent` with the point's base or
//! scene, which the server's player record and the save follow (server
//! 26430253, `sys.player_update_important_locations`). A set here writes the
//! same entity and raises the same event. The target's transform is
//! written by the game only when it falls back to the world's start
//! (`sys.update_player_spawn`), so where a character respawns is read from
//! the spawn point's own `CurrentTransform`.

use enshwick_core::abi::RespawnPointInfo;

/// The ABI kind of each `PlayerSpawnType`, in the order the layout reads
/// the game's values.
const KINDS: [u32; 4] = [
    RespawnPointInfo::START_LOCATION,
    RespawnPointInfo::SAVE_POINT,
    RespawnPointInfo::BASE,
    RespawnPointInfo::NO_RESPAWN,
];

/// The GUID the game leaves in a base field that names no base.
const NO_BASE: [u8; 16] = [0; 16];

/// The ABI kind of the game's `PlayerSpawnType` value `raw`, given the
/// game's values in [`KINDS`] order; [`RespawnPointInfo::OTHER`] for a value
/// none of them is.
fn kind(raw: u64, values: &[Option<u64>; 4]) -> u32 {
    values
        .iter()
        .position(|&value| value == Some(raw))
        .and_then(|at| KINDS.get(at).copied())
        .unwrap_or(RespawnPointInfo::OTHER)
}

/// Where in `SelectedTargets`, of `size` bytes, the spawn point target's
/// `u32` entity sits: target `spawn` of the `TargetInfo` array at `targets`,
/// each `info` bytes holding its entity at `entity`. `None` when the sum
/// overflows or the entity would end past the struct, so a changed build
/// never yields a write outside it.
fn target_entity(
    targets: usize,
    spawn: usize,
    info: usize,
    entity: usize,
    size: usize,
) -> Option<usize> {
    let at = spawn
        .checked_mul(info)?
        .checked_add(targets)?
        .checked_add(entity)?;
    (entity.checked_add(size_of::<u32>())? <= info && at.checked_add(size_of::<u32>())? <= size)
        .then_some(at)
}

/// Where `PlayerSpawnInfoChangeEvent` holds its fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EventLayout {
    hash: u32,
    size: usize,
    character: usize,
    base: usize,
    scene: usize,
}

impl EventLayout {
    /// Whether every field lies inside the event, so [`event_bytes`] fails
    /// only for a point that names no base or scene.
    fn fits(&self) -> bool {
        [(self.character, 16), (self.base, 16), (self.scene, 4)]
            .into_iter()
            .all(|(at, len)| at.checked_add(len).is_some_and(|end| end <= self.size))
    }
}

/// The event's bytes for `character`, naming the spawn point's `base`
/// GUID or, without a base, its `scene` id, as the game raises it; the
/// loader stamps the time. `None` when the point names neither, since the
/// game raises no event then, or when a field lies past the event, which
/// [`EventLayout::fits`] rules out first.
fn event_bytes(
    layout: &EventLayout,
    character: [u8; 16],
    base: Option<[u8; 16]>,
    scene: Option<u32>,
) -> Option<Vec<u8>> {
    let base = base.filter(|&id| id != NO_BASE);
    let scene = scene.filter(|&id| id != 0);
    if base.is_none() && scene.is_none() {
        return None;
    }
    let mut bytes = vec![0u8; layout.size];
    bytes
        .get_mut(layout.character..layout.character.checked_add(16)?)?
        .copy_from_slice(&character);
    let base_field = bytes.get_mut(layout.base..layout.base.checked_add(16)?)?;
    if let Some(base) = base {
        base_field.copy_from_slice(&base);
    }
    let scene_field = bytes.get_mut(layout.scene..layout.scene.checked_add(4)?)?;
    if base.is_none()
        && let Some(scene) = scene
    {
        scene_field.copy_from_slice(&scene.to_le_bytes());
    }
    Some(bytes)
}

#[cfg(windows)]
pub(crate) use game::{RespawnError, respawn_point, set_respawn_point};

#[cfg(windows)]
mod game {
    use std::sync::OnceLock;

    use enshwick_core::Entity;
    use enshwick_core::abi::RespawnPointInfo;

    use super::{EventLayout, event_bytes, kind, target_entity};
    use crate::base::emits::EmitError;
    use crate::engine::catalogue::{self, component_hash, ids, offset, size};
    use crate::engine::loader::Loader;
    use crate::engine::world::{WorldError, character_of, field, player_entities, pose};
    use crate::world::writes::{WriteError, component_mut};

    /// The respawn point's layout, from the catalogue.
    #[derive(Debug)]
    struct Layout {
        /// `SelectedTargets`' hash.
        targets: u32,
        /// Where `SelectedTargets` holds the spawn point target's entity.
        target_entity: usize,
        /// `PlayerSpawnPoint`'s hash and where it holds its
        /// `PlayerSpawnType`.
        spawn_point: (u32, usize),
        /// The game's `PlayerSpawnType` values, in the order of `KINDS`.
        kinds: [Option<u64>; 4],
        /// What a set needs beyond a read, when this build has it all.
        set: Option<SetLayout>,
    }

    /// What a set reads from the spawn point and raises.
    #[derive(Debug)]
    struct SetLayout {
        /// `BaseIdComponent`'s hash and where it holds the base's GUID.
        base: (u32, usize),
        /// `StoredSceneId`'s hash and where it holds the scene's id.
        scene: (u32, usize),
        event: EventLayout,
    }

    static LAYOUT: OnceLock<Option<Layout>> = OnceLock::new();

    fn layout(loader: &Loader) -> Option<&'static Layout> {
        LAYOUT
            .get_or_init(|| {
                let at = |id| offset(loader, id);
                let spawn = usize::try_from(catalogue::constant(
                    ids::TARGET_TYPE_SPAWN_POINT,
                    &loader.reflection,
                )?)
                .ok()?;
                let value = |id| catalogue::constant(id, &loader.reflection);
                let set = (|| {
                    Some(SetLayout {
                        base: (
                            component_hash(loader, ids::BASE_ID_COMPONENT)?,
                            at(ids::BASE_ID_COMPONENT_BASE_ID)?,
                        ),
                        scene: (
                            component_hash(loader, ids::STORED_SCENE_ID)?,
                            at(ids::STORED_SCENE_ID_UNIQUE_ID)?,
                        ),
                        event: EventLayout {
                            hash: component_hash(loader, ids::PLAYER_SPAWN_INFO_CHANGE_EVENT)?,
                            size: size(loader, ids::PLAYER_SPAWN_INFO_CHANGE_EVENT)?,
                            character: at(ids::PLAYER_SPAWN_INFO_CHANGE_EVENT_CHARACTER)?,
                            base: at(ids::PLAYER_SPAWN_INFO_CHANGE_EVENT_BASE)?,
                            scene: at(ids::PLAYER_SPAWN_INFO_CHANGE_EVENT_SCENE)?,
                        },
                    })
                })()
                .filter(|set| set.event.fits());
                Some(Layout {
                    targets: component_hash(loader, ids::SELECTED_TARGETS)?,
                    target_entity: target_entity(
                        at(ids::SELECTED_TARGETS_TARGETS)?,
                        spawn,
                        size(loader, ids::TARGET_INFO)?,
                        at(ids::TARGET_INFO_ENTITY)?,
                        size(loader, ids::SELECTED_TARGETS)?,
                    )?,
                    spawn_point: (
                        component_hash(loader, ids::PLAYER_SPAWN_POINT)?,
                        at(ids::PLAYER_SPAWN_POINT_TARGET)?,
                    ),
                    kinds: [
                        value(ids::PLAYER_SPAWN_TYPE_START_LOCATION),
                        value(ids::PLAYER_SPAWN_TYPE_SAVE_POINT),
                        value(ids::PLAYER_SPAWN_TYPE_BASE),
                        value(ids::PLAYER_SPAWN_TYPE_NO_RESPAWN),
                    ],
                    set,
                })
            })
            .as_ref()
    }

    /// Why a respawn point was not read or set.
    #[derive(Debug, thiserror::Error)]
    pub(crate) enum RespawnError {
        /// The build lacks the respawn point's layout.
        #[error("this build lacks the respawn point's layout")]
        Unread,
        /// The entity is no player's character.
        #[error("not a player's character")]
        NotPlayer,
        /// The character's respawn point is not in the world.
        #[error("no respawn point in the world")]
        NoPoint,
        /// The entity is no spawn point a player respawns at.
        #[error("not a spawn point players respawn at")]
        NotASpawnPoint,
        /// The server's world, unreadable now.
        #[error(transparent)]
        World(#[from] WorldError),
        /// The character's targets were not written.
        #[error(transparent)]
        Write(#[from] WriteError),
        /// The event was not raised.
        #[error(transparent)]
        Emit(#[from] EmitError),
    }

    /// `entity`'s `PlayerSpawnType` as an ABI kind, when it is a spawn
    /// point.
    fn kind_of(loader: &Loader, layout: &Layout, entity: Entity) -> Option<u32> {
        let (hash, at) = layout.spawn_point;
        let raw: u8 = field(loader, entity, hash, at).ok()?;
        Some(kind(u64::from(raw), &layout.kinds))
    }

    /// Fails with [`RespawnError::NotPlayer`] unless `character` is a
    /// player's character in game.
    fn player(loader: &Loader, character: Entity) -> Result<(), RespawnError> {
        if player_entities(loader)?.contains(&character.get()) {
            Ok(())
        } else {
            Err(RespawnError::NotPlayer)
        }
    }

    /// The respawn point of `character`'s player: the spawn point, its kind
    /// and where it stands. On the server's game thread.
    pub(crate) fn respawn_point(
        loader: &Loader,
        character: Entity,
    ) -> Result<RespawnPointInfo, RespawnError> {
        let layout = layout(loader).ok_or(RespawnError::Unread)?;
        player(loader, character)?;
        let point = Entity::from_engine(field(
            loader,
            character,
            layout.targets,
            layout.target_entity,
        )?);
        let kind = kind_of(loader, layout, point).ok_or(RespawnError::NoPoint)?;
        let (fixed, orientation) = pose(loader, point).ok_or(RespawnError::NoPoint)?;
        Ok(enshwick_abi::headed!(RespawnPointInfo {
            entity: point,
            kind,
            position: enshwick_core::ecs::metres(fixed),
            orientation,
        }))
    }

    /// Makes `spawn_point` the respawn point of `character`'s player and
    /// raises the game's `PlayerSpawnInfoChangeEvent` for it when the point
    /// names a base or scene, as the game does when the character stands in
    /// it. On the server's game thread between steps. Every step that can
    /// fail runs before the write, so a refused call changes nothing.
    pub(crate) fn set_respawn_point(
        loader: &Loader,
        character: Entity,
        spawn_point: Entity,
    ) -> Result<(), RespawnError> {
        let layout = layout(loader).ok_or(RespawnError::Unread)?;
        let set = layout.set.as_ref().ok_or(RespawnError::Unread)?;
        crate::engine::world::entity_call(loader)?;
        match kind_of(loader, layout, spawn_point) {
            None | Some(RespawnPointInfo::NO_RESPAWN) => {
                return Err(RespawnError::NotASpawnPoint);
            }
            Some(_) => {}
        }
        player(loader, character)?;
        // `player` found the character in game, so no identity means this
        // build lacks the player slot's character.
        let identity = character_of(loader, character).ok_or(RespawnError::Unread)?;
        let (hash, at) = set.base;
        let base = field::<[u8; 16]>(loader, spawn_point, hash, at).ok();
        let (hash, at) = set.scene;
        let scene = field::<u32>(loader, spawn_point, hash, at).ok();
        let event = event_bytes(&set.event, identity.0, base, scene);
        let targets = component_mut(loader, character.get(), layout.targets)?;
        if let Some(bytes) = event {
            crate::base::emits::push(loader, set.event.hash, &bytes)?;
        }
        // SAFETY: `targets` is the non-null data of the character's live
        // `SelectedTargets`, handed out writable and marked changed by
        // `component_mut` on the game thread between steps, where no system
        // runs; the spawn point target's `u32` entity sits at
        // `target_entity`, inside its size (checked by `target_entity`).
        unsafe {
            enshwick_core::raw::write_at(targets.cast(), layout.target_entity, spawn_point.get());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALUES: [Option<u64>; 4] = [Some(0), Some(1), Some(2), Some(3)];
    const LAYOUT: EventLayout = EventLayout {
        hash: 0x1556_9f57,
        size: 48,
        character: 8,
        base: 24,
        scene: 40,
    };

    #[test]
    fn a_spawn_type_reads_as_its_abi_kind_and_an_unknown_one_as_other() {
        assert_eq!(kind(2, &VALUES), RespawnPointInfo::BASE);
        assert_eq!(kind(3, &VALUES), RespawnPointInfo::NO_RESPAWN);
        assert_eq!(kind(9, &VALUES), RespawnPointInfo::OTHER);
        let renumbered = [Some(5), None, Some(0), Some(1)];
        assert_eq!(kind(0, &renumbered), RespawnPointInfo::BASE);
        assert_eq!(kind(5, &renumbered), RespawnPointInfo::START_LOCATION);
    }

    #[test]
    fn the_spawn_target_s_entity_sits_inside_its_info_and_the_struct() {
        assert_eq!(
            target_entity(16, 3, 72, 0x2c, 400),
            Some(16 + 3 * 72 + 0x2c)
        );
    }

    #[test]
    fn a_changed_layout_that_overflows_or_ends_past_the_struct_gives_no_offset() {
        assert_eq!(target_entity(16, usize::MAX / 2, 72, 0, usize::MAX), None);
        assert_eq!(target_entity(usize::MAX - 2, 0, 72, 0, usize::MAX), None);
        assert_eq!(target_entity(16, 3, 72, 0x2c, 16 + 3 * 72 + 0x2c + 3), None);
        assert_eq!(target_entity(16, 3, 8, 6, 4096), None, "past its own info");
    }

    #[test]
    fn the_event_names_the_base_and_no_scene_when_the_point_has_a_base() {
        let bytes = event_bytes(&LAYOUT, [0xa1; 16], Some([0xb2; 16]), Some(7)).unwrap();
        assert_eq!(bytes.len(), 48);
        assert_eq!(bytes[8..24], [0xa1; 16]);
        assert_eq!(bytes[24..40], [0xb2; 16]);
        assert_eq!(bytes[40..44], [0; 4]);
        assert_eq!(bytes[..8], [0; 8], "the loader stamps the time");
    }

    #[test]
    fn the_event_names_the_scene_and_no_base_for_a_save_point() {
        let bytes = event_bytes(&LAYOUT, [0xa1; 16], None, Some(0x0102_0304)).unwrap();
        assert_eq!(bytes[24..40], [0; 16]);
        assert_eq!(bytes[40..44], 0x0102_0304u32.to_le_bytes());
    }

    #[test]
    fn a_nil_base_gives_way_to_the_scene() {
        let bytes = event_bytes(&LAYOUT, [0xa1; 16], Some(NO_BASE), Some(9)).unwrap();
        assert_eq!(bytes[40..44], 9u32.to_le_bytes());
    }

    #[test]
    fn a_point_that_names_no_base_or_scene_raises_no_event() {
        assert_eq!(event_bytes(&LAYOUT, [1; 16], None, None), None);
        assert_eq!(event_bytes(&LAYOUT, [1; 16], Some(NO_BASE), Some(0)), None);
    }

    #[test]
    fn an_event_layout_past_its_size_does_not_fit_and_gives_no_bytes() {
        assert!(LAYOUT.fits());
        let short = EventLayout { size: 42, ..LAYOUT };
        assert!(!short.fits());
        assert_eq!(event_bytes(&short, [1; 16], None, Some(1)), None);
        assert!(
            !EventLayout {
                scene: usize::MAX,
                ..LAYOUT
            }
            .fits()
        );
    }
}
