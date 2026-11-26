//! Voiced lines: a sound container per voice language, which dialogue,
//! lore readings and journal entries name.

use super::add::{self, NewSound, NewTake};
use super::decode::{Channels, Trim};
use super::{SAMPLE_RATE, SoundChange};
use crate::Off;
use crate::abi::Side;
use crate::content::{EngineFacts as _, Guid, Id};
use crate::resources::{Registry, Value};
use enshwick_content::__load::*;

const LANGUAGES: &str = "languages";
const DURATIONS: &str = "durations";
/// The frames of the game's unit of a line's length, 10 ms.
const FRAMES_PER_UNIT: usize = SAMPLE_RATE as usize / 100;

/// A voiced line, the game's `VoiceoverResource`, by its GUID: what a
/// dialogue line, a lore reading or a journal entry names to speak.
///
/// # Examples
///
/// ```
/// use enshwick::content::Guid;
/// use enshwick::sound::VoiceoverId;
///
/// let guid = Guid::named("you.lore:welcome");
/// assert_eq!(VoiceoverId::from_guid(guid).guid(), guid);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
pub struct VoiceoverId(Guid);

impl VoiceoverId {
    /// The line with GUID `guid`: one of the game's, as its archive names
    /// it, or a mod's, under the GUID of its content id.
    ///
    /// # Examples
    ///
    /// See [`VoiceoverId`].
    #[must_use]
    pub const fn from_guid(guid: Guid) -> Self {
        Self(guid)
    }

    /// Its GUID.
    ///
    /// # Examples
    ///
    /// See [`VoiceoverId`].
    #[must_use]
    pub const fn guid(self) -> Guid {
        self.0
    }
}

/// One voice language's recording of a [`NewVoiceover`]: the language, by
/// the game's name for its slot in snake case (`english`,
/// `simplified_chinese`), and the takes its container picks among.
///
/// # Examples
///
/// See [`NewVoiceover`].
#[derive(Clone, Debug, PartialEq, crate::Script)]
#[non_exhaustive]
pub struct VoiceLine {
    /// The language: a field of the game's `Voiceover` in snake case; the
    /// game's voice language setting picks one, and a language without a
    /// slot of its own speaks the first.
    pub language: String,
    /// Its takes, 1 to [`MAX_NEW_TAKES`](super::MAX_NEW_TAKES).
    pub takes: Vec<NewTake>,
}

impl VoiceLine {
    /// The line in `language` of the one take `file`.
    ///
    /// # Examples
    ///
    /// See [`NewVoiceover`].
    #[must_use]
    pub fn new(language: impl Into<String>, file: impl Into<String>) -> Self {
        Self {
            language: language.into(),
            takes: vec![NewTake::new(file)],
        }
    }
}

/// A voiced line for
/// [`Load::add_voiceover`](../struct.Load.html#method.add_voiceover) and
/// [`Load::change_voiceover`](../struct.Load.html#method.change_voiceover):
/// audio files per voice language, read as
/// [`Load::add_sound`](../struct.Load.html#method.add_sound) reads them.
///
/// # Examples
///
/// ```
/// use enshwick::content::id;
/// use enshwick::sound::{NewVoiceover, VoiceLine};
///
/// let mut welcome = NewVoiceover::new(
///         id!("welcome"),
///         vec![VoiceLine::new("english", "voice/en/welcome.ogg"), VoiceLine::new("german", "voice/de/welcome.ogg")],
///     );
/// welcome.fallback = Some("english".into());
/// assert_eq!(welcome.lines.len(), 2);
/// ```
#[derive(Clone, Debug, crate::Script)]
#[non_exhaustive]
pub struct NewVoiceover {
    /// The line's content id; each language's container is
    /// `<id>.<language>` and its recordings `<id>.<language>.sound` on.
    pub id: Id,
    /// One recording per language, each language at most once.
    pub lines: Vec<VoiceLine>,
    /// The language whose recording also speaks in every language the line
    /// leaves empty; `None` leaves them silent, as the game does.
    #[script(default)]
    pub fallback: Option<String>,
    /// Which channels the recordings keep.
    #[script(default)]
    pub channels: Channels,
    /// A gain in decibels on every take, within
    /// [`GAIN_DB`](super::GAIN_DB).
    #[script(default)]
    pub gain_db: f32,
    /// The part of each take's file it plays.
    #[script(default)]
    pub trim: Trim,
    /// Each language's container values where they differ from those of a
    /// mod's new sound; a lore reading plays well as
    /// [`has_position`](SoundChange::has_position) `false`.
    #[script(default)]
    pub container: SoundChange,
}

