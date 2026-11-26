//! Quest step objectives the journal counts: each a counter of the
//! loader's that the knowledge domain raises on the events it already gets.

use crate::content::knowledge::KnowledgeQuery;
use crate::content::{Guid, Id};
use crate::knowledge::{KnowledgeId, Scope};
use crate::typed::FactionType;
use crate::{ItemId, Off, RecipeId};

/// What a player does that an [`Objective`] counts, for each
/// [`QuestStep`](super::QuestStep) that lists it.
///
/// The name is enshwick's, not the game's.
///
/// # Examples
///
/// ```
/// use enshwick::content::id;
/// use enshwick::content::journal::{CraftTarget, NewQuest, Objective, QuestStep};
///
/// fn init(load: &mut enshwick::Load<'_>, bow: enshwick::RecipeId) -> Result<(), enshwick::Off> {
///     let mut step = QuestStep::owned(id!("bows.make").guid(), id!("bows.make.text").guid());
///     step.objectives = vec![Objective::Craft { target: CraftTarget::Recipe(bow), count: 2 }];
///     load.add_quest(NewQuest::new(id!("bows"), id!("bows.name").guid(), vec![step]))?;
///     Ok(())
/// }
/// ```
#[derive(Clone, Debug, PartialEq, crate::Script)]
#[non_exhaustive]
pub enum Objective {
    /// Holds `count` of the item, counted from the inventory each time the
    /// player picks one up.
    Collect {
        /// The item.
        item: ItemId,
        /// How many, from 1.
        count: u32,
    },
    /// Kills `count` enemies of the target, credited to the player the game
    /// gives the kill's experience.
    Kill {
        /// What it kills.
        target: KillTarget,
        /// How many, from 1.
        count: u32,
    },
    /// Crafts `count` of the target, credited as a reward for crafting is.
    Craft {
        /// What it crafts.
        target: CraftTarget,
        /// How many, from 1.
        count: u32,
    },
    /// Places `count` of the item as a building or a prop.
    Build {
        /// The item placed.
        item: ItemId,
        /// How many, from 1.
        count: u32,
    },
    /// Comes within `radius` metres of `at`.
    Reach {
        /// The place, in metres.
        at: [f32; 3],
        /// How near, in metres, above 0.
        radius: f32,
    },
    /// Gets `count` from the mod's own signal `signal`, which
    /// [`Quests::signal`](../../quests/struct.Quests.html#method.signal)
    /// sends.
    Custom {
        /// The signal's content id.
        signal: Id,
        /// How many, from 1.
        count: u32,
    },
}

/// What a [`Objective::Kill`] counts.
///
/// The name is enshwick's, not the game's.
///
/// # Examples
///
/// ```
/// use enshwick::content::journal::{KillTarget, Objective};
/// use enshwick::typed::FactionType;
///
/// let vukah = Objective::Kill { target: KillTarget::Faction(FactionType::Vukah), count: 5 };
/// assert_eq!(vukah.count(), 5);
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub enum KillTarget {
    /// An entity of this template.
    Template(Guid),
    /// An entity of this faction.
    Faction(FactionType),
}

/// What a [`Objective::Craft`] counts.
///
/// The name is enshwick's, not the game's.
///
/// # Examples
///
/// See [`Objective`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub enum CraftTarget {
    /// This recipe.
    Recipe(RecipeId),
    /// Any vanilla recipe that makes this item; a mod's own recipe counts
    /// as [`CraftTarget::Recipe`].
    Item(ItemId),
}

impl Objective {
    /// The count it asks for: 1 for [`Objective::Reach`].
    ///
    /// # Examples
    ///
    /// See [`KillTarget`].
    #[must_use]
    pub const fn count(&self) -> u32 {
        match *self {
            Self::Collect { count, .. }
            | Self::Kill { count, .. }
            | Self::Craft { count, .. }
            | Self::Build { count, .. }
            | Self::Custom { count, .. } => count,
            Self::Reach { .. } => 1,
        }
    }

    /// Why the loader cannot count it, `None` when it can.
    pub(crate) fn problem(&self) -> Option<&'static str> {
        match *self {
            Self::Reach { at, radius } => {
                (!(radius > 0.0 && radius.is_finite() && at.iter().all(|v| v.is_finite())))
                    .then_some("a place needs finite coordinates and a radius above 0")
            }
            _ if self.count() == 0 => Some("counts start at 1"),
            _ => None,
        }
    }
}

/// A quest step whose objectives the loader counts, as `Load::add_quest`
/// filled it.
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "the content crate builds it and the knowledge crate reads it; no mod names it"
)]
pub struct CountedStep {
    /// The quest.
    pub quest: KnowledgeId,
    /// The step, from 0.
    pub step: u32,
    /// Whose the counters are: each player's for a player's quest, the
    /// world's for a world quest.
    pub scope: Scope,
    /// When the step shows.
    pub shown_when: KnowledgeQuery,
    /// When it is done.
    pub done_when: KnowledgeQuery,
    /// The step's own bool, set once every counter reaches its count; `None`
    /// when its one objective's count is its `done_when`.
    pub done_bool: Option<KnowledgeId>,
    /// Each objective with its counter.
    pub objectives: Vec<(Objective, KnowledgeId)>,
}

/// The steps with objectives a mod added, which the knowledge domain counts.
#[derive(Debug, Default)]
pub(crate) struct Counted(pub(crate) Vec<CountedStep>);

/// Checks `objectives`, with the step `at`'s name for a refusal.
pub(crate) fn check(quest: Id, at: usize, objectives: &[Objective]) -> Result<(), Off> {
    match objectives.iter().find_map(Objective::problem) {
        Some(why) => Err(Off::new(format!(
            "quest {quest} step {at}: objective: {why}"
        ))),
        None => Ok(()),
    }
}
