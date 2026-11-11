//! The journal's quests and lore pages, mods' included, and where each
//! player and the world stand in each quest.
//!
//! [`Load::quests`] gives the [`Quests`] of the running game. On first use
//! they read the journal as the game's first read builds it, every mod's
//! quests and changes in, and keep it for the process, as the game keeps
//! the journal it first built; they answer a quest's state from the
//! knowledge its steps ask for: a step shows while its `shown_when` holds
//! and is done once its `done_when` holds; the game pays a quest when its
//! last step is done and sets the quest's own bool.
//!
//! ```
//! use enshwick::quests::Quests;
//!
//! fn report(quests: &Quests, player: enshwick::Entity) {
//!     for quest in quests.all() {
//!         if quests.state(player, quest.id).is_some_and(|state| state.rewarded) {
//!             enshwick::info!("quest {} done, {} steps", quest.id, quest.steps.len());
//!         }
//!     }
//! }
//! ```

use std::sync::{Arc, Mutex};

use crate::content::EngineFacts as _;
use crate::content::journal::{JOURNAL_REGISTRY, QuestReward};
use crate::content::knowledge::{ExperienceValue, KnowledgeQuery, KnowledgeTest};
use crate::locks::Relock as _;
use crate::resources::Value;
use crate::typed::{
    JournalCollectionResource, JournalEntryResource, JournalQuestResource, JournalRegistryResource,
    Named as _, QuestSourceType, QuestType, Reflected,
};
use crate::{Entity, ItemId, Load, Off};

use crate::knowledge::{Knowledge, KnowledgeId, Scope};

/// The key of a text the journal names, the FNV-1a of its text tag's GUID.
///
/// # Examples
///
/// ```
/// use enshwick::content::Guid;
/// use enshwick::quests::TextKey;
///
/// let tag = Guid::named("you.trail:trail.name");
/// assert_eq!(TextKey::of(tag).get(), tag.fnv1a());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
pub struct TextKey(u32);

impl TextKey {
    /// The key of the text tag `tag`.
    ///
    /// # Examples
    ///
    /// See [`TextKey`].
    #[must_use]
    pub fn of(tag: crate::content::Guid) -> Self {
        Self(tag.fnv1a())
    }

    /// Its value.
    ///
    /// # Examples
    ///
    /// See [`TextKey`].
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    /// The key `key` holds, `None` for 0, which names no text.
    fn of_key(key: u32) -> Option<Self> {
        (key != 0).then_some(Self(key))
    }
}

/// The id of a journal entry: a quest's step or a lore page and its
/// entries.
///
/// # Examples
///
/// ```
/// use enshwick::quests::EntryId;
///
/// assert_eq!(EntryId::new(9).get(), 9);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
pub struct EntryId(u32);

impl EntryId {
    /// The entry of id `id`.
    ///
    /// # Examples
    ///
    /// See [`EntryId`].
    #[must_use]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Its id.
    ///
    /// # Examples
    ///
    /// See [`EntryId`].
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// A step of a quest or an entry of a lore page, as the journal holds it.
///
/// The name is enshwick's; the game's is `JournalEntryResource`.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct StepInfo {
    /// Its entry id.
    pub entry: EntryId,
    /// Its title's text.
    pub name: Option<TextKey>,
    /// Its text.
    pub text: Option<TextKey>,
    /// When the journal shows it; `None` for always.
    pub shown_when: Option<KnowledgeQuery>,
    /// When it is done; `None` for never, as a lore page's entries.
    pub done_when: Option<KnowledgeQuery>,
    /// The parts the journal counts as it moves on.
    pub progress: Vec<KnowledgeQuery>,
    /// The text tag of the map marker its "show on map" points at.
    pub marker: Option<TextKey>,
    /// The item whose icon it shows.
    pub icon: Option<ItemId>,
    /// The level it suits; 0 for none.
    pub level: u8,
}

