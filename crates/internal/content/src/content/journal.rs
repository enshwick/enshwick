//! Quests and journal pages from mods, and changes to the game's quests.
//!
//! A quest is steps that show and complete on knowledge, and rewards: the
//! game gives their experience when the last step completes and only draws
//! their items, which the mod gives itself (see [`QuestReward`]).
//! [`Load::add_quest`](../../struct.Load.html#method.add_quest) adds one to
//! the journal beside the game's, and a mod moves it on by granting the
//! knowledge its steps ask for. `Load::add_journal_page` adds a lore page
//! whose entries show on knowledge. [`Load::change_quest`](../../struct.Load.html#method.change_quest) changes a
//! vanilla quest's rewards, who it unlocks for and its steps' knowledge.
//! The journal, its tracker and its notices are the game's own screens: a
//! client draws a mod's quest from the data when it runs the mod too.
//!
//! ```
//! use enshwick::content::{Guid, id};
//! use enshwick::content::journal::{NewQuest, QuestReward, QuestStep};
//! use enshwick::content::knowledge::{KnowledgeQuery, NewKnowledge};
//! use enshwick::knowledge::{KnowledgeType, Scope};
//!
//! fn init(load: &mut enshwick::Load<'_>, torch: Guid) -> Result<(), enshwick::Off> {
//!     let given = load.add_knowledge(NewKnowledge::new(id!("trail-given"), Scope::Player, KnowledgeType::Bool))?;
//!     let found = load.add_knowledge(NewKnowledge::new(id!("trail-found"), Scope::Player, KnowledgeType::Bool))?;
//!     let step = QuestStep::new(
//!         id!("trail.step").guid(),
//!         id!("trail.step.text").guid(),
//!         KnowledgeQuery::is_set(given, Scope::Player),
//!         KnowledgeQuery::is_set(found, Scope::Player),
//!     );
//!     load.add_quest(NewQuest {
//!         rewards: vec![QuestReward::Item(torch)],
//!         ..NewQuest::new(id!("trail"), id!("trail.name").guid(), vec![step])
//!     })?;
//!     Ok(())
//! }
//! ```

use crate::__load::*;
use std::sync::{Arc, Mutex};

use crate::abi::Level;
use crate::content::knowledge::{
    ExperienceValue, KnowledgeQuery, NewKnowledge, hash_key, key_of_hash, serve_list,
};
use crate::knowledge::{KnowledgeId, KnowledgeType, Scope};
use crate::locks::Relock as _;
use crate::resources::Value;
use crate::typed::{
    JournalRegistryResource, Named as _, QuestSourceType, QuestType, Reflected as _,
};
use crate::{Host, ItemId, Off};

use super::{EngineFacts, Guid, Id};

mod change;
pub mod item_set_category;
pub(crate) mod item_sets;
#[cfg(feature = "unstable")]
pub mod lore_category;
#[cfg(feature = "typed")]
pub(crate) mod objectives;

pub use change::{PageChange, QuestChange, StepChange, StepEdit, StepInsert, StepMove};
use change::{change_page_in, change_quest_in, remove_page_in, remove_quest_in};
pub use item_sets::{
    ItemSetCategory, ItemSetChange, ItemSetEntry, ItemSetHint, ItemSetId, NewItemSet,
};

#[cfg(feature = "typed")]
#[doc(hidden)]
pub use objectives::CountedStep;
#[cfg(feature = "typed")]
pub use objectives::{CraftTarget, KillTarget, Objective};

/// The game's journal: every quest, lore page and item set (server 387 and
/// 399, `JournalRegistryResource`).
pub const JOURNAL_REGISTRY: Guid = Guid::parse("33701b26-ec1d-423f-8e06-49f023b91b7f");

/// The game's names of the journal's lists and of what an entry holds.
const QUESTS: &str = "quests";
const COLLECTIONS: &str = "collections";
const ENTRIES: &str = "entries";
const ENTRY_ID: &str = "entryId";
const DOCUMENT: &str = "referencedDocumentName";
const LORE_CATEGORY: &str = "loreCategory";
const REWARDS: &str = "rewards";
const FOR_ALL: &str = "unlockForAllPlayers";
const SHOWN: &str = "knowledgeRequirement";
const DONE: &str = "completionRequirement";
const PROGRESS: &str = "progressStepsRequirement";

