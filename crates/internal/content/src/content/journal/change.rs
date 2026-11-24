//! Changes and removals of the game's quests and lore pages: every field the
//! journal holds, steps inserted, removed and moved.

use std::cell::LazyCell;

use rustc_hash::FxHashMap;

use crate::content::knowledge::{KnowledgeQuery, hash_key, key_of_hash};
use crate::knowledge::KnowledgeId;
use crate::resources::Value;
use crate::typed::{KnowledgeQueryDataType, QuestSourceType, QuestType, Reflected as _};
use crate::{ItemId, Off};

use super::{
    COLLECTIONS, DOCUMENT, DONE, ENTRIES, EntryId, FOR_ALL, Guid, PROGRESS, QUERY_ID, QUESTS,
    QuestReward, QuestStep, REWARDS, SHOWN, TYPE, entry_id, holds, item_sets, list_mut, never,
    text_key,
};

/// The knowledge each game query reads, by query id, sorted; `None` when
/// the query database does not read, so every game query may ask.
pub(super) type Reads = Option<Vec<(u32, Vec<u32>)>>;

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
    value.field(QUERY_ID).and_then(key_of_hash)
}

/// Whether the step query `query` asks for one of the knowledge `ids`: names
/// it, or is a game query that `reads` lists it under; `reads` is read only
/// for a game query.
fn reads_knowledge(
    query: &Value,
    ids: &[u32],
    reads: &LazyCell<Reads, impl FnOnce() -> Reads>,
) -> bool {
    let Some(named) = query_id(query) else {
        return false;
    };
    ids.contains(&named)
        || matches!(
            query.field(TYPE).map(KnowledgeQueryDataType::from_value),
            Some(Ok(KnowledgeQueryDataType::Extern))
        ) && LazyCell::force(reads).as_ref().is_none_or(|reads| {
            reads
                .binary_search_by_key(&named, |(query, _)| *query)
                .is_ok_and(|at| reads[at].1.iter().any(|read| ids.contains(read)))
        })
}

/// Every knowledge query in `value`, in order: each struct that names a
/// query id, nested ones too.
fn queries(value: &Value) -> Vec<&Value> {
    fn walk<'v>(value: &'v Value, out: &mut Vec<&'v Value>) {
        match value {
            Value::Struct(_) if value.field(QUERY_ID).is_some() => out.push(value),
            Value::Struct(fields) => fields.iter().for_each(|(_, field)| walk(field, out)),
            Value::List(items) => items.iter().for_each(|item| walk(item, out)),
            Value::Variant(variant) => walk(variant.value(), out),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(value, &mut out);
    out
}

/// Every step of every collection in the list `list` of `journal`, with
/// the ids of its collection and its own.
fn steps<'v>(journal: &'v Value, list: &str) -> impl Iterator<Item = (u32, u32, &'v Value)> {
    let listed = |value: Option<&'v Value>| match value {
        Some(Value::List(items)) => items.as_slice(),
        _ => &[],
    };
    listed(journal.field(list))
        .iter()
        .filter_map(|held| Some((entry_id(held)?, held)))
        .flat_map(move |(owner, held)| {
            listed(held.field(ENTRIES))
                .iter()
                .filter_map(move |step| Some((owner, entry_id(step)?, step)))
        })
}

/// The queries of `found` the game's same entry, `game`, does not hold, each
/// of the game's matching one copy: all of them where the game has none.
fn added(found: Vec<&Value>, game: Option<&[&Value]>) -> Vec<Value> {
    let mut unmatched = game.unwrap_or_default().to_vec();
    let mut put = Vec::new();
    for query in found {
        match unmatched.iter().position(|held| *held == query) {
            Some(at) => {
                let _matched = unmatched.swap_remove(at);
            }
            None => put.push(query.clone()),
        }
    }
    put
}

