//! Sounds a client mod plays through the game's sound system, in 2D, at a
//! place, on an entity or as music, and steers through a handle.

use core::time::Duration;

use super::SoundContainerId;
use crate::abi::{SoundInfo, SoundPlay, SoundSteer, Status};
use crate::{Entity, Host, Off};

/// Where a sound plays.
///
/// # Examples
///
/// See [`Play`].
#[derive(Clone, Copy, Debug, Default, PartialEq, crate::Script)]
#[non_exhaustive]
pub enum Place {
    /// From no place, as the interface's own sounds: the same in both ears
    /// wherever the camera looks.
    #[default]
    Screen,
    /// At a place in the world, in metres; it pans and fades with the
    /// camera as the game's own world sounds do.
    At([f64; 3]),
    /// On an entity of the client's world, following it each client step;
    /// it ends with a short fade once the entity is gone.
    On(Entity),
    /// As music. A new music sound of the same mod fades out its earlier
    /// one, as the game's music does.
    Music,
}

/// A sound to start: which container, where, how loud and how it fades in.
///
/// # Examples
///
/// ```
/// use core::time::Duration;
/// use enshwick::content::Guid;
/// use enshwick::sound::{Place, Play, SoundContainerId};
///
/// let bell = SoundContainerId::from_guid(Guid::named("you.bells:bell"));
/// let play = Play::new(bell, Place::At([10.0, 0.0, 0.0]))
///     .volume(0.5)
///     .fade_in(Duration::from_millis(300));
/// assert_eq!(play.volume, Some(0.5));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, crate::Script)]
#[non_exhaustive]
pub struct Play {
    /// The container, the game's or a mod's.
    pub container: SoundContainerId,
    /// Where it plays.
    #[script(default)]
    pub place: Place,
    /// Its volume, 0 to 1, which the game multiplies by the container's;
    /// `None` plays at full volume.
    #[script(default)]
    pub volume: Option<f32>,
    /// How long it fades in, at most [`MAX_FADE`].
    #[script(default)]
    pub fade_in: Duration,
}

impl Play {
    /// `container` at `place`, at full volume, without a fade.
    ///
    /// # Examples
    ///
    /// See [`Play`].
    #[must_use]
    pub const fn new(container: SoundContainerId, place: Place) -> Self {
        Self {
            container,
            place,
            volume: None,
            fade_in: Duration::ZERO,
        }
    }

    /// The same play at `volume`, 0 to 1.
    ///
    /// # Examples
    ///
    /// See [`Play`].
    #[must_use]
    pub const fn volume(mut self, volume: f32) -> Self {
        self.volume = Some(volume);
        self
    }

    /// The same play fading in over `fade`.
    ///
    /// # Examples
    ///
    /// See [`Play`].
    #[must_use]
    pub const fn fade_in(mut self, fade: Duration) -> Self {
        self.fade_in = fade;
        self
    }

    /// The play as the loader takes it.
    pub(super) fn raw(&self) -> SoundPlay {
        let (place, entity, at) = match self.place {
            Place::Screen => (SoundPlay::SCREEN, Entity::NONE, [0.0; 3]),
            Place::At(at) => (SoundPlay::AT, Entity::NONE, at),
            Place::On(entity) => (SoundPlay::ON, entity, [0.0; 3]),
            Place::Music => (SoundPlay::MUSIC, Entity::NONE, [0.0; 3]),
        };
        SoundPlay {
            container: self.container.guid(),
            place,
            entity,
            at,
            volume: self.volume.unwrap_or(1.0),
            fade_in_ms: millis(self.fade_in),
            ..SoundPlay::EMPTY
        }
    }
}

/// The longest fade a play or a change of a playing sound takes.
pub const MAX_FADE: Duration = Duration::from_millis(SoundPlay::MAX_FADE_MS as u64);

/// The most sounds one mod starts in one frame of the client; the next
/// start fails until the frame starts them.
pub const MAX_STARTS: u32 = SoundPlay::MAX_STARTS;

/// `fade` in whole milliseconds, past [`MAX_FADE`] when it does not fit.
pub(super) fn millis(fade: Duration) -> u32 {
    u32::try_from(fade.as_millis()).unwrap_or(u32::MAX)
}

