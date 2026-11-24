//! Changes and removals of the game's quests and lore pages: every field the
//! journal holds, steps inserted, removed and moved.

use crate::content::knowledge::{KnowledgeQuery, hash_key, key_of_hash};
use crate::knowledge::KnowledgeId;
use crate::resources::Value;
use crate::typed::{QuestSourceType, QuestType, Reflected as _};
use crate::{ItemId, Off};

use super::{
    COLLECTIONS, DOCUMENT, DONE, ENTRIES, EntryId, FOR_ALL, Guid, PROGRESS, QUESTS, QuestReward,
    QuestStep, REWARDS, SHOWN, entry_id, holds, list_mut, never, text_key,
};

/// A change of one step of a vanilla quest or entry of a vanilla page; `None`
/// keeps a field. A text tag of zeros clears the map marker and the
/// voiceover, and item 0 the icon.
///
/// The name is enshwick's; the game's step is `JournalEntryResource`.
///
/// # Examples
///
/// See [`Load::change_quest`](../../struct.Load.html#method.change_quest).
#[derive(Clone, Debug, Default, PartialEq, Eq, crate::Script)]
pub struct StepChange {
    /// The step, from 0, where the step edits before it leave it.
    pub step: u32,
    /// Its title's text tag.
    pub name: Option<Guid>,
    /// Its text's tag.
    pub text: Option<Guid>,
    /// When the journal shows it.
    pub shown_when: Option<KnowledgeQuery>,
    /// When it is done.
    pub done_when: Option<KnowledgeQuery>,
    /// The parts the journal counts as it moves on, in place of its own.
    pub progress: Option<Vec<KnowledgeQuery>>,
    /// The text tag of the map marker its "show on map" points at, as a
    /// marker of `ui::map` carries it.
    pub marker: Option<Guid>,
    /// The voiceover it plays.
    pub voiceover: Option<Guid>,
    /// The item whose icon it shows.
    pub icon: Option<ItemId>,
    /// The level it suits; 0 for none.
    pub level: Option<u8>,
}

/// A step inserted into a vanilla quest or page by [`StepEdit::Insert`].
///
/// # Examples
///
/// See [`Load::change_quest`](../../struct.Load.html#method.change_quest).
#[derive(Clone, Debug, PartialEq, crate::Script)]
pub struct StepInsert {
    /// Where it goes, from 0; the step there and those after move down. The
    /// step count puts it last.
    pub at: u32,
    /// The step. A quest's names both its queries; a page entry needs only
    /// `shown_when`.
    pub step: QuestStep,
}

/// A step moved inside a vanilla quest or page by [`StepEdit::Move`].
///
/// # Examples
///
/// See [`Load::change_quest`](../../struct.Load.html#method.change_quest).
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
pub struct StepMove {
    /// The step to move, from 0.
    pub from: u32,
    /// Where it ends up, from 0, counted once it has left `from`.
    pub to: u32,
}

/// One edit of the steps of a vanilla quest or the entries of a vanilla
/// page, applied in order, each on the steps the edits before it left.
///
/// The name is enshwick's, not the game's.
///
/// # Examples
///
/// See [`Load::change_quest`](../../struct.Load.html#method.change_quest).
#[derive(Clone, Debug, PartialEq, crate::Script)]
#[non_exhaustive]
pub enum StepEdit {
    /// Changes a step's fields.
    Change(StepChange),
    /// Inserts a step with an entry id of its own.
    Insert(StepInsert),
    /// Removes the step of this index.
    Remove(u32),
    /// Moves a step.
    Move(StepMove),
}

/// A change of a vanilla quest for [`Load::change_quest`](../../struct.Load.html#method.change_quest); `None` and empty
/// lists keep what the quest holds.
///
/// # Examples
///
/// See [`Load::change_quest`](../../struct.Load.html#method.change_quest).
#[derive(Clone, Debug, Default, PartialEq, crate::Script)]
pub struct QuestChange {
    /// Its title's text tag.
    pub name: Option<Guid>,
    /// Who gives it, which groups it in the journal.
    pub source: Option<QuestSourceType>,
    /// Whether it is the world's, each player's, or the game's `Auto`.
    pub kind: Option<QuestType>,
    /// Whether the journal lists it as a main quest.
    pub main: Option<bool>,
    /// Whether one player's completion unlocks it for every player.
    pub for_all_players: Option<bool>,
    /// Where the journal sorts it among the others.
    pub priority: Option<u32>,
    /// Whether it belongs to the tutorial.
    pub tutorial: Option<bool>,
    /// The text tag of the document it names.
    pub document: Option<Guid>,
    /// The rewards in place of the quest's.
    pub rewards: Option<Vec<QuestReward>>,
    /// Rewards after those.
    #[script(default)]
    pub add_rewards: Vec<QuestReward>,
    /// Edits of its steps, in order.
    #[script(default)]
    pub steps: Vec<StepEdit>,
}

