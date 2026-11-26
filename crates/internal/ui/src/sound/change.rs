//! What a mod changes in a sound container: any of its values, and which
//! recordings it plays.

use core::ops::RangeInclusive;
use core::time::Duration;

use super::SoundId;
use crate::content::Guid;
use crate::resources::{Value, Variant};

/// The loudest volume a change sets, as a factor of the recording's.
pub const MAX_VOLUME: f32 = 4.0;
/// The pitch factors a change sets, from two octaves down to two up.
pub const PITCH: RangeInclusive<f32> = 0.25..=4.0;
/// The farthest distance a change sets, in metres.
pub const MAX_DISTANCE: f32 = 500.0;
/// The longest pause a change sets between two plays of a looped container.
pub const MAX_SLEEP: Duration = Duration::from_secs(600);
/// The most takes a container holds, with those of earlier mods.
pub const MAX_TAKES: usize = MAX_TAKES_U32 as usize;
const MAX_TAKES_U32: u32 = 64;
/// The most plays of one container at once a change sets: the size of the
/// game scene's pool of sound instances (client 26430625).
pub const MAX_COUNT: u32 = 8_192;

#[cfg(feature = "unstable")]
pub(super) const MODE: &str = "mode";
pub(super) const SCENE: &str = "scene";
pub(super) const LOOP: &str = "loop";
pub(super) const LOOP_SAME_ENTRY: &str = "loopSameChosenEntry";
pub(super) const AVOID_REPEATING: &str = "avoidRepeatingLastX";
pub(super) const VOLUME: &str = "volume";
pub(super) const VOLUME_RANDOMNESS: &str = "volumeRandomness";
pub(super) const PITCH_FIELD: &str = "pitch";
pub(super) const PITCH_RANDOMNESS: &str = "pitchRandomness";
pub(super) const SLEEP: &str = "sleep";
pub(super) const SLEEP_RANDOMNESS: &str = "sleepRandomness";
pub(super) const HAS_POSITION: &str = "hasPosition";
pub(super) const MIN_DISTANCE: &str = "minDistance";
pub(super) const MAX_DISTANCE_FIELD: &str = "maxDistance";
pub(super) const HAS_RANDOM_POSITION: &str = "hasRandomPosition";
pub(super) const RANDOM_LEFT_RIGHT: &str = "randomLeftRight";
pub(super) const RANDOM_FRONT_BACK: &str = "randomFrontBack";
pub(super) const MUTE_ON_MAIN: &str = "muteOnMain";
pub(super) const SPEAKER: &str = "gamepadSpeaker";
pub(super) const SPEAKER_VOLUME: &str = "gamepadSpeakerVolume";
pub(super) const VIBRATION: &str = "gamepadVibration";
pub(super) const VIBRATION_INTENSITY: &str = "gamepadVibrationIntensity";
pub(super) const MAX_COUNT_LIMITING: &str = "useMaxCountLimiting";
pub(super) const MAX_COUNT_FIELD: &str = "maxCount";
pub(super) const STEALING_MODE: &str = "stealingMode";
pub(super) const PRIORITY: &str = "priority";
pub(super) const ENTRIES: &str = "entries";
pub(super) const ENTRY_CHANCE: &str = "chance";
pub(super) const ENTRY_SOUND: &str = "sound";
const TIME_VALUE: &str = "value";

/// The game's scene of a sound container, `SoundContainerScene`.
///
/// # Examples
///
/// ```
/// use enshwick::sound::{SoundChange, SoundContainerScene};
///
/// let mut music = SoundChange::default();
/// music.scene = Some(SoundContainerScene::Music);
/// assert_eq!(music.scene, Some(SoundContainerScene::Music));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub enum SoundContainerScene {
    /// A sound effect.
    #[default]
    Sfx,
    /// Music.
    Music,
    /// An interface sound, heard from no place in the world.
    Ui,
    /// A voice.
    Voice,
}

impl SoundContainerScene {
    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::Sfx => "Sfx",
            Self::Music => "Music",
            Self::Ui => "Ui",
            Self::Voice => "Voice",
        }
    }
}

/// The game's `SoundContainerStealingMode`: which play of a container a new
/// one stops once [`SoundChange::max_count`] of them play.
///
/// # Examples
///
/// ```
/// use enshwick::sound::{SoundChange, SoundContainerStealingMode};
///
/// let mut three = SoundChange::default();
/// three.max_count = Some(3);
/// three.stealing_mode = Some(SoundContainerStealingMode::Furthest);
/// assert_eq!(three.max_count, Some(3));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub enum SoundContainerStealingMode {
    /// The play that started first stops.
    Oldest,
    /// The play farthest from the listener stops.
    Furthest,
    /// The new play does not start.
    Ignore,
}

