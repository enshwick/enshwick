//! The world's save: what is on disk, holding saves while a mod needs the
//! world unsaved, and keeping a mod's entities out of it.

use core::time::Duration;

use super::World;
use crate::abi::{Header, SaveInfo as RawSaveInfo, Status};
use crate::{Entity, WorldError};

/// A world's GUID, which its save keeps across saves; a copied save keeps
/// it too.
///
/// # Examples
///
/// See [`SaveInfo`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, crate::Script)]
pub struct WorldGuid(pub [u8; 16]);

/// The save of the world the server runs, from [`World::save_info`].
///
/// # Examples
///
/// ```
/// fn last_save(world: &enshwick::world::World) {
///     if let Some(info) = world.save_info() {
///         enshwick::info!("saved {} s after the epoch, {} bytes", info.saved_at.as_secs(), info.size);
///     }
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct SaveInfo {
    /// The world's GUID.
    pub world: WorldGuid,
    /// When the save was written, as time since the Unix epoch.
    pub saved_at: Duration,
    /// The save file's size in bytes.
    pub size: u64,
    /// The backups the game keeps beside it, 0 to 10.
    pub backups: u8,
}

#[crate::script(saves)]
impl World {
    /// The world's save as it is on disk; `None` before the first save of
    /// a new world, while no world runs, or when the file does not read.
    /// Any thread.
    ///
    /// # Examples
    ///
    /// See [`SaveInfo`].
    #[must_use]
    #[script]
    pub fn save_info(self) -> Option<SaveInfo> {
        let mut out = RawSaveInfo {
            header: Header::of::<RawSaveInfo>(),
            world: [0; 16],
            saved_at: 0,
            size: 0,
            backups: 0,
            reserved: 0,
        };
        let status = (self.0.host.api().save_info)(self.0.host.handle(), &raw mut out);
        (status == Status::OK).then(|| SaveInfo {
            world: WorldGuid(out.world),
            saved_at: Duration::from_secs(out.saved_at),
            size: out.size,
            backups: u8::try_from(out.backups).unwrap_or(u8::MAX),
        })
    }

    /// Holds the world's saves while `held`, or lets them go: the game
    /// saves only while no running mod holds them, so its timed saves wait
    /// and [`World::save`] is refused meanwhile, for example while players
    /// are outside a safe room. Each mod holds its own; a mod that turns off lets go,
    /// and every hold ends with its hosted game.
    /// As a dedicated server or a hosted game shuts down, every hold lets go,
    /// so the final save runs; a mod that holds saves again inside its
    /// [`ShutdownStarted`](crate::events::ShutdownStarted) handler refuses
    /// that save, and a hold asked for after that handler is not taken. The
    /// log names each mod that holds or lets go; once saves stay held for 10
    /// minutes, the log and a notice to each administrator online warn,
    /// again every 10 minutes while they stay held. Takes effect after the
    /// next step, and at once inside the
    /// [`ShutdownStarted`](crate::events::ShutdownStarted) handler; any
    /// thread.
    ///
    /// # Errors
    ///
    /// [`WorldError::Missing`] on a build without the save layout or on a
    /// client.
    ///
    /// # Examples
    ///
    /// ```
    /// fn guard(world: &enshwick::world::World, in_danger: bool) {
    ///     if let Err(why) = world.hold_saves(in_danger) {
    ///         enshwick::warn!("saves not held: {why}");
    ///     }
    /// }
    /// ```
    #[script]
    pub fn hold_saves(self, held: bool) -> Result<(), WorldError> {
        WorldError::from_status((self.0.host.api().hold_saves)(
            self.0.host.handle(),
            u8::from(held),
        ))
    }

    /// Keeps an entity of the mod's out of the world's saves, or puts it back
    /// in them.
    ///
    /// With `saved` false the game's own `DoNotSave` component leaves it out:
    /// it is gone after the server restarts, while the world goes on. Only
    /// entities a mod spawned ([`Effects::spawn`](crate::effects::Effects::spawn))
    /// or placed (a transaction's props) since the world loaded are allowed,
    /// from the step after their spawn, so no mod drops the game's own data
    /// from a save; once the world loads again, they count as the game's.
    /// The change shows from the next step and counts from the next save
    /// that starts after it. For a new entity,
    /// [`Spawn::do_not_save`](crate::effects::Spawn::do_not_save) keeps it
    /// out from its first step. On the server's game thread only.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] for an entity no mod spawned, such as a
    /// vanilla one or one spawned before the world last loaded,
    /// [`WorldError::Gone`] for an entity not in the world yet or any more,
    /// [`WorldError::Missing`] where no server runs, as on a client in
    /// another's game, or on a build without the commands, and
    /// [`WorldError::WrongThread`] off the game thread.
    ///
    /// # Examples
    ///
    /// ```
    /// fn forget(world: &enshwick::world::World, decoy: enshwick::Entity) {
    ///     if let Err(why) = world.set_saved(decoy, false) {
    ///         enshwick::warn!("decoy {decoy} stays in the save: {why}");
    ///     }
    /// }
    /// ```
    #[script]
    pub fn set_saved(self, entity: Entity, saved: bool) -> Result<(), WorldError> {
        WorldError::check(
            (self.0.host.api().set_saved)(self.0.host.handle(), entity, u8::from(saved)),
            WorldError::Gone,
        )
    }
}