impl StepInfo {
    fn of(entry: &JournalEntryResource) -> Self {
        Self {
            entry: EntryId(entry.entry_id.value),
            name: TextKey::of_key(entry.name.value),
            text: TextKey::of_key(entry.text.value),
            shown_when: enshwick_content::__internal::query_from_data(&entry.knowledge_requirement),
            done_when: enshwick_content::__internal::query_from_data(&entry.completion_requirement),
            progress: entry
                .progress_steps_requirement
                .iter()
                .filter_map(KnowledgeQuery::from_data)
                .collect(),
            marker: TextKey::of_key(entry.map_marker_reference.value),
            icon: (entry.item_icon_id.value != 0).then(|| ItemId::new(entry.item_icon_id.value)),
            level: entry.recommended_level,
        }
    }
}

/// A lore page of the journal.
///
/// The name is enshwick's; the game's is `JournalCollectionResource`.
///
/// # Examples
///
/// ```
/// fn titled(quests: &enshwick::quests::Quests) -> usize {
///     quests.pages().iter().filter(|page| page.name.is_some()).count()
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct PageInfo {
    /// Its entry id.
    pub id: EntryId,
    /// The text of the category the journal lists it under.
    pub category: Option<TextKey>,
    /// Its title's text.
    pub name: Option<TextKey>,
    /// Where the journal sorts it among the others.
    pub priority: u32,
    /// Whether it belongs to the tutorial.
    pub tutorial: bool,
    /// Its entries, in order.
    pub entries: Vec<StepInfo>,
}

impl PageInfo {
    fn of(page: &JournalCollectionResource) -> Self {
        Self {
            id: EntryId(page.entry_id.value),
            category: TextKey::of_key(page.lore_category.value),
            name: TextKey::of_key(page.name.value),
            priority: page.priority,
            tutorial: page.is_tutorial,
            entries: page.entries.iter().map(StepInfo::of).collect(),
        }
    }
}

/// A quest of the journal, the game's or a mod's.
///
/// The name is enshwick's; the game's is `JournalQuestResource`.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct QuestInfo {
    /// Its quest id: the player's bool the game sets when it pays it.
    pub id: KnowledgeId,
    /// Its title's text.
    pub name: Option<TextKey>,
    /// Who gives it, which groups it in the journal.
    pub source: QuestSourceType,
    /// Whether it is the world's, each player's, or the game's `Auto`.
    pub kind: QuestType,
    /// Whether the journal lists it as a main quest.
    pub main: bool,
    /// Whether one player's completion unlocks it for every player.
    pub for_all_players: bool,
    /// Whether it belongs to the tutorial.
    pub tutorial: bool,
    /// Where the journal sorts it among the others.
    pub priority: u32,
    /// Its steps, in order.
    pub steps: Vec<StepInfo>,
    /// What the journal shows as its reward, as [`QuestReward`] says the
    /// game gives it; a reward of the game's with an item and experience
    /// reads as two.
    pub rewards: Vec<QuestReward>,
}

impl QuestInfo {
    fn of(quest: &JournalQuestResource) -> Self {
        let page = PageInfo::of(&quest.base);
        let rewards = quest
            .rewards
            .iter()
            .flat_map(|reward| {
                let item = (reward.item_ref != crate::content::Guid([0; 16]))
                    .then_some(QuestReward::Item(reward.item_ref));
                let experience = (reward.experience_value_id.value != 0).then(|| {
                    QuestReward::Experience(ExperienceValue::new(reward.experience_value_id.value))
                });
                item.into_iter().chain(experience)
            })
            .collect();
        Self {
            id: KnowledgeId::new(page.id.get()),
            name: page.name,
            source: quest.source.clone(),
            kind: quest.r#type.clone(),
            main: quest.is_main_quest,
            for_all_players: quest.unlock_for_all_players,
            tutorial: page.tutorial,
            priority: page.priority,
            steps: page.entries,
            rewards,
        }
    }
}

/// Where a player or the world stands in one step; `None` where the answer
/// is not known.
///
/// Not known are a game query the game has not answered (not evaluated
/// yet, or a build without the game's query calls), a form enshwick does
/// not read, and knowledge that does not read, such as a player not in the
/// game.
///
/// # Examples
///
/// See [`QuestState`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct StepState {
    /// Whether the journal shows it.
    pub shown: Option<bool>,
    /// Whether it is done.
    pub done: Option<bool>,
}

