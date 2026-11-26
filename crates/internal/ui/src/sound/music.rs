//! A mod's music: recordings the client's ambience plays as music where
//! and when the mod says, among the game's own tracks.

use core::time::Duration;

use super::add::{self, NewTake};
use super::decode::{Channels, Shape, Trim};
use super::{MAX_SLEEP, SoundId};
use crate::Off;
use crate::abi::Side;
use crate::content::{EngineFacts as _, Guid, Id};
use crate::resources::{Registry, Value};
use enshwick_content::__load::*;

/// The volume of most of the game's music away from battle.
pub const MUSIC_VOLUME: f32 = 0.2;
/// The length of the game's day, the bound of a time of day.
pub const DAY: Duration = Duration::from_hours(24);

const MUSIC: &str = "music";
const SOUNDS: &str = "sounds";
const FLAGS: &str = "flags";
const IN_ALL_ENVIRONMENTS: &str = "InAllEnvironments";
const IN_FOG: &str = "inFog";
const INDOORS: &str = "indoors";
const ENVIRONMENT: &str = "environment";
const PLAYBACK_TYPE: &str = "playbackType";
const START_TIME: &str = "startTime";
const END_TIME: &str = "endTime";
const TIME_VALUE: &str = "value";
const VOLUME: &str = "volume";
const ORIGINAL_VOLUME: &str = "originalVolume";
const AVOID_REPEATING: &str = "avoidRepeatingLastX";
const SLEEP: &str = "sleep";
const SLEEP_RANDOMNESS: &str = "sleepRandomness";
const SOUND_START: &str = "soundStart";
const SOUND_COUNT: &str = "soundCount";

/// Music for [`Load::add_music`](../struct.Load.html#method.add_music):
/// audio files in the mod's folder, read as
/// [`Load::add_sound`](../struct.Load.html#method.add_sound) reads them, and
/// where and when the client's ambience plays them as music, picking one
/// at a time.
///
/// # Examples
///
/// ```
/// use enshwick::content::id;
/// use enshwick::sound::{Channels, NewMusic};
///
/// let mut night = NewMusic::new(id!("night"), vec!["music/night1.ogg".into(), "music/night2.ogg".into()]);
/// night.environment = Some("Grassland".into());
/// night.in_battle = Some(false);
/// night.start_time = Some(std::time::Duration::from_hours(22));
/// night.end_time = Some(std::time::Duration::from_hours(6));
/// night.channels = Channels::Stereo;
/// assert_eq!(night.files.len(), 2);
/// ```
#[derive(Clone, Debug, crate::Script)]
#[non_exhaustive]
pub struct NewMusic {
    /// The music's content id; its recordings are `<id>.sound` on.
    pub id: Id,
    /// Its files, 1 to [`MAX_NEW_TAKES`](super::MAX_NEW_TAKES); the client
    /// plays one at a time and not the one it played last.
    pub files: Vec<String>,
    /// Which channels its recordings keep.
    #[script(default)]
    pub channels: Channels,
    /// A gain in decibels on every file, within
    /// [`GAIN_DB`](super::GAIN_DB).
    #[script(default)]
    pub gain_db: f32,
    /// The part of each file it plays.
    #[script(default)]
    pub trim: Trim,
    /// Where it plays: one of the game's `AmbientEnvironment` names, such
    /// as `Grassland` or `Capital`; `None` for everywhere.
    #[script(default)]
    pub environment: Option<String>,
    /// Whether it plays only in the fog (`true`) or only out of it; `None`
    /// for both.
    #[script(default)]
    pub in_fog: Option<bool>,
    /// Whether it plays only indoors or only outdoors; `None` for both.
    #[script(default)]
    pub indoors: Option<bool>,
    /// Whether it plays only in battle or only out of it; `None` for both.
    #[script(default)]
    pub in_battle: Option<bool>,
    /// The time of day it starts playing, at most [`DAY`]; `None` for
    /// midnight.
    #[script(default)]
    pub start_time: Option<Duration>,
    /// The time of day it stops, at most [`DAY`], earlier than the start
    /// for a span over midnight; `None` for the end of the day.
    #[script(default)]
    pub end_time: Option<Duration>,
    /// Its volume, from 0 to 1; `None` for [`MUSIC_VOLUME`].
    #[script(default)]
    pub volume: Option<f32>,
    /// The pause after each file, at most [`MAX_SLEEP`].
    #[script(default)]
    pub sleep: Duration,
    /// The random spread of that pause, at most [`MAX_SLEEP`].
    #[script(default)]
    pub sleep_randomness: Duration,
}