/// A change of a vanilla lore page for `Load::change_journal_page`; `None`
/// and an empty list keep what the page holds.
///
/// The name is enshwick's; the game's page is `JournalCollectionResource`.
///
/// # Examples
///
/// See `Load::change_journal_page`.
#[derive(Clone, Debug, Default, PartialEq, crate::Script)]
pub struct PageChange {
    /// The text tag of the game's lore category it moves to; refused as
    /// `Load::add_journal_page` refuses one.
    pub category: Option<Guid>,
    /// Its title's text tag.
    pub name: Option<Guid>,
    /// Where the journal sorts it among the others.
    pub priority: Option<u32>,
    /// Whether it belongs to the tutorial.
    pub tutorial: Option<bool>,
    /// The text tag of the document it names.
    pub document: Option<Guid>,
    /// Edits of its entries, in order.
    #[script(default)]
    pub entries: Vec<StepEdit>,
}

/// The fields of a collection, quest or page, a change sets.
struct Header {
    name: Option<Guid>,
    priority: Option<u32>,
    tutorial: Option<bool>,
    document: Option<Guid>,
}

/// Sets field `name` of `value` to `new`; a field the type lacks is an error.
fn set(value: &mut Value, name: &str, new: Value) -> Result<(), Off> {
    *value
        .field_mut(name)
        .ok_or_else(|| Off::new(format!("journal: no field {name}")))? = new;
    Ok(())
}

/// The id of the query `value` names, `None` for a form without one.
fn query_id(value: &Value) -> Option<u32> {
    value.field("knowledgeOrQueryId").and_then(key_of_hash)
}

/// Whether a step of the collection `entry` asks for knowledge `id`.
fn asks_for(entry: &Value, id: u32) -> bool {
    let Some(Value::List(steps)) = entry.field(ENTRIES) else {
        return false;
    };
    steps.iter().any(|step| {
        let progress = match step.field(PROGRESS) {
            Some(Value::List(parts)) => parts.as_slice(),
            _ => &[],
        };
        [step.field(SHOWN), step.field(DONE)]
            .into_iter()
            .flatten()
            .chain(progress)
            .any(|query| query_id(query) == Some(id))
    })
}

impl StepChange {
    /// Sets the fields it names on the journal entry `entry`.
    fn apply(&self, entry: &mut Value) -> Result<(), Off> {
        let tag = |guid: Guid| hash_key(guid.fnv1a());
        if let Some(name) = self.name {
            set(entry, "name", tag(name))?;
        }
        if let Some(text) = self.text {
            set(entry, "text", tag(text))?;
        }
        for (field, query) in [(SHOWN, self.shown_when), (DONE, self.done_when)] {
            if let Some(query) = query {
                set(entry, field, query.to_value()?)?;
            }
        }
        if let Some(progress) = &self.progress {
            let parts = progress
                .iter()
                .map(|query| query.to_value())
                .collect::<Result<_, _>>()?;
            set(entry, PROGRESS, Value::List(parts))?;
        }
        if let Some(marker) = self.marker {
            set(entry, "mapMarkerReference", hash_key(text_key(marker)))?;
        }
        if let Some(voiceover) = self.voiceover {
            set(entry, "voiceover", Value::Guid(voiceover))?;
        }
        if let Some(icon) = self.icon {
            set(entry, "itemIconId", hash_key(icon.get()))?;
        }
        if let Some(level) = self.level {
            set(entry, "recommendedLevel", Value::Uint(level.into()))?;
        }
        Ok(())
    }
}

