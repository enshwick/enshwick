//! Sounds a client mod plays through the game's sound system and steers
//! after they start.

use crate::abi::{Entity, Header, Metres, PlayerKey};
use crate::content::Guid;

/// A sound a client mod starts, which [`HostApi::sound_play`] queues for the
/// client's game thread.
///
/// [`HostApi::sound_play`]: crate::abi::HostApi::sound_play
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Entity, Header, SoundPlay};
/// use enshwick::content::Guid;
///
/// let play = enshwick::abi::headed!(SoundPlay {
///     container: Guid::named("you.bells:bell"),
///     place: SoundPlay::AT,
///     at: [10.0, 0.0, 0.0],
///     ..SoundPlay::EMPTY
/// });
/// assert_eq!((play.volume, play.entity), (1.0, Entity::NONE));
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct SoundPlay {
    /// Size and version of this struct.
    pub header: Header,
    /// The sound container, the game's or a mod's.
    pub container: Guid,
    /// Where it plays: [`SoundPlay::SCREEN`] and the other place constants.
    pub place: u32,
    /// The entity of the client's world it follows, for [`SoundPlay::ON`].
    pub entity: Entity,
    /// Where it plays in the world, for [`SoundPlay::AT`].
    pub at: Metres,
    /// Its volume, 0 to 1, which the game multiplies by the container's.
    pub volume: f32,
    /// How long it fades in, in milliseconds, at most
    /// [`SoundPlay::MAX_FADE_MS`].
    pub fade_in_ms: u32,
}

impl_headed!(SoundPlay);

impl SoundPlay {
    /// From no place, as an interface sound: the game's `Ui` scene.
    pub const SCREEN: u32 = 0;
    /// At [`at`](Self::at) in the world: the game's `Game` scene.
    pub const AT: u32 = 1;
    /// On [`entity`](Self::entity), following it: the game's `Game` scene.
    pub const ON: u32 = 2;
    /// As music: the game's `Music` scene.
    pub const MUSIC: u32 = 3;
    /// The longest fade a play or a steer takes, in milliseconds.
    pub const MAX_FADE_MS: u32 = 60_000;
    /// The most sounds one mod starts in one frame of the client.
    pub const MAX_STARTS: u32 = 64;
    /// Its header set, no container, on the screen at full volume.
    pub const EMPTY: Self = Self {
        header: Header::of::<Self>(),
        container: Guid::NIL,
        place: Self::SCREEN,
        entity: Entity::NONE,
        at: [0.0; 3],
        volume: 1.0,
        fade_in_ms: 0,
    };

    /// Whether the loader takes this play: a known place, a volume of 0 to
    /// 1, a fade of at most [`SoundPlay::MAX_FADE_MS`], a position
    /// [`in_fixed_range`](crate::abi::in_fixed_range) for [`SoundPlay::AT`]
    /// and an entity for [`SoundPlay::ON`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::SoundPlay;
    ///
    /// assert!(SoundPlay::EMPTY.in_bounds());
    /// assert!(!enshwick::abi::headed!(SoundPlay { volume: 1.5, ..SoundPlay::EMPTY }).in_bounds());
    /// assert!(!enshwick::abi::headed!(SoundPlay { place: SoundPlay::ON, ..SoundPlay::EMPTY }).in_bounds());
    /// ```
    #[must_use]
    pub fn in_bounds(&self) -> bool {
        let placed = match self.place {
            Self::SCREEN | Self::MUSIC => true,
            Self::AT => crate::abi::in_fixed_range(self.at),
            Self::ON => self.entity != Entity::NONE,
            _ => false,
        };
        placed && (0.0..=1.0).contains(&self.volume) && self.fade_in_ms <= Self::MAX_FADE_MS
    }
}

/// A change to a sound a client mod started, which [`HostApi::sound_steer`]
/// queues.
///
/// [`HostApi::sound_steer`]: crate::abi::HostApi::sound_steer
///
/// # Examples
///
/// ```
/// use enshwick::abi::SoundSteer;
///
/// let fade_out = enshwick::abi::headed!(SoundSteer { kind: SoundSteer::STOP, fade_ms: 500, ..SoundSteer::EMPTY });
/// assert_eq!(fade_out.volume, 0.0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct SoundSteer {
    /// Size and version of this struct.
    pub header: Header,
    /// What changes: [`SoundSteer::STOP`] and the other kind constants.
    pub kind: u32,
    /// The new volume, 0 to 1, for [`SoundSteer::VOLUME`].
    pub volume: f32,
    /// How long the stop or the volume change fades, in milliseconds, at
    /// most [`SoundPlay::MAX_FADE_MS`].
    pub fade_ms: u32,
    /// Zero.
    pub reserved: u32,
    /// The new place in the world, for [`SoundSteer::POSITION`].
    pub at: Metres,
}

