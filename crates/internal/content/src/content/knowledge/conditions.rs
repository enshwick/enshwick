//! The tests of knowledge entries that quests, the journal and rules hold.

use crate::Off;
use crate::knowledge::{KnowledgeId, Scope};
use crate::resources::Value;
use crate::typed::{CompareOperator, KnowledgeQueryData, KnowledgeQueryDataType};

use super::{GameQuery, hash_key};

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
    /// The game query of the id holds. The game reads only the query's own
    /// answer here and no compare (server 26430253), so a query that holds
    /// where another fails is a query of its own with `invert`.
    Holds,
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
            Self::Holds | Self::Unknown => return None,
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
#[non_exhaustive]
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
    /// A `KnowledgeQuery` of its parts.
    ///
    /// # Examples
    ///
    /// See [`KnowledgeQuery`].
    #[must_use]
    pub const fn new(id: KnowledgeId, scope: Scope, test: KnowledgeTest) -> Self {
        Self { id, scope, test }
    }
}

impl KnowledgeQuery {
    /// Whether the bool `id` is set.
    ///
    /// # Examples
    ///
    /// See the [module](super).
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
    /// assert_eq!(met.not(), None);
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
    /// game has no opposite of: an exact count other than 0, flags, a game
    /// query, or an unknown form.
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
            KnowledgeTest::Equals(_)
            | KnowledgeTest::HasFlag(_)
            | KnowledgeTest::Holds
            | KnowledgeTest::Unknown => {
                return None;
            }
        };
        Some(Self { test, ..self })
    }

    /// The game's query data, in the forms the game's own steps take
    /// (server 387): a bool equal to 1, a count compared with its operator
    /// (at least `n` as greater than `n - 1`), flags equal to their bits,
    /// and a game query equal to 0, the compare the game writes on every
    /// one and reads on none.
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
            KnowledgeTest::Holds => ("Extern", "Equals", 0),
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
        let test = match data.r#type {
            KnowledgeQueryDataType::SimpleBool => Some(KnowledgeTest::IsSet),
            KnowledgeQueryDataType::SimpleFlag if value != 0 => Some(KnowledgeTest::HasFlag(value)),
            KnowledgeQueryDataType::SimpleCount => match data.compare_operator {
                CompareOperator::GreaterThan => value.checked_add(1).map(KnowledgeTest::AtLeast),
                CompareOperator::LessThan if value > 0 => Some(KnowledgeTest::Below(value)),
                CompareOperator::Equals => Some(KnowledgeTest::Equals(value)),
                _ => None,
            },
            // The game's evaluator reads the query's state bit alone and neither the
            // compare value nor the operator (FUN_140a284c0, server 26430253).
            KnowledgeQueryDataType::Extern => Some(KnowledgeTest::Holds),
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