/// A fresh entry id for the `n`th step inserted into the collection `owner`:
/// the first of a run of names the journal `registry` does not hold, so
/// every side that makes the same changes in the same order agrees on it.
fn fresh_id(registry: &Value, taken: &[u32], owner: u32, n: usize) -> u32 {
    (0..=u32::MAX)
        .map(|k| Guid::named(&format!("journal:{owner}.insert-{n}.{k}")).fnv1a())
        .find(|&id| id != 0 && !taken.contains(&id) && !holds(registry, id))
        .unwrap_or_default()
}

/// The index `at` of a list of `len`, or an error naming the `owner`.
fn index(owner: &str, at: u32, len: usize) -> Result<usize, Off> {
    usize::try_from(at)
        .ok()
        .filter(|&at| at < len)
        .ok_or_else(|| Off::new(format!("{owner}: no step {at}")))
}

/// Applies `edits` to the steps of the collection `entry` of id `owner` in
/// `registry`; `page` takes a step without `done_when` as an entry the page
/// never completes.
fn edit_steps(
    registry: &Value,
    entry: &mut Value,
    owner: u32,
    name: &str,
    page: bool,
    edits: &[StepEdit],
) -> Result<(), Off> {
    let inserts = edits
        .iter()
        .filter(|edit| matches!(edit, StepEdit::Insert(_)))
        .count();
    let mut ids = Vec::with_capacity(inserts);
    for n in 0..inserts {
        let id = fresh_id(registry, &ids, owner, n);
        ids.push(id);
    }
    let mut fresh = ids.into_iter();
    let steps = list_mut(entry, ENTRIES)?;
    for edit in edits {
        match edit {
            StepEdit::Change(change) => {
                let at = index(name, change.step, steps.len())?;
                change.apply(&mut steps[at])?;
            }
            StepEdit::Insert(insert) => {
                let at = usize::try_from(insert.at)
                    .ok()
                    .filter(|&at| at <= steps.len())
                    .ok_or_else(|| {
                        Off::new(format!("{name}: no step {} to insert at", insert.at))
                    })?;
                let mut step = insert.step.clone();
                if step.shown_when.is_none() || (!page && step.done_when.is_none()) {
                    return Err(Off::new(format!(
                        "{name}: an inserted step names {}",
                        if page {
                            "its shown_when"
                        } else {
                            "both its queries"
                        }
                    )));
                }
                #[cfg(feature = "typed")]
                if !step.objectives.is_empty() {
                    return Err(Off::new(format!(
                        "{name}: objectives count only on a mod's own quest"
                    )));
                }
                if page {
                    step.done_when = step.done_when.or(step.shown_when);
                }
                let id = fresh.next().unwrap_or_default();
                let mut value = step.to_value(id)?;
                if page && insert.step.done_when.is_none() {
                    set(&mut value, DONE, never())?;
                }
                steps.insert(at, value);
            }
            StepEdit::Remove(at) => {
                let _removed = steps.remove(index(name, *at, steps.len())?);
            }
            StepEdit::Move(StepMove { from, to }) => {
                let step = steps.remove(index(name, *from, steps.len())?);
                let to = usize::try_from(*to)
                    .ok()
                    .filter(|&to| to <= steps.len())
                    .ok_or_else(|| Off::new(format!("{name}: no step {to} to move to")))?;
                steps.insert(to, step);
            }
        }
    }
    if steps.is_empty() && !page {
        return Err(Off::new(format!("{name}: no steps left")));
    }
    Ok(())
}

/// Sets the collection fields of `header` on `entry`.
fn set_header(entry: &mut Value, header: &Header) -> Result<(), Off> {
    if let Some(name) = header.name {
        set(entry, "name", hash_key(name.fnv1a()))?;
    }
    if let Some(priority) = header.priority {
        set(entry, "priority", Value::Uint(priority.into()))?;
    }
    if let Some(tutorial) = header.tutorial {
        set(entry, "isTutorial", Value::Bool(tutorial))?;
    }
    if let Some(document) = header.document {
        set(entry, DOCUMENT, hash_key(text_key(document)))?;
    }
    Ok(())
}

/// The index of the collection `id` in the list `list` of `registry`.
fn position(registry: &Value, list: &str, id: u32) -> Option<usize> {
    match registry.field(list) {
        Some(Value::List(all)) => all.iter().position(|held| entry_id(held) == Some(id)),
        _ => None,
    }
}

