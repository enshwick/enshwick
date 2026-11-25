//! Sounds a server mod plays on the clients that run Enshwick and the mod,
//! for one player, near a place or for everyone, and steers afterwards.

use core::time::Duration;

use super::play::{Play, Sounds, loader, millis};
use crate::abi::{SoundAudience, SoundPlay, SoundSteer};
use crate::{Off, PlayerKey};

/// The widest [`Audience::Near`] radius, in metres: no sound container
/// carries farther.
pub const MAX_RADIUS: f32 = SoundAudience::MAX_RADIUS;

/// The most sounds one mod sends clients in one server step; the next one
/// fails until the next step.
pub const MAX_PLAYS: u32 = SoundAudience::MAX_PLAYS;

/// The clients a server mod's sound plays on: only clients that run
/// Enshwick and the mod hear it.
///
/// # Examples
///
/// See [`play_for`].
#[derive(Clone, Copy, Debug, PartialEq, crate::Script)]
#[non_exhaustive]
pub enum Audience {
    /// The client of one player.
    Player(PlayerKey),
    /// Every client.
    Everyone,
    /// The clients whose player's character stands within `radius` metres
    /// of `at` when the sound starts.
    Near {
        /// The centre, in metres.
        at: [f64; 3],
        /// How far, up to [`MAX_RADIUS`]; `None` for [`MAX_RADIUS`], as far
        /// as any sound carries.
        radius: Option<f32>,
    },
}

impl Audience {
    /// The audience as the loader takes it.
    fn raw(self) -> SoundAudience {
        match self {
            Self::Player(player) => SoundAudience {
                kind: SoundAudience::PLAYER,
                player,
                ..SoundAudience::EMPTY
            },
            Self::Everyone => SoundAudience::EMPTY,
            Self::Near { at, radius } => SoundAudience {
                kind: SoundAudience::NEAR,
                at,
                radius: radius.unwrap_or(MAX_RADIUS),
                ..SoundAudience::EMPTY
            },
        }
    }
}

/// A sound the mod's server part sent clients with [`play_for`], which
/// steers it on each of them.
///
/// Clients send nothing back, so a remote sound has no state to read. A
/// change reaches the clients the sound went to, while it is among the
/// mod's 256 newest; after that, and once the sound ended on a client, it
/// changes nothing.
///
/// # Examples
///
/// ```
/// use core::time::Duration;
/// use enshwick::sound::RemoteSound;
///
/// fn fade_out(sound: RemoteSound) -> Result<(), enshwick::Off> {
///     sound.set_volume(0.2, Duration::from_millis(500))?;
///     sound.stop(Duration::from_secs(2))
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RemoteSound(u64);

impl crate::script::Object for RemoteSound {
    const CLASS: &'static crate::script::Class = &crate::script::Class::of::<Self>(
        "RemoteSound",
        "A sound the mod's server part sent clients, which it steers on each of them.",
    );
    const PARTS: &'static [&'static [crate::script::Function<Self>]] = &[Self::SCRIPT_HANDLE];
}

#[crate::script(handle)]
impl RemoteSound {
    /// Stops the sound on its clients, fading it out over `fade`.
    ///
    /// # Errors
    ///
    /// [`Off`] for a fade past [`MAX_FADE`](super::MAX_FADE) and where no
    /// server runs.
    ///
    /// # Examples
    ///
    /// See [`RemoteSound`].
    #[script]
    pub fn stop(self, fade: Duration) -> Result<(), Off> {
        self.steer(SoundSteer {
            kind: SoundSteer::STOP,
            fade_ms: millis(fade),
            ..SoundSteer::EMPTY
        })
    }

    /// Changes the sound's volume on its clients to `volume`, 0 to 1, over
    /// `fade`.
    ///
    /// # Errors
    ///
    /// [`Off`] for a volume outside 0 to 1, a fade past
    /// [`MAX_FADE`](super::MAX_FADE) and where no server runs.
    ///
    /// # Examples
    ///
    /// See [`RemoteSound`].
    #[script]
    pub fn set_volume(self, volume: f32, fade: Duration) -> Result<(), Off> {
        self.steer(SoundSteer {
            kind: SoundSteer::VOLUME,
            volume,
            fade_ms: millis(fade),
            ..SoundSteer::EMPTY
        })
    }