/// The priority the journal sorts a mod's quests and pages by, the most
/// common of the game's own quests (server 387).
const PRIORITY: u32 = 100;

/// The id of a journal entry: a quest's step, a lore page or one of its
/// entries.
///
/// # Examples
///
/// ```
/// use enshwick::content::journal::EntryId;
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

/// The key the journal holds for the text tag `tag`: 0, which names no
/// text, for a tag of zeros.
fn text_key(tag: Guid) -> u32 {
    if tag == Guid([0; 16]) { 0 } else { tag.fnv1a() }
}

/// A query no knowledge holds, as the game's own lore entries complete
/// (server 387).
fn never() -> Value {
    Value::Struct(vec![
        ("knowledgeOrQueryId".into(), hash_key(0)),
        ("compareValue".into(), Value::Uint(0)),
        ("compareOperator".into(), Value::Name("GreaterThan".into())),
        ("type".into(), Value::Name("Extern".into())),
        ("isExplicitPlayerKnowledgeQuery".into(), Value::Bool(false)),
    ])
}

/// What the journal shows as a quest's reward.
///
/// The game gives the experience when the quest's last step completes and
/// only draws the item: its own quests name an NPC, a station or a chest
/// icon there (client 26430625). A mod hands the items of its own quest out
/// on [`QuestCompleted`](crate::events::QuestCompleted). The name is
/// enshwick's, not the game's.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub enum QuestReward {
    /// One of the item of this GUID.
    Item(Guid),
    /// This experience value.
    Experience(ExperienceValue),
}

impl QuestReward {
    /// The game's reward value.
    fn to_value(self) -> Value {
        let (item, id, experience) = match self {
            Self::Item(guid) => (Value::Guid(guid), ItemId::from_guid(guid).get(), 0),
            Self::Experience(value) => (Value::Guid(Guid([0; 16])), 0, value.get()),
        };
        Value::Struct(vec![
            ("itemRef".into(), item),
            ("itemId".into(), hash_key(id)),
            ("experienceValueId".into(), hash_key(experience)),
        ])
    }
}

/// One step of a quest: its title and text, shown while `shown_when` holds,
/// done once `done_when` holds.
///
/// A query left `None` is the loader's: a step without `done_when` gets a
/// bool of its own, and one without `shown_when` shows once the step before
/// it is done (the first, once a bool of its own is set), each the player's
/// for a player's quest and the world's for a world quest, so
/// [`Quests`](../../quests/struct.Quests.html) moves the quest exactly.
///
/// The name is enshwick's; the game's is `JournalEntryResource`.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Debug, PartialEq, crate::Script)]
pub struct QuestStep {
    /// Its title's text tag; give it text with [`Load::add_text`](../../struct.Load.html#method.add_text).
    pub name: Guid,
    /// Its text's tag.
    pub text: Guid,
    /// When the journal shows it; `None` for the loader's.
    pub shown_when: Option<KnowledgeQuery>,
    /// When it is done; `None` for the loader's.
    pub done_when: Option<KnowledgeQuery>,
    /// Parts the journal counts as it moves on, each done when it holds.
    #[script(default)]
    pub progress: Vec<KnowledgeQuery>,
    /// The item whose icon it shows.
    pub icon: Option<ItemId>,
    /// The level it suits; 0 for none.
    #[script(default)]
    pub level: u8,
    /// The text tag of the map marker its "show on map" points at, as a
    /// marker of `ui::map` carries it; `None` for no marker.
    pub marker: Option<Guid>,
    /// The voiceover it plays; `None` for none.
    pub voiceover: Option<Guid>,
    /// What the journal counts on it, each on a counter of the loader's
    /// that its `progress` shows; a step without `done_when` is done once
    /// every count is reached.
    #[cfg(feature = "typed")]
    #[script(default)]
    pub objectives: Vec<Objective>,
}