impl NewMusic {
    /// The music `id` of `files`, playing everywhere, all day, at
    /// [`MUSIC_VOLUME`] without a pause.
    ///
    /// # Examples
    ///
    /// See [`NewMusic`].
    #[must_use]
    pub fn new(id: Id, files: Vec<String>) -> Self {
        Self {
            id,
            files,
            channels: Channels::default(),
            gain_db: 0.0,
            trim: Trim::default(),
            environment: None,
            in_fog: None,
            indoors: None,
            in_battle: None,
            start_time: None,
            end_time: None,
            volume: None,
            sleep: Duration::ZERO,
            sleep_randomness: Duration::ZERO,
        }
    }
}

/// The values of a music row the mod gives, checked.
#[derive(Clone, Debug, PartialEq)]
struct Row {
    environment: Option<String>,
    in_fog: Option<bool>,
    indoors: Option<bool>,
    in_battle: Option<bool>,
    start: Duration,
    end: Duration,
    volume: f32,
    sleep: Duration,
    sleep_randomness: Duration,
}

impl Row {
    /// The row of `new`, refused for a value outside its bound.
    fn of(new: &NewMusic) -> Result<Self, Off> {
        let volume = new.volume.unwrap_or(MUSIC_VOLUME);
        if !(0.0..=1.0).contains(&volume) {
            return Err(Off::new(format!("volume {volume} is outside 0 to 1")));
        }
        let start = new.start_time.unwrap_or(Duration::ZERO);
        let end = new.end_time.unwrap_or(DAY);
        for (name, time) in [("start time", start), ("end time", end)] {
            if time > DAY {
                return Err(Off::new(format!("{name} {time:?} is past a day")));
            }
        }
        for (name, sleep) in [
            ("sleep", new.sleep),
            ("sleep randomness", new.sleep_randomness),
        ] {
            if sleep > MAX_SLEEP {
                return Err(Off::new(format!("{name} {sleep:?} is past {MAX_SLEEP:?}")));
            }
        }
        Ok(Self {
            environment: new.environment.clone(),
            in_fog: new.in_fog,
            indoors: new.indoors,
            in_battle: new.in_battle,
            start,
            end,
            volume,
            sleep: new.sleep,
            sleep_randomness: new.sleep_randomness,
        })
    }

    /// The game's row: `template`, a music row of the game, with this row's
    /// values over it, playing `count` sounds from `start`.
    fn over(&self, mut template: Value, start: usize, count: usize) -> Result<Value, Off> {
        let either = |value: Option<bool>| match value {
            Some(true) => "Yes",
            Some(false) => "No",
            None => "Any",
        };
        let battle = match self.in_battle {
            Some(true) => "PlayOnlyWhenInBattle",
            Some(false) => "PlayOnlyWhenNotInBattle",
            None => "PlayAlways",
        };
        let nanos = |d: Duration| Value::Int(i64::try_from(d.as_nanos()).unwrap_or(i64::MAX));
        let time = |d: Duration| Value::Struct(vec![(TIME_VALUE.to_owned(), nanos(d))]);
        let flags = if self.environment.is_none() {
            vec![IN_ALL_ENVIRONMENTS.to_owned()]
        } else {
            Vec::new()
        };
        let count_u64 = u64::try_from(count).unwrap_or(u64::MAX);
        let mut values = vec![
            (FLAGS, Value::Flags(flags)),
            (IN_FOG, Value::Name(either(self.in_fog).to_owned())),
            (INDOORS, Value::Name(either(self.indoors).to_owned())),
            (PLAYBACK_TYPE, Value::Name(battle.to_owned())),
            (START_TIME, time(self.start)),
            (END_TIME, time(self.end)),
            // Every game music row keeps volume 1 and its own in originalVolume.
            (VOLUME, Value::Float(1.0)),
            (ORIGINAL_VOLUME, Value::Float(f64::from(self.volume))),
            (AVOID_REPEATING, Value::Uint(count_u64.saturating_sub(1))),
            (SLEEP, time(self.sleep)),
            (SLEEP_RANDOMNESS, time(self.sleep_randomness)),
            (
                SOUND_START,
                Value::Uint(u64::try_from(start).unwrap_or(u64::MAX)),
            ),
            (SOUND_COUNT, Value::Uint(count_u64)),
        ];
        if let Some(environment) = &self.environment {
            values.push((ENVIRONMENT, Value::Name(environment.clone())));
        }
        for (name, value) in values {
            *template
                .field_mut(name)
                .ok_or_else(|| Off::new(format!("the game's music row has no {name}")))? = value;
        }
        Ok(template)
    }
}

