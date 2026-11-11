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

use std::sync::{Arc, Mutex};

use crate::abi::Level;
use crate::knowledge::{KnowledgeId, KnowledgeType, Scope};
use crate::locks::Relock as _;
use crate::resources::Value;
use crate::typed::{
    CompareOperator, GameKnowledgeResource, KnowledgeQueryData, KnowledgeQueryDataType, Named as _,
};
use crate::{Host, Load, Off};

use super::{EngineFacts, Guid, Id};

mod experience;

pub use experience::NewExperience;
pub(crate) use experience::{balance_parts, balancing_table, source_type, values_of};

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

/// What a [`KnowledgeQuery`] asks of its entry, or of the game query it
/// names.
///
/// The name is enshwick's, not the game's.
///
/// # Examples
///
/// See [`KnowledgeQuery`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub enum KnowledgeTest {
    /// A bool set, or a counter or flags other than 0.
    IsSet,
    /// A counter at this count or above, from 1.
    AtLeast(u32),
    /// A counter at exactly this count; `Equals(0)` is an entry not set.
    Equals(u32),
    /// A counter below this count, from 1.
    Below(u32),
    /// Flags holding any of these bits, not 0.
    HasFlag(u32),
    /// The game query of the id holds.
    Holds,
    /// The game query of the id does not hold.
    Fails,
    /// A form of the game's that enshwick does not read, such as a test
    /// that never holds or one a later build adds; only the game answers
    /// it, and a mod cannot write it.
    Unknown,
}

impl KnowledgeTest {
    /// Whether an entry of `value` passes it; `None` for a game query's
    /// test or an unknown form, which only the game answers.
    #[must_use]
    pub(crate) const fn passes(self, value: u32) -> Option<bool> {
        Some(match self {
            Self::IsSet => value != 0,
            Self::AtLeast(count) => value >= count,
            Self::Equals(count) => value == count,
            Self::Below(count) => value < count,
            Self::HasFlag(bits) => value & bits != 0,
            Self::Holds | Self::Fails | Self::Unknown => return None,
        })
    }
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

    /// Whether the game query `query` holds: one of the game's named
    /// queries, as its quest steps and recipes name them.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::knowledge::{GameQuery, KnowledgeQuery, KnowledgeTest};
    ///
    /// let met = KnowledgeQuery::game(GameQuery::new(0x5e11_c0de));
    /// assert_eq!(met.test, KnowledgeTest::Holds);
    /// assert_eq!(met.not().map(|q| q.test), Some(KnowledgeTest::Fails));
    /// ```
    #[must_use]
    pub const fn game(query: GameQuery) -> Self {
        Self {
            id: KnowledgeId::new(query.get()),
            scope: Scope::World,
            test: KnowledgeTest::Holds,
        }
    }

    /// The query that holds where this one does not; `None` for a test the
    /// game has no opposite of: an exact count other than 0, flags, or an
    /// unknown form.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::knowledge::{KnowledgeQuery, KnowledgeTest};
    /// use enshwick::knowledge::{KnowledgeId, Scope};
    ///
    /// let unseen = KnowledgeQuery::is_set(KnowledgeId::new(3), Scope::Player).not();
    /// assert_eq!(unseen.map(|q| q.test), Some(KnowledgeTest::Equals(0)));
    /// ```
    #[must_use]
    pub const fn not(self) -> Option<Self> {
        let test = match self.test {
            KnowledgeTest::IsSet => KnowledgeTest::Equals(0),
            KnowledgeTest::Equals(0) => KnowledgeTest::IsSet,
            KnowledgeTest::AtLeast(count) => KnowledgeTest::Below(count),
            KnowledgeTest::Below(count) => KnowledgeTest::AtLeast(count),
            KnowledgeTest::Holds => KnowledgeTest::Fails,
            KnowledgeTest::Fails => KnowledgeTest::Holds,
            KnowledgeTest::Equals(_) | KnowledgeTest::HasFlag(_) | KnowledgeTest::Unknown => {
                return None;
            }
        };
        Some(Self { test, ..self })
    }

