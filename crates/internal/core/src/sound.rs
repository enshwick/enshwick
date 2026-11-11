//! The game's sounds: a client plays any sound container, the game's own or
//! a mod's, by its [`SoundContainerId`].
//!
//! A sound plays inside a frame of the interface with
//! [`Frame::play`](crate::ui::Frame::play), heard only on the client that
//! plays it and from no place in the world.
//!
//! # Examples
//!
//! ```
//! use enshwick::content::Guid;
//! use enshwick::sound::SoundContainerId;
//!
//! const BELL: SoundContainerId = SoundContainerId::from_guid(Guid::named("you.bells:bell"));
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     load.on_frame(|frame| {
//!         frame.window("bells", "Bells", |frame| {
//!             if frame.button("Ring") && frame.play(BELL).is_err() {
//!                 enshwick::warn!("this game holds no bell");
//!             }
//!         });
//!     })
//! }
//! ```

#[cfg(feature = "unstable")]
mod add;

use core::ops::RangeInclusive;

use crate::abi::Side;
use crate::content::{Guid, types};
use crate::resources::Value;
use crate::{Load, Off};

#[cfg(feature = "unstable")]
pub use add::{MAX_FILE_BYTES, MAX_MOD_BYTES, NewSound, SAMPLE_RATE, SoundContainerScene};

/// The loudest volume a change sets, as a factor of the recording's.
pub const MAX_VOLUME: f32 = 4.0;
/// The pitch factors a change sets, from two octaves down to two up.
pub const PITCH: RangeInclusive<f32> = 0.25..=4.0;
/// The farthest distance a change sets, in metres.
pub const MAX_DISTANCE: f32 = 500.0;

const VOLUME: &str = "volume";
const VOLUME_RANDOMNESS: &str = "volumeRandomness";
const PITCH_FIELD: &str = "pitch";
const PITCH_RANDOMNESS: &str = "pitchRandomness";
const MIN_DISTANCE: &str = "minDistance";
const MAX_DISTANCE_FIELD: &str = "maxDistance";
const LOOP: &str = "loop";
const PRIORITY: &str = "priority";

/// A sound container by its GUID: the sounds one event of the game plays,
/// and how it plays them.
///
/// # Examples
///
/// ```
/// use enshwick::content::Guid;
/// use enshwick::sound::SoundContainerId;
///
/// let guid = Guid::named("you.bells:bell");
/// assert_eq!(SoundContainerId::from_guid(guid).guid(), guid);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
pub struct SoundContainerId(Guid);

impl SoundContainerId {
    /// The container with GUID `guid`: one of the game's, as its archive
    /// names it, or a mod's, under the GUID of its content id
    /// ([`Guid::named`]).
    ///
    /// # Examples
    ///
    /// See [`SoundContainerId`].
    #[must_use]
    pub const fn from_guid(guid: Guid) -> Self {
        Self(guid)
    }

    /// Its GUID.
    ///
    /// # Examples
    ///
    /// See [`SoundContainerId`].
    #[must_use]
    pub const fn guid(self) -> Guid {
        self.0
    }
}

/// New values for a sound container, each `None` to keep the one it has:
/// the game's, or the last earlier mod's.
///
/// # Examples
///
/// See [`Load::change_sound`].
#[derive(Clone, Copy, Debug, Default, PartialEq, crate::Script)]
pub struct SoundChange {
    /// The game's volume factor, from 0 (silent) to [`MAX_VOLUME`].
    pub volume: Option<f32>,
    /// The game's random spread of the volume, from 0 to [`MAX_VOLUME`].
    pub volume_randomness: Option<f32>,
    /// The game's pitch factor, within [`PITCH`].
    pub pitch: Option<f32>,
    /// The game's random spread of the pitch, from 0 to the top of
    /// [`PITCH`].
    pub pitch_randomness: Option<f32>,
    /// The game's least distance of a placed sound, from 0 to
    /// [`MAX_DISTANCE`] metres.
    pub min_distance: Option<f32>,
    /// The game's greatest distance of a placed sound, from 0 to
    /// [`MAX_DISTANCE`] metres.
    pub max_distance: Option<f32>,
    /// Whether the container plays again from the start when it ends.
    pub looped: Option<bool>,
    /// The game's priority of the container among sounds playing at once.
    pub priority: Option<u16>,
}

/// Why a [`SoundChange`] is refused.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub(crate) enum ChangeSoundError {
    /// A value lies outside its bound.
    #[error("{field} {value} is outside {range:?}")]
    OutOfBounds {
        /// The value's reflected name.
        field: &'static str,
        /// The value given.
        value: f32,
        /// Its bound.
        range: RangeInclusive<f32>,
    },
    /// The least distance is past the greatest.
    #[error("the least distance {min} is past the greatest {max}")]
    Reversed {
        /// The least distance given.
        min: f32,
        /// The greatest distance given.
        max: f32,
    },
}

impl SoundChange {
    /// The fields to write over the container, by their reflected names.
    fn fields(&self) -> Result<Vec<(String, Value)>, ChangeSoundError> {
        let mut fields = Vec::new();
        let mut float = |name: &'static str, value: Option<f32>, range: RangeInclusive<f32>| {
            if let Some(value) = value {
                if !range.contains(&value) {
                    return Err(ChangeSoundError::OutOfBounds {
                        field: name,
                        value,
                        range,
                    });
                }
                fields.push((name.to_owned(), Value::Float(f64::from(value))));
            }
            Ok(())
        };
        float(VOLUME, self.volume, 0.0..=MAX_VOLUME)?;
        float(VOLUME_RANDOMNESS, self.volume_randomness, 0.0..=MAX_VOLUME)?;
        float(PITCH_FIELD, self.pitch, PITCH)?;
        float(PITCH_RANDOMNESS, self.pitch_randomness, 0.0..=*PITCH.end())?;
        float(MIN_DISTANCE, self.min_distance, 0.0..=MAX_DISTANCE)?;
        float(MAX_DISTANCE_FIELD, self.max_distance, 0.0..=MAX_DISTANCE)?;
        if let (Some(min), Some(max)) = (self.min_distance, self.max_distance)
            && min > max
        {
            return Err(ChangeSoundError::Reversed { min, max });
        }
        if let Some(looped) = self.looped {
            fields.push((LOOP.to_owned(), Value::Bool(looped)));
        }
        if let Some(priority) = self.priority {
            fields.push((PRIORITY.to_owned(), Value::Uint(u64::from(priority))));
        }
        Ok(fields)
    }
}

