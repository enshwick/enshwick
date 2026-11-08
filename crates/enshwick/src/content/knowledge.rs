//! Knowledge from mods: bools, counters and flags of the world or of each
//! player, stored, granted and saved by the game like its own.
//!
//! [`Load::add_knowledge`] adds an entry to the game's knowledge, with
//! skill points and experience for a player's entry; the game then grants,
//! sends and saves it as it does a vanilla one, through
//! [`Knowledge`](crate::knowledge::Knowledge). A [`KnowledgeQuery`] tests
//! one entry, vanilla or a mod's, where quests, journal pages and skill
//! nodes ask for knowledge.
//!
//! ```
//! use enshwick::content::id;
//! use enshwick::content::knowledge::{KnowledgeQuery, NewKnowledge};
//! use enshwick::knowledge::{KnowledgeType, Scope};
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let found = load.add_knowledge(NewKnowledge {
//!         skill_points: 1,
//!         ..NewKnowledge::new(id!("found-the-shrine"), Scope::Player, KnowledgeType::Bool)
//!     })?;
//!     let _query = KnowledgeQuery::is_set(found, Scope::Player);
//!     Ok(())
//! }
//! ```

use crate::game_events::ExperienceSourceType;
use crate::knowledge::{KnowledgeId, KnowledgeType, Scope};
use crate::resources::{Registry, Value};
use crate::typed::{GameKnowledgeResource, Named as _};
use crate::{Host, Load, Off};

use super::{EngineFacts, Guid, Id};

/// The game's knowledge: every world and player entry and the skill
/// points they unlock (server 387 and 399, `GameKnowledgeResource`).
pub const GAME_KNOWLEDGE: Guid = Guid::parse("cb1eeb8b-f58a-4785-bf0f-6dd5437e9a5b");

/// The game's names of the knowledge maps, their parts and their values.
const WORLD_KNOWLEDGE: &str = "worldKnowledge";
const PLAYER_KNOWLEDGE: &str = "playerKnowledge";
const SKILL_POINTS: &str = "skillPoints";
const ENTRIES: &str = "entries";
const KEYS: &str = "keys";
const VALUES: &str = "values";
const START: &str = "start";
const COUNT: &str = "count";
const ID: &str = "id";
const VALUE: &str = "value";
const KNOWLEDGE_ID: &str = "knowledgeId";
const AMOUNT: &str = "amount";
const EXPERIENCE_VALUES: &str = "experienceValues";
const SOURCE: &str = "source";
const EXPERIENCE_VALUE_ID: &str = "experienceValueId";
const EXPERIENCE_GAIN: &str = "experienceGain";

/// An experience value of the game: how much experience of which source a
/// knowledge entry or a quest gives, by its id in the game's balancing.
///
/// # Examples
///
/// ```
/// use enshwick::content::knowledge::ExperienceValue;
///
/// let value = ExperienceValue::new(1_674_984_971);
/// assert_eq!(value.get(), 1_674_984_971);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
pub struct ExperienceValue(u32);

impl ExperienceValue {
    /// The experience value with id `id`.
    ///
    /// # Examples
    ///
    /// See [`ExperienceValue`].
    #[must_use]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Its id.
    ///
    /// # Examples
    ///
    /// See [`ExperienceValue`].
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// A new experience value for [`Load::add_experience`]: the experience of
/// one source that a knowledge entry or a quest reward gives.
///
/// # Examples
///
/// ```
/// use enshwick::content::id;
/// use enshwick::content::knowledge::{NewExperience, NewKnowledge};
/// use enshwick::game_events::ExperienceSourceType;
/// use enshwick::knowledge::{KnowledgeType, Scope};
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let found = load.add_experience(NewExperience::new(id!("shrine-xp"), ExperienceSourceType::Exploration, 250))?;
///     load.add_knowledge(NewKnowledge {
///         experience: Some(found),
///         ..NewKnowledge::new(id!("found-the-shrine"), Scope::Player, KnowledgeType::Bool)
///     })?;
///     Ok(())
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
pub struct NewExperience {
    /// Its content id; its experience value is the FNV-1a of the id's GUID.
    pub id: Id,
    /// The source the game counts it under, whose experience rate scales it.
    pub source: ExperienceSourceType,
    /// The experience it gives.
    pub gain: u32,
}

impl NewExperience {
    /// The experience value `id` that gives `gain` experience of `source`.
    ///
    /// # Examples
    ///
    /// See [`NewExperience`].
    #[must_use]
    pub const fn new(id: Id, source: ExperienceSourceType, gain: u32) -> Self {
        Self { id, source, gain }
    }

    /// Its experience value.
    fn value(self) -> ExperienceValue {
        ExperienceValue(self.id.guid().fnv1a())
    }

    /// The game's balance value of it, with the engine's value `source` of
    /// its source.
    fn to_value(self, source: u64) -> Value {
        Value::Struct(vec![
            (SOURCE.into(), Value::Uint(source)),
            (EXPERIENCE_VALUE_ID.into(), hash_key(self.value().get())),
            (EXPERIENCE_GAIN.into(), Value::Uint(self.gain.into())),
        ])
    }
}

/// The engine's value of `source` on the build `facts` describe; `None`
/// when the build lacks its constant.
pub(crate) fn source_value(
    facts: &(impl EngineFacts + ?Sized),
    source: ExperienceSourceType,
) -> Option<u64> {
    if let ExperienceSourceType::Other(raw) = source {
        return Some(raw.into());
    }
    let (_, id) = ExperienceSourceType::IDS
        .iter()
        .find(|(known, _)| *known == source)?;
    facts
        .offset(id)
        .ok()
        .and_then(|raw| u64::try_from(raw).ok())
}

/// The source whose engine value is `raw` on the build `facts` describe.
pub(crate) fn source_of(facts: &(impl EngineFacts + ?Sized), raw: u8) -> ExperienceSourceType {
    ExperienceSourceType::IDS
        .iter()
        .map(|(source, _)| *source)
        .find(|&source| source_value(facts, source) == Some(raw.into()))
        .unwrap_or(ExperienceSourceType::Other(raw))
}

/// The key of the game's balancing table, the one resource of its type.
pub(crate) fn balancing_table(host: Host) -> Result<crate::abi::ResourceKey, Off> {
    let hash = host.hash(crate::engine_ids::BALANCING_TABLE)?;
    let registry = Registry::game()?;
    let ty = registry
        .hashed(hash)
        .ok_or_else(|| Off::new("balancing: the table's type is not reflected"))?;
    match host.resource_keys(Some(ty))?.as_slice() {
        [one] => Ok(*one),
        keys => Err(Off::new(format!(
            "balancing: {} tables in the archive, not one",
            keys.len()
        ))),
    }
}

/// The experience values of the balancing table `table`.
pub(crate) fn experience_values(table: &Value) -> Result<&[Value], Off> {
    match table.field(EXPERIENCE_VALUES) {
        Some(Value::List(values)) => Ok(values),
        _ => Err(Off::new("balancing: no list of experience values")),
    }
}

/// The value id, source and gain of the balance value `value`, its source
/// as the decoder gives it.
pub(crate) fn balance_parts(value: &Value) -> Option<(u32, &Value, u32)> {
    let id = value.field(EXPERIENCE_VALUE_ID).and_then(key_of_hash)?;
    let source = value.field(SOURCE)?;
    let gain = match value.field(EXPERIENCE_GAIN)? {
        Value::Uint(gain) => u32::try_from(*gain).ok()?,
        _ => return None,
    };
    Some((id, source, gain))
}

/// Whether the balancing table `table` holds the experience value `key`.
fn holds_experience(table: &Value, key: u32) -> Result<bool, Off> {
    Ok(experience_values(table)?
        .iter()
        .any(|value| value.field(EXPERIENCE_VALUE_ID).and_then(key_of_hash) == Some(key)))
}

/// `table` with `value`, the balance value of `key`, after its experience
/// values, which the game keeps in no order of id; the server builds the
/// table with it (server 224, read on a server copy). A value of `key` the
/// table holds is replaced: only the mod's own earlier value can be there,
/// since vanilla ids and other mods' ids are refused before.
fn add_experience_to(mut table: Value, key: u32, value: Value) -> Result<Value, Off> {
    let Some(Value::List(values)) = table.field_mut(EXPERIENCE_VALUES) else {
        return Err(Off::new("balancing: no list of experience values"));
    };
    match values
        .iter_mut()
        .find(|held| held.field(EXPERIENCE_VALUE_ID).and_then(key_of_hash) == Some(key))
    {
        Some(held) => *held = value,
        None => values.push(value),
    }
    Ok(table)
}

/// What a [`KnowledgeQuery`] asks of its entry.
///
/// The name is enshwick's, not the game's.
///
/// # Examples
///
/// See [`KnowledgeQuery`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub enum KnowledgeTest {
    /// A bool set; a counter is tested with `AtLeast`, since the game asks
    /// this as a value equal to 1.
    IsSet,
    /// A counter at this count or above, from 1.
    AtLeast(u32),
}

/// A test of one knowledge entry, vanilla or a mod's, as quests, journal
/// pages and skill nodes ask for knowledge.
///
/// The name is enshwick's; the game's is `KnowledgeQueryData`.
///
/// # Examples
///
/// ```
/// use enshwick::content::knowledge::{KnowledgeQuery, KnowledgeTest};
/// use enshwick::knowledge::{KnowledgeId, Scope};
///
/// let kills = KnowledgeQuery::at_least(KnowledgeId::new(7), Scope::World, 25);
/// assert_eq!(kills.test, KnowledgeTest::AtLeast(25));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
pub struct KnowledgeQuery {
    /// The entry tested.
    pub id: KnowledgeId,
    /// Whose the entry is: a player's entry is tested for the player the
    /// game asks about, a world's for the world.
    pub scope: Scope,
    /// What it asks of the entry.
    pub test: KnowledgeTest,
}

impl KnowledgeQuery {
    /// Whether the bool `id` is set.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub const fn is_set(id: KnowledgeId, scope: Scope) -> Self {
        Self {
            id,
            scope,
            test: KnowledgeTest::IsSet,
        }
    }

    /// Whether the counter `id` is at `count` or above.
    ///
    /// # Examples
    ///
    /// See [`KnowledgeQuery`].
    #[must_use]
    pub const fn at_least(id: KnowledgeId, scope: Scope, count: u32) -> Self {
        Self {
            id,
            scope,
            test: KnowledgeTest::AtLeast(count),
        }
    }

    /// The game's query data: a bool equal to 1, or a counter greater than
    /// one less than the count, as the game's own steps ask (server 387).
    pub(crate) fn to_value(self) -> Result<Value, Off> {
        let (kind, operator, compare) = match self.test {
            KnowledgeTest::IsSet => ("SimpleBool", "Equals", 1),
            KnowledgeTest::AtLeast(0) => {
                return Err(Off::new(format!(
                    "knowledge query {}: counts start at 1",
                    self.id
                )));
            }
            KnowledgeTest::AtLeast(count) => ("SimpleCount", "GreaterThan", count - 1),
        };
        Ok(Value::Struct(vec![
            ("knowledgeOrQueryId".into(), hash_key(self.id.get())),
            ("compareValue".into(), Value::Uint(compare.into())),
            ("compareOperator".into(), Value::Name(operator.into())),
            ("type".into(), Value::Name(kind.into())),
            // The game marks a query of an entry only players have (server 387).
            (
                "isExplicitPlayerKnowledgeQuery".into(),
                Value::Bool(self.scope == Scope::Player),
            ),
        ]))
    }
}

/// A new knowledge entry for [`Load::add_knowledge`].
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
pub struct NewKnowledge {
    /// Its content id; its knowledge id is the id's
    /// [`KnowledgeId::from_guid`].
    pub id: Id,
    /// Whose it is.
    pub scope: Scope,
    /// What it is: a bool, a counter or flags.
    pub kind: KnowledgeType,
    /// The bits a flags entry may hold; 0 for a bool or a counter.
    #[script(default)]
    pub flags: u32,
    /// The skill points a player's entry gives when it is granted.
    #[script(default)]
    pub skill_points: u8,
    /// The experience a player's entry gives when it is granted.
    pub experience: Option<ExperienceValue>,
}

impl NewKnowledge {
    /// An entry `id` of `scope` and `kind`, with no flags, skill points or
    /// experience.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub const fn new(id: Id, scope: Scope, kind: KnowledgeType) -> Self {
        Self {
            id,
            scope,
            kind,
            flags: 0,
            skill_points: 0,
            experience: None,
        }
    }

    /// The game's value of the entry with knowledge id `key`.
    fn to_value(self, key: u32) -> Result<Value, Off> {
        let refuse = |why: &str| Err(Off::new(format!("knowledge {}: {why}", self.id)));
        let kind = match self.kind {
            KnowledgeType::Bool => "Bool",
            KnowledgeType::Counter => "Counter",
            KnowledgeType::Flags => "Flags",
            _ => return refuse("no kind the game names"),
        };
        match (self.kind, self.flags) {
            (KnowledgeType::Flags, 0) => return refuse("flags need the bits they may hold"),
            (KnowledgeType::Flags, _) | (_, 0) => {}
            _ => return refuse("only flags hold bits"),
        }
        let player = self.scope == Scope::Player;
        if !player && (self.skill_points > 0 || self.experience.is_some()) {
            return refuse("only a player's entry gives skill points or experience");
        }
        let mut fields = vec![
            (ID.into(), knowledge_id(key)),
            ("sendToAnalytics".into(), Value::Bool(false)),
            ("hasOptionalPlayerProgression".into(), Value::Bool(false)),
            ("isPlayerKnowledge".into(), Value::Bool(player)),
            ("type".into(), Value::Name(kind.into())),
            ("expectedFlags".into(), Value::Uint(self.flags.into())),
        ];
        if player {
            let experience = self.experience.map_or(0, ExperienceValue::get);
            fields.extend([
                ("isAdditionalPlayerKnowledgePerFlag".into(), Value::Uint(0)),
                ("experienceGainId".into(), hash_key(experience)),
                (
                    "unlockedSkillPoints".into(),
                    Value::Uint(self.skill_points.into()),
                ),
            ]);
        }
        Ok(Value::Struct(fields))
    }
}

