//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::sound::{NewSound, SoundContainerId};
use crate::{Load, Off};
use enshwick_ui::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(sound_add)]
impl Load<'_> {
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// Adds the sound in `new.file` and a container that plays it, under
    /// `new.id`, and returns the container, which
    /// [`Frame::play`](crate::ui::Frame::play), [`Load::change_sound`] and
    /// other mods use. The client holds it as the game holds its own
    /// uncompressed sounds, built from one of them, streamed from memory;
    /// nothing is written to the archive or a save. A server checks the
    /// file and serves nothing. Wrap the call in [`Load::feature`] so a bad
    /// file turns off that sound alone, with the reason on the Mods screen.
    ///
    /// # Errors
    ///
    /// [`Off`] for a mod not of side both, a path outside the mod's folder,
    /// a file past [`MAX_FILE_BYTES`](crate::sound::MAX_FILE_BYTES) or past the mod's [`MAX_MOD_BYTES`](crate::sound::MAX_MOD_BYTES),
    /// one that is not 16-bit PCM WAV at [`SAMPLE_RATE`](crate::sound::SAMPLE_RATE), a container value
    /// outside its bound, an id another
    /// content id holds, after init, and on a client whose game holds no
    /// streamed uncompressed sound to build from.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::id;
    /// use enshwick::sound::NewSound;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let horn = load.feature("horn", |load| load.add_sound(NewSound::new(id!("horn"), "sounds/horn.wav")));
    ///     load.on_frame(move |frame| {
    ///         if let Some(horn) = horn
    ///             && frame.button("Horn")
    ///         {
    ///             let _played = frame.play(horn);
    ///         }
    ///     })
    /// }
    /// ```
    #[script(name = "add")]
    pub fn add_sound(&mut self, new: NewSound) -> Result<SoundContainerId, Off>;
}
