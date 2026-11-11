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

#[cfg(feature = "unstable")]
pub mod lore_category;

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
/// The name is enshwick's; the game's is `JournalEntryResource`.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
pub struct QuestStep {
    /// Its title's text tag; give it text with [`Load::add_text`](../../struct.Load.html#method.add_text).
    pub name: Guid,
    /// Its text's tag.
    pub text: Guid,
    /// When the journal shows it.
    pub shown_when: KnowledgeQuery,
    /// When it is done.
    pub done_when: KnowledgeQuery,
    /// Parts the journal counts as it moves on, each done when it holds.
    #[script(default)]
    pub progress: Vec<KnowledgeQuery>,
    /// The item whose icon it shows.
    pub icon: Option<ItemId>,
    /// The level it suits; 0 for none.
    #[script(default)]
    pub level: u8,
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
            shown_when,
            done_when,
            progress: Vec::new(),
            icon: None,
            level: 0,
        }
    }

    /// The game's journal entry of the step with entry id `entry`.
    fn to_value(&self, entry: u32) -> Result<Value, Off> {
        let progress = self
            .progress
            .iter()
            .map(|query| query.to_value())
            .collect::<Result<_, _>>()?;
        Ok(Value::Struct(vec![
            (ENTRY_ID.into(), hash_key(entry)),
            ("name".into(), hash_key(self.name.fnv1a())),
            ("text".into(), hash_key(self.text.fnv1a())),
            ("mapMarkerReference".into(), hash_key(0)),
            (SHOWN.into(), self.shown_when.to_value()?),
            (DONE.into(), self.done_when.to_value()?),
            (PROGRESS.into(), Value::List(progress)),
            ("voiceover".into(), Value::Guid(Guid([0; 16]))),
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
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
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

/// A change of a step of a vanilla quest; `None` keeps a query.
///
/// # Examples
///
/// See [`Load::change_quest`](../../struct.Load.html#method.change_quest).
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
pub struct StepChange {
    /// The step, from 0.
    pub step: u32,
    /// When the journal shows it.
    pub shown_when: Option<KnowledgeQuery>,
    /// When it is done.
    pub done_when: Option<KnowledgeQuery>,
}

/// A change of a vanilla quest for [`Load::change_quest`](../../struct.Load.html#method.change_quest); `None` and empty
/// lists keep what the quest holds.
///
/// # Examples
///
/// See [`Load::change_quest`](../../struct.Load.html#method.change_quest).
#[derive(Clone, Debug, Default, PartialEq, Eq, crate::Script)]
pub struct QuestChange {
    /// The rewards in place of the quest's.
    pub rewards: Option<Vec<QuestReward>>,
    /// Rewards after those.
    #[script(default)]
    pub add_rewards: Vec<QuestReward>,
    /// Whether one player's completion unlocks it for every player.
    pub for_all_players: Option<bool>,
    /// Changes of its steps' knowledge.
    #[script(default)]
    pub steps: Vec<StepChange>,
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

/// The game's journal quest of `new`, naming the journal's `document`.
pub(crate) fn quest_value(new: &NewQuest, document: Value) -> Result<Value, Off> {
    let quest = KnowledgeId::from_guid(new.id.guid()).get();
    let steps = new
        .steps
        .iter()
        .zip(step_ids(new.id, new.steps.len()))
        .map(|(step, id)| step.to_value(id))
        .collect::<Result<_, _>>()?;
    let mut fields = header(quest, 0, new.name, document, steps);
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
fn category_key(registry: &Value, new: &NewJournalPage) -> Result<u32, Off> {
    let held = held_categories(registry);
    let key = new.category.fnv1a();
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
        "journal page {}: the game has no lore category {} (key {key}), and the journal shows no page of another; the game's are {}",
        new.id,
        new.category,
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
    let category = category_key(registry, new)?;
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
    // An entry no knowledge completes, as the game's own pages hold it (server 387).
    let never = Value::Struct(vec![
        ("knowledgeOrQueryId".into(), hash_key(0)),
        ("compareValue".into(), Value::Uint(0)),
        ("compareOperator".into(), Value::Name("GreaterThan".into())),
        ("type".into(), Value::Name("Extern".into())),
        ("isExplicitPlayerKnowledgeQuery".into(), Value::Bool(false)),
    ]);
    let entries = new
        .entries
        .iter()
        .zip(&ids)
        .map(|(entry, &id)| {
            let mut step =
                QuestStep::new(entry.name, entry.text, entry.shown_when, entry.shown_when)
                    .to_value(id)?;
            if let Some(done) = step.field_mut(DONE) {
                *done = never.clone();
            }
            Ok(step)
        })
        .collect::<Result<_, Off>>()?;
    let fields = header(page, category, new.name, document(registry), entries);
    list_mut(registry, COLLECTIONS)?.push(Value::Struct(fields));
    Ok(())
}

/// Changes the vanilla quest `quest` in `registry` by `change`; on an error
/// the registry is left as it was.
fn change_quest_in(
    registry: &mut Value,
    quest: KnowledgeId,
    change: &QuestChange,
) -> Result<(), Off> {
    let held = list_mut(registry, QUESTS)?
        .iter_mut()
        .find(|held| entry_id(held) == Some(quest.get()))
        .ok_or_else(|| Off::new(format!("quest {quest}: no quest of the journal")))?;
    let steps = list_mut(held, ENTRIES)?.len();
    let mut queries = Vec::new();
    for step in &change.steps {
        let at = usize::try_from(step.step)
            .ok()
            .filter(|&at| at < steps)
            .ok_or_else(|| Off::new(format!("quest {quest}: no step {}", step.step)))?;
        for (field, query) in [(SHOWN, step.shown_when), (DONE, step.done_when)] {
            if let Some(query) = query {
                queries.push((at, field, query.to_value()?));
            }
        }
    }
    let rewards = list_mut(held, REWARDS)?;
    if let Some(new) = &change.rewards {
        *rewards = new.iter().map(|reward| reward.to_value()).collect();
    }
    rewards.extend(change.add_rewards.iter().map(|reward| reward.to_value()));
    if let (Some(all), Some(held)) = (change.for_all_players, held.field_mut(FOR_ALL)) {
        *held = Value::Bool(all);
    }
    let steps = list_mut(held, ENTRIES)?;
    for (at, field, query) in queries {
        if let Some(held) = steps[at].field_mut(field) {
            *held = query;
        }
    }
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
}

impl Edit {
    /// Makes the change in `registry`, which is left as it was on an error.
    fn apply(&self, registry: &mut Value) -> Result<(), Off> {
        match self {
            Self::Quest(new) => add_quest_to(registry, new),
            #[cfg(feature = "unstable")]
            Self::Page(new) => add_page_to(registry, new),
            Self::Change(quest, change) => change_quest_in(registry, *quest, change),
        }
    }
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
            for edit in edits {
                if let Err(off) = edit.apply(&mut registry) {
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
}

impl LoadMethods for crate::BaseLoad {
    fn add_quest(&mut self, new: NewQuest) -> Result<KnowledgeId, Off> {
        if new.steps.is_empty() {
            return Err(Off::new(format!("quest {}: no steps", new.id)));
        }
        for (step, id) in new.steps.iter().zip(step_ids(new.id, new.steps.len())) {
            step.to_value(id)?;
        }
        self.knowledge_without(new.id)?;
        let key = self.journal_key()?;
        self.declare(new.id)?;
        let quest = self.serve_knowledge(NewKnowledge::new(
            new.id,
            Scope::Player,
            KnowledgeType::Bool,
        ))?;
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
        category_key(&self.host().game_value(key)?, &new)?;
        self.declare(new.id)?;
        self.edit_journal(key, Edit::Page(new))
    }

    fn change_quest(&mut self, quest: KnowledgeId, change: QuestChange) -> Result<(), Off> {
        let key = self.journal_key()?;
        change_quest_in(&mut self.host().game_value(key)?, quest, &change)?;
        self.edit_journal(key, Edit::Change(quest, change))
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
mod tests {
    use super::*;

    /// The category of the game's one page in [`registry`].
    const CATEGORY: Guid = Guid([14; 16]);

    fn query(id: u32) -> KnowledgeQuery {
        KnowledgeQuery::is_set(KnowledgeId::new(id), Scope::Player)
    }

    fn registry() -> Value {
        let mut vanilla = NewQuest::new(
            Id::new("demo.vanilla:q"),
            Guid([1; 16]),
            vec![QuestStep::new(
                Guid([2; 16]),
                Guid([3; 16]),
                query(1),
                query(2),
            )],
        );
        vanilla.rewards = vec![QuestReward::Item(Guid([4; 16]))];
        let page = header(7, CATEGORY.fnv1a(), Guid([8; 16]), hash_key(0), Vec::new());
        let tutorial = header(9, 0, Guid([8; 16]), hash_key(0), Vec::new());
        let empty = Value::Struct(vec![
            (QUESTS.into(), Value::List(Vec::new())),
            (
                COLLECTIONS.into(),
                Value::List(vec![Value::Struct(page), Value::Struct(tutorial)]),
            ),
        ]);
        let mut held = empty;
        add_quest_to(&mut held, &vanilla).unwrap();
        if let Some(Value::List(quests)) = held.field_mut(QUESTS)
            && let Some(document) = quests[0].field_mut(DOCUMENT)
        {
            *document = hash_key(1_768_495_365);
        }
        held
    }

    fn quests(registry: &Value) -> &[Value] {
        match registry.field(QUESTS) {
            Some(Value::List(quests)) => quests,
            _ => &[],
        }
    }

    #[test]
    fn a_quest_joins_the_journal_with_its_steps_rewards_and_the_game_s_document() {
        let new = NewQuest {
            rewards: vec![QuestReward::Experience(ExperienceValue::new(9))],
            ..NewQuest::new(
                Id::new("you.trail:trail"),
                Guid([5; 16]),
                vec![
                    QuestStep::new(Guid([6; 16]), Guid([7; 16]), query(3), query(4)),
                    QuestStep::new(Guid([8; 16]), Guid([9; 16]), query(4), query(5)),
                ],
            )
        };
        let mut added = registry();
        add_quest_to(&mut added, &new).unwrap();
        let quest = &quests(&added)[1];
        assert_eq!(
            entry_id(quest),
            Some(KnowledgeId::from_guid(new.id.guid()).get())
        );
        assert_eq!(quest.field(DOCUMENT), Some(&hash_key(1_768_495_365)));
        let Some(Value::List(steps)) = quest.field(ENTRIES) else {
            panic!("{quest:?}")
        };
        assert_eq!(steps.len(), 2);
        assert_ne!(entry_id(&steps[0]), entry_id(&steps[1]));
        assert_eq!(steps[1].field(SHOWN), Some(&query(4).to_value().unwrap()));
        let before = added.clone();
        assert!(add_quest_to(&mut added, &new).is_err(), "twice");
        assert_eq!(added, before, "left as it was");
    }

    #[cfg(feature = "unstable")]
    #[test]
    fn a_page_s_entries_show_on_knowledge_and_never_complete() {
        let page = NewJournalPage {
            id: Id::new("you.trail:lore"),
            category: CATEGORY,
            name: Guid([11; 16]),
            entries: vec![PageEntry {
                name: Guid([12; 16]),
                text: Guid([13; 16]),
                shown_when: query(6),
            }],
        };
        let mut added = registry();
        add_page_to(&mut added, &page).unwrap();
        let Some(Value::List(pages)) = added.field(COLLECTIONS) else {
            panic!("{added:?}")
        };
        let last = pages.last().unwrap();
        assert_eq!(last.field(LORE_CATEGORY), Some(&hash_key(CATEGORY.fnv1a())));
        let Some(Value::List(entries)) = last.field(ENTRIES) else {
            panic!("{last:?}")
        };
        assert_eq!(entries[0].field(SHOWN), Some(&query(6).to_value().unwrap()));
        assert_eq!(
            entries[0].field(DONE).and_then(|done| done.field("type")),
            Some(&Value::Name("Extern".into()))
        );
    }

    #[cfg(feature = "unstable")]
    #[test]
    fn a_page_of_a_category_no_game_page_has_is_refused_with_the_game_s() {
        let page = |category| NewJournalPage {
            id: Id::new("you.trail:lore"),
            category,
            name: Guid([11; 16]),
            entries: vec![PageEntry {
                name: Guid([12; 16]),
                text: Guid([13; 16]),
                shown_when: query(6),
            }],
        };
        let mut before = registry();
        let general = header(
            10,
            lore_category::GENERAL.fnv1a(),
            Guid([8; 16]),
            hash_key(0),
            Vec::new(),
        );
        list_mut(&mut before, COLLECTIONS)
            .unwrap()
            .push(Value::Struct(general));
        let mut added = before.clone();
        let own = Guid([10; 16]);
        let off = add_page_to(&mut added, &page(own)).unwrap_err().to_string();
        assert!(off.contains(&own.to_string()), "{off}");
        assert!(
            off.contains(&lore_category::GENERAL.to_string()),
            "a category enshwick names reads as the tag a mod passes: {off}"
        );
        assert!(
            off.contains(&format!("key {}", CATEGORY.fnv1a())),
            "one it does not name reads as its key: {off}"
        );
        assert_eq!(added, before, "left as it was");
        add_page_to(&mut added, &page(lore_category::GENERAL)).unwrap();
    }

    #[cfg(feature = "unstable")]
    #[test]
    fn the_tutorial_pages_category_0_is_no_category_a_page_may_take() {
        assert_eq!(held_categories(&registry()), [CATEGORY.fnv1a()]);
    }

    #[test]
    fn two_changes_of_a_vanilla_quest_chain_and_a_step_past_its_own_is_refused() {
        let vanilla = KnowledgeId::from_guid(Id::new("demo.vanilla:q").guid());
        let first = QuestChange {
            rewards: Some(Vec::new()),
            for_all_players: Some(true),
            ..QuestChange::default()
        };
        let second = QuestChange {
            add_rewards: vec![QuestReward::Item(Guid([14; 16]))],
            steps: vec![StepChange {
                step: 0,
                shown_when: None,
                done_when: Some(query(7)),
            }],
            ..QuestChange::default()
        };
        let mut twice = registry();
        change_quest_in(&mut twice, vanilla, &first).unwrap();
        change_quest_in(&mut twice, vanilla, &second).unwrap();
        let quest = &quests(&twice)[0];
        assert_eq!(
            quest.field(REWARDS),
            Some(&Value::List(vec![
                QuestReward::Item(Guid([14; 16])).to_value()
            ]))
        );
        assert_eq!(quest.field(FOR_ALL), Some(&Value::Bool(true)));
        let Some(Value::List(steps)) = quest.field(ENTRIES) else {
            panic!("{quest:?}")
        };
        assert_eq!(steps[0].field(DONE), Some(&query(7).to_value().unwrap()));
        assert_eq!(
            steps[0].field(SHOWN),
            Some(&query(1).to_value().unwrap()),
            "kept"
        );
        let past = QuestChange {
            steps: vec![StepChange {
                step: 1,
                shown_when: Some(query(1)),
                done_when: None,
            }],
            ..QuestChange::default()
        };
        let mut held = registry();
        let past = QuestChange {
            rewards: Some(Vec::new()),
            ..past
        };
        assert!(change_quest_in(&mut held, vanilla, &past).is_err());
        assert_eq!(held, registry(), "the rewards are kept when a step is past");
        assert!(change_quest_in(&mut held, KnowledgeId::new(5), &first).is_err());
    }
}