    /// Moves the sound on its clients to `at`, in metres.
    ///
    /// # Errors
    ///
    /// [`Off`] for a position not finite or at 2^31 metres or more on an
    /// axis, and where no server runs.
    ///
    /// # Examples
    ///
    /// ```
    /// fn move_to_origin(sound: enshwick::sound::RemoteSound) -> Result<(), enshwick::Off> {
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
}

impl RemoteSound {
    /// Sends `steer` for this sound to its clients.
    fn steer(self, steer: SoundSteer) -> Result<(), Off> {
        let host = loader()?;
        let status = (host.api().sound_steer_for)(host.handle(), self.0, &raw const steer);
        crate::load::ok(status, (), || {
            "a change of a sound sent to clients".to_owned()
        })
    }
}

/// Unstable: needs the `unstable` feature and may change in any release,
/// since its play on a client is not yet seen in the running game.
///
/// Plays `play` from the server on the clients of `audience` that run
/// Enshwick and the mod.
///
/// Each such client plays it as the mod's client part's own
/// [`play`](super::play) would; a vanilla client hears nothing. A hosting
/// client's own player hears it too. From any thread; an
/// [`Audience::Near`] or a [`Place::On`](super::Place::On) on the server's
/// game thread, where the loader reads where characters and entities
/// stand.
///
/// Entity ids are the server world's and name no client entity, so
/// [`Place::On`](super::Place::On) plays where the entity stands when the
/// sound starts, without following it. A client checks the container
/// again: one that lacks it drops the sound.
///
/// # Errors
///
/// [`Off`] for an audience or play out of bounds, an entity of
/// [`Place::On`](super::Place::On) not in the world, past [`MAX_PLAYS`]
/// sounds in one server step, a read of the world off the game thread, a
/// mod that does not run on both sides and where no server runs.
///
/// # Examples
///
/// ```
/// use enshwick::content::Guid;
/// use enshwick::sound::{Audience, Place, Play, RemoteSound, SoundContainerId};
///
/// const HORN: SoundContainerId = SoundContainerId::from_guid(Guid::named("you.horns:horn"));
///
/// fn sound_the_horn(at: [f64; 3]) -> Result<RemoteSound, enshwick::Off> {
///     let near = Audience::Near { at, radius: Some(80.0) };
///     enshwick::sound::play_for(near, Play::new(HORN, Place::At(at)))
/// }
/// ```
pub fn play_for(audience: Audience, play: Play) -> Result<RemoteSound, Off> {
    let host = loader()?;
    let audience = audience.raw();
    let raw: SoundPlay = play.raw();
    let mut id = 0u64;
    let status = (host.api().sound_play_for)(
        host.handle(),
        &raw const audience,
        &raw const raw,
        &raw mut id,
    );
    crate::load::ok(status, RemoteSound(id), || {
        format!("sound container {} for clients", raw.container)
    })
}

#[crate::script(remote)]
#[expect(
    clippy::unused_self,
    reason = "a script calls these through the namespace's handle, which holds nothing"
)]
impl Sounds {
    /// Unstable: needs the `unstable` feature and may change in any release,
    /// since its play on a client is not yet seen in the running game.
    ///
    /// Plays `sound` on the clients of `audience` from the server. As
    /// [`play_for`].
    ///
    /// # Errors
    ///
    /// As [`play_for`].
    ///
    /// # Examples
    ///
    /// See [`play_for`].
    #[script]
    pub fn play_for(self, audience: Audience, sound: Play) -> Result<RemoteSound, Off> {
        play_for(audience, sound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_audience_fills_its_own_fields_of_the_raw_audience() {
        let player = Audience::Player(PlayerKey::new(7)).raw();
        assert_eq!(
            (player.kind, player.player),
            (SoundAudience::PLAYER, PlayerKey::new(7))
        );
        assert_eq!(Audience::Everyone.raw().kind, SoundAudience::EVERYONE);
        let near = Audience::Near {
            at: [1.0, 2.0, 3.0],
            radius: None,
        }
        .raw();
        assert_eq!(
            (near.kind, near.at, near.radius),
            (SoundAudience::NEAR, [1.0, 2.0, 3.0], MAX_RADIUS)
        );
    }
}
