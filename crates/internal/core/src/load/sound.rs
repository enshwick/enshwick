//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::sound::{SoundChange, SoundContainerId};
use crate::{Load, Off};
use enshwick_ui::__load::*;

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// Changes the values of sound container `sound` where `change` gives
    /// them, as the client reads it from the game's archive, after the
    /// changes of mods loaded before; nothing is written to the archive or
    /// a save. On a server, which plays no sound, it checks `change` and
    /// does nothing more.
    ///
    /// # Errors
    ///
    /// [`Off`] for a value outside its bound, a least distance past the
    /// greatest, after init, and on a client whose game lacks the type.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::Guid;
    /// use enshwick::sound::{SoundChange, SoundContainerId};
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let chest = SoundContainerId::from_guid(Guid::named("you.bells:bell"));
    ///     load.change_sound(chest, SoundChange { volume: Some(0.2), ..SoundChange::default() })
    /// }
    /// ```
    pub fn change_sound(&mut self, sound: SoundContainerId, change: SoundChange)
    -> Result<(), Off>;
}

#[cfg(feature = "unstable")]
mod add;
mod script;