impl SoundContainerStealingMode {
    const fn name(self) -> &'static str {
        match self {
            Self::Oldest => "Oldest",
            Self::Furthest => "Furthest",
            Self::Ignore => "Ignore",
        }
    }
}

/// How far from its place each play of a placed container is moved at
/// random, in metres; both 0 turns the spread off.
///
/// # Examples
///
/// ```
/// use enshwick::sound::{RandomPosition, SoundChange};
///
/// let mut birds = SoundChange::default();
/// birds.random_position = Some(RandomPosition { left_right: 8.0, front_back: 4.0 });
/// assert!(birds.random_position.is_some());
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, crate::Script)]
#[expect(
    clippy::exhaustive_structs,
    reason = "a spread on the two axes of the ground"
)]
pub struct RandomPosition {
    /// The spread to either side, from 0 to [`MAX_DISTANCE`].
    pub left_right: f32,
    /// The spread forwards and back, from 0 to [`MAX_DISTANCE`].
    pub front_back: f32,
}

/// One recording a container may play, and how often against its other
/// takes. "Take" is enshwick's word for the game's container entry.
///
/// # Examples
///
/// See [`Takes`].
#[derive(Clone, Copy, Debug, PartialEq, crate::Script)]
#[non_exhaustive]
pub struct Take {
    /// The recording: the game's, or one a mod added or replaced.
    pub sound: SoundId,
    /// Its weight among the container's takes, from 0 to 1; the game picks
    /// a take at random by these weights.
    pub chance: f32,
}

impl Take {
    /// The take of `sound` at weight 1.
    ///
    /// # Examples
    ///
    /// See [`Takes`].
    #[must_use]
    pub const fn new(sound: SoundId) -> Self {
        Self { sound, chance: 1.0 }
    }
}

/// Which recordings a container plays, as a change gives them.
///
/// # Examples
///
/// ```
/// use enshwick::content::Guid;
/// use enshwick::sound::{SoundChange, SoundId, Take, Takes};
///
/// let creak = SoundId::from_guid(Guid::named("you.doors:creak.sound"));
/// let mut change = SoundChange::default();
/// let mut take = Take::new(creak);
/// take.chance = 0.5;
/// change.takes = Some(Takes::Add(vec![take]));
/// assert!(change.takes.is_some());
/// ```
#[derive(Clone, Debug, PartialEq, crate::Script)]
#[non_exhaustive]
pub enum Takes {
    /// These takes instead of every take it held.
    Replace(Vec<Take>),
    /// These takes after those it holds.
    Add(Vec<Take>),
}

impl Takes {
    /// The takes the change gives.
    pub(super) fn takes(&self) -> &[Take] {
        match self {
            Self::Replace(takes) | Self::Add(takes) => takes,
        }
    }
}

/// New values for a sound container, each `None` to keep the one it has:
/// the game's, or the last earlier mod's. Where one value turns a feature
/// on (a spread, a limit, a gamepad output), 0 turns it off.
///
/// # Examples
///
/// See [`Load::change_sound`](../struct.Load.html#method.change_sound).
#[derive(Clone, Debug, Default, PartialEq, crate::Script)]
#[non_exhaustive]
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
    /// The scene it plays in, which sets the volume slider that governs it.
    pub scene: Option<SoundContainerScene>,
    /// Whether it plays at a place in the world rather than in the
    /// listener's head.
    pub has_position: Option<bool>,
    /// The pause between two plays of a looped container, at most
    /// [`MAX_SLEEP`].
    pub sleep: Option<Duration>,
    /// The random spread of that pause, at most [`MAX_SLEEP`].
    pub sleep_randomness: Option<Duration>,
    /// How many of the takes played last the game avoids picking again, at
    /// most [`MAX_TAKES`].
    pub avoid_repeating_last: Option<u32>,
    /// Whether a looped container plays the same take each time round.
    pub loop_same_take: Option<bool>,
    /// The most plays of the container at once, at most [`MAX_COUNT`];
    /// 0 for no limit.
    pub max_count: Option<u32>,
    /// Which play a new one stops at [`max_count`](Self::max_count).
    pub stealing_mode: Option<SoundContainerStealingMode>,
    /// How far each play of a placed container moves at random.
    pub random_position: Option<RandomPosition>,
    /// The game's `muteOnMain` flag.
    pub mute_on_main: Option<bool>,
    /// The volume it plays at on a gamepad's speaker, from 0 (off) to
    /// [`MAX_VOLUME`].
    pub gamepad_speaker: Option<f32>,
    /// The strength of a gamepad's vibration with it, from 0 (off) to 1.
    pub gamepad_vibration: Option<f32>,
    /// Which recordings it plays.
    pub takes: Option<Takes>,
}