#[crate::script(sound)]
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
    pub fn change_sound(
        &mut self,
        sound: SoundContainerId,
        change: SoundChange,
    ) -> Result<(), Off> {
        let fields = change
            .fields()
            .map_err(|why| Off::new(format!("sound container {}: {why}", sound.guid())))?;
        if self.host().build().side != Side::CLIENT {
            return Ok(());
        }
        let key = types::SOUND_CONTAINER_RESOURCE.key(sound.guid())?;
        crate::resources::serve_value(self.host(), key, key, move |_| {
            Ok(Value::Struct(fields.clone()))
        })
    }

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
    ) -> Result<(), Off> {
        self.change_sound(SoundContainerId::from_guid(sound), change)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_change_names_only_the_values_it_gives() {
        let change = SoundChange {
            volume: Some(0.2),
            looped: Some(false),
            ..SoundChange::default()
        };
        assert_eq!(
            change.fields(),
            Ok(vec![
                (VOLUME.to_owned(), Value::Float(f64::from(0.2f32))),
                (LOOP.to_owned(), Value::Bool(false)),
            ])
        );
        assert_eq!(SoundChange::default().fields(), Ok(Vec::new()));
    }

    #[test]
    fn values_outside_their_bounds_are_refused() {
        for change in [
            SoundChange {
                volume: Some(4.5),
                ..SoundChange::default()
            },
            SoundChange {
                volume: Some(-0.1),
                ..SoundChange::default()
            },
            SoundChange {
                volume: Some(f32::NAN),
                ..SoundChange::default()
            },
            SoundChange {
                pitch: Some(0.2),
                ..SoundChange::default()
            },
            SoundChange {
                pitch: Some(4.1),
                ..SoundChange::default()
            },
            SoundChange {
                max_distance: Some(501.0),
                ..SoundChange::default()
            },
            SoundChange {
                min_distance: Some(20.0),
                max_distance: Some(10.0),
                ..SoundChange::default()
            },
        ] {
            assert!(change.fields().is_err(), "{change:?}");
        }
        let edges = SoundChange {
            volume: Some(MAX_VOLUME),
            pitch: Some(*PITCH.start()),
            min_distance: Some(0.0),
            max_distance: Some(MAX_DISTANCE),
            ..SoundChange::default()
        };
        assert_eq!(edges.fields().map(|f| f.len()), Ok(4));
    }
}