/// Why the client did not start a sound.
///
/// # Examples
///
/// See [`SoundState`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub enum PlayFailure {
    /// The game did not start it: its sound pool is full, or it found no
    /// recording of the container.
    GameRefused,
    /// The loader does not reach the game's scene for the place on this
    /// game build.
    NoScene,
    /// The loader's sound playback is off on this game build or after a
    /// fault.
    Off,
    /// The entity it was to follow is not in the client's world.
    NoEntity,
    /// The client's sounds already follow 64 entities, every mod's
    /// together, and this sound's entity is not among them.
    FollowFull,
}

/// Where a sound stands.
///
/// # Examples
///
/// ```
/// use enshwick::sound::{PlayFailure, SoundHandle, SoundState};
///
/// fn report(sound: SoundHandle) {
///     match sound.state() {
///         SoundState::Failed(PlayFailure::NoScene) => enshwick::warn!("no music on this build"),
///         SoundState::Ended => enshwick::info!("the bell rang"),
///         _ => {}
///     }
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub enum SoundState {
    /// Queued: the client's next frame starts it.
    Pending,
    /// Playing, or fading in or out.
    Playing,
    /// Played out, stopped, its entity gone, its world ended or its mod
    /// turned off.
    Ended,
    /// Never started.
    Failed(PlayFailure),
}

impl SoundState {
    /// The state the loader wrote.
    fn from_raw(raw: SoundInfo) -> Self {
        match (raw.state, raw.reason) {
            (SoundInfo::PENDING, _) => Self::Pending,
            (SoundInfo::PLAYING, _) => Self::Playing,
            (SoundInfo::FAILED, SoundInfo::GAME_REFUSED) => Self::Failed(PlayFailure::GameRefused),
            (SoundInfo::FAILED, SoundInfo::NO_SCENE) => Self::Failed(PlayFailure::NoScene),
            (SoundInfo::FAILED, SoundInfo::NO_ENTITY) => Self::Failed(PlayFailure::NoEntity),
            (SoundInfo::FAILED, SoundInfo::FOLLOW_FULL) => Self::Failed(PlayFailure::FollowFull),
            (SoundInfo::FAILED, _) => Self::Failed(PlayFailure::Off),
            _ => Self::Ended,
        }
    }
}

/// A sound the mod started with [`play`], which steers it while it plays.
///
/// A handle names its sound alone: once that sound ended, every call on it
/// changes nothing and [`SoundHandle::state`] reads [`SoundState::Ended`],
/// whatever the client plays since.
///
/// # Examples
///
/// ```
/// use core::time::Duration;
/// use enshwick::sound::SoundHandle;
///
/// fn hush(sound: SoundHandle) -> Result<(), enshwick::Off> {
///     sound.set_volume(0.2, Duration::from_millis(500))?;
///     if sound.is_playing() {
///         sound.stop(Duration::from_secs(2))?;
///     }
///     Ok(())
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SoundHandle(u64);

impl crate::script::Object for SoundHandle {
    const CLASS: &'static crate::script::Class = &crate::script::Class::of::<Self>(
        "SoundHandle",
        "A sound the mod started, which it steers while it plays.",
    );
    const PARTS: &'static [&'static [crate::script::Function<Self>]] = &[Self::SCRIPT_HANDLE];
}

#[crate::script(handle)]
impl SoundHandle {
    /// Stops the sound, fading it out over `fade`.
    ///
    /// # Errors
    ///
    /// [`Off`] for a fade past [`MAX_FADE`], when the mod's queue of
    /// changes is full and on a server.
    ///
    /// # Examples
    ///
    /// See [`SoundHandle`].
    #[script]
    pub fn stop(self, fade: Duration) -> Result<(), Off> {
        self.steer(SoundSteer {
            kind: SoundSteer::STOP,
            fade_ms: millis(fade),
            ..SoundSteer::EMPTY
        })
    }

    /// Changes the sound's volume to `volume`, 0 to 1, over `fade`.
    ///
    /// # Errors
    ///
    /// [`Off`] for a volume outside 0 to 1 or a fade past [`MAX_FADE`],
    /// when the mod's queue of changes is full and on a server.
    ///
    /// # Examples
    ///
    /// See [`SoundHandle`].
    #[script]
    pub fn set_volume(self, volume: f32, fade: Duration) -> Result<(), Off> {
        self.steer(SoundSteer {
            kind: SoundSteer::VOLUME,
            volume,
            fade_ms: millis(fade),
            ..SoundSteer::EMPTY
        })
    }