impl_headed!(SoundSteer);

impl SoundSteer {
    /// Stops the sound, fading it out over `fade_ms`.
    pub const STOP: u32 = 0;
    /// Changes its volume over `fade_ms`.
    pub const VOLUME: u32 = 1;
    /// Moves it to `at`; a sound that followed an entity follows it no more,
    /// and one on the screen or of music has no place and stays.
    pub const POSITION: u32 = 2;
    /// Stops every sound of the mod, fading each out over `fade_ms`; the id
    /// is not read.
    pub const STOP_ALL: u32 = 3;
    /// Its header set, a stop without a fade.
    pub const EMPTY: Self = Self {
        header: Header::of::<Self>(),
        kind: Self::STOP,
        volume: 0.0,
        fade_ms: 0,
        reserved: 0,
        at: [0.0; 3],
    };

    /// Whether the loader takes this steer: a known kind, a volume of 0 to
    /// 1 for [`SoundSteer::VOLUME`], a position
    /// [`in_fixed_range`](crate::abi::in_fixed_range) for
    /// [`SoundSteer::POSITION`] and a fade of at most
    /// [`SoundPlay::MAX_FADE_MS`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::SoundSteer;
    ///
    /// assert!(SoundSteer::EMPTY.in_bounds());
    /// assert!(!enshwick::abi::headed!(SoundSteer { kind: SoundSteer::POSITION, at: [f64::NAN; 3], ..SoundSteer::EMPTY }).in_bounds());
    /// ```
    #[must_use]
    pub fn in_bounds(&self) -> bool {
        let kind = match self.kind {
            Self::STOP | Self::STOP_ALL => true,
            Self::VOLUME => (0.0..=1.0).contains(&self.volume),
            Self::POSITION => crate::abi::in_fixed_range(self.at),
            _ => false,
        };
        kind && self.fade_ms <= SoundPlay::MAX_FADE_MS
    }
}

/// The clients a server mod's sound plays on, which
/// [`HostApi::sound_play_for`] reads.
///
/// [`HostApi::sound_play_for`]: crate::abi::HostApi::sound_play_for
///
/// # Examples
///
/// ```
/// use enshwick::abi::SoundAudience;
///
/// let near = enshwick::abi::headed!(SoundAudience { kind: SoundAudience::NEAR, at: [5.0, 0.0, 5.0], radius: 40.0, ..SoundAudience::EMPTY });
/// assert!(near.in_bounds());
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct SoundAudience {
    /// Size and version of this struct.
    pub header: Header,
    /// Who hears it: [`SoundAudience::PLAYER`] and the other kind constants.
    pub kind: u32,
    /// For [`SoundAudience::NEAR`], how far from `at` a player's character
    /// stands at most, in metres, up to [`SoundAudience::MAX_RADIUS`].
    pub radius: f32,
    /// The player, for [`SoundAudience::PLAYER`].
    pub player: PlayerKey,
    /// The centre, for [`SoundAudience::NEAR`].
    pub at: Metres,
}

impl_headed!(SoundAudience);

impl SoundAudience {
    /// The client of [`player`](Self::player).
    pub const PLAYER: u32 = 0;
    /// Every client.
    pub const EVERYONE: u32 = 1;
    /// The clients whose player's character stands within
    /// [`radius`](Self::radius) of [`at`](Self::at) when the sound starts.
    pub const NEAR: u32 = 2;
    /// The widest radius, in metres: no sound container carries farther.
    pub const MAX_RADIUS: f32 = 500.0;
    /// The most sounds one mod sends clients in one server step.
    pub const MAX_PLAYS: u32 = 32;
    /// Its header set, every client.
    pub const EMPTY: Self = Self {
        header: Header::of::<Self>(),
        kind: Self::EVERYONE,
        radius: 0.0,
        player: PlayerKey(0),
        at: [0.0; 3],
    };