/// Changes the vanilla quest `quest` in `registry` by `change`; on an error
/// the registry is left as it was.
pub(super) fn change_quest_in(
    registry: &mut Value,
    quest: KnowledgeId,
    change: &QuestChange,
) -> Result<(), Off> {
    let name = format!("quest {quest}");
    let at = position(registry, QUESTS, quest.get())
        .ok_or_else(|| Off::new(format!("{name}: no quest of the journal")))?;
    let mut held = list_mut(registry, QUESTS)?[at].clone();
    set_header(
        &mut held,
        &Header {
            name: change.name,
            priority: change.priority,
            tutorial: change.tutorial,
            document: change.document,
        },
    )?;
    if let Some(source) = &change.source {
        set(&mut held, "source", source.to_value())?;
    }
    if let Some(kind) = &change.kind {
        set(&mut held, "type", kind.to_value())?;
    }
    if let Some(main) = change.main {
        set(&mut held, "isMainQuest", Value::Bool(main))?;
    }
    if let Some(all) = change.for_all_players {
        set(&mut held, FOR_ALL, Value::Bool(all))?;
    }
    let rewards = list_mut(&mut held, REWARDS)?;
    if let Some(new) = &change.rewards {
        *rewards = new.iter().map(|reward| reward.to_value()).collect();
    }
    rewards.extend(change.add_rewards.iter().map(|reward| reward.to_value()));
    edit_steps(
        registry,
        &mut held,
        quest.get(),
        &name,
        false,
        &change.steps,
    )?;
    list_mut(registry, QUESTS)?[at] = held;
    Ok(())
}

/// Changes the vanilla page `page` in `registry` by `change`, its category
/// already checked to the key `category`; on an error the registry is left
/// as it was.
pub(super) fn change_page_in(
    registry: &mut Value,
    page: EntryId,
    category: Option<u32>,
    change: &PageChange,
) -> Result<(), Off> {
    let name = format!("journal page {}", page.get());
    let at = position(registry, COLLECTIONS, page.get())
        .ok_or_else(|| Off::new(format!("{name}: no page of the journal")))?;
    let mut held = list_mut(registry, COLLECTIONS)?[at].clone();
    set_header(
        &mut held,
        &Header {
            name: change.name,
            priority: change.priority,
            tutorial: change.tutorial,
            document: change.document,
        },
    )?;
    if let Some(category) = category {
        set(&mut held, super::LORE_CATEGORY, hash_key(category))?;
    }
    edit_steps(
        registry,
        &mut held,
        page.get(),
        &name,
        true,
        &change.entries,
    )?;
    list_mut(registry, COLLECTIONS)?[at] = held;
    Ok(())
}

/// Removes the vanilla quest `quest` from `registry`, refused while a quest
/// of another mod asks for its knowledge: one neither in the game's quests
/// `vanilla` (sorted) nor the mod's own `own`.
pub(super) fn remove_quest_in(
    registry: &mut Value,
    quest: KnowledgeId,
    vanilla: &[u32],
    own: &[u32],
) -> Result<(), Off> {
    let id = quest.get();
    let at = position(registry, QUESTS, id)
        .ok_or_else(|| Off::new(format!("quest {quest}: no quest of the journal")))?;
    let quests = list_mut(registry, QUESTS)?;
    if let Some(chained) = quests.iter().find(|held| {
        entry_id(held).is_some_and(|held| {
            held != id && vanilla.binary_search(&held).is_err() && !own.contains(&held)
        }) && asks_for(held, id)
    }) {
        return Err(Off::new(format!(
            "quest {quest}: the quest {} of another mod chains on it",
            entry_id(chained).unwrap_or_default()
        )));
    }
    let _removed = quests.remove(at);
    Ok(())
}

/// Removes the vanilla page `page` from `registry`.
pub(super) fn remove_page_in(registry: &mut Value, page: EntryId) -> Result<(), Off> {
    let at = position(registry, COLLECTIONS, page.get()).ok_or_else(|| {
        Off::new(format!(
            "journal page {}: no page of the journal",
            page.get()
        ))
    })?;
    let _removed = list_mut(registry, COLLECTIONS)?.remove(at);
    Ok(())
}