    /// The game's query data, in the forms the game's own steps take
    /// (server 387): a bool equal to 1, a count compared with its operator
    /// (at least `n` as greater than `n - 1`), flags equal to their bits,
    /// and a game query equal to 1 or 0.
    pub(crate) fn to_value(self) -> Result<Value, Off> {
        let refuse = |why: &str| Err(Off::new(format!("knowledge query {}: {why}", self.id)));
        let (kind, operator, compare) = match self.test {
            KnowledgeTest::IsSet => ("SimpleBool", "Equals", 1),
            KnowledgeTest::AtLeast(0) | KnowledgeTest::Below(0) => {
                return refuse("counts start at 1");
            }
            KnowledgeTest::HasFlag(0) => return refuse("flags need the bits they test"),
            KnowledgeTest::Unknown => return refuse("a form enshwick does not write"),
            KnowledgeTest::AtLeast(count) => ("SimpleCount", "GreaterThan", count - 1),
            KnowledgeTest::Equals(count) => ("SimpleCount", "Equals", count),
            KnowledgeTest::Below(count) => ("SimpleCount", "LessThan", count),
            KnowledgeTest::HasFlag(bits) => ("SimpleFlag", "Equals", bits),
            KnowledgeTest::Holds => ("Extern", "Equals", 1),
            KnowledgeTest::Fails => ("Extern", "Equals", 0),
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

    /// The query the game's query data `data` asks, `None` for one that
    /// asks nothing (id 0, as a lore page's entries are never done). A form
    /// enshwick does not read, one that always or never holds or that the
    /// build adds, reads as [`KnowledgeTest::Unknown`]. A Simple query's
    /// entry is the player's when the query says so, else the world's: the
    /// game reads a player's entry first in either case.
    pub(crate) fn from_data(data: &KnowledgeQueryData) -> Option<Self> {
        let id = data.knowledge_or_query_id.value;
        if id == 0 {
            return None;
        }
        let value = data.compare_value;
        let compare = |held: u32| match data.compare_operator {
            CompareOperator::GreaterThan => Some(held > value),
            CompareOperator::LessThan => Some(held < value),
            CompareOperator::Equals => Some(held == value),
            _ => None,
        };
        let test = match data.r#type {
            KnowledgeQueryDataType::SimpleBool => Some(KnowledgeTest::IsSet),
            KnowledgeQueryDataType::SimpleFlag if value != 0 => Some(KnowledgeTest::HasFlag(value)),
            KnowledgeQueryDataType::SimpleCount => match data.compare_operator {
                CompareOperator::GreaterThan => value.checked_add(1).map(KnowledgeTest::AtLeast),
                CompareOperator::LessThan if value > 0 => Some(KnowledgeTest::Below(value)),
                CompareOperator::Equals => Some(KnowledgeTest::Equals(value)),
                _ => None,
            },
            // A game query's state is 0 or 1.
            KnowledgeQueryDataType::Extern => match (compare(1), compare(0)) {
                (Some(true), Some(false)) => Some(KnowledgeTest::Holds),
                (Some(false), Some(true)) => Some(KnowledgeTest::Fails),
                _ => None,
            },
            _ => None,
        };
        let scope = if data.is_explicit_player_knowledge_query {
            Scope::Player
        } else {
            Scope::World
        };
        Some(Self {
            id: KnowledgeId::new(id),
            scope,
            test: test.unwrap_or(KnowledgeTest::Unknown),
        })
    }
}

/// A query of the game's query database: one of the game's named queries
/// or a mod's, by its id.
///
/// The name is enshwick's; the game's is `GameKnowledgeQueryResource`.
///
/// # Examples
///
/// See [`KnowledgeQuery::game`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
pub struct GameQuery(u32);

impl GameQuery {
    /// The game query of id `id`.
    ///
    /// # Examples
    ///
    /// See [`KnowledgeQuery::game`].
    #[must_use]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Its id.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::knowledge::GameQuery;
    ///
    /// assert_eq!(GameQuery::new(7).get(), 7);
    /// ```
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
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