/// Where a player or the world stands in a quest, from the knowledge its
/// steps ask for.
///
/// # Examples
///
/// ```
/// fn at(quests: &enshwick::quests::Quests, player: enshwick::Entity, quest: enshwick::knowledge::KnowledgeId) -> Option<usize> {
///     let state = quests.state(player, quest)?;
///     state.steps.iter().position(|step| step.done != Some(true))
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct QuestState {
    /// Each step's, in order.
    pub steps: Vec<StepState>,
    /// Whether its first step shows, so the journal lists it.
    pub started: bool,
    /// Whether its last step is done.
    pub done: bool,
    /// Whether the game paid it: the quest's own bool is set.
    pub rewarded: bool,
}

/// The journal as the game built it this world.
#[derive(Debug, Default)]
struct Journal {
    quests: Vec<QuestInfo>,
    pages: Vec<PageInfo>,
}

impl Journal {
    /// The quests and pages of the journal `registry`; a list that does not
    /// read, such as one whose type a build changed, fails the whole read.
    fn of(registry: &Value) -> Result<Self, Off> {
        fn list<T: Reflected>(registry: &Value, name: &str) -> Result<Vec<T>, Off> {
            match registry.field(name) {
                Some(Value::List(all)) => all
                    .iter()
                    .map(|value| {
                        T::from_value(value)
                            .map_err(|error| Off::new(format!("journal {name}: {error}")))
                    })
                    .collect(),
                _ => Err(Off::new(format!("journal: no list of {name}"))),
            }
        }
        let quests: Vec<JournalQuestResource> = list(registry, QUESTS)?;
        let pages: Vec<JournalCollectionResource> = list(registry, COLLECTIONS)?;
        Ok(Self {
            quests: quests.iter().map(QuestInfo::of).collect(),
            pages: pages.iter().map(PageInfo::of).collect(),
        })
    }
}

/// The game's names of the journal's lists of quests and lore pages.
const QUESTS: &str = "quests";
const COLLECTIONS: &str = "collections";

/// The journal a [`Quests`] read, kept once a read succeeds.
#[derive(Debug, Default)]
struct Read {
    journal: Option<Arc<Journal>>,
    /// The reason of the last failed read logged.
    warned: Option<String>,
}

impl Read {
    /// Whether to log a failed read for `why`: once per reason in a row, so
    /// the expected refusal during init does not hide a later failure.
    fn first(&mut self, why: &str) -> bool {
        if self.warned.as_deref() == Some(why) {
            return false;
        }
        self.warned = Some(why.to_owned());
        true
    }
}

/// The quests and lore pages of the running game and where each player
/// stands in them; get it from [`Load::quests`].
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug)]
pub struct Quests {
    knowledge: Knowledge,
    /// The journal once read; one per mod, for the life of the process.
    journal: &'static Mutex<Read>,
}

impl crate::script::Bound for Quests {
    const NAME: &'static str = "quests";
    const DOC: &'static str = "The journal's quests and lore pages, mods' included, and where each player stands in them.";
    const PARTS: &'static [&'static [crate::script::Function<Self>]] = &[Self::SCRIPT_QUESTS];

    fn resolve(load: &mut Load<'_>) -> Result<Self, Off> {
        Ok(load.quests())
    }
}

impl Quests {
    /// The journal, read once it reads, such as after init; empty until
    /// then, with a warning for each new reason it does not read.
    fn journal(&self) -> Arc<Journal> {
        let mut held = self.journal.relock();
        if let Some(journal) = &held.journal {
            return Arc::clone(journal);
        }
        let host = self.knowledge.host;
        let start = std::time::Instant::now();
        let read = host
            .hash(JournalRegistryResource::ID)
            .map(|hash| JOURNAL_REGISTRY.key_by_hash(hash))
            .and_then(|key| host.served_value(key))
            .and_then(|registry| Journal::of(&registry));
        match read {
            Ok(journal) => {
                host.log(
                    crate::abi::Level::INFO,
                    &format!(
                        "quests: read the journal, {} quests and {} pages, in {} ms",
                        journal.quests.len(),
                        journal.pages.len(),
                        start.elapsed().as_millis()
                    ),
                );
                Arc::clone(held.journal.insert(Arc::new(journal)))
            }
            Err(off) => {
                // Not kept, so a read during init or before the world is tried again.
                let why = off.to_string();
                if held.first(&why) {
                    host.log(crate::abi::Level::WARN, &format!("quests: {why}"));
                }
                Arc::default()
            }
        }
    }

