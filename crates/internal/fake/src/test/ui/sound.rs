//! The sounds a mod started, as a test reads them.

#[cfg(feature = "unstable")]
use enshwick_core::abi::{SoundInfo, SoundPlay};
#[cfg(feature = "unstable")]
use enshwick_core::content::Guid;
#[cfg(feature = "unstable")]
use enshwick_core::sound::Place;

#[cfg(feature = "unstable")]
use super::super::Installed;
use super::super::table;

/// One sound the mod started, as the fake loader keeps it.
#[cfg(feature = "unstable")]
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

#[cfg(feature = "unstable")]
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

#[cfg(feature = "unstable")]
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

    /// Every sound the mod's server part sent clients with
    /// `sound::play_for`, in order: the players whose client got it, and
    /// the sound as they play it, changed by the mod's steers since.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.sent_sounds().is_empty());
    /// ```
    #[must_use]
    pub fn sent_sounds(&self) -> Vec<(Vec<enshwick_core::PlayerKey>, FakeSound)> {
        table::with(Vec::new(), |s| {
            s.ui.sound
                .sent
                .iter()
                .map(|sent| {
                    (
                        sent.to.clone(),
                        FakeSound::of(&sent.kept.play, sent.kept.info.state),
                    )
                })
                .collect()
        })
    }

    /// Takes `play` as this client gets a sound the mod's server part sent
    /// it: queued as the mod's own start, which [`Installed::sounds`] then
    /// lists, or dropped, as the loader drops it, for a container the client
    /// lacks, a play out of bounds or a full frame. Whether it queued.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{Side, SoundPlay};
    /// use enshwick::content::Guid;
    ///
    /// let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).install();
    /// let unknown = enshwick::abi::headed!(SoundPlay { container: Guid::named("you.bells:none"), ..SoundPlay::EMPTY });
    /// assert!(!loader.hear_sound(unknown));
    /// assert!(loader.sounds().is_empty());
    /// ```
    #[must_use = "the answer says whether the client plays it"]
    pub fn hear_sound(&self, play: SoundPlay) -> bool {
        table::hear(play)
    }
}
