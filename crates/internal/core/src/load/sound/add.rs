//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::sound::{AddedSound, NewSound, SoundId};
use crate::{Load, Off};
use enshwick_ui::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(sound_add)]
impl Load<'_> {
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// Adds a recording for each of `new.takes` and a container that picks
    /// among them, under `new.id`, and returns both: the container, which
    /// [`Frame::play`](crate::ui::Frame::play), [`Load::change_sound`] and
    /// other mods use, and the recordings, which other containers' takes
    /// name. The client holds each as the game holds its own `FramelessQOA`
    /// sounds at [`SAMPLE_RATE`](crate::sound::SAMPLE_RATE), inline up to
    /// [`INLINE_FRAMES`](crate::sound::INLINE_FRAMES) and streamed past it;
    /// nothing is written to the archive or a save. A server checks the
    /// files and serves nothing. Wrap the call in [`Load::feature`] so a bad
    /// file turns off that sound alone, with the reason on the Mods screen.
    ///
    /// # Errors
    ///
    /// [`Off`] for a mod not of side both, no takes or more than
    /// [`MAX_NEW_TAKES`](crate::sound::MAX_NEW_TAKES), a take's chance
    /// outside 0 to 1, a path outside the mod's folder, a file past
    /// [`MAX_FILE_BYTES`](crate::sound::MAX_FILE_BYTES), longer than
    /// [`MAX_TAKE`](crate::sound::MAX_TAKE) or past the mod's
    /// [`MAX_MOD_BYTES`](crate::sound::MAX_MOD_BYTES), one that is not WAV,
    /// FLAC, Ogg Vorbis or MP3, a gain outside
    /// [`GAIN_DB`](crate::sound::GAIN_DB), a container value outside its
    /// bound, takes that replace the recordings, an added take whose
    /// recording is neither the game's nor a mod's loaded before, an id
    /// another content id holds, after init, and on a client whose game
    /// holds no `FramelessQOA` sounds to build from.
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
    ///         if let Some(horn) = &horn
    ///             && frame.button("Horn")
    ///         {
    ///             let _played = frame.play(horn.container);
    ///         }
    ///     })
    /// }
    /// ```
    #[script(name = "add")]
    pub fn add_sound(&mut self, new: NewSound) -> Result<AddedSound, Off>;

    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// Replaces the game's recording `sound` with the mod's `file`, read as
    /// [`Load::add_sound`] reads one, wherever any container plays it, on the
    /// client as it reads the game's archive; nothing is written to the
    /// archive or a save. Mods that replace one recording apply in load
    /// order: the last one's file plays, and the log names the one it
    /// replaced. A server checks the file and serves nothing.
    ///
    /// # Errors
    ///
    /// As [`Load::add_sound`] for the file, and on a client whose game
    /// holds no recording `sound`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::Guid;
    /// use enshwick::sound::SoundId;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let creak = SoundId::from_guid(Guid::parse("4f1d6a3e-90b2-4c7e-8d15-2a6f0c93b7e1"));
    ///     let _replaced = load.feature("creak", |load| load.replace_sound(creak, "sounds/creak.wav"));
    ///     Ok(())
    /// }
    /// ```
    #[script(name = "replace")]
    pub fn replace_sound(&mut self, sound: SoundId, file: &str) -> Result<(), Off>;
}