    /// Whether `query` holds for the player `player`, or for the world when
    /// `player` is `None`: a player's entry is read for the player, and for
    /// the world a query of a player's entry does not hold; a game query is
    /// the game's answer. `None` where the answer is not known: knowledge
    /// that does not read, such as a player not in the game, a game query
    /// the game has not answered, or a form enshwick does not read.
    fn holds(&self, player: Option<Entity>, query: KnowledgeQuery) -> Option<bool> {
        if let KnowledgeTest::Holds | KnowledgeTest::Fails = query.test {
            let held = self.game_query(player.unwrap_or(Entity(0)), query.id.get())?;
            return Some(held == (query.test == KnowledgeTest::Holds));
        }
        let known = match (player, query.scope) {
            (Some(player), _) => self.knowledge.get(player, query.id)?,
            (None, Scope::Player) => return Some(false),
            (None, _) => self.knowledge.world(query.id)?,
        };
        enshwick_content::__internal::passes(query.test, known.value)
    }

    /// Whether the game query `query` holds for the character `entity`, or
    /// for the world for `Entity(0)`, as the game answers it.
    fn game_query(&self, entity: Entity, query: u32) -> Option<bool> {
        let host = self.knowledge.host;
        let mut holds = 0u32;
        let status = (host.api().game_query)(host.handle(), entity, query, &raw mut holds);
        (status == crate::abi::Status::OK).then_some(holds != 0)
    }

    fn state_of(&self, player: Option<Entity>, quest: KnowledgeId) -> Option<QuestState> {
        let journal = self.journal();
        let info = journal.quests.iter().find(|info| info.id == quest)?;
        let holds =
            |query: Option<KnowledgeQuery>| query.map_or(Some(true), |q| self.holds(player, q));
        let steps: Vec<StepState> = info
            .steps
            .iter()
            .map(|step| StepState {
                shown: holds(step.shown_when),
                done: step
                    .done_when
                    .map_or(Some(false), |q| self.holds(player, q)),
            })
            .collect();
        let rewarded = player.is_some_and(|player| self.knowledge.completed(player, quest));
        Some(QuestState {
            started: steps.first().is_some_and(|step| step.shown == Some(true)),
            done: steps.last().is_some_and(|step| step.done == Some(true)),
            rewarded,
            steps,
        })
    }
}

#[crate::script(quests)]
impl Quests {
    /// Every quest of the journal, the game's and mods', in its order;
    /// empty until the journal reads, such as during init.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    #[script]
    pub fn all(&self) -> Vec<QuestInfo> {
        self.journal().quests.clone()
    }

    /// The quest `quest`, `None` for no quest of the journal.
    ///
    /// # Examples
    ///
    /// ```
    /// fn steps(quests: &enshwick::quests::Quests, quest: enshwick::knowledge::KnowledgeId) -> usize {
    ///     quests.quest(quest).map_or(0, |info| info.steps.len())
    /// }
    /// ```
    #[must_use]
    #[script]
    pub fn quest(&self, quest: KnowledgeId) -> Option<QuestInfo> {
        self.journal()
            .quests
            .iter()
            .find(|info| info.id == quest)
            .cloned()
    }

    /// Every lore page of the journal, the game's and mods'.
    ///
    /// # Examples
    ///
    /// See [`PageInfo`].
    #[must_use]
    #[script]
    pub fn pages(&self) -> Vec<PageInfo> {
        self.journal().pages.clone()
    }

    /// Where the player whose character is `player` stands in `quest`,
    /// reading the player's entries and the world's as the game does for
    /// its rewards; `None` for no quest of the journal.
    ///
    /// # Examples
    ///
    /// See [`QuestState`].
    #[must_use]
    #[script]
    pub fn state(&self, player: Entity, quest: KnowledgeId) -> Option<QuestState> {
        self.state_of(Some(player), quest)
    }