impl QuestStep {
    /// A step of `name` and `text`, shown when `shown_when` holds and done
    /// when `done_when` holds, with no parts, icon or level.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub const fn new(
        name: Guid,
        text: Guid,
        shown_when: KnowledgeQuery,
        done_when: KnowledgeQuery,
    ) -> Self {
        Self {
            name,
            text,
            shown_when: Some(shown_when),
            done_when: Some(done_when),
            progress: Vec::new(),
            icon: None,
            level: 0,
            marker: None,
            voiceover: None,
            #[cfg(feature = "typed")]
            objectives: Vec::new(),
        }
    }

    /// A step of `name` and `text` on the loader's knowledge, which
    /// [`Quests`](../../quests/struct.Quests.html) moves on.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::id;
    /// use enshwick::content::journal::{NewQuest, QuestStep};
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let steps = vec![
    ///         QuestStep::owned(id!("hunt.find").guid(), id!("hunt.find.text").guid()),
    ///         QuestStep::owned(id!("hunt.kill").guid(), id!("hunt.kill.text").guid()),
    ///     ];
    ///     load.add_quest(NewQuest::new(id!("hunt"), id!("hunt.name").guid(), steps))?;
    ///     Ok(())
    /// }
    /// ```
    #[must_use]
    pub const fn owned(name: Guid, text: Guid) -> Self {
        Self {
            name,
            text,
            shown_when: None,
            done_when: None,
            progress: Vec::new(),
            icon: None,
            level: 0,
            marker: None,
            voiceover: None,
            #[cfg(feature = "typed")]
            objectives: Vec::new(),
        }
    }

    /// The game's journal entry of the step with entry id `entry`, whose
    /// queries the loader has filled.
    fn to_value(&self, entry: u32) -> Result<Value, Off> {
        let progress = self
            .progress
            .iter()
            .map(|query| query.to_value())
            .collect::<Result<_, _>>()?;
        let query = |query: Option<KnowledgeQuery>| {
            query
                .ok_or_else(|| Off::new("quest step: a query the loader did not fill"))?
                .to_value()
        };
        Ok(Value::Struct(vec![
            (ENTRY_ID.into(), hash_key(entry)),
            ("name".into(), hash_key(self.name.fnv1a())),
            ("text".into(), hash_key(self.text.fnv1a())),
            (
                "mapMarkerReference".into(),
                hash_key(self.marker.map_or(0, text_key)),
            ),
            (SHOWN.into(), query(self.shown_when)?),
            (DONE.into(), query(self.done_when)?),
            (PROGRESS.into(), Value::List(progress)),
            (
                "voiceover".into(),
                Value::Guid(self.voiceover.unwrap_or(Guid([0; 16]))),
            ),
            (
                "itemIconId".into(),
                hash_key(self.icon.map_or(0, ItemId::get)),
            ),
            ("recommendedLevel".into(), Value::Uint(self.level.into())),
        ]))
    }
}

/// A new quest for [`Load::add_quest`](../../struct.Load.html#method.add_quest).
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Debug, PartialEq, crate::Script)]
pub struct NewQuest {
    /// Its content id; its quest id is the id's [`KnowledgeId::from_guid`].
    pub id: Id,
    /// Its title's text tag.
    pub name: Guid,
    /// Who gives it, which groups it in the journal.
    pub source: QuestSourceType,
    /// Whether it is the world's, each player's, or the game's `Auto`.
    pub kind: QuestType,
    /// Whether the journal lists it as a main quest.
    #[script(default)]
    pub main: bool,
    /// Whether one player's completion unlocks it for every player.
    #[script(default)]
    pub for_all_players: bool,
    /// Its steps, in order.
    pub steps: Vec<QuestStep>,
    /// What the journal shows as its reward; see [`QuestReward`] for what
    /// the game gives of it.
    #[script(default)]
    pub rewards: Vec<QuestReward>,
    /// Where the journal sorts it among the others; `None` for the
    /// priority most of the game's quests hold, 100.
    pub priority: Option<u32>,
    /// Whether it belongs to the tutorial.
    #[script(default)]
    pub tutorial: bool,
    /// The text tag of the document it names; `None` for the one the
    /// game's quests share.
    pub document: Option<Guid>,
}

impl NewQuest {
    /// A player's quest `id` titled `name` with `steps`, given by no one,
    /// with no rewards.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub const fn new(id: Id, name: Guid, steps: Vec<QuestStep>) -> Self {
        Self {
            id,
            name,
            source: QuestSourceType::None,
            kind: QuestType::PlayerQuest,
            main: false,
            for_all_players: false,
            steps,
            rewards: Vec::new(),
            priority: None,
            tutorial: false,
            document: None,
        }
    }
}