/// `held`, the game's ambient collection, with `row` playing `sounds`
/// appended to its music: only the two fields it changes.
fn with_music(held: &Value, row: &Row, sounds: &[Guid]) -> Result<Value, Off> {
    let (Some(Value::List(music)), Some(Value::List(held_sounds))) =
        (held.field(MUSIC), held.field(SOUNDS))
    else {
        return Err(Off::new("the game's ambience holds no music"));
    };
    let template = music
        .first()
        .cloned()
        .ok_or_else(|| Off::new("the game's ambience holds no music row to build from"))?;
    let mut music = music.clone();
    music.push(row.over(template, held_sounds.len(), sounds.len())?);
    let mut all = held_sounds.clone();
    all.extend(sounds.iter().copied().map(Value::Guid));
    Ok(Value::Struct(vec![
        (MUSIC.to_owned(), Value::List(music)),
        (SOUNDS.to_owned(), Value::List(all)),
    ]))
}

/// Checks that `environment` is one of the game's.
fn check_environment(load: &crate::BaseLoad, environment: &str) -> Result<(), Off> {
    let hash = load.hash(crate::engine_ids::AMBIENT_ENVIRONMENT)?;
    let ty = Registry::game()?
        .hashed(hash)
        .ok_or_else(|| Off::new("this game names no ambient environments"))?;
    if ty.constants().iter().any(|c| c.name() == environment) {
        return Ok(());
    }
    let names: Vec<&str> = ty
        .constants()
        .iter()
        .map(crate::resources::Constant::name)
        .collect();
    Err(Off::new(format!(
        "no environment {environment}: the game's are {}",
        names.join(", ")
    )))
}