/// The queries another mod put in the journal before a patch, which the
/// game's archive does not hold there: by list, collection and step, and by
/// item set. The patch's own changes come after, so they are never among
/// them. Made once per patch that removes a quest.
#[derive(Debug, Default)]
pub(super) struct Others {
    steps: FxHashMap<(&'static str, u32, u32), Vec<Value>>,
    sets: FxHashMap<u32, Vec<Value>>,
}

/// What other mods put in the journal, for a patch with a removal, or why
/// the game's journal did not read; `None` for a patch without one.
pub(super) type Found = Option<Result<Others, Off>>;

impl Others {
    /// What `found`, the journal a patch was given, asks that the game's
    /// journal `game` does not.
    pub(super) fn new(found: &Value, game: &Value) -> Self {
        let mut others = Self::default();
        for list in [QUESTS, COLLECTIONS] {
            let game_steps: FxHashMap<(u32, u32), Vec<&Value>> = steps(game, list)
                .map(|(owner, step, value)| ((owner, step), queries(value)))
                .collect();
            for (owner, step, value) in steps(found, list) {
                let put = added(
                    queries(value),
                    game_steps.get(&(owner, step)).map(Vec::as_slice),
                );
                if !put.is_empty() {
                    others.steps.insert((list, owner, step), put);
                }
            }
        }
        let game_sets: FxHashMap<u32, Vec<&Value>> = item_sets::sets(game)
            .map(|(set, value)| (set, queries(value)))
            .collect();
        for (set, value) in item_sets::sets(found) {
            let put = added(queries(value), game_sets.get(&set).map(Vec::as_slice));
            if !put.is_empty() {
                others.sets.insert(set, put);
            }
        }
        others
    }

    /// The others `found` holds.
    ///
    /// # Errors
    ///
    /// [`Off`] when the game's journal did not read, or was not read.
    pub(super) fn of(found: &Found) -> Result<&Self, Off> {
        match found {
            Some(Ok(others)) => Ok(others),
            Some(Err(off)) => Err(Off::new(format!("the game's journal: {off}"))),
            None => Err(Off::new("the game's journal was not read")),
        }
    }

    /// Why the quest `quest` stays: a query another mod put in a step of a
    /// quest or an entry of a page, or in an item set, still in `journal`,
    /// that asks for one of the knowledge `ids`, itself or through a game
    /// query of `reads`; `None` when none does.
    fn hold(
        &self,
        journal: &Value,
        quest: u32,
        ids: &[u32],
        reads: &LazyCell<Reads, impl FnOnce() -> Reads>,
    ) -> Option<String> {
        let asks = |queries: &[Value]| {
            queries
                .iter()
                .any(|query| reads_knowledge(query, ids, reads))
        };
        for (list, kind, part) in [
            (QUESTS, "quest", "step"),
            (COLLECTIONS, "journal page", "entry"),
        ] {
            if let Some((owner, ..)) = steps(journal, list).find(|&(owner, step, _)| {
                (list != QUESTS || owner != quest)
                    && self
                        .steps
                        .get(&(list, owner, step))
                        .is_some_and(|put| asks(put))
            }) {
                return Some(format!(
                    "another mod's {part} in the {kind} {owner} asks for it"
                ));
            }
        }
        item_sets::sets(journal)
            .find(|&(set, _)| self.sets.get(&set).is_some_and(|put| asks(put)))
            .map(|(set, _)| format!("another mod's item set {set} asks for it"))
    }
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
        set(&mut held, TYPE, kind.to_value())?;
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

/// Removes the vanilla quest `quest` from `registry`, refused while anything
/// `others` tells another mod put in the journal asks for its or a step's
/// knowledge, itself or through a game query of `reads`: a step of a quest,
/// vanilla or its own, an entry of a lore page, or an item set.
pub(super) fn remove_quest_in(
    registry: &mut Value,
    quest: KnowledgeId,
    others: &Others,
    reads: &LazyCell<Reads, impl FnOnce() -> Reads>,
) -> Result<(), Off> {
    let id = quest.get();
    let at = position(registry, QUESTS, id)
        .ok_or_else(|| Off::new(format!("quest {quest}: no quest of the journal")))?;
    let mut ids = vec![id];
    if let Some(Value::List(all)) = registry.field(QUESTS)
        && let Some(Value::List(steps)) = all[at].field(ENTRIES)
    {
        ids.extend(steps.iter().filter_map(entry_id));
    }
    if let Some(why) = others.hold(registry, id, &ids, reads) {
        return Err(Off::new(format!("quest {quest}: {why}")));
    }
    let _removed = list_mut(registry, QUESTS)?.remove(at);
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