    /// Its knowledge id.
    fn key(self) -> u32 {
        KnowledgeId::from_guid(self.id.guid()).get()
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

/// The knowledge entries a mod added, which one patch of the game's
/// knowledge serves, read when the game first builds it.
#[derive(Debug)]
pub(crate) struct AddedKnowledge {
    /// The ids of the game's own entries, in both maps, sorted.
    vanilla: Box<[u32]>,
    /// The mod's entries, in the order it added them.
    added: Arc<Mutex<Vec<NewKnowledge>>>,
}

impl AddedKnowledge {
    /// Reads the ids of the game's knowledge `resource` and serves the
    /// patch that adds the mod's entries; an entry another mod's took is
    /// logged and left out.
    fn serve(host: Host, resource: crate::abi::ResourceKey) -> Result<Self, Off> {
        let mut vanilla = known_ids(&host.game_value(resource)?);
        vanilla.sort_unstable();
        let added = serve_list(host, resource, move |value, added| {
            let (value, refused) = add_to(value, added)?;
            for off in refused {
                host.log(Level::WARN, &off.to_string());
            }
            Ok(value)
        })?;
        Ok(Self {
            vanilla: vanilla.into(),
            added,
        })
    }
}

/// Serves one patch of the resource `key` that makes all of a mod's
/// changes `T` with `apply` each time the game builds it, one decode and
/// one encode however many there are, and returns the list the mod adds
/// its changes to.
pub(crate) fn serve_list<T: Send + 'static>(
    host: Host,
    key: crate::abi::ResourceKey,
    apply: impl Fn(Value, &[T]) -> Result<Value, Off> + Send + Sync + 'static,
) -> Result<Arc<Mutex<Vec<T>>>, Off> {
    let list = Arc::new(Mutex::new(Vec::new()));
    let served = Arc::clone(&list);
    crate::resources::serve_value(host, key, key, move |value| apply(value, &served.relock()))?;
    Ok(list)
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

/// The keys of the map `map` of `knowledge`, empty for no such map.
fn keys_in<'a>(knowledge: &'a Value, map: &str) -> &'a [Value] {
    match knowledge.field(map).and_then(|map| map.field(KEYS)) {
        Some(Value::List(keys)) => keys,
        _ => &[],
    }
}

/// The ids the world and the player map of `knowledge` hold.
fn known_ids(knowledge: &Value) -> Vec<u32> {
    [WORLD_KNOWLEDGE, PLAYER_KNOWLEDGE]
        .iter()
        .flat_map(|map| keys_in(knowledge, map))
        .filter_map(key_of)
        .collect()
}

/// Whether the world or the player map of `knowledge` holds `key`.
fn knows(knowledge: &Value, key: u32) -> bool {
    [WORLD_KNOWLEDGE, PLAYER_KNOWLEDGE]
        .iter()
        .any(|map| knows_in(knowledge, map, key))
}

/// Whether the map `map` of `knowledge` holds `key`.
fn knows_in(knowledge: &Value, map: &str, key: u32) -> bool {
    keys_in(knowledge, map)
        .iter()
        .any(|held| key_of(held) == Some(key))
}

/// The list in field `name` of `value`, taken out of it.
fn take_list(value: &mut Value, name: &str) -> Result<Vec<Value>, Off> {
    match value.field_mut(name) {
        Some(Value::List(list)) => Ok(core::mem::take(list)),
        _ => Err(Off::new(format!("knowledge map: no list of {name}"))),
    }
}

/// Inserts the entries `added`, each a value under its key, into the static
/// hash map `map` in one rebuild: the keys and values ordered by bucket
/// again, each bucket's run of them counted again, as the game's lookup
/// reads them (a key's bucket is the key masked by the bucket count less
/// one, server 387).
fn insert(map: &mut Value, added: Vec<(u32, Value)>) -> Result<(), Off> {
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
    let mut pairs = Vec::with_capacity(keys.len() + added.len());
    for (held, entry) in keys.into_iter().zip(values) {
        let id = key_of(&held).ok_or_else(|| Off::new("knowledge map: a key holds no id"))?;
        pairs.push((id, held, entry));
    }
    pairs.extend(
        added
            .into_iter()
            .map(|(key, value)| (key, knowledge_id(key), value)),
    );
    let mut ids: Vec<u32> = pairs.iter().map(|(id, ..)| *id).collect();
    ids.sort_unstable();
    if let Some(&[twice, _]) = ids.windows(2).find(|pair| pair[0] == pair[1]) {
        return Err(Off::new(format!("knowledge {twice}: the game knows it")));
    }
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

/// `knowledge` with the entries `added` in its maps, one rebuild of each
/// map for all of them, and their skill point unlocks in the list the game
/// keeps ordered by id (server 387). An entry either map of the game holds
/// already, or whose key an entry before it took, is left out and returned
/// with the reason, so the rest still serve.
fn add_to(mut knowledge: Value, added: &[NewKnowledge]) -> Result<(Value, Vec<Off>), Off> {
    let mut refused = Vec::new();
    let mut kept: Vec<&NewKnowledge> = Vec::with_capacity(added.len());
    for player in [false, true] {
        let map = if player {
            PLAYER_KNOWLEDGE
        } else {
            WORLD_KNOWLEDGE
        };
        let mut entries = Vec::new();
        for new in added
            .iter()
            .filter(|new| (new.scope == Scope::Player) == player)
        {
            let key = new.key();
            if knows(&knowledge, key) || kept.iter().any(|held| held.key() == key) {
                refused.push(Off::new(format!("knowledge {}: the game knows it", new.id)));
                continue;
            }
            entries.push((key, new.to_value(key)?));
            kept.push(new);
        }
        if entries.is_empty() {
            continue;
        }
        let held = knowledge
            .field_mut(map)
            .ok_or_else(|| Off::new(format!("knowledge: no {map}")))?;
        insert(held, entries)?;
    }
    let mut points = kept
        .iter()
        .filter(|new| new.skill_points > 0)
        .map(|new| {
            Value::Struct(vec![
                (KNOWLEDGE_ID.into(), knowledge_id(new.key())),
                (AMOUNT.into(), Value::Uint(new.skill_points.into())),
            ])
        })
        .peekable();
    if points.peek().is_some() {
        let Some(Value::List(unlocks)) = knowledge.field_mut(SKILL_POINTS) else {
            return Err(Off::new("knowledge: no list of skill points"));
        };
        unlocks.extend(points);
        unlocks
            .sort_by_cached_key(|unlock| unlock.field(KNOWLEDGE_ID).and_then(key_of).unwrap_or(0));
    }
    Ok((knowledge, refused))
}

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::add_knowledge`.
    #[doc(hidden)]
    fn add_knowledge(&mut self, new: NewKnowledge) -> Result<KnowledgeId, Off>;

    /// As `Load::add_experience`.
    #[doc(hidden)]
    fn add_experience(&mut self, new: NewExperience) -> Result<ExperienceValue, Off>;

    /// As `Load::added_knowledge`.
    #[doc(hidden)]
    fn added_knowledge(&mut self) -> Result<&AddedKnowledge, Off>;

    /// As `Load::knowledge_without`.
    #[doc(hidden)]
    fn knowledge_without(&mut self, id: Id) -> Result<(), Off>;

    /// As `Load::serve_knowledge`.
    #[doc(hidden)]
    fn serve_knowledge(&mut self, new: NewKnowledge) -> Result<KnowledgeId, Off>;
}

impl LoadMethods for BaseLoad {
    fn add_knowledge(&mut self, new: NewKnowledge) -> Result<KnowledgeId, Off> {
        new.to_value(KnowledgeId::from_guid(new.id.guid()).get())?;
        self.knowledge_without(new.id)?;
        self.declare(new.id)?;
        self.serve_knowledge(new)
    }

    fn add_experience(&mut self, new: NewExperience) -> Result<ExperienceValue, Off> {
        experience::add(self, new)
    }

    fn added_knowledge(&mut self) -> Result<&AddedKnowledge, Off> {
        if self.slot::<AddedKnowledge>().is_none() {
            let resource = GAME_KNOWLEDGE.key_by_hash(self.hash(GameKnowledgeResource::ID)?);
            *self.slot() = Some(AddedKnowledge::serve(self.host(), resource)?);
        }
        self.slot::<AddedKnowledge>()
            .as_ref()
            .ok_or_else(|| Off::new("knowledge: the mod's entries are not served"))
    }

    fn knowledge_without(&mut self, id: Id) -> Result<(), Off> {
        let key = KnowledgeId::from_guid(id.guid()).get();
        if self.added_knowledge()?.vanilla.binary_search(&key).is_ok() {
            return Err(Off::new(format!("knowledge {id}: the game knows it")));
        }
        Ok(())
    }

    fn serve_knowledge(&mut self, new: NewKnowledge) -> Result<KnowledgeId, Off> {
        self.added_knowledge()?.added.relock().push(new);
        Ok(KnowledgeId::new(new.key()))
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
        let added = keys.iter().map(|&key| (key, Value::Uint(key.into())));
        insert(&mut map, added.collect()).unwrap();
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
        assert_eq!(known_ids(&knowledge), [3, 8]);
        assert!(known_ids(&Value::Struct(Vec::new())).is_empty());
    }

    #[test]
    fn a_key_the_map_holds_or_given_twice_is_refused() {
        let mut map = map(4, &[2]);
        assert!(insert(&mut map.clone(), vec![(2, Value::Nil)]).is_err());
        assert!(insert(&mut map.clone(), vec![(5, Value::Nil), (5, Value::Nil)]).is_err());
        let mut odd = map.clone();
        if let Some(entries) = odd.field_mut(ENTRIES) {
            *entries = Value::List(vec![Value::Nil; 3]);
        }
        assert!(
            insert(&mut odd, vec![(9, Value::Nil)]).is_err(),
            "3 buckets"
        );
        insert(&mut map, vec![(9, Value::Nil), (13, Value::Nil)]).unwrap();
        assert_eq!(find(&map, 2), Some(2), "kept");
    }

    fn knowledge(player: &[u32], unlocks: &[u32]) -> Value {
        let unlock = |key: u32| {
            Value::Struct(vec![
                (KNOWLEDGE_ID.into(), knowledge_id(key)),
                (AMOUNT.into(), Value::Uint(1)),
            ])
        };
        Value::Struct(vec![
            (WORLD_KNOWLEDGE.into(), map(4, &[])),
            (PLAYER_KNOWLEDGE.into(), map(4, player)),
            (
                SKILL_POINTS.into(),
                Value::List(unlocks.iter().copied().map(unlock).collect()),
            ),
        ])
    }

    fn unlock_ids(knowledge: &Value) -> Vec<u32> {
        let Some(Value::List(unlocks)) = knowledge.field(SKILL_POINTS) else {
            panic!("{knowledge:?}")
        };
        unlocks
            .iter()
            .filter_map(|u| u.field(KNOWLEDGE_ID).and_then(key_of))
            .collect()
    }

    #[test]
    fn skill_points_join_the_unlocks_in_id_order() {
        let new = |name, points| NewKnowledge {
            skill_points: points,
            ..NewKnowledge::new(Id::new(name), Scope::Player, KnowledgeType::Bool)
        };
        let (a, b) = (new("you.lore:a", 2), new("you.lore:b", 1));
        let (low, high) = (a.key().min(b.key()) - 1, a.key().max(b.key()) + 1);
        let (added, refused) = add_to(knowledge(&[low], &[low, high]), &[a, b]).unwrap();
        assert!(refused.is_empty(), "{refused:?}");
        let mut sorted = vec![low, a.key(), b.key(), high];
        sorted.sort_unstable();
        assert_eq!(unlock_ids(&added), sorted);
    }

    #[test]
    fn an_entry_the_other_map_holds_is_left_out_and_the_rest_serve() {
        let player = NewKnowledge::new(Id::new("you.lore:p"), Scope::Player, KnowledgeType::Bool);
        let world = NewKnowledge::new(Id::new("you.lore:w"), Scope::World, KnowledgeType::Bool);
        let (added, refused) = add_to(knowledge(&[world.key()], &[]), &[world, player]).unwrap();
        assert_eq!(refused.len(), 1, "{refused:?}");
        assert!(
            refused[0].to_string().contains("you.lore:w"),
            "{}",
            refused[0]
        );
        assert!(knows_in(&added, PLAYER_KNOWLEDGE, player.key()));
        assert!(!knows_in(&added, WORLD_KNOWLEDGE, world.key()));
    }

    #[test]
    fn an_entry_its_own_map_holds_is_left_out_and_the_rest_serve() {
        let taken = NewKnowledge::new(Id::new("you.lore:t"), Scope::Player, KnowledgeType::Bool);
        let free = NewKnowledge::new(Id::new("you.lore:f"), Scope::Player, KnowledgeType::Bool);
        let (added, refused) =
            add_to(knowledge(&[taken.key()], &[]), &[taken, free, free]).unwrap();
        assert_eq!(
            refused.len(),
            2,
            "the taken one and the second free: {refused:?}"
        );
        assert!(
            refused[0].to_string().contains("you.lore:t"),
            "{}",
            refused[0]
        );
        assert!(knows_in(&added, PLAYER_KNOWLEDGE, free.key()));
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
}