/// A `HashKey32` of `key`.
pub(crate) fn hash_key(key: u32) -> Value {
    Value::Struct(vec![(VALUE.into(), Value::Uint(key.into()))])
}

/// A `KnowledgeId` of `key`.
fn knowledge_id(key: u32) -> Value {
    Value::Struct(vec![(ID.into(), hash_key(key))])
}

/// The id a `HashKey32` holds.
pub(crate) fn key_of_hash(value: &Value) -> Option<u32> {
    match value.field(VALUE) {
        Some(Value::Uint(key)) => u32::try_from(*key).ok(),
        _ => None,
    }
}

/// The id a `KnowledgeId`, or another value of an `id` hash, holds.
pub(crate) fn key_of(value: &Value) -> Option<u32> {
    value.field(ID).and_then(key_of_hash)
}

/// Whether the world or the player map of `knowledge` holds `key`.
fn knows(knowledge: &Value, key: u32) -> bool {
    [WORLD_KNOWLEDGE, PLAYER_KNOWLEDGE].iter().any(|map| {
        matches!(knowledge.field(map).and_then(|map| map.field(KEYS)),
            Some(Value::List(keys)) if keys.iter().any(|held| key_of(held) == Some(key)))
    })
}

/// The list in field `name` of `value`, taken out of it.
fn take_list(value: &mut Value, name: &str) -> Result<Vec<Value>, Off> {
    match value.field_mut(name) {
        Some(Value::List(list)) => Ok(core::mem::take(list)),
        _ => Err(Off::new(format!("knowledge map: no list of {name}"))),
    }
}