    /// Moves a sound in the world to `at`, in metres; a sound that followed
    /// an entity stays at `at` from then on, and a [`Place::Screen`] or
    /// [`Place::Music`] sound has no place and does not move.
    ///
    /// # Errors
    ///
    /// [`Off`] for a position not finite or at 2^31 metres or more on an
    /// axis, when the mod's queue of changes is full and on a server.
    ///
    /// # Examples
    ///
    /// ```
    /// fn move_to_origin(sound: enshwick::sound::SoundHandle) -> Result<(), enshwick::Off> {
    ///     sound.set_position([0.0, 0.0, 0.0])
    /// }
    /// ```
    #[script]
    pub fn set_position(self, at: [f64; 3]) -> Result<(), Off> {
        self.steer(SoundSteer {
            kind: SoundSteer::POSITION,
            at,
            ..SoundSteer::EMPTY
        })
    }

    /// Whether the sound is queued or playing.
    ///
    /// # Examples
    ///
    /// See [`SoundHandle`].
    #[must_use]
    #[script]
    pub fn is_playing(self) -> bool {
        matches!(self.state(), SoundState::Pending | SoundState::Playing)
    }

    /// Where the sound stands; [`SoundState::Ended`] where no loader answers.
    ///
    /// # Examples
    ///
    /// See [`SoundState`].
    #[must_use]
    #[script]
    pub fn state(self) -> SoundState {
        let Some(host) = crate::host() else {
            return SoundState::Ended;
        };
        let mut raw = SoundInfo {
            state: SoundInfo::ENDED,
            ..SoundInfo::EMPTY
        };
        let status = (host.api().sound_state)(host.handle(), self.0, &raw mut raw);
        if status == Status::OK {
            SoundState::from_raw(raw)
        } else {
            SoundState::Ended
        }
    }
}

impl SoundHandle {
    /// Queues `steer` for this sound.
    fn steer(self, steer: SoundSteer) -> Result<(), Off> {
        steer_raw(self.0, steer)
    }
}

/// Queues `steer` for the mod's sound `id`.
fn steer_raw(id: u64, steer: SoundSteer) -> Result<(), Off> {
    let host = loader()?;
    let status = (host.api().sound_steer)(host.handle(), id, &raw const steer);
    crate::load::ok(status, (), || "a sound change".to_owned())
}

/// The loader, or why not.
pub(super) fn loader() -> Result<Host, Off> {
    crate::host().ok_or_else(|| Off::new("no loader runs this mod"))
}

/// Unstable: needs the `unstable` feature and may change in any release,
/// since its play on a client is not yet seen in the running game.
///
/// Starts `play` on this client, from any thread, inside a frame or not:
/// the client's game thread starts it in its next frame. Client only.
///
/// # Errors
///
/// [`Off`] for a container neither the game's archive nor a mod holds, a
/// volume outside 0 to 1, a fade past [`MAX_FADE`], a position not finite
/// or at 2^31 metres or more on an axis, entity 0, past [`MAX_STARTS`]
/// starts waiting for one frame, on a server and on a game build where the
/// loader's playback is off.
///
/// # Examples
///
/// ```
/// use enshwick::content::Guid;
/// use enshwick::sound::{Place, Play, SoundContainerId, SoundHandle};
///
/// const BELL: SoundContainerId = SoundContainerId::from_guid(Guid::named("you.bells:bell"));
///
/// fn ring(at: [f64; 3]) -> Result<SoundHandle, enshwick::Off> {
///     enshwick::sound::play(Play::new(BELL, Place::At(at)))
/// }
/// ```
pub fn play(play: Play) -> Result<SoundHandle, Off> {
    let host = loader()?;
    let raw = play.raw();
    let mut id = 0u64;
    let status = (host.api().sound_play)(host.handle(), &raw const raw, &raw mut id);
    let container = play.container.guid();
    crate::load::ok(status, SoundHandle(id), || {
        format!("sound container {container}")
    })
}

