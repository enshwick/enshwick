//! The fake's spawn points and the respawn point each player's character
//! holds, as the loader reads and sets them.

use crate::test::table::base::args;
use crate::test::table::entries;
use enshwick_core::Entity;
use enshwick_core::abi::{Header, RespawnPointInfo, Status};
use enshwick_core::content::Guid;
#[cfg(feature = "unstable")]
use enshwick_core::game_events::UniqueSceneId;

use crate::test::FakeHost;
use crate::test::table::{self, with};

/// A spawn point in the fake's world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FakeSpawnPoint {
    pub(crate) entity: Entity,
    /// A `RespawnPointInfo` kind.
    pub(crate) kind: u32,
    pub(crate) position: [f64; 3],
    pub(crate) orientation: [f32; 4],
    /// The base it stands in, which a set's event names first.
    pub(crate) base: Option<Guid>,
    /// Its scene, which a set's event names without a base.
    pub(crate) scene: Option<u32>,
}

/// The GUID the fake gives the character `character` in a respawn point
/// event: its entity id, little-endian, in the first four bytes.
fn character_guid(character: Entity) -> Guid {
    let mut guid = [0; 16];
    guid[..4].copy_from_slice(&character.0.to_le_bytes());
    Guid(guid)
}

impl FakeHost {
    /// Places a spawn point `entity` of `kind`, a [`RespawnPointInfo`] kind,
    /// at `position` in metres, for `World::respawn_point` and
    /// `World::set_respawn_point`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::RespawnPointInfo;
    ///
    /// let _loader = enshwick::test::FakeHost::new()
    ///     .spawn_point(enshwick::Entity(40), RespawnPointInfo::BASE, [10.0, 2.0, 4.0])
    ///     .install();
    /// ```
    #[must_use]
    pub fn spawn_point(mut self, entity: Entity, kind: u32, position: [f64; 3]) -> Self {
        self.world.spawn_points.push(FakeSpawnPoint {
            entity,
            kind,
            position,
            orientation: [0.0, 0.0, 0.0, 1.0],
            base: None,
            scene: None,
        });
        self
    }

    /// Names the base and the scene of the spawn point `spawn_point` placed
    /// with [`spawn_point`](Self::spawn_point), so `World::set_respawn_point`
    /// on it raises `PlayerSpawnInfoChangeEvent` under the layout of
    /// [`FakeHost::game_events`], as the loader does: with the base, else
    /// the scene, and the character as a GUID holding its entity id,
    /// little-endian, in its first four bytes. A point that names neither
    /// raises no event, as in the game.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::RespawnPointInfo;
    /// use enshwick::game_events::UniqueSceneId;
    ///
    /// let _loader = enshwick::test::FakeHost::new()
    ///     .game_events()
    ///     .spawn_point(enshwick::Entity(40), RespawnPointInfo::SAVE_POINT, [0.0; 3])
    ///     .spawn_point_names(enshwick::Entity(40), None, Some(UniqueSceneId(7)))
    ///     .install();
    /// ```
    #[cfg(feature = "unstable")]
    #[must_use]
    pub fn spawn_point_names(
        mut self,
        spawn_point: Entity,
        base: Option<Guid>,
        scene: Option<UniqueSceneId>,
    ) -> Self {
        for point in self
            .world
            .spawn_points
            .iter_mut()
            .filter(|p| p.entity == spawn_point)
        {
            point.base = base;
            point.scene = scene.map(|UniqueSceneId(id)| id);
        }
        self
    }

    /// Makes spawn point `spawn_point` the respawn point of the character
    /// `character`, as the game does when the character stands in it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::RespawnPointInfo;
    ///
    /// let _loader = enshwick::test::FakeHost::new()
    ///     .spawn_point(enshwick::Entity(40), RespawnPointInfo::SAVE_POINT, [0.0; 3])
    ///     .respawns_at(enshwick::Entity(2), enshwick::Entity(40))
    ///     .install();
    /// ```
    #[must_use]
    pub fn respawns_at(mut self, character: Entity, spawn_point: Entity) -> Self {
        self.world.respawn_points.insert(character.0, spawn_point);
        self
    }
}