/// Inserts the entry `value` under `key` into the static hash map `map`:
/// the keys and values ordered by bucket again, each bucket's run of them
/// counted again, as the game's lookup reads them (a key's bucket is the
/// key masked by the bucket count less one, server 387).
fn insert(map: &mut Value, key: u32, value: Value) -> Result<(), Off> {
    let buckets = take_list(map, ENTRIES)?.len();
    if !buckets.is_power_of_two() {
        return Err(Off::new(format!(
            "knowledge map: {buckets} buckets, not a power of two"
        )));
    }
    let keys = take_list(map, KEYS)?;
    let values = take_list(map, VALUES)?;
    if keys.len() != values.len() {
        return Err(Off::new("knowledge map: keys and values differ in count"));
    }
    let mut pairs = Vec::with_capacity(keys.len() + 1);
    for (held, entry) in keys.into_iter().zip(values) {
        let id = key_of(&held).ok_or_else(|| Off::new("knowledge map: a key holds no id"))?;
        if id == key {
            return Err(Off::new(format!("knowledge {key}: the game knows it")));
        }
        pairs.push((id, held, entry));
    }
    pairs.push((key, knowledge_id(key), value));
    let mask = buckets - 1;
    // Stable: vanilla keys keep their order within a bucket.
    pairs.sort_by_key(|(id, ..)| *id as usize & mask);
    let mut counts = vec![0u32; buckets];
    for (id, ..) in &pairs {
        counts[*id as usize & mask] += 1;
    }
    let mut start = 0u32;
    let entries = counts
        .into_iter()
        .map(|count| {
            let entry = Value::Struct(vec![
                (START.into(), Value::Uint(start.into())),
                (COUNT.into(), Value::Uint(count.into())),
            ]);
            start += count;
            entry
        })
        .collect();
    let (keys, values) = pairs
        .into_iter()
        .map(|(_, key, value)| (key, value))
        .unzip();
    for (name, list) in [(ENTRIES, entries), (KEYS, keys), (VALUES, values)] {
        if let Some(held) = map.field_mut(name) {
            *held = Value::List(list);
        }
    }
    Ok(())
}