    /// Where the world stands in `quest`, from the world's entries alone:
    /// a step that asks a player's entry reads as not held. `rewarded` is
    /// a player's and reads `false`.
    ///
    /// # Examples
    ///
    /// ```
    /// fn shown(quests: &enshwick::quests::Quests, quest: enshwick::knowledge::KnowledgeId) -> bool {
    ///     quests.world_state(quest).is_some_and(|state| state.started)
    /// }
    /// ```
    #[must_use]
    #[script]
    pub fn world_state(&self, quest: KnowledgeId) -> Option<QuestState> {
        self.state_of(None, quest)
    }
}

impl Load<'_> {
    /// The [`Quests`] of the running game; every call of one mod gives the
    /// same, which reads the journal once.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> enshwick::quests::Quests {
    ///     load.quests()
    /// }
    /// ```
    #[must_use]
    pub fn quests(&mut self) -> Quests {
        if let Some(quests) = *self.slot::<Quests>() {
            return quests;
        }
        let quests = Quests {
            knowledge: self.knowledge(),
            journal: Box::leak(Box::default()),
        };
        *self.slot() = Some(quests);
        quests
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::knowledge::KnowledgeTest;
    use crate::knowledge::Scope;
    use crate::typed::runtime::Constant;
    use crate::typed::{CompareOperator, KnowledgeQueryData, KnowledgeQueryDataType};

    fn data(
        id: u32,
        kind: KnowledgeQueryDataType,
        op: CompareOperator,
        value: u32,
    ) -> KnowledgeQueryData {
        // Only the generated types' own crate writes one out whole.
        let mut data: KnowledgeQueryData = crate::typed::runtime::blank().unwrap();
        data.knowledge_or_query_id.value = id;
        data.compare_value = value;
        data.compare_operator = op;
        data.r#type = kind;
        data.is_explicit_player_knowledge_query = true;
        data
    }

    #[test]
    fn the_game_s_query_forms_read_as_the_tests_they_ask() {
        use CompareOperator as Op;
        use KnowledgeQueryDataType as Ty;
        let read = |kind, op, value| {
            enshwick_content::__internal::query_from_data(&data(7, kind, op, value)).map(|q| q.test)
        };
        assert_eq!(
            read(Ty::SimpleBool, Op::Equals, 1),
            Some(KnowledgeTest::IsSet)
        );
        assert_eq!(
            read(Ty::SimpleCount, Op::GreaterThan, 24),
            Some(KnowledgeTest::AtLeast(25))
        );
        assert_eq!(
            read(Ty::SimpleCount, Op::Equals, 0),
            Some(KnowledgeTest::Equals(0))
        );
        assert_eq!(
            read(Ty::SimpleCount, Op::LessThan, 3),
            Some(KnowledgeTest::Below(3))
        );
        assert_eq!(
            read(Ty::SimpleFlag, Op::Equals, 2),
            Some(KnowledgeTest::HasFlag(2))
        );
        assert_eq!(read(Ty::Extern, Op::Equals, 1), Some(KnowledgeTest::Holds));
        assert_eq!(
            read(Ty::Extern, Op::GreaterThan, 0),
            Some(KnowledgeTest::Holds)
        );
        assert_eq!(read(Ty::Extern, Op::Equals, 0), Some(KnowledgeTest::Fails));
        assert_eq!(
            enshwick_content::__internal::query_from_data(&data(0, Ty::Extern, Op::GreaterThan, 0)),
            None,
            "a lore entry's never"
        );
    }

    #[test]
    fn a_form_enshwick_does_not_read_is_unknown_not_always_or_never() {
        use CompareOperator as Op;
        use KnowledgeQueryDataType as Ty;
        let read = |kind, op, value| {
            enshwick_content::__internal::query_from_data(&data(7, kind, op, value)).map(|q| q.test)
        };
        for (kind, op, value, why) in [
            (Ty::SimpleCount, Op::LessThan, 0, "never holds"),
            (Ty::SimpleCount, Op::GreaterThan, u32::MAX, "never holds"),
            (Ty::Extern, Op::GreaterThan, 1, "never holds"),
            (Ty::Extern, Op::LessThan, 2, "always holds"),
            (Ty::SimpleFlag, Op::Equals, 0, "no bits"),
            (
                Ty::SimpleCount,
                Op::Other(Constant::Number(9)),
                1,
                "an operator the build adds",
            ),
        ] {
            assert_eq!(read(kind, op, value), Some(KnowledgeTest::Unknown), "{why}");
        }
        assert_eq!(
            enshwick_content::__internal::passes(KnowledgeTest::Unknown, 0),
            None
        );
        let unknown = KnowledgeQuery {
            id: KnowledgeId::new(7),
            scope: Scope::World,
            test: KnowledgeTest::Unknown,
        };
        assert_eq!(unknown.not(), None);
    }

    #[test]
    fn every_form_mods_write_reads_back_as_itself() {
        let id = KnowledgeId::new(11);
        for test in [
            KnowledgeTest::IsSet,
            KnowledgeTest::AtLeast(1),
            KnowledgeTest::AtLeast(25),
            KnowledgeTest::Equals(0),
            KnowledgeTest::Equals(4),
            KnowledgeTest::Below(2),
            KnowledgeTest::HasFlag(0b101),
            KnowledgeTest::Holds,
            KnowledgeTest::Fails,
        ] {
            let query = KnowledgeQuery {
                id,
                scope: Scope::Player,
                test,
            };
            let value = query.to_value().unwrap();
            let data = KnowledgeQueryData::from_value(&value).unwrap();
            assert_eq!(
                enshwick_content::__internal::query_from_data(&data),
                Some(query),
                "{test:?}"
            );
            if let Some(not) = query.not() {
                assert_eq!(not.not(), Some(query), "{test:?}");
                for held in [0, 1, 2, 3, 24, 25, 26] {
                    if let (Some(a), Some(b)) = (
                        enshwick_content::__internal::passes(test, held),
                        enshwick_content::__internal::passes(not.test, held),
                    ) {
                        assert_ne!(a, b, "{test:?} at {held}");
                    }
                }
            }
        }
        for refused in [
            KnowledgeTest::AtLeast(0),
            KnowledgeTest::Below(0),
            KnowledgeTest::HasFlag(0),
            KnowledgeTest::Unknown,
        ] {
            let query = KnowledgeQuery {
                id,
                scope: Scope::World,
                test: refused,
            };
            assert!(query.to_value().is_err(), "{refused:?}");
        }
    }

    #[test]
    fn a_failed_read_warns_once_per_reason_in_a_row() {
        let mut read = Read::default();
        assert!(read.first("refused during init"));
        assert!(!read.first("refused during init"));
        assert!(
            read.first("journal quests: a field the build changed"),
            "a later failure is not hidden by the init one"
        );
        assert!(!read.first("journal quests: a field the build changed"));
    }

    #[test]
    fn a_quest_reads_its_steps_rewards_and_lore_fields() {
        let new = crate::content::journal::NewQuest {
            rewards: vec![QuestReward::Experience(ExperienceValue::new(9))],
            ..crate::content::journal::NewQuest::new(
                crate::content::Id::new("you.trail:trail"),
                crate::content::Guid([5; 16]),
                vec![crate::content::journal::QuestStep::new(
                    crate::content::Guid([6; 16]),
                    crate::content::Guid([7; 16]),
                    KnowledgeQuery::is_set(KnowledgeId::new(3), Scope::Player),
                    KnowledgeQuery::at_least(KnowledgeId::new(4), Scope::World, 5),
                )],
            )
        };
        let value =
            crate::content::journal::quest_value(&new, crate::content::knowledge::hash_key(0))
                .unwrap();
        let quest = QuestInfo::of(&JournalQuestResource::from_value(&value).unwrap());
        assert_eq!(quest.id, KnowledgeId::from_guid(new.id.guid()));
        assert_eq!(quest.name, Some(TextKey::of(crate::content::Guid([5; 16]))));
        assert_eq!(quest.rewards, new.rewards);
        assert_eq!(quest.steps.len(), 1);
        let step = &quest.steps[0];
        assert_eq!(step.shown_when, Some(new.steps[0].shown_when));
        assert_eq!(step.done_when, Some(new.steps[0].done_when));
        assert_eq!(step.marker, None);
    }
}
