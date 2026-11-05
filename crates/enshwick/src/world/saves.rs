//! The world's save: what is on disk, and holding saves while a mod needs
//! the world unsaved.

use core::time::Duration;

use super::World;
use crate::WorldError;
use crate::abi::{Header, SaveInfo as RawSaveInfo, Status};

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
    /// saves only while no running mod holds them, so its timed saves and
    /// [`World::save`] wait meanwhile, for example while players are outside
    /// a safe room. Each mod holds its own; a mod that turns off lets go.
    /// The log names each mod that holds or lets go. Takes effect after the
    /// next step; any thread.
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
}
