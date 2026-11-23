//! The sounds a mod started, as a test reads them.

use enshwick_core::abi::{SoundInfo, SoundPlay};
use enshwick_core::content::Guid;
use enshwick_core::sound::Place;

use super::super::{Installed, table};

/// One sound the mod started, as the fake loader keeps it.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct FakeSound {
    /// The container's GUID.
    pub container: Guid,
    /// Where it plays now: a moved sound plays at its new place.
    pub place: Place,
    /// Its volume now.
    pub volume: f32,
    /// Whether it is queued or playing.
    pub playing: bool,
}

impl FakeSound {
    /// The sound the loader keeps as `play` in state `state`.
    fn of(play: &SoundPlay, state: u32) -> Self {
        let place = match play.place {
            SoundPlay::AT => Place::At(play.at),
            SoundPlay::ON => Place::On(play.entity),
            SoundPlay::MUSIC => Place::Music,
            _ => Place::Screen,
        };
        Self {
            container: play.container,
            place,
            volume: play.volume,
            playing: matches!(state, SoundInfo::PENDING | SoundInfo::PLAYING),
        }
    }
}

/// Starts the waiting sounds, as the loader's next frame does.
pub(super) fn frame() {
    table::with((), |s| s.ui.sound.frame());
}

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// Every sound the mod started, in order; the next
    /// [`Installed::draw_frame`] starts the queued ones.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::Side;
    ///
    /// let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).install();
    /// assert!(loader.sounds().is_empty());
    /// ```
    #[must_use]
    pub fn sounds(&self) -> Vec<FakeSound> {
        table::with(Vec::new(), |s| {
            s.ui.sound
                .sounds
                .iter()
                .map(|kept| FakeSound::of(&kept.play, kept.info.state))
                .collect()
        })
    }
}
