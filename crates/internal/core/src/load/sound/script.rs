//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::content::Guid;
use crate::sound::SoundChange;
use crate::{Load, Off};
use enshwick_ui::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(sound)]
impl Load<'_> {
    /// Changes the sound container with GUID `sound`, as
    /// [`Load::change_sound`] does.
    ///
    /// # Errors
    ///
    /// As [`Load::change_sound`].
    #[script(name = "change")]
    pub(crate) fn change_sound_script(
        &mut self,
        sound: Guid,
        change: SoundChange,
    ) -> Result<(), Off>;
}