/// One entry of a journal page, shown once `shown_when` holds.
///
/// # Examples
///
/// See `Load::add_journal_page`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
pub struct PageEntry {
    /// Its title's text tag.
    pub name: Guid,
    /// Its text's tag.
    pub text: Guid,
    /// When the journal shows it.
    pub shown_when: KnowledgeQuery,
}

/// A new lore page for `Load::add_journal_page`.
///
/// The name is enshwick's; the game's is `JournalCollectionResource`.
///
/// # Examples
///
/// See `Load::add_journal_page`.
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
pub struct NewJournalPage {
    /// Its content id.
    pub id: Id,
    /// The text tag of the game's lore category the journal lists it under:
    /// the journal has a card only for its own categories, so
    /// `Load::add_journal_page` refuses any other tag. Rust names them in
    /// `journal::lore_category` behind the `unstable` feature; Luau has no
    /// such names and passes the tag.
    pub category: Guid,
    /// Its title's text tag.
    pub name: Guid,
    /// Its entries, in order.
    pub entries: Vec<PageEntry>,
}

/// The entry id of a journal entry.
fn entry_id(value: &Value) -> Option<u32> {
    value.field(ENTRY_ID).and_then(key_of_hash)
}

/// The list in field `name` of `value`, to change.
fn list_mut<'a>(value: &'a mut Value, name: &str) -> Result<&'a mut Vec<Value>, Off> {
    match value.field_mut(name) {
        Some(Value::List(list)) => Ok(list),
        _ => Err(Off::new(format!("journal: no list of {name}"))),
    }
}

/// The entry ids a quest's steps take: one per step, from the quest's
/// content id.
fn step_ids(id: Id, steps: usize) -> Vec<u32> {
    (0..steps)
        .map(|step| Guid::named(&format!("{id}.step-{step}")).fnv1a())
        .collect()
}

/// The document the game's own journal entries name: the first vanilla
/// quest's, which nearly every quest and page shares (server 387).
fn document(registry: &Value) -> Value {
    registry
        .field(QUESTS)
        .and_then(|quests| match quests {
            Value::List(quests) => quests.first(),
            _ => None,
        })
        .and_then(|quest| quest.field(DOCUMENT))
        .cloned()
        .unwrap_or_else(|| hash_key(0))
}

/// The header fields every journal collection holds, quests and pages alike.
fn header(
    entry: u32,
    category: u32,
    name: Guid,
    document: Value,
    entries: Vec<Value>,
) -> Vec<(String, Value)> {
    vec![
        (ENTRY_ID.into(), hash_key(entry)),
        (LORE_CATEGORY.into(), hash_key(category)),
        ("name".into(), hash_key(name.fnv1a())),
        (DOCUMENT.into(), document),
        ("priority".into(), Value::Uint(PRIORITY.into())),
        ("isTutorial".into(), Value::Bool(false)),
        (ENTRIES.into(), Value::List(entries)),
    ]
}

/// Whether the journal holds an entry `id` among its quests, its pages or
/// their steps.
fn holds(registry: &Value, id: u32) -> bool {
    [QUESTS, COLLECTIONS].iter().any(|list| match registry.field(list) {
        Some(Value::List(all)) => all.iter().any(|entry| {
            entry_id(entry) == Some(id)
                || matches!(entry.field(ENTRIES), Some(Value::List(steps)) if steps.iter().any(|s| entry_id(s) == Some(id)))
        }),
        _ => false,
    })
}

/// Adds the quest `new` to `registry` after the game's; on an error the
/// registry is left as it was.
fn add_quest_to(registry: &mut Value, new: &NewQuest) -> Result<(), Off> {
    let quest = KnowledgeId::from_guid(new.id.guid()).get();
    if let Some(id) = core::iter::once(quest)
        .chain(step_ids(new.id, new.steps.len()))
        .find(|&id| holds(registry, id))
    {
        return Err(Off::new(format!(
            "quest {}: the journal holds {id}",
            new.id
        )));
    }
    let quest = quest_value(new, document(registry))?;
    list_mut(registry, QUESTS)?.push(quest);
    Ok(())
}