impl crate::test::Installed {
    /// Raises the game's respawn point change for the character
    /// `character`, naming a base or a save point's scene, as the game does
    /// when the character stands in a spawn point, under the layout of
    /// [`FakeHost::game_events`]; returns how many handlers ran.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::game_events::UniqueSceneId;
    ///
    /// let loader = enshwick::test::FakeHost::new().game_events().install();
    /// let character = enshwick::content::Guid([0xa1; 16]);
    /// assert_eq!(loader.respawn_point_moved(character, None, Some(UniqueSceneId(7))), 0);
    /// ```
    #[cfg(feature = "unstable")]
    #[must_use = "the count says whether the mod handles the event"]
    #[expect(
        clippy::unused_self,
        reason = "the receiver proves the fake is installed, as for every raise"
    )]
    pub fn respawn_point_moved(
        &self,
        character: Guid,
        base: Option<Guid>,
        scene: Option<UniqueSceneId>,
    ) -> usize {
        raise(character, base, scene.map(|UniqueSceneId(id)| id))
    }
}

/// Raises `PlayerSpawnInfoChangeEvent` under the fake's game event layout;
/// how many handlers ran, 0 without the layout.
fn raise(character: Guid, base: Option<Guid>, scene: Option<u32>) -> usize {
    use enshwick_core::__fake::ids;

    let layout = with(None, |s| {
        let host = &s.host;
        let at = |id: &str| usize::try_from(*host.base.constants.get(id)?).ok();
        Some((
            *host
                .base
                .type_hashes
                .get(ids::PLAYER_SPAWN_INFO_CHANGE_EVENT)?,
            usize::try_from(
                *host
                    .base
                    .type_sizes
                    .get(ids::PLAYER_SPAWN_INFO_CHANGE_EVENT)?,
            )
            .ok()?,
            at(ids::PLAYER_SPAWN_INFO_CHANGE_EVENT_CHARACTER)?,
            at(ids::PLAYER_SPAWN_INFO_CHANGE_EVENT_BASE)?,
            at(ids::PLAYER_SPAWN_INFO_CHANGE_EVENT_SCENE)?,
        ))
    });
    let Some((hash, size, character_at, base_at, scene_at)) = layout else {
        return 0;
    };
    let mut bytes = vec![0u8; size];
    bytes[character_at..character_at + 16].copy_from_slice(&character.0);
    if let Some(base) = base {
        bytes[base_at..base_at + 16].copy_from_slice(&base.0);
    } else if let Some(scene) = scene {
        bytes[scene_at..scene_at + 4].copy_from_slice(&scene.to_le_bytes());
    }
    crate::test::base::events::raise_server(hash, &bytes)
}

/// Whether the fake reads no respawn point: a client that does not host,
/// or no world running. A read needs no entity commands, as in the loader.
fn unread(s: &table::State) -> bool {
    !(s.host.base.serving()) || table::world::no_world(s)
}

/// Whether the fake sets no respawn point: as [`unread`], or without the
/// entity commands a set needs.
fn off(s: &table::State) -> bool {
    table::world::entity_calls_off(s)
}

entries! {
    pub(crate) fn respawn_point(
        me as Caller,
        character: Entity,
        out: *mut RespawnPointInfo as args::HeadedOut,
    ) -> Status {
        if !table::on_game_thread() {
            return Status::WRONG_THREAD;
        }
        with(Status::UNAVAILABLE, |s| {
            if unread(s) {
                return Status::UNAVAILABLE;
            }
            if !s.host.base.players.iter().any(|p| p.entity == character) {
                return Status::MISSING;
            }
            let point = s
                .host
                .world
                .respawn_points
                .get(&character.0)
                .and_then(|at| s.host.world.spawn_points.iter().find(|p| p.entity == *at));
            let Some(point) = point else {
                return Status::MISSING;
            };
            table::base::io::put_headed(
                out,
                RespawnPointInfo {
                    header: Header::of::<RespawnPointInfo>(),
                    entity: point.entity,
                    kind: point.kind,
                    position: point.position,
                    orientation: point.orientation,
                },
            )
        })
    }

    pub(crate) fn set_respawn_point(
        me as Caller,
        character: Entity,
        spawn_point: Entity,
    ) -> Status {
        if !table::on_game_thread() {
            return Status::WRONG_THREAD;
        }
        let set = with(Err(Status::UNAVAILABLE), |s| {
            if off(s) {
                return Err(Status::UNAVAILABLE);
            }
            let point = s
                .host
                .world
                .spawn_points
                .iter()
                .find(|p| p.entity == spawn_point && p.kind != RespawnPointInfo::NO_RESPAWN)
                .copied()
                .ok_or(Status::BAD_ARGS)?;
            if !s.host.base.players.iter().any(|p| p.entity == character) {
                return Err(Status::MISSING);
            }
            s.host.world.respawn_points.insert(character.0, spawn_point);
            Ok(point)
        });
        match set {
            Ok(point) => {
                if point.base.is_some() || point.scene.is_some() {
                    let _ran = raise(character_guid(character), point.base, point.scene);
                }
                Status::OK
            }
            Err(status) => status,
        }
    }
}