/// The key of the client's one ambient collection.
fn collection(load: &crate::BaseLoad) -> Result<crate::abi::ResourceKey, Off> {
    let hash = load.hash(crate::engine_ids::AMBIENT_SFX_COLLECTION)?;
    let ty = Registry::game()?
        .hashed(hash)
        .ok_or_else(|| Off::new("this game has no ambient collection"))?;
    let keys = load.host().resource_keys(Some(ty))?;
    match keys.as_slice() {
        [key] => Ok(*key),
        _ => Err(Off::new(format!(
            "the game holds {} ambient collections, not one",
            keys.len()
        ))),
    }
}

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::add_music`.
    #[doc(hidden)]
    fn add_music(&mut self, new: NewMusic) -> Result<Vec<SoundId>, Off>;
}

impl LoadMethods for crate::BaseLoad {
    fn add_music(&mut self, new: NewMusic) -> Result<Vec<SoundId>, Off> {
        let id = new.id;
        let fail = move |why: Off| Off::new(format!("music {id}: {why}"));
        if !self.joins() {
            return Err(fail(Off::new("a mod with music runs on both sides")));
        }
        let row = Row::of(&new).map_err(fail)?;
        let client = self.host().build().side == Side::CLIENT;
        // A server plays no music and may not reflect the environments.
        if let (true, Some(environment)) = (client, &row.environment) {
            check_environment(self, environment).map_err(fail)?;
        }
        let ids = add::take_ids(id, new.files.len()).map_err(fail)?;
        let takes: Vec<NewTake> = new.files.iter().map(NewTake::new).collect();
        let shape = Shape {
            channels: new.channels,
            gain_db: new.gain_db,
            trim: new.trim,
        };
        let recordings = add::recordings(self, &takes, shape).map_err(fail)?;
        let key = if client {
            let key = collection(self).map_err(fail)?;
            // Found before any id is declared, so a game without them leaves none.
            super::recording::bases(self).map_err(fail)?;
            Some(key)
        } else {
            None
        };
        self.declare(id)?;
        let sounds = add::serve_recordings(self, ids, recordings).map_err(fail)?;
        if let Some(key) = key {
            let guids: Vec<Guid> = sounds.iter().map(|s| s.guid()).collect();
            crate::resources::serve_value(self.host(), key, key, move |held| {
                with_music(&held, &row, &guids).map_err(fail)
            })?;
        }
        Ok(sounds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn template() -> Value {
        let time = |n| Value::Struct(vec![(TIME_VALUE.into(), Value::Int(n))]);
        Value::Struct(
            [
                (FLAGS, Value::Flags(Vec::new())),
                (IN_FOG, Value::Name("No".into())),
                (INDOORS, Value::Name("No".into())),
                (ENVIRONMENT, Value::Name("Desert".into())),
                (PLAYBACK_TYPE, Value::Name("PlayOnlyWhenNotInBattle".into())),
                (START_TIME, time(0)),
                (END_TIME, time(0)),
                (VOLUME, Value::Float(1.0)),
                (ORIGINAL_VOLUME, Value::Float(0.2)),
                (AVOID_REPEATING, Value::Uint(9)),
                (SLEEP, time(0)),
                (SLEEP_RANDOMNESS, time(0)),
                (SOUND_START, Value::Uint(0)),
                (SOUND_COUNT, Value::Uint(10)),
                ("pitch", Value::Float(1.0)),
            ]
            .map(|(n, v)| (n.to_owned(), v))
            .to_vec(),
        )
    }

    #[test]
    fn a_row_keeps_the_template_s_other_values_and_points_at_its_sounds() {
        let new = NewMusic {
            environment: Some("Grassland".into()),
            in_fog: Some(true),
            in_battle: Some(true),
            start_time: Some(Duration::from_hours(22)),
            end_time: Some(Duration::from_hours(6)),
            volume: Some(0.5),
            ..NewMusic::new(Id::new("you.tunes:night"), vec!["a.wav".into()])
        };
        let row = Row::of(&new).unwrap().over(template(), 40, 3).unwrap();
        assert_eq!(
            row.field(ENVIRONMENT),
            Some(&Value::Name("Grassland".into()))
        );
        assert_eq!(row.field(IN_FOG), Some(&Value::Name("Yes".into())));
        assert_eq!(row.field(INDOORS), Some(&Value::Name("Any".into())));
        assert_eq!(
            row.field(PLAYBACK_TYPE),
            Some(&Value::Name("PlayOnlyWhenInBattle".into()))
        );
        assert_eq!(
            row.field(START_TIME).and_then(|t| t.field(TIME_VALUE)),
            Some(&Value::Int(79_200_000_000_000))
        );
        assert_eq!(row.field(ORIGINAL_VOLUME), Some(&Value::Float(0.5)));
        assert_eq!(row.field(VOLUME), Some(&Value::Float(1.0)));
        assert_eq!(row.field(SOUND_START), Some(&Value::Uint(40)));
        assert_eq!(row.field(SOUND_COUNT), Some(&Value::Uint(3)));
        assert_eq!(row.field(AVOID_REPEATING), Some(&Value::Uint(2)));
        assert_eq!(row.field("pitch"), Some(&Value::Float(1.0)));
        assert_eq!(row.field(FLAGS), Some(&Value::Flags(Vec::new())));
    }

    #[test]
    fn music_without_an_environment_plays_in_all_of_them_all_day() {
        let new = NewMusic::new(Id::new("you.tunes:any"), vec!["a.wav".into()]);
        let row = Row::of(&new).unwrap().over(template(), 0, 1).unwrap();
        assert_eq!(
            row.field(FLAGS),
            Some(&Value::Flags(vec![IN_ALL_ENVIRONMENTS.into()]))
        );
        assert_eq!(
            row.field(END_TIME).and_then(|t| t.field(TIME_VALUE)),
            Some(&Value::Int(86_400_000_000_000))
        );
        assert_eq!(
            row.field(PLAYBACK_TYPE),
            Some(&Value::Name("PlayAlways".into()))
        );
    }

    #[test]
    fn a_row_refuses_values_past_their_bounds() {
        let base = NewMusic::new(Id::new("you.tunes:odd"), vec!["a.wav".into()]);
        for (new, why) in [
            (
                NewMusic {
                    volume: Some(1.5),
                    ..base.clone()
                },
                "volume",
            ),
            (
                NewMusic {
                    end_time: Some(DAY + Duration::from_secs(1)),
                    ..base.clone()
                },
                "past a day",
            ),
            (
                NewMusic {
                    sleep: MAX_SLEEP + Duration::from_secs(1),
                    ..base.clone()
                },
                "sleep",
            ),
        ] {
            let refused = Row::of(&new).unwrap_err().to_string();
            assert!(refused.contains(why), "{refused}");
        }
    }
}