/// `knowledge` with the entry `new` under `key` in its map, and its skill
/// point unlock in the list the game keeps ordered by id (server 387).
fn add_to(mut knowledge: Value, new: NewKnowledge, key: u32) -> Result<Value, Off> {
    let other = if new.scope == Scope::Player {
        WORLD_KNOWLEDGE
    } else {
        PLAYER_KNOWLEDGE
    };
    if let Some(Value::List(keys)) = knowledge.field(other).and_then(|map| map.field(KEYS))
        && keys.iter().any(|held| key_of(held) == Some(key))
    {
        return Err(Off::new(format!("knowledge {}: the game knows it", new.id)));
    }
    let map = if new.scope == Scope::Player {
        PLAYER_KNOWLEDGE
    } else {
        WORLD_KNOWLEDGE
    };
    let value = new.to_value(key)?;
    let held = knowledge
        .field_mut(map)
        .ok_or_else(|| Off::new(format!("knowledge: no {map}")))?;
    insert(held, key, value)?;
    if new.skill_points > 0 {
        let Some(Value::List(unlocks)) = knowledge.field_mut(SKILL_POINTS) else {
            return Err(Off::new("knowledge: no list of skill points"));
        };
        let at = unlocks.partition_point(|unlock| {
            unlock.field(KNOWLEDGE_ID).and_then(key_of).unwrap_or(0) < key
        });
        unlocks.insert(
            at,
            Value::Struct(vec![
                (KNOWLEDGE_ID.into(), knowledge_id(key)),
                (AMOUNT.into(), Value::Uint(new.skill_points.into())),
            ]),
        );
    }
    Ok(knowledge)
}