/// Why a [`SoundChange`] is refused.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum ChangeSoundError {
    /// A value lies outside its bound.
    #[error("{field} {value} is outside {range:?}")]
    OutOfBounds {
        /// The value's reflected name.
        field: &'static str,
        /// The value given.
        value: f64,
        /// Its bound.
        range: RangeInclusive<f64>,
    },
    /// The least distance is past the greatest.
    #[error("the least distance {min} is past the greatest {max}")]
    Reversed {
        /// The least distance given.
        min: f32,
        /// The greatest distance given.
        max: f32,
    },
    /// A change gives no take to replace the container's with.
    #[error("takes replaced by none")]
    NoTakes,
    /// More takes than a container holds.
    #[error("{0} takes, past {MAX_TAKES}")]
    TooManyTakes(usize),
    /// The container's takes do not read as a list.
    #[error("the container's {ENTRIES} is no list")]
    EntriesNotAList,
}

/// The fields a change writes, by their reflected names, checked against
/// their bounds as they are added.
#[derive(Default)]
struct Fields(Vec<(String, Value)>);

impl Fields {
    fn set(&mut self, name: &str, value: Value) {
        self.0.push((name.to_owned(), value));
    }

    fn within(
        field: &'static str,
        value: f64,
        range: RangeInclusive<f64>,
    ) -> Result<(), ChangeSoundError> {
        if range.contains(&value) {
            Ok(())
        } else {
            Err(ChangeSoundError::OutOfBounds {
                field,
                value,
                range,
            })
        }
    }

    fn float(
        &mut self,
        name: &'static str,
        value: Option<f32>,
        range: RangeInclusive<f32>,
    ) -> Result<(), ChangeSoundError> {
        if let Some(value) = value {
            let range = f64::from(*range.start())..=f64::from(*range.end());
            Self::within(name, f64::from(value), range)?;
            self.set(name, Value::Float(f64::from(value)));
        }
        Ok(())
    }

    fn time(
        &mut self,
        name: &'static str,
        value: Option<Duration>,
    ) -> Result<(), ChangeSoundError> {
        if let Some(value) = value {
            let max = MAX_SLEEP.as_secs_f64();
            Self::within(name, value.as_secs_f64(), 0.0..=max)?;
            // At most MAX_SLEEP, so the nanoseconds fit.
            let nanos = i64::try_from(value.as_nanos()).unwrap_or(i64::MAX);
            self.set(
                name,
                Value::Struct(vec![(TIME_VALUE.to_owned(), Value::Int(nanos))]),
            );
        }
        Ok(())
    }

    fn flag(&mut self, name: &str, value: Option<bool>) {
        if let Some(value) = value {
            self.set(name, Value::Bool(value));
        }
    }

    /// A value that turns a feature on above 0: its flag and the value.
    fn switched(
        &mut self,
        flag: &str,
        name: &'static str,
        value: Option<f32>,
        max: f32,
    ) -> Result<(), ChangeSoundError> {
        if let Some(value) = value {
            self.float(name, Some(value), 0.0..=max)?;
            self.set(flag, Value::Bool(value > 0.0));
        }
        Ok(())
    }
}

