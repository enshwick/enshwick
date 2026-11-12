//! `Load`'s script-only method of this module, out of a mod's reach.

use super::SoundContainerId;
use crate::__load::*;
use crate::Off;
use crate::content::Guid;

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::change_sound_script`.
    #[doc(hidden)]
    fn change_sound_script(&mut self, sound: Guid, change: super::SoundChange) -> Result<(), Off>;
}

impl LoadMethods for crate::BaseLoad {
    fn change_sound_script(&mut self, sound: Guid, change: super::SoundChange) -> Result<(), Off> {
        self.change_sound(SoundContainerId::from_guid(sound), change)
    }
}