#[crate::script(content_knowledge)]
impl Load<'_> {
    /// Adds a knowledge entry beside the game's own, on the server and the
    /// client alike, and returns its knowledge id. The game grants, sends
    /// and saves it like a vanilla entry; a player's entry gives its skill
    /// points and experience when granted.
    ///
    /// A save keeps the values of the mod's entries while the mod runs; a
    /// save made without the mod drops them, so they read 0 when the mod
    /// comes back (server 399, read on a server copy).
    ///
    /// # Errors
    ///
    /// [`Off`] for flags without bits or bits on another kind, skill points
    /// or experience on a world's entry, an id the game already knows, or
    /// an id the loader refuses (see [`Load::declare`]).
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::id;
    /// use enshwick::content::knowledge::NewKnowledge;
    /// use enshwick::knowledge::{KnowledgeType, Scope};
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let kills = load.add_knowledge(NewKnowledge::new(id!("wolves-slain"), Scope::World, KnowledgeType::Counter))?;
    ///     enshwick::info!("wolves slain: knowledge {kills}");
    ///     Ok(())
    /// }
    /// ```
    #[script]
    pub fn add_knowledge(&mut self, new: NewKnowledge) -> Result<KnowledgeId, Off> {
        new.to_value(KnowledgeId::from_guid(new.id.guid()).get())?;
        let resource = self.knowledge_without(new.id)?;
        self.declare(new.id)?;
        self.serve_knowledge(resource, new)
    }

    /// Adds an experience value beside the game's own and returns it, for
    /// a knowledge entry's [`NewKnowledge::experience`] or a quest's
    /// experience reward. The game counts the experience under its source,
    /// whose experience rate in the world settings scales it, as it does a
    /// vanilla value's; [`vanilla::experience_values`](super::vanilla::experience_values)
    /// lists those. Adding the same id again replaces its source and gain.
    ///
    /// # Errors
    ///
    /// [`Off`] for a source the build lacks, an experience value the game
    /// already holds, or an id the loader refuses (see [`Load::declare`]).
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::id;
    /// use enshwick::content::knowledge::NewExperience;
    /// use enshwick::game_events::ExperienceSourceType;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let reward = load.add_experience(NewExperience::new(id!("trail-xp"), ExperienceSourceType::Quest, 1200))?;
    ///     enshwick::info!("trail reward: experience value {}", reward.get());
    ///     Ok(())
    /// }
    /// ```
    #[script]
    pub fn add_experience(&mut self, new: NewExperience) -> Result<ExperienceValue, Off> {
        let host = self.host();
        let source = source_value(&host, new.source).ok_or_else(|| {
            Off::new(format!(
                "experience {}: the build lacks the source {:?}",
                new.id, new.source
            ))
        })?;
        let resource = balancing_table(host)?;
        let key = new.value().get();
        if holds_experience(&host.game_value(resource)?, key)? {
            return Err(Off::new(format!(
                "experience {}: the game holds it",
                new.id
            )));
        }
        self.declare(new.id)?;
        let value = new.to_value(source);
        // shortcut: each value decodes and encodes the whole table once at its first build; batch a mod's values when a mod adds hundreds.
        crate::resources::serve_value(host, resource, resource, move |table| {
            add_experience_to(table, key, value.clone())
        })?;
        Ok(new.value())
    }
}