impl SoundChange {
    /// The values to write over the container, by their reflected names,
    /// without its takes.
    pub(crate) fn fields(&self) -> Result<Vec<(String, Value)>, ChangeSoundError> {
        let mut fields = Fields::default();
        fields.float(VOLUME, self.volume, 0.0..=MAX_VOLUME)?;
        fields.float(VOLUME_RANDOMNESS, self.volume_randomness, 0.0..=MAX_VOLUME)?;
        fields.float(PITCH_FIELD, self.pitch, PITCH)?;
        fields.float(PITCH_RANDOMNESS, self.pitch_randomness, 0.0..=*PITCH.end())?;
        fields.float(MIN_DISTANCE, self.min_distance, 0.0..=MAX_DISTANCE)?;
        fields.float(MAX_DISTANCE_FIELD, self.max_distance, 0.0..=MAX_DISTANCE)?;
        check_distances(self, None, None)?;
        fields.flag(LOOP, self.looped);
        if let Some(priority) = self.priority {
            fields.set(PRIORITY, Value::Uint(u64::from(priority)));
        }
        if let Some(scene) = self.scene {
            fields.set(SCENE, Value::Name(scene.name().to_owned()));
        }
        fields.flag(HAS_POSITION, self.has_position);
        fields.time(SLEEP, self.sleep)?;
        fields.time(SLEEP_RANDOMNESS, self.sleep_randomness)?;
        if let Some(last) = self.avoid_repeating_last {
            Fields::within(
                AVOID_REPEATING,
                f64::from(last),
                0.0..=f64::from(MAX_TAKES_U32),
            )?;
            fields.set(AVOID_REPEATING, Value::Uint(u64::from(last)));
        }
        fields.flag(LOOP_SAME_ENTRY, self.loop_same_take);
        if let Some(count) = self.max_count {
            Fields::within(
                MAX_COUNT_FIELD,
                f64::from(count),
                0.0..=f64::from(MAX_COUNT),
            )?;
            fields.set(MAX_COUNT_LIMITING, Value::Bool(count > 0));
            fields.set(MAX_COUNT_FIELD, Value::Uint(u64::from(count)));
        }
        if let Some(mode) = self.stealing_mode {
            fields.set(STEALING_MODE, Value::Name(mode.name().to_owned()));
        }
        if let Some(RandomPosition {
            left_right,
            front_back,
        }) = self.random_position
        {
            fields.float(RANDOM_LEFT_RIGHT, Some(left_right), 0.0..=MAX_DISTANCE)?;
            fields.float(RANDOM_FRONT_BACK, Some(front_back), 0.0..=MAX_DISTANCE)?;
            fields.set(
                HAS_RANDOM_POSITION,
                Value::Bool(left_right > 0.0 || front_back > 0.0),
            );
        }
        fields.flag(MUTE_ON_MAIN, self.mute_on_main);
        fields.switched(SPEAKER, SPEAKER_VOLUME, self.gamepad_speaker, MAX_VOLUME)?;
        fields.switched(VIBRATION, VIBRATION_INTENSITY, self.gamepad_vibration, 1.0)?;
        if let Some(takes) = &self.takes {
            check_takes(takes.takes(), matches!(takes, Takes::Replace(_)))?;
        }
        Ok(fields.0)
    }
}

/// Checks the takes one change gives: their count and weights.
fn check_takes(takes: &[Take], replace: bool) -> Result<(), ChangeSoundError> {
    if replace && takes.is_empty() {
        return Err(ChangeSoundError::NoTakes);
    }
    if takes.len() > MAX_TAKES {
        return Err(ChangeSoundError::TooManyTakes(takes.len()));
    }
    for take in takes {
        Fields::within(ENTRY_CHANCE, f64::from(take.chance), 0.0..=1.0)?;
    }
    Ok(())
}

/// The container entry of the game's type `entry_type` that plays `take`.
pub(super) fn entry(entry_type: &str, take: Take) -> Value {
    Value::Variant(Box::new(Variant::new(
        entry_type.to_owned(),
        Value::Struct(vec![
            (
                ENTRY_CHANCE.to_owned(),
                Value::Float(f64::from(take.chance)),
            ),
            (ENTRY_SOUND.to_owned(), Value::Guid(take.sound.guid())),
        ]),
    )))
}

/// The container's takes after `takes`: `held`, its entries so far, kept
/// for an addition and dropped for a replacement.
pub(super) fn merged_entries(
    held: Option<&Value>,
    takes: &Takes,
    entry_type: &str,
) -> Result<Value, ChangeSoundError> {
    let mut entries = match (takes, held) {
        (Takes::Replace(_), _) | (Takes::Add(_), None) => Vec::new(),
        (Takes::Add(_), Some(Value::List(held))) => held.clone(),
        (Takes::Add(_), Some(_)) => return Err(ChangeSoundError::EntriesNotAList),
    };
    entries.extend(takes.takes().iter().map(|take| entry(entry_type, *take)));
    if entries.len() > MAX_TAKES {
        return Err(ChangeSoundError::TooManyTakes(entries.len()));
    }
    Ok(Value::List(entries))
}