    /// Whether the loader takes this audience: a known kind, a player for
    /// [`SoundAudience::PLAYER`], a centre
    /// [`in_fixed_range`](crate::abi::in_fixed_range) and a radius of 0 to
    /// [`SoundAudience::MAX_RADIUS`] for [`SoundAudience::NEAR`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::SoundAudience;
    ///
    /// assert!(SoundAudience::EMPTY.in_bounds());
    /// assert!(!enshwick::abi::headed!(SoundAudience { kind: SoundAudience::PLAYER, ..SoundAudience::EMPTY }).in_bounds());
    /// assert!(!enshwick::abi::headed!(SoundAudience { kind: SoundAudience::NEAR, radius: 900.0, ..SoundAudience::EMPTY }).in_bounds());
    /// ```
    #[must_use]
    pub fn in_bounds(&self) -> bool {
        match self.kind {
            Self::PLAYER => self.player.0 != 0,
            Self::EVERYONE => true,
            Self::NEAR => {
                crate::abi::in_fixed_range(self.at)
                    && (0.0..=Self::MAX_RADIUS).contains(&self.radius)
            }
            _ => false,
        }
    }

    /// Whether the player `key`, whose character stands `at` (`None`
    /// without one), is of this audience.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::PlayerKey;
    /// use enshwick::abi::SoundAudience;
    ///
    /// let near = enshwick::abi::headed!(SoundAudience { kind: SoundAudience::NEAR, radius: 10.0, ..SoundAudience::EMPTY });
    /// assert!(near.hears(PlayerKey::new(7), Some([6.0, 0.0, 8.0])));
    /// assert!(!near.hears(PlayerKey::new(7), Some([6.0, 0.0, 8.1])));
    /// assert!(!near.hears(PlayerKey::new(7), None));
    /// ```
    #[must_use]
    pub fn hears(&self, key: PlayerKey, at: Option<Metres>) -> bool {
        match self.kind {
            Self::PLAYER => key == self.player,
            Self::EVERYONE => true,
            Self::NEAR => at.is_some_and(|at| {
                let reach = f64::from(self.radius);
                let squared: f64 = at.iter().zip(self.at).map(|(a, c)| (a - c) * (a - c)).sum();
                squared <= reach * reach
            }),
            _ => false,
        }
    }
}

/// Where a sound a client mod started stands, which
/// [`HostApi::sound_state`] writes.
///
/// [`HostApi::sound_state`]: crate::abi::HostApi::sound_state
///
/// # Examples
///
/// ```
/// use enshwick::abi::SoundInfo;
///
/// assert_eq!(SoundInfo::EMPTY.state, SoundInfo::PENDING);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct SoundInfo {
    /// Size and version of this struct.
    pub header: Header,
    /// [`SoundInfo::PENDING`] and the other state constants.
    pub state: u32,
    /// Why it failed, for [`SoundInfo::FAILED`]: [`SoundInfo::GAME_REFUSED`]
    /// and the other reason constants; 0 otherwise.
    pub reason: u32,
}

impl_headed!(SoundInfo);

impl SoundInfo {
    /// Queued: the client's game thread has not started it yet.
    pub const PENDING: u32 = 0;
    /// Playing, or fading in or out.
    pub const PLAYING: u32 = 1;
    /// Ended: played out, stopped, its entity gone, its world ended or its
    /// mod turned off. An id the loader no longer keeps reads as ended.
    pub const ENDED: u32 = 2;
    /// Never started: [`reason`](Self::reason) says why.
    pub const FAILED: u32 = 3;
    /// The game did not start it: its sound pool is full, or it found no
    /// recording of the container.
    pub const GAME_REFUSED: u32 = 1;
    /// The place's game scene is not reached on this build.
    pub const NO_SCENE: u32 = 2;
    /// The loader's sound playback is off on this build or after a fault.
    pub const OFF: u32 = 3;
    /// The entity it was to follow is not in the client's world.
    pub const NO_ENTITY: u32 = 4;
    /// The client's sounds already follow the most entities they follow at
    /// once, 64 over every mod, and this one is not among them.
    pub const FOLLOW_FULL: u32 = 5;
    /// Its header set, pending.
    pub const EMPTY: Self = Self {
        header: Header::of::<Self>(),
        state: Self::PENDING,
        reason: 0,
    };
}