/// Fills the queries `new`'s steps leave to the loader: a step's missing
/// `done_when` is `own`'s bool of the part `step-<n>.done`, a missing
/// `shown_when` the step before's `done_when`, or the bool of the part
/// `started` for the first step.
fn fill_owned(
    new: &mut NewQuest,
    mut own: impl FnMut(&'static str) -> Result<KnowledgeQuery, Off>,
) -> Result<(), Off> {
    let mut before: Option<KnowledgeQuery> = None;
    for (at, step) in new.steps.iter_mut().enumerate() {
        if step.shown_when.is_none() {
            step.shown_when = Some(match before {
                Some(done) => done,
                None => own("started")?,
            });
        }
        if step.done_when.is_none() {
            // shortcut: a step's part name lives for the process; a mod adds few quests, once, during init.
            let part: &'static str = Box::leak(format!("step-{at}.done").into_boxed_str());
            step.done_when = Some(own(part)?);
        }
        before = step.done_when;
    }
    Ok(())
}

/// Gives each objective of `new`'s steps an `own` counter of the part
/// `step-<n>.obj-<m>` and its count in the step's `progress`; a step of one
/// objective and no `done_when` is done on that count. Returns, per step
/// with objectives, its index, its counters and whether the loader's own
/// done bool (filled later) completes it.
#[cfg(feature = "typed")]
fn count_objectives(
    new: &mut NewQuest,
    scope: Scope,
    mut own: impl FnMut(&'static str, KnowledgeType) -> Result<KnowledgeId, Off>,
) -> Result<Vec<(u32, Vec<KnowledgeId>, bool)>, Off> {
    let mut steps = Vec::new();
    for (at, step) in new.steps.iter_mut().enumerate() {
        if step.objectives.is_empty() {
            continue;
        }
        objectives::check(new.id, at, &step.objectives)?;
        let mut counters = Vec::with_capacity(step.objectives.len());
        for (m, objective) in step.objectives.iter().enumerate() {
            // shortcut: a counter's part name lives for the process; a mod adds few quests, once, during init.
            let part: &'static str = Box::leak(format!("step-{at}.obj-{m}").into_boxed_str());
            let counter = own(part, KnowledgeType::Counter)?;
            step.progress
                .push(KnowledgeQuery::at_least(counter, scope, objective.count()));
            counters.push(counter);
        }
        let own_done = step.done_when.is_none() && counters.len() > 1;
        if step.done_when.is_none() && counters.len() == 1 {
            step.done_when = step.progress.last().copied();
        }
        let at = u32::try_from(at).map_err(|_| Off::new("quest: too many steps"))?;
        steps.push((at, counters, own_done));
    }
    Ok(steps)
}

/// The scope of the knowledge the loader gives a quest of `kind`: the
/// world's for a world quest, else each player's.
fn owned_scope(kind: &QuestType) -> Scope {
    if *kind == QuestType::WorldQuest {
        Scope::World
    } else {
        Scope::Player
    }
}

/// The game's journal quest of `new`, naming the journal's `document`.
pub(crate) fn quest_value(new: &NewQuest, document: Value) -> Result<Value, Off> {
    let quest = KnowledgeId::from_guid(new.id.guid()).get();
    let steps = new
        .steps
        .iter()
        .zip(step_ids(new.id, new.steps.len()))
        .map(|(step, id)| step.to_value(id))
        .collect::<Result<_, _>>()?;
    let document = new.document.map_or(document, |tag| hash_key(text_key(tag)));
    let mut fields = header(quest, 0, new.name, document, steps);
    for (name, value) in &mut fields {
        match name.as_str() {
            "priority" => *value = Value::Uint(new.priority.unwrap_or(PRIORITY).into()),
            "isTutorial" => *value = Value::Bool(new.tutorial),
            _ => {}
        }
    }
    fields.extend([
        ("source".into(), new.source.to_value()),
        ("type".into(), new.kind.to_value()),
        ("isMainQuest".into(), Value::Bool(new.main)),
        (FOR_ALL.into(), Value::Bool(new.for_all_players)),
        (
            REWARDS.into(),
            Value::List(new.rewards.iter().map(|reward| reward.to_value()).collect()),
        ),
    ]);
    Ok(Value::Struct(fields))
}

/// The key of `new`'s category when a page of `registry` is listed under
/// it. The client draws a card only for the lore categories of its own UI
/// data, which are the ones the game's pages name, and drops a page of any
/// other without a word (client 26430625).
#[cfg(feature = "unstable")]
fn category_key(registry: &Value, owner: &str, category: Guid) -> Result<u32, Off> {
    let held = held_categories(registry);
    let key = category.fnv1a();
    if held.contains(&key) {
        return Ok(key);
    }
    let names: Vec<String> = held
        .iter()
        .map(|&held| {
            lore_category::ALL
                .iter()
                .find(|tag| tag.fnv1a() == held)
                .map_or_else(|| format!("key {held}"), ToString::to_string)
        })
        .collect();
    Err(Off::new(format!(
        "{owner}: the game has no lore category {category} (key {key}), and the journal shows no page of another; the game's are {}",
        names.join(", ")
    )))
}

/// The lore categories the pages of `registry` are listed under, sorted;
/// the tutorial pages' 0 is no card and left out.
#[cfg(feature = "unstable")]
fn held_categories(registry: &Value) -> Vec<u32> {
    let mut held: Vec<u32> = match registry.field(COLLECTIONS) {
        Some(Value::List(pages)) => pages
            .iter()
            .filter_map(|page| page.field(LORE_CATEGORY).and_then(key_of_hash))
            .filter(|&key| key != 0)
            .collect(),
        _ => Vec::new(),
    };
    held.sort_unstable();
    held.dedup();
    held
}

/// Adds the lore page `new` to `registry` after the game's; on an error the
/// registry is left as it was.
#[cfg(feature = "unstable")]
fn add_page_to(registry: &mut Value, new: &NewJournalPage) -> Result<(), Off> {
    let category = category_key(registry, &format!("journal page {}", new.id), new.category)?;
    let page = new.id.guid().fnv1a();
    let ids = step_ids(new.id, new.entries.len());
    if let Some(id) = core::iter::once(page)
        .chain(ids.iter().copied())
        .find(|&id| holds(registry, id))
    {
        return Err(Off::new(format!(
            "journal page {}: the journal holds {id}",
            new.id
        )));
    }
    let entries = new
        .entries
        .iter()
        .zip(&ids)
        .map(|(entry, &id)| {
            let mut step =
                QuestStep::new(entry.name, entry.text, entry.shown_when, entry.shown_when)
                    .to_value(id)?;
            if let Some(done) = step.field_mut(DONE) {
                *done = never();
            }
            Ok(step)
        })
        .collect::<Result<_, Off>>()?;
    let fields = header(page, category, new.name, document(registry), entries);
    list_mut(registry, COLLECTIONS)?.push(Value::Struct(fields));
    Ok(())
}

/// One change a mod makes to the journal.
#[derive(Debug)]
pub(crate) enum Edit {
    /// A quest of its own.
    Quest(NewQuest),
    /// A lore page of its own.
    #[cfg(feature = "unstable")]
    Page(NewJournalPage),
    /// A change of a vanilla quest.
    Change(KnowledgeId, QuestChange),
    /// A change of a vanilla page, its category's key checked.
    #[cfg(feature = "unstable")]
    ChangePage(EntryId, Option<u32>, PageChange),
    /// A removal of a vanilla quest, with the game's quest ids, sorted.
    Remove(KnowledgeId, Arc<[u32]>),
    /// A removal of a vanilla page.
    #[cfg(feature = "unstable")]
    RemovePage(EntryId),
    /// An item set of its own, or a change or removal of a vanilla one.
    ItemSet(item_sets::SetEdit),
}

impl Edit {
    /// The quest id of a quest of the mod's own.
    fn own_quest(&self) -> Option<u32> {
        match self {
            Self::Quest(new) => Some(KnowledgeId::from_guid(new.id.guid()).get()),
            _ => None,
        }
    }

    /// Makes the change in `registry`, which is left as it was on an error;
    /// `own` holds the quest ids of the mod's own quests.
    fn apply(&self, registry: &mut Value, own: &[u32]) -> Result<(), Off> {
        match self {
            Self::Quest(new) => add_quest_to(registry, new),
            #[cfg(feature = "unstable")]
            Self::Page(new) => add_page_to(registry, new),
            Self::Change(quest, change) => change_quest_in(registry, *quest, change),
            #[cfg(feature = "unstable")]
            Self::ChangePage(page, category, change) => {
                change_page_in(registry, *page, *category, change)
            }
            Self::Remove(quest, vanilla) => remove_quest_in(registry, *quest, vanilla, own),
            #[cfg(feature = "unstable")]
            Self::RemovePage(page) => remove_page_in(registry, *page),
            Self::ItemSet(edit) => edit.apply(registry),
        }
    }
}

/// The quest ids of the journal `registry`, sorted.
fn quest_ids(registry: &Value) -> Arc<[u32]> {
    let mut ids: Vec<u32> = match registry.field(QUESTS) {
        Some(Value::List(quests)) => quests.iter().filter_map(entry_id).collect(),
        _ => Vec::new(),
    };
    ids.sort_unstable();
    ids.into()
}

/// Whether the journal `registry` holds the page `page`.
#[cfg(feature = "unstable")]
fn holds_page(registry: &Value, page: EntryId) -> bool {
    matches!(registry.field(COLLECTIONS), Some(Value::List(pages))
        if pages.iter().any(|held| entry_id(held) == Some(page.get())))
}

/// The journal changes a mod made, in order, which one patch of the
/// journal serves, read when the game first builds it.
#[derive(Debug)]
pub(crate) struct AddedJournal(Arc<Mutex<Vec<Edit>>>);

impl AddedJournal {
    /// Serves the patch of the journal `key` that makes the mod's changes;
    /// a change that fails is logged and left out, and the rest are made.
    fn serve(host: Host, key: crate::abi::ResourceKey) -> Result<Self, Off> {
        let edits = serve_list(host, key, move |mut registry, edits: &[Edit]| {
            let own: Vec<u32> = edits.iter().filter_map(Edit::own_quest).collect();
            for edit in edits {
                if let Err(off) = edit.apply(&mut registry, &own) {
                    host.log(Level::WARN, &off.to_string());
                }
            }
            Ok(registry)
        })?;
        Ok(Self(edits))
    }
}

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::add_quest`.
    #[doc(hidden)]
    fn add_quest(&mut self, new: NewQuest) -> Result<KnowledgeId, Off>;

    /// As `Load::add_journal_page`.
    #[doc(hidden)]
    #[cfg(feature = "unstable")]
    fn add_journal_page(&mut self, new: NewJournalPage) -> Result<(), Off>;

    /// As `Load::change_quest`.
    #[doc(hidden)]
    fn change_quest(&mut self, quest: KnowledgeId, change: QuestChange) -> Result<(), Off>;

    /// As `Load::remove_quest`.
    #[doc(hidden)]
    fn remove_quest(&mut self, quest: KnowledgeId) -> Result<(), Off>;

    /// As `Load::change_journal_page`.
    #[doc(hidden)]
    #[cfg(feature = "unstable")]
    fn change_journal_page(&mut self, page: EntryId, change: PageChange) -> Result<(), Off>;

    /// As `Load::remove_journal_page`.
    #[doc(hidden)]
    #[cfg(feature = "unstable")]
    fn remove_journal_page(&mut self, page: EntryId) -> Result<(), Off>;
}

impl LoadMethods for crate::BaseLoad {
    fn add_quest(&mut self, mut new: NewQuest) -> Result<KnowledgeId, Off> {
        if new.steps.is_empty() {
            return Err(Off::new(format!("quest {}: no steps", new.id)));
        }
        self.knowledge_without(new.id)?;
        let (id, scope) = (new.id, owned_scope(&new.kind));
        let part = |part: &'static str| {
            id.try_part(part)
                .ok_or_else(|| Off::new(format!("quest {id}: no room for part {part} in its id")))
        };
        // Every check runs on a copy first, so a refused quest declares and serves nothing.
        let mut checked = new.clone();
        let mut check = |name: &'static str| {
            let owned = part(name)?;
            self.knowledge_without(owned)?;
            Ok(KnowledgeId::from_guid(owned.guid()))
        };
        #[cfg(feature = "typed")]
        count_objectives(&mut checked, scope, |name, _| check(name))?;
        fill_owned(&mut checked, |name| {
            check(name).map(|known| KnowledgeQuery::is_set(known, scope))
        })?;
        for (step, id) in checked.steps.iter().zip(step_ids(id, checked.steps.len())) {
            step.to_value(id)?;
        }
        let key = self.journal_key()?;
        let mut own = |name: &'static str, kind: KnowledgeType| {
            let owned = part(name)?;
            self.declare(owned)?;
            self.serve_knowledge(NewKnowledge::new(owned, scope, kind))
        };
        #[cfg(feature = "typed")]
        let counted = count_objectives(&mut new, scope, &mut own)?;
        fill_owned(&mut new, |name| {
            own(name, KnowledgeType::Bool).map(|known| KnowledgeQuery::is_set(known, scope))
        })?;
        self.declare(new.id)?;
        let quest = self.serve_knowledge(NewKnowledge::new(
            new.id,
            Scope::Player,
            KnowledgeType::Bool,
        ))?;
        #[cfg(feature = "typed")]
        self.slot::<objectives::Counted>()
            .get_or_insert_with(Default::default)
            .0
            .extend(
                counted
                    .into_iter()
                    .filter_map(|(step, counters, own_done)| {
                        let at = new.steps.get(step as usize)?;
                        let (shown_when, done_when) = (at.shown_when?, at.done_when?);
                        Some(CountedStep {
                            quest,
                            step,
                            scope,
                            shown_when,
                            done_when,
                            done_bool: own_done.then_some(done_when.id),
                            objectives: at.objectives.iter().cloned().zip(counters).collect(),
                        })
                    }),
            );
        self.edit_journal(key, Edit::Quest(new))?;
        Ok(quest)
    }

    #[cfg(feature = "unstable")]
    fn add_journal_page(&mut self, new: NewJournalPage) -> Result<(), Off> {
        if new.entries.is_empty() {
            return Err(Off::new(format!("journal page {}: no entries", new.id)));
        }
        for entry in &new.entries {
            entry.shown_when.to_value()?;
        }
        let key = self.journal_key()?;
        category_key(
            &self.host().game_value(key)?,
            &format!("journal page {}", new.id),
            new.category,
        )?;
        self.declare(new.id)?;
        self.edit_journal(key, Edit::Page(new))
    }

    fn change_quest(&mut self, quest: KnowledgeId, change: QuestChange) -> Result<(), Off> {
        let key = self.journal_key()?;
        change_quest_in(&mut self.host().game_value(key)?, quest, &change)?;
        self.edit_journal(key, Edit::Change(quest, change))
    }

    fn remove_quest(&mut self, quest: KnowledgeId) -> Result<(), Off> {
        let key = self.journal_key()?;
        let vanilla = quest_ids(&self.host().game_value(key)?);
        if vanilla.binary_search(&quest.get()).is_err() {
            return Err(Off::new(format!(
                "quest {quest}: no quest of the game's journal"
            )));
        }
        self.edit_journal(key, Edit::Remove(quest, vanilla))
    }

    #[cfg(feature = "unstable")]
    fn change_journal_page(&mut self, page: EntryId, change: PageChange) -> Result<(), Off> {
        let key = self.journal_key()?;
        let mut held = self.host().game_value(key)?;
        let category = change
            .category
            .map(|category| category_key(&held, &format!("journal page {}", page.get()), category))
            .transpose()?;
        change_page_in(&mut held, page, category, &change)?;
        self.edit_journal(key, Edit::ChangePage(page, category, change))
    }

    #[cfg(feature = "unstable")]
    fn remove_journal_page(&mut self, page: EntryId) -> Result<(), Off> {
        let key = self.journal_key()?;
        if !holds_page(&self.host().game_value(key)?, page) {
            return Err(Off::new(format!(
                "journal page {}: no page of the game's journal",
                page.get()
            )));
        }
        self.edit_journal(key, Edit::RemovePage(page))
    }
}

/// The base handle's helpers of this module for the crate's own code.
pub(crate) trait LoadHelpers {
    /// The journal's resource key on the running build.
    fn journal_key(&self) -> Result<crate::abi::ResourceKey, Off>;

    /// Adds `edit` to the changes the one patch of the journal `key` makes
    /// for the mod, serving the patch with the first.
    fn edit_journal(&mut self, key: crate::abi::ResourceKey, edit: Edit) -> Result<(), Off>;
}

impl LoadHelpers for crate::BaseLoad {
    fn journal_key(&self) -> Result<crate::abi::ResourceKey, Off> {
        Ok(JOURNAL_REGISTRY.key_by_hash(self.hash(JournalRegistryResource::ID)?))
    }

    fn edit_journal(&mut self, key: crate::abi::ResourceKey, edit: Edit) -> Result<(), Off> {
        let host = self.host();
        let journal = match self.slot::<AddedJournal>() {
            Some(journal) => journal,
            empty => empty.insert(AddedJournal::serve(host, key)?),
        };
        journal.0.relock().push(edit);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