/// The GUIDs of the recordings `change` names.
/// Checks that the least distance `change` gives, else `min`, is not past
/// the greatest it gives, else `max`: the container's own where it gives
/// one alone.
pub(super) fn check_distances(
    change: &SoundChange,
    min: Option<f32>,
    max: Option<f32>,
) -> Result<(), ChangeSoundError> {
    match (change.min_distance.or(min), change.max_distance.or(max)) {
        (Some(min), Some(max)) if min > max => Err(ChangeSoundError::Reversed { min, max }),
        _ => Ok(()),
    }
}

/// The least and greatest distance `held` keeps.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a distance the game keeps as a float32"
)]
pub(super) fn held_distances(held: &Value) -> (Option<f32>, Option<f32>) {
    let float = |name| match held.field(name) {
        Some(Value::Float(value)) => Some(*value as f32),
        _ => None,
    };
    (float(MIN_DISTANCE), float(MAX_DISTANCE_FIELD))
}

pub(super) fn named_sounds(change: &SoundChange) -> impl Iterator<Item = Guid> + '_ {
    change
        .takes
        .iter()
        .flat_map(|takes| takes.takes().iter().map(|take| take.sound.guid()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_value() -> SoundChange {
        SoundChange {
            volume: Some(0.5),
            volume_randomness: Some(0.1),
            pitch: Some(1.5),
            pitch_randomness: Some(0.2),
            min_distance: Some(1.0),
            max_distance: Some(40.0),
            looped: Some(true),
            priority: Some(60),
            scene: Some(SoundContainerScene::Music),
            has_position: Some(false),
            sleep: Some(Duration::from_millis(250)),
            sleep_randomness: Some(Duration::from_millis(50)),
            avoid_repeating_last: Some(2),
            loop_same_take: Some(true),
            max_count: Some(3),
            stealing_mode: Some(SoundContainerStealingMode::Furthest),
            random_position: Some(RandomPosition {
                left_right: 2.0,
                front_back: 0.0,
            }),
            mute_on_main: Some(true),
            gamepad_speaker: Some(0.0),
            gamepad_vibration: Some(0.5),
            takes: None,
        }
    }

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
    fn a_value_that_turns_a_feature_on_turns_it_off_at_zero() {
        let fields = every_value().fields().unwrap();
        let named = |name: &str| fields.iter().find(|(n, _)| n == name).map(|(_, v)| v);
        assert_eq!(named(MAX_COUNT_LIMITING), Some(&Value::Bool(true)));
        assert_eq!(named(HAS_RANDOM_POSITION), Some(&Value::Bool(true)));
        assert_eq!(named(SPEAKER), Some(&Value::Bool(false)));
        assert_eq!(named(VIBRATION), Some(&Value::Bool(true)));
        assert_eq!(
            named(SLEEP),
            Some(&Value::Struct(vec![(
                "value".into(),
                Value::Int(250_000_000)
            )]))
        );
        let unlimited = SoundChange {
            max_count: Some(0),
            random_position: Some(RandomPosition::default()),
            ..SoundChange::default()
        };
        let fields = unlimited.fields().unwrap();
        assert!(fields.contains(&(MAX_COUNT_LIMITING.to_owned(), Value::Bool(false))));
        assert!(fields.contains(&(HAS_RANDOM_POSITION.to_owned(), Value::Bool(false))));
    }

    #[test]
    #[expect(clippy::too_many_lines, reason = "one refused change per field")]
    fn values_outside_their_bounds_are_refused() {
        let take = Take::new(SoundId::from_guid(Guid::named("you.a:b")));
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
            SoundChange {
                sleep: Some(MAX_SLEEP + Duration::from_nanos(1)),
                ..SoundChange::default()
            },
            SoundChange {
                sleep_randomness: Some(Duration::from_secs(601)),
                ..SoundChange::default()
            },
            SoundChange {
                avoid_repeating_last: Some(65),
                ..SoundChange::default()
            },
            SoundChange {
                max_count: Some(MAX_COUNT + 1),
                ..SoundChange::default()
            },
            SoundChange {
                random_position: Some(RandomPosition {
                    left_right: -1.0,
                    front_back: 0.0,
                }),
                ..SoundChange::default()
            },
            SoundChange {
                random_position: Some(RandomPosition {
                    left_right: 0.0,
                    front_back: MAX_DISTANCE + 1.0,
                }),
                ..SoundChange::default()
            },
            SoundChange {
                gamepad_speaker: Some(4.5),
                ..SoundChange::default()
            },
            SoundChange {
                gamepad_vibration: Some(1.5),
                ..SoundChange::default()
            },
            SoundChange {
                takes: Some(Takes::Replace(Vec::new())),
                ..SoundChange::default()
            },
            SoundChange {
                takes: Some(Takes::Add(vec![take; MAX_TAKES + 1])),
                ..SoundChange::default()
            },
            SoundChange {
                takes: Some(Takes::Add(vec![Take {
                    chance: 1.5,
                    ..take
                }])),
                ..SoundChange::default()
            },
            SoundChange {
                takes: Some(Takes::Add(vec![Take {
                    chance: f32::NAN,
                    ..take
                }])),
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
            sleep: Some(MAX_SLEEP),
            avoid_repeating_last: Some(64),
            takes: Some(Takes::Replace(vec![take; MAX_TAKES])),
            ..SoundChange::default()
        };
        assert_eq!(edges.fields().map(|f| f.len()), Ok(6));
    }

    #[test]
    fn takes_replace_or_follow_those_held_up_to_the_bound() {
        let take = |n: u8| Take::new(SoundId::from_guid(Guid([n; 16])));
        let held = Value::List(vec![entry("demo::Entry", take(1))]);
        let add = Takes::Add(vec![take(2)]);
        let Ok(Value::List(added)) = merged_entries(Some(&held), &add, "demo::Entry") else {
            panic!("an addition reads");
        };
        assert_eq!(
            added,
            vec![entry("demo::Entry", take(1)), entry("demo::Entry", take(2))]
        );
        let replace = Takes::Replace(vec![take(3)]);
        assert_eq!(
            merged_entries(Some(&held), &replace, "demo::Entry"),
            Ok(Value::List(vec![entry("demo::Entry", take(3))]))
        );
        let full = Value::List(vec![entry("demo::Entry", take(1)); MAX_TAKES]);
        assert_eq!(
            merged_entries(Some(&full), &add, "demo::Entry"),
            Err(ChangeSoundError::TooManyTakes(MAX_TAKES + 1))
        );
        assert_eq!(
            merged_entries(Some(&Value::Uint(1)), &add, "demo::Entry"),
            Err(ChangeSoundError::EntriesNotAList)
        );
    }

    /// Every name this module writes is one the generated catalogue reads,
    /// and the container's every field is one it can write.
    #[cfg(feature = "typed")]
    #[test]
    fn every_field_name_is_the_catalogue_s() {
        use crate::typed::Reflected as _;
        use crate::typed::{SoundContainerResource, SoundContainerShared};

        let mut fields = every_value().fields().unwrap();
        fields.push((MODE.to_owned(), Value::Name("Random".into())));
        let shared = SoundContainerShared::from_value(&Value::Struct(fields.clone()))
            .expect("the change names every field of the container");
        let Value::Struct(read) = shared.to_value() else {
            panic!("a struct")
        };
        assert_eq!(read.len(), fields.len(), "no name of ours is unknown");
        let take = Take::new(SoundId::from_guid(Guid::named("you.a:b")));
        let mut resource = fields;
        resource.push((
            ENTRIES.to_owned(),
            merged_entries(
                None,
                &Takes::Add(vec![take]),
                "SoundContainerResourceSoundEntry",
            )
            .unwrap(),
        ));
        resource.push(("extraRumble".to_owned(), Value::Nil));
        assert!(SoundContainerResource::from_value(&Value::Struct(resource)).is_ok());
    }

    #[test]
    fn a_distance_given_alone_is_checked_against_the_container_s_other() {
        let held = Value::Struct(vec![
            (MIN_DISTANCE.to_owned(), Value::Float(2.0)),
            (MAX_DISTANCE_FIELD.to_owned(), Value::Float(30.0)),
        ]);
        let (min, max) = held_distances(&held);
        assert_eq!((min, max), (Some(2.0), Some(30.0)));
        let at = |min_distance, max_distance| SoundChange {
            min_distance,
            max_distance,
            ..SoundChange::default()
        };
        assert!(check_distances(&at(Some(30.0), None), min, max).is_ok());
        assert_eq!(
            check_distances(&at(Some(50.0), None), min, max),
            Err(ChangeSoundError::Reversed {
                min: 50.0,
                max: 30.0
            })
        );
        assert!(check_distances(&at(None, Some(1.0)), min, max).is_err());
        assert!(check_distances(&at(Some(50.0), Some(60.0)), min, max).is_ok());
        assert!(check_distances(&at(Some(50.0), None), None, None).is_ok());
    }
}