/// Unstable: needs the `unstable` feature and may change in any release,
/// since its play on a client is not yet seen in the running game.
///
/// Stops every sound this mod plays, fading each out over `fade`.
///
/// # Errors
///
/// [`Off`] for a fade past [`MAX_FADE`] and on a server.
///
/// # Examples
///
/// ```
/// fn quiet() -> Result<(), enshwick::Off> {
///     enshwick::sound::stop_all(core::time::Duration::ZERO)
/// }
/// ```
pub fn stop_all(fade: Duration) -> Result<(), Off> {
    steer_raw(
        0,
        SoundSteer {
            kind: SoundSteer::STOP_ALL,
            fade_ms: millis(fade),
            ..SoundSteer::EMPTY
        },
    )
}

/// The sound playback a script reaches as `enshwick.sound`, alongside the
/// functions it calls while the mod starts.
///
/// # Examples
///
/// ```
/// use enshwick::script::Bound;
///
/// assert!(enshwick::sound::Sounds::functions().any(|f| f.name() == "play"));
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Sounds(());

impl crate::script::Bound for Sounds {
    const NAME: &'static str = "sound";
    const DOC: &'static str =
        "The game's sounds: playing them on a client, changing containers and hearing notifiers.";
    const PARTS: &'static [&'static [crate::script::Function<Self>]] =
        &[Self::SCRIPT_PLAYBACK, Self::SCRIPT_REMOTE];

    fn resolve(_load: &mut impl crate::Loading) -> Result<Self, Off> {
        Ok(Self(()))
    }
}

#[crate::script(playback)]
#[expect(
    clippy::unused_self,
    reason = "a script calls these through the namespace's handle, which holds nothing"
)]
impl Sounds {
    /// Unstable: needs the `unstable` feature and may change in any release,
    /// since its play on a client is not yet seen in the running game.
    ///
    /// Starts `sound` on this client: the next frame plays it, and its
    /// handle steers it. As [`play`].
    ///
    /// # Errors
    ///
    /// As [`play`].
    ///
    /// # Examples
    ///
    /// See [`play`].
    #[script]
    pub fn play(self, sound: Play) -> Result<SoundHandle, Off> {
        play(sound)
    }

    /// Unstable: needs the `unstable` feature and may change in any release,
    /// since its play on a client is not yet seen in the running game.
    ///
    /// Stops every sound this mod plays, fading each out over `fade`. As
    /// [`stop_all`].
    ///
    /// # Errors
    ///
    /// As [`stop_all`].
    ///
    /// # Examples
    ///
    /// See [`stop_all`].
    #[script]
    pub fn stop_all(self, fade: Duration) -> Result<(), Off> {
        stop_all(fade)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::Guid;

    const BELL: SoundContainerId = SoundContainerId::from_guid(Guid([7; 16]));

    #[test]
    fn each_place_fills_its_own_fields_of_the_raw_play() {
        let at = Play::new(BELL, Place::At([1.0, 2.0, 3.0])).raw();
        assert_eq!(
            (at.place, at.at, at.entity),
            (SoundPlay::AT, [1.0, 2.0, 3.0], Entity::NONE)
        );
        let on = Play::new(BELL, Place::On(Entity(9))).raw();
        assert_eq!((on.place, on.entity), (SoundPlay::ON, Entity(9)));
        assert_eq!(Play::new(BELL, Place::Music).raw().place, SoundPlay::MUSIC);
        assert_eq!(Play::new(BELL, Place::Screen).raw().volume, 1.0);
    }

    #[test]
    fn a_fade_too_long_for_milliseconds_stays_past_the_bound() {
        assert!(millis(Duration::from_secs(u64::MAX)) > SoundPlay::MAX_FADE_MS);
        assert_eq!(millis(Duration::from_millis(250)), 250);
    }

    #[test]
    fn every_failure_reason_reads_back_as_its_state() {
        let failed = |reason| SoundInfo {
            state: SoundInfo::FAILED,
            reason,
            ..SoundInfo::EMPTY
        };
        assert_eq!(
            SoundState::from_raw(failed(SoundInfo::NO_SCENE)),
            SoundState::Failed(PlayFailure::NoScene)
        );
        assert_eq!(
            SoundState::from_raw(failed(SoundInfo::NO_ENTITY)),
            SoundState::Failed(PlayFailure::NoEntity)
        );
        assert_eq!(
            SoundState::from_raw(failed(SoundInfo::FOLLOW_FULL)),
            SoundState::Failed(PlayFailure::FollowFull)
        );
        let unknown = SoundInfo {
            state: 9,
            ..SoundInfo::EMPTY
        };
        assert_eq!(SoundState::from_raw(unknown), SoundState::Ended);
    }
}