impl NewVoiceover {
    /// The line `id` of `lines`, with no fallback and the default values.
    ///
    /// # Examples
    ///
    /// See [`NewVoiceover`].
    #[must_use]
    pub fn new(id: Id, lines: Vec<VoiceLine>) -> Self {
        Self {
            id,
            lines,
            fallback: None,
            channels: Channels::default(),
            gain_db: 0.0,
            trim: Trim::default(),
            container: SoundChange::default(),
        }
    }
}

/// The game's voice languages in slot order, from the fields of its
/// `Voiceover` in snake case.
fn languages(load: &crate::BaseLoad) -> Result<Vec<String>, Off> {
    let hash = load.hash(crate::engine_ids::VOICEOVER_LANGUAGES)?;
    let ty = Registry::game()?
        .hashed(hash)
        .ok_or_else(|| Off::new("this game names no voice languages"))?;
    Ok(ty.fields().iter().map(|f| snake(f.name())).collect())
}

/// `name` in snake case: `simplifiedChinese` as `simplified_chinese`.
fn snake(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for c in name.chars() {
        if c.is_ascii_uppercase() {
            if !out.is_empty() {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// The slot of each line in `languages` and of the fallback, checked: every
/// language known and given once, the fallback one of them.
fn slots(
    lines: &[VoiceLine],
    fallback: Option<&str>,
    languages: &[String],
) -> Result<(Vec<usize>, Option<usize>), Off> {
    let slot = |language: &str| {
        languages.iter().position(|l| l == language).ok_or_else(|| {
            Off::new(format!(
                "no voice language {language}: the game's are {}",
                languages.join(", ")
            ))
        })
    };
    if lines.is_empty() {
        return Err(Off::new("a voiced line needs a language"));
    }
    let mut taken = Vec::with_capacity(lines.len());
    for line in lines {
        let at = slot(&line.language)?;
        if taken.contains(&at) {
            return Err(Off::new(format!("{} is given twice", line.language)));
        }
        taken.push(at);
    }
    let fallback = match fallback {
        Some(language) => {
            let at = slot(language)?;
            if !taken.contains(&at) {
                return Err(Off::new(format!(
                    "the fallback {language} is none of the line's languages"
                )));
            }
            Some(at)
        }
        None => None,
    };
    Ok((taken, fallback))
}

/// A line's length in the game's 10 ms units, `frames` long at
/// [`SAMPLE_RATE`], rounded up.
fn units(frames: usize) -> u64 {
    u64::try_from(frames.div_ceil(FRAMES_PER_UNIT))
        .unwrap_or(u64::MAX)
        .min(u64::from(u16::MAX))
}

/// The containers and lengths a voiced line's slots hold.
#[derive(Clone, Debug, PartialEq)]
struct Slots {
    containers: Vec<Guid>,
    lengths: Vec<u64>,
}

impl Slots {
    /// `held`'s slots, of the game's `slots` languages.
    fn held(held: &Value, slots: usize) -> Result<Self, Off> {
        let (Some(Value::List(containers)), Some(Value::List(lengths))) =
            (held.field(LANGUAGES), held.field(DURATIONS))
        else {
            return Err(Off::new("the game's voiced line holds no languages"));
        };
        let containers: Vec<Guid> = containers
            .iter()
            .map(|c| match c {
                Value::Guid(guid) => *guid,
                _ => Guid::NIL,
            })
            .collect();
        let lengths: Vec<u64> = lengths
            .iter()
            .map(|l| match l {
                Value::Uint(units) => *units,
                _ => 0,
            })
            .collect();
        if containers.len() != slots || lengths.len() != slots {
            return Err(Off::new(format!(
                "the game's voiced line holds {} slots for {slots} languages",
                containers.len()
            )));
        }
        Ok(Self {
            containers,
            lengths,
        })
    }

    /// Empty slots for `lines.count` languages.
    fn empty(lines: &Lines) -> Self {
        Self {
            containers: vec![Guid::NIL; lines.count],
            lengths: vec![0; lines.count],
        }
    }

    /// `lines` put in their slots, and the fallback's in each slot left
    /// empty.
    fn with(mut self, lines: &Lines) -> Self {
        for made in &lines.made {
            self.containers[made.slot] = made.container;
            self.lengths[made.slot] = made.length;
        }
        let fallback = lines
            .fallback
            .and_then(|slot| lines.made.iter().find(|made| made.slot == slot));
        if let Some(fallback) = fallback {
            for slot in 0..self.containers.len() {
                if self.containers[slot] == Guid::NIL {
                    self.containers[slot] = fallback.container;
                    self.lengths[slot] = fallback.length;
                }
            }
        }
        self
    }

    fn value(self) -> Value {
        Value::Struct(vec![
            (
                LANGUAGES.to_owned(),
                Value::List(self.containers.into_iter().map(Value::Guid).collect()),
            ),
            (
                DURATIONS.to_owned(),
                Value::List(self.lengths.into_iter().map(Value::Uint).collect()),
            ),
        ])
    }
}

/// A content id the mod builds at run time; ids live for the process.
fn line_id(id: Id, language: &str) -> Result<Id, Off> {
    let name: &'static str = Box::leak(format!("{id}.{language}").into_boxed_str());
    Id::try_new(name).ok_or_else(|| Off::new(format!("{name} is no content id")))
}

/// One language's sound of a voiced line: its slot, container and length.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Made {
    slot: usize,
    container: Guid,
    length: u64,
}

/// The sounds of a voiced line's languages, the fallback's slot and the
/// count of the game's languages.
#[derive(Clone, Debug, PartialEq)]
struct Lines {
    made: Vec<Made>,
    fallback: Option<usize>,
    count: usize,
}

/// Adds each line of `new` as a sound.
fn add_lines(load: &mut crate::BaseLoad, new: NewVoiceover) -> Result<Lines, Off> {
    let NewVoiceover {
        id,
        lines,
        fallback,
        channels,
        gain_db,
        trim,
        container,
    } = new;
    // A server plays no line and may not reflect the voice languages, so
    // only a client maps the lines to the game's slots.
    let (languages, (taken, fallback)) = if load.host().build().side == Side::CLIENT {
        let languages = languages(load)?;
        let slots = slots(&lines, fallback.as_deref(), &languages)?;
        (languages, slots)
    } else {
        (Vec::new(), (vec![0; lines.len()], None))
    };
    let mut made = Vec::with_capacity(lines.len());
    for (line, slot) in lines.into_iter().zip(taken) {
        let sound = NewSound {
            id: line_id(id, &line.language)?,
            takes: line.takes,
            channels,
            gain_db,
            trim,
            container: container.clone(),
        };
        let (added, frames) = add::added(load, sound)?;
        made.push(Made {
            slot,
            container: added.container.guid(),
            length: units(frames),
        });
    }
    Ok(Lines {
        made,
        fallback,
        count: languages.len(),
    })
}

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::add_voiceover`.
    #[doc(hidden)]
    fn add_voiceover(&mut self, new: NewVoiceover) -> Result<VoiceoverId, Off>;

    /// As `Load::change_voiceover`.
    #[doc(hidden)]
    fn change_voiceover(&mut self, voiceover: VoiceoverId, new: NewVoiceover) -> Result<(), Off>;
}

impl LoadMethods for crate::BaseLoad {
    fn add_voiceover(&mut self, new: NewVoiceover) -> Result<VoiceoverId, Off> {
        let id = new.id;
        let fail = |why: Off| Off::new(format!("voiced line {id}: {why}"));
        let lines = add_lines(self, new).map_err(fail)?;
        let guid = self.declare(id)?;
        if self.host().build().side == Side::CLIENT {
            let key = guid.key_by_hash(self.hash(crate::engine_ids::VOICEOVER)?);
            let value = Slots::empty(&lines).with(&lines).value();
            crate::resources::serve_new(self.host(), key, &value).map_err(fail)?;
        }
        Ok(VoiceoverId::from_guid(guid))
    }

    fn change_voiceover(&mut self, voiceover: VoiceoverId, new: NewVoiceover) -> Result<(), Off> {
        let line = voiceover.guid();
        let fail = move |why: Off| Off::new(format!("voiced line {line}: {why}"));
        let host = self.host();
        let key = if host.build().side == Side::CLIENT {
            let key = line.key_by_hash(self.hash(crate::engine_ids::VOICEOVER)?);
            if host.game_resource(key).is_none() {
                return Err(fail(Off::new("the game holds no such line")));
            }
            Some(key)
        } else {
            None
        };
        let lines = add_lines(self, new).map_err(fail)?;
        let Some(key) = key else {
            return Ok(());
        };
        crate::resources::serve_value(host, key, key, move |held| {
            Ok(Slots::held(&held, lines.count)
                .map_err(fail)?
                .with(&lines)
                .value())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Vec<String> {
        [
            "english",
            "german",
            "french",
            "spanish",
            "simplified_chinese",
        ]
        .map(String::from)
        .to_vec()
    }

    #[test]
    fn a_language_takes_the_snake_case_of_the_game_s_field() {
        assert_eq!(snake("simplifiedChinese"), "simplified_chinese");
        assert_eq!(snake("english"), "english");
    }

    #[test]
    fn lines_take_their_language_s_slot_once_and_the_fallback_is_one_of_them() {
        let lines = [
            VoiceLine::new("german", "de.wav"),
            VoiceLine::new("english", "en.wav"),
        ];
        assert_eq!(
            slots(&lines, Some("german"), &names()).unwrap(),
            (vec![1, 0], Some(1))
        );
        let refused = |lines: &[VoiceLine], fallback| {
            slots(lines, fallback, &names()).unwrap_err().to_string()
        };
        assert!(refused(&lines, Some("french")).contains("none of the line's"));
        assert!(refused(&[VoiceLine::new("klingon", "k.wav")], None).contains("english, german"));
        let twice = [
            VoiceLine::new("german", "a.wav"),
            VoiceLine::new("german", "b.wav"),
        ];
        assert!(refused(&twice, None).contains("twice"));
        assert!(refused(&[], None).contains("needs a language"));
    }

    #[test]
    fn a_length_counts_started_10_ms_units_up_to_the_game_s_bound() {
        assert_eq!(units(0), 0);
        assert_eq!(units(1), 1);
        assert_eq!(units(480), 1);
        assert_eq!(units(481), 2);
        assert_eq!(units(usize::MAX), u64::from(u16::MAX));
    }

    #[test]
    fn a_fallback_fills_only_the_slots_left_empty() {
        let (a, b, c) = (Guid([1; 16]), Guid([2; 16]), Guid([3; 16]));
        let made = |slot, container, length| Made {
            slot,
            container,
            length,
        };
        let held = Slots {
            containers: vec![c, Guid::NIL, Guid::NIL],
            lengths: vec![9, 0, 0],
        };
        let changed = Lines {
            made: vec![made(1, a, 5), made(2, b, 7)],
            fallback: Some(2),
            count: 3,
        };
        let filled = held.with(&changed);
        assert_eq!(filled.containers, [c, a, b]);
        assert_eq!(filled.lengths, [9, 5, 7]);
        let added = Lines {
            made: vec![made(0, a, 5)],
            fallback: Some(0),
            count: 3,
        };
        let filled = Slots::empty(&added).with(&added);
        assert_eq!(filled.containers, [a, a, a]);
        assert_eq!(filled.lengths, [5, 5, 5]);
    }
}