impl Load<'_> {
    /// The game's knowledge's resource key, once it is known not to hold the
    /// entry `id`: other mods' ids are refused by `declare`, the game's only
    /// here, so a quest never goes in without its knowledge.
    pub(super) fn knowledge_without(&self, id: Id) -> Result<crate::abi::ResourceKey, Off> {
        let resource = GAME_KNOWLEDGE.key_by_hash(self.hash(GameKnowledgeResource::ID)?);
        if knows(
            &self.host().game_value(resource)?,
            KnowledgeId::from_guid(id.guid()).get(),
        ) {
            return Err(Off::new(format!("knowledge {id}: the game knows it")));
        }
        Ok(resource)
    }

    /// Serves the game's knowledge `resource` with `new`, whose id the mod
    /// declared.
    pub(super) fn serve_knowledge(
        &self,
        resource: crate::abi::ResourceKey,
        new: NewKnowledge,
    ) -> Result<KnowledgeId, Off> {
        let key = KnowledgeId::from_guid(new.id.guid()).get();
        // shortcut: each entry decodes and encodes the whole resource once at its first build; batch a mod's entries when a mod adds hundreds.
        crate::resources::serve_value(self.host(), resource, resource, move |value| {
            add_to(value, new, key)
        })?;
        Ok(KnowledgeId::new(key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(buckets: usize, keys: &[u32]) -> Value {
        let mut map = Value::Struct(vec![
            (ENTRIES.into(), Value::List(vec![Value::Nil; buckets])),
            (KEYS.into(), Value::List(Vec::new())),
            (VALUES.into(), Value::List(Vec::new())),
        ]);
        for &key in keys {
            insert(&mut map, key, Value::Uint(key.into())).unwrap();
        }
        map
    }

    /// The key the game's lookup finds for `key`: its bucket's run.
    fn find(map: &Value, key: u32) -> Option<u64> {
        let Some(Value::List(entries)) = map.field(ENTRIES) else {
            return None;
        };
        let bucket = &entries[key as usize & (entries.len() - 1)];
        let read = |name| match bucket.field(name) {
            Some(Value::Uint(n)) => usize::try_from(*n).ok(),
            _ => None,
        };
        let (start, count) = (read(START)?, read(COUNT)?);
        let (Some(Value::List(keys)), Some(Value::List(values))) =
            (map.field(KEYS), map.field(VALUES))
        else {
            return None;
        };
        (start..start + count)
            .find(|&i| key_of(&keys[i]) == Some(key))
            .and_then(|i| match values[i] {
                Value::Uint(value) => Some(value),
                _ => None,
            })
    }

    #[test]
    fn every_key_inserted_is_found_in_its_bucket_after_the_rebuild() {
        let keys = [2, 6, 3, 10, 7, 0, 14, 9];
        let map = map(4, &keys);
        for key in keys {
            assert_eq!(find(&map, key), Some(key.into()), "{key}");
        }
        assert_eq!(find(&map, 5), None);
    }

    #[test]
    fn an_id_either_map_of_the_game_holds_is_known() {
        let knowledge = Value::Struct(vec![
            (WORLD_KNOWLEDGE.into(), map(4, &[3])),
            (PLAYER_KNOWLEDGE.into(), map(4, &[8])),
        ]);
        assert!(knows(&knowledge, 3) && knows(&knowledge, 8));
        assert!(!knows(&knowledge, 5));
    }

    #[test]
    fn a_key_the_map_holds_is_refused() {
        let mut map = map(4, &[2]);
        assert!(insert(&mut map, 2, Value::Nil).is_err());
        let mut odd = map.clone();
        if let Some(entries) = odd.field_mut(ENTRIES) {
            *entries = Value::List(vec![Value::Nil; 3]);
        }
        assert!(insert(&mut odd, 9, Value::Nil).is_err(), "3 buckets");
    }

    #[test]
    fn skill_points_join_the_unlocks_in_id_order() {
        let unlock = |key: u32| {
            Value::Struct(vec![
                (KNOWLEDGE_ID.into(), knowledge_id(key)),
                (AMOUNT.into(), Value::Uint(1)),
            ])
        };
        let knowledge = Value::Struct(vec![
            (WORLD_KNOWLEDGE.into(), map(4, &[])),
            (PLAYER_KNOWLEDGE.into(), map(4, &[1])),
            (SKILL_POINTS.into(), Value::List(vec![unlock(1), unlock(9)])),
        ]);
        let new = NewKnowledge {
            skill_points: 2,
            ..NewKnowledge::new(
                Id::new("you.lore:shrine"),
                Scope::Player,
                KnowledgeType::Bool,
            )
        };
        let added = add_to(knowledge.clone(), new, 5).unwrap();
        let Some(Value::List(unlocks)) = added.field(SKILL_POINTS) else {
            panic!("{added:?}")
        };
        let ids: Vec<_> = unlocks
            .iter()
            .filter_map(|u| u.field(KNOWLEDGE_ID).and_then(key_of))
            .collect();
        assert_eq!(ids, [1, 5, 9]);
        let world = NewKnowledge::new(Id::new("you.lore:w"), Scope::World, KnowledgeType::Bool);
        assert!(
            add_to(knowledge, world, 1).is_err(),
            "a player's id already"
        );
    }

    #[test]
    fn entries_that_do_not_fit_their_kind_or_scope_are_refused() {
        let id = Id::new("you.lore:x");
        let flags = NewKnowledge::new(id, Scope::Player, KnowledgeType::Flags);
        assert!(flags.to_value(1).is_err(), "no bits");
        assert!(NewKnowledge { flags: 3, ..flags }.to_value(1).is_ok());
        let bits = NewKnowledge {
            flags: 1,
            ..NewKnowledge::new(id, Scope::Player, KnowledgeType::Bool)
        };
        assert!(bits.to_value(1).is_err());
        let world = NewKnowledge {
            skill_points: 1,
            ..NewKnowledge::new(id, Scope::World, KnowledgeType::Bool)
        };
        assert!(world.to_value(1).is_err());
    }

    #[test]
    fn queries_take_the_forms_of_the_game_s_own_steps() {
        let id = KnowledgeId::new(7);
        let set = KnowledgeQuery::is_set(id, Scope::Player)
            .to_value()
            .unwrap();
        assert_eq!(set.field("type"), Some(&Value::Name("SimpleBool".into())));
        assert_eq!(set.field("compareValue"), Some(&Value::Uint(1)));
        assert_eq!(
            set.field("isExplicitPlayerKnowledgeQuery"),
            Some(&Value::Bool(true))
        );
        let count = KnowledgeQuery::at_least(id, Scope::World, 25)
            .to_value()
            .unwrap();
        assert_eq!(
            count.field("compareOperator"),
            Some(&Value::Name("GreaterThan".into()))
        );
        assert_eq!(count.field("compareValue"), Some(&Value::Uint(24)));
        assert!(
            KnowledgeQuery::at_least(id, Scope::World, 0)
                .to_value()
                .is_err()
        );
    }

    /// Engine facts that answer constants through the function they hold.
    struct Sources(fn(&str) -> Option<usize>);

    impl EngineFacts for Sources {
        fn offset(&self, id: &str) -> Result<usize, Off> {
            (self.0)(id).ok_or_else(|| Off::new(id.to_owned()))
        }

        fn size(&self, id: &str) -> Result<usize, Off> {
            Err(Off::new(id.to_owned()))
        }

        fn hash(&self, id: &str) -> Result<u32, Off> {
            Err(Off::new(id.to_owned()))
        }
    }

    #[test]
    fn a_source_maps_to_its_engine_value_and_back_and_an_unnamed_one_stays_its_number() {
        use crate::engine_ids as ids;
        let facts = Sources(|id| match id {
            ids::EXPERIENCE_SOURCE_QUEST => Some(7),
            ids::EXPERIENCE_SOURCE_LORE => Some(2),
            _ => None,
        });
        assert_eq!(source_value(&facts, ExperienceSourceType::Quest), Some(7));
        assert_eq!(
            source_value(&facts, ExperienceSourceType::Mining),
            None,
            "the build lacks it"
        );
        assert_eq!(
            source_value(&facts, ExperienceSourceType::Other(9)),
            Some(9)
        );
        assert_eq!(source_of(&facts, 2), ExperienceSourceType::Lore);
        assert_eq!(source_of(&facts, 7), ExperienceSourceType::Quest);
        assert_eq!(source_of(&facts, 3), ExperienceSourceType::Other(3));
    }

    #[test]
    fn an_experience_value_follows_the_table_s_own_and_a_second_of_its_id_replaces_it() {
        let new = NewExperience::new(Id::new("you.lore:xp"), ExperienceSourceType::Quest, 1200);
        let vanilla = Value::Struct(vec![
            (SOURCE.into(), Value::Name("Mining".into())),
            (EXPERIENCE_VALUE_ID.into(), hash_key(5)),
            (EXPERIENCE_GAIN.into(), Value::Uint(10)),
        ]);
        let table = Value::Struct(vec![(EXPERIENCE_VALUES.into(), Value::List(vec![vanilla]))]);
        let key = new.value().get();
        let added = add_experience_to(table.clone(), key, new.to_value(3)).unwrap();
        let values = experience_values(&added).unwrap();
        assert_eq!(values.len(), 2);
        assert_eq!(
            balance_parts(&values[0]).map(|(id, ..)| id),
            Some(5),
            "vanilla first"
        );
        assert_eq!(
            balance_parts(&values[1]),
            Some((key, &Value::Uint(3), 1200))
        );
        assert!(holds_experience(&added, key).unwrap());
        assert!(!holds_experience(&table, key).unwrap());
        let again = NewExperience { gain: 5, ..new };
        let replaced = add_experience_to(added, key, again.to_value(3)).unwrap();
        let values = experience_values(&replaced).unwrap();
        assert_eq!(values.len(), 2, "replaced, not added");
        assert_eq!(balance_parts(&values[1]), Some((key, &Value::Uint(3), 5)));
        assert!(experience_values(&Value::Struct(Vec::new())).is_err());
        assert!(add_experience_to(Value::Struct(Vec::new()), key, Value::Nil).is_err());
    }
}
