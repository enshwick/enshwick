//! The journal's item sets: collections of items, each entry done when its
//! item is picked up, the set paying experience once every entry is. A mod
//! adds sets of its own and changes or removes the game's.
//!
//! The game keeps a set in a category of `itemSetCategories` and its pay in
//! `itemSetRewards`, a row that names the set's completion query; a set
//! shows while its unlock query holds and is complete while its completion
//! query holds, both queries of the game's query database: the game's own
//! sets ask an any-of and an all-of of their entries (server 26430253).

use crate::__load::*;
use crate::content::knowledge::{
    ExperienceValue, KnowledgeQuery, KnowledgeTest, NewKnowledge, NewQuery, QueryForm, hash_key,
    key_of_hash,
};
use crate::knowledge::{KnowledgeId, KnowledgeType, Scope};
use crate::resources::Value;
use crate::{ItemId, Off, RecipeId};

use super::{Edit, Guid, Id, LoadHelpers as _, QUERY_ID, item_set_category, list_mut};

const CATEGORIES: &str = "itemSetCategories";
const REWARDS: &str = "itemSetRewards";
const CATEGORY_NAME: &str = "categoryName";
const ENTRIES: &str = "entries";
const SET_ID: &str = "setId";
const ENTRY_ID: &str = "entryId";
const DONE: &str = "completionKnowledgeQuery";
const SHOWN: &str = "unlockKnowledgeQuery";
const EXPERIENCE: &str = "experienceValueId";
const RECIPES: &str = "recipeRewards";
const REWARD_QUERY: &str = "knowledgeQueryData";

/// The flag of an item's own knowledge the game sets when a player first
/// picks the item up, which its sets' entries ask (server 26430253).
const PICKED_UP: u32 = 1;

/// The id of an item set of the journal.
///
/// # Examples
///
/// ```
/// use enshwick::content::journal::ItemSetId;
///
/// assert_eq!(ItemSetId::new(4).get(), 4);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
pub struct ItemSetId(u32);

impl ItemSetId {
    /// The set of id `id`.
    ///
    /// # Examples
    ///
    /// See [`ItemSetId`].
    #[must_use]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Its id.
    ///
    /// # Examples
    ///
    /// See [`ItemSetId`].
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// A category of the journal's collection screen, by the key of its title
/// text, as the journal holds it; [`item_set_category`](super::item_set_category)
/// names the game's.
///
/// # Examples
///
/// ```
/// use enshwick::content::journal::{ItemSetCategory, item_set_category};
///
/// assert_eq!(ItemSetCategory::new(3_331_860_567), item_set_category::COLLECTIONS);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
pub struct ItemSetCategory(u32);

impl ItemSetCategory {
    /// The category of key `key`.
    ///
    /// # Examples
    ///
    /// See [`ItemSetCategory`].
    #[must_use]
    pub const fn new(key: u32) -> Self {
        Self(key)
    }

    /// Its key.
    ///
    /// # Examples
    ///
    /// ```
    /// assert_eq!(enshwick::content::journal::item_set_category::MUSIC.get(), 2_689_204_112);
    /// ```
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// A hint of an item set entry: an objective the collection screen shows
/// while `shown_when` holds, ticked once `done_when` holds.
///
/// The name is enshwick's; the game's is `JournalItemSetEntryHintResource`.
///
/// # Examples
///
/// See [`NewItemSet`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
pub struct ItemSetHint {
    /// When the screen shows it.
    pub shown_when: KnowledgeQuery,
    /// When it is ticked.
    pub done_when: KnowledgeQuery,
    /// Its objective's text tag.
    pub objective: Guid,
    /// Its description's text tag.
    pub text: Guid,
    /// The text tag of the map marker it points at, as a marker of
    /// `ui::map` carries it; `None` for no marker.
    pub marker: Option<Guid>,
    /// The level it suits; 0 for none.
    pub level: u8,
}

/// One entry of an item set: an item, done once `done_when` holds.
///
/// The name is enshwick's; the game's is `JournalItemSetEntryResource`.
///
/// # Examples
///
/// See [`NewItemSet`].
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
pub struct ItemSetEntry {
    /// The item it shows.
    pub item: ItemId,
    /// When it is done; `None` for once the player first picks the item up,
    /// as the game's own entries ask.
    pub done_when: Option<KnowledgeQuery>,
    /// The recipe the screen names beside it.
    pub recipe: Option<RecipeId>,
    /// Its hints, in order.
    #[script(default)]
    pub hints: Vec<ItemSetHint>,
}

impl ItemSetEntry {
    /// An entry of `item`, done once a player first picks it up, with no
    /// recipe or hints.
    ///
    /// # Examples
    ///
    /// See [`NewItemSet`].
    #[must_use]
    pub const fn new(item: ItemId) -> Self {
        Self {
            item,
            done_when: None,
            recipe: None,
            hints: Vec::new(),
        }
    }

    /// When it is done, the pickup flag of its item for none given.
    fn done(&self) -> KnowledgeQuery {
        self.done_when.unwrap_or(KnowledgeQuery {
            id: KnowledgeId::new(self.item.get()),
            scope: Scope::Player,
            test: KnowledgeTest::HasFlag(PICKED_UP),
        })
    }

    /// The game's entry of it with entry id `entry`.
    fn to_value(&self, entry: u32) -> Result<Value, Off> {
        let hints = self
            .hints
            .iter()
            .map(|hint| {
                Ok(Value::Struct(vec![
                    (
                        "hintStateKnowledgeQuery".into(),
                        hint.shown_when.to_value()?,
                    ),
                    (
                        "completionStateKnowledgeQuery".into(),
                        hint.done_when.to_value()?,
                    ),
                    ("objective".into(), hash_key(hint.objective.fnv1a())),
                    ("hintDescription".into(), hash_key(hint.text.fnv1a())),
                    (
                        "mapMarkerReference".into(),
                        hash_key(hint.marker.map_or(0, super::text_key)),
                    ),
                    ("recommendedLevel".into(), Value::Uint(hint.level.into())),
                ]))
            })
            .collect::<Result<_, Off>>()?;
        Ok(Value::Struct(vec![
            (ENTRY_ID.into(), hash_key(entry)),
            ("itemId".into(), hash_key(self.item.get())),
            (
                "lockedImageOverride".into(),
                Value::Struct(vec![("image".into(), Value::Guid(Guid([0; 16])))]),
            ),
            (DONE.into(), self.done().to_value()?),
            ("questReference".into(), hash_key(0)),
            (
                "recipeId".into(),
                hash_key(self.recipe.map_or(0, RecipeId::get)),
            ),
            ("collectibleDisplayProp".into(), hash_key(0)),
            ("hasMultipleDisplayProps".into(), Value::Bool(false)),
            ("hints".into(), Value::List(hints)),
        ]))
    }
}

/// A new item set for [`Load::add_item_set`](../../struct.Load.html#method.add_item_set).
///
/// The name is enshwick's; the game's is `JournalItemSetResource`.
///
/// # Examples
///
/// ```
/// use enshwick::ItemId;
/// use enshwick::content::id;
/// use enshwick::content::journal::{ItemSetEntry, NewItemSet, item_set_category};
///
/// fn init(load: &mut enshwick::Load<'_>, shells: [ItemId; 2]) -> Result<(), enshwick::Off> {
///     load.add_item_set(NewItemSet::new(
///         id!("shells"),
///         item_set_category::COLLECTIONS,
///         id!("shells.name").guid(),
///         shells.map(ItemSetEntry::new).to_vec(),
///     ))?;
///     Ok(())
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
pub struct NewItemSet {
    /// Its content id; its set id is the FNV-1a of the id's GUID.
    pub id: Id,
    /// The game's category the collection screen lists it under, one of
    /// [`item_set_category`](super::item_set_category); a category no set of
    /// the game's journal is listed under is refused.
    pub category: ItemSetCategory,
    /// Its title's text tag.
    pub name: Guid,
    /// Its entries, in order.
    pub entries: Vec<ItemSetEntry>,
    /// The recipes the collection screen lists as its reward.
    #[script(default)]
    pub recipe_rewards: Vec<RecipeId>,
    /// The experience the game gives when every entry is done.
    pub experience: Option<ExperienceValue>,
    /// When the screen shows it; `None` for once one entry is done, as the
    /// game's own sets show.
    pub shown_when: Option<KnowledgeQuery>,
}

impl NewItemSet {
    /// A set `id` titled `name` of `entries` under `category`, shown once
    /// one entry is done, paying nothing.
    ///
    /// # Examples
    ///
    /// See [`NewItemSet`].
    #[must_use]
    pub const fn new(
        id: Id,
        category: ItemSetCategory,
        name: Guid,
        entries: Vec<ItemSetEntry>,
    ) -> Self {
        Self {
            id,
            category,
            name,
            entries,
            recipe_rewards: Vec::new(),
            experience: None,
            shown_when: None,
        }
    }
}

/// A change of a vanilla item set for [`Load::change_item_set`](../../struct.Load.html#method.change_item_set); `None` keeps
/// what the set holds.
///
/// # Examples
///
/// See [`Load::change_item_set`](../../struct.Load.html#method.change_item_set).
#[derive(Clone, Debug, Default, PartialEq, Eq, crate::Script)]
pub struct ItemSetChange {
    /// Its title's text tag.
    pub name: Option<Guid>,
    /// The experience the game gives when it is complete.
    pub experience: Option<ExperienceValue>,
    /// The recipes the collection screen lists as its reward.
    pub recipe_rewards: Option<Vec<RecipeId>>,
    /// Its entries in place of its own, at least one; a set given entries
    /// names its `done_when`, since the game's completion query asks for the
    /// old ones.
    pub entries: Option<Vec<ItemSetEntry>>,
    /// When it is complete and pays: a game query, such as one of
    /// [`Load::add_query`](../../struct.Load.html#method.add_query).
    pub done_when: Option<KnowledgeQuery>,
    /// When the screen shows it.
    pub shown_when: Option<KnowledgeQuery>,
}

/// A set a mod adds, with the queries and knowledge the loader made for it.
#[derive(Debug)]
pub(crate) struct AddedSet {
    new: NewItemSet,
    category: u32,
    done: KnowledgeQuery,
    shown: KnowledgeQuery,
    rewarded: KnowledgeId,
}

/// One change a mod makes to the journal's item sets.
#[derive(Debug)]
pub(crate) enum SetEdit {
    /// A set of its own.
    Add(Box<AddedSet>),
    /// A change of a vanilla set.
    Change(ItemSetId, ItemSetChange),
    /// A removal of a vanilla set.
    Remove(ItemSetId),
}

/// The entry ids of a set's entries: one per entry, from the set's id.
fn entry_ids(id: Id, entries: usize) -> Vec<u32> {
    (0..entries)
        .map(|n| Guid::named(&format!("{id}.entry-{n}")).fnv1a())
        .collect()
}

/// The sets of `registry`, by category.
fn categories(registry: &Value) -> &[Value] {
    match registry.field(CATEGORIES) {
        Some(Value::List(all)) => all,
        _ => &[],
    }
}

/// The sets of the category `category`.
fn sets_of(category: &Value) -> &[Value] {
    match category.field(ENTRIES) {
        Some(Value::List(sets)) => sets,
        _ => &[],
    }
}

/// The key a `HashKey32` field `name` of `value` holds.
fn key(value: &Value, name: &str) -> Option<u32> {
    value.field(name).and_then(key_of_hash)
}

/// The id of the query a query data field `name` of `value` names.
fn query_of(value: &Value, name: &str) -> Option<u32> {
    value
        .field(name)
        .and_then(|query| query.field(QUERY_ID))
        .and_then(key_of_hash)
}

/// Whether `registry` holds a set or a set entry of id `id`; the whole
/// journal is [`super::holds`].
pub(super) fn sets_hold(registry: &Value, id: u32) -> bool {
    categories(registry).iter().flat_map(sets_of).any(|set| {
        key(set, SET_ID) == Some(id)
            || matches!(set.field(ENTRIES), Some(Value::List(entries))
                if entries.iter().any(|entry| key(entry, ENTRY_ID) == Some(id)))
    })
}

/// The category and position of the set `set` in `registry`.
fn position(registry: &Value, set: ItemSetId) -> Option<(usize, usize)> {
    categories(registry)
        .iter()
        .enumerate()
        .find_map(|(c, category)| {
            sets_of(category)
                .iter()
                .position(|held| key(held, SET_ID) == Some(set.get()))
                .map(|at| (c, at))
        })
}

/// The key of the category `category` when a set of `registry` is listed
/// under it, or an error naming those it may take.
fn category_key(registry: &Value, owner: &str, category: ItemSetCategory) -> Result<u32, Off> {
    let held: Vec<u32> = categories(registry)
        .iter()
        .filter_map(|category| key(category, CATEGORY_NAME))
        .collect();
    if held.contains(&category.get()) {
        return Ok(category.get());
    }
    let names: Vec<String> = held
        .iter()
        .map(|&held| {
            item_set_category::ALL
                .iter()
                .zip(item_set_category::NAMES)
                .find(|(known, _)| known.get() == held)
                .map_or_else(|| format!("key {held}"), |(_, name)| name.to_owned())
        })
        .collect();
    Err(Off::new(format!(
        "{owner}: the game has no item set category of key {}; its categories are {}",
        category.get(),
        names.join(", ")
    )))
}

/// A list of `RecipeId` values.
fn recipes(recipes: &[RecipeId]) -> Value {
    Value::List(
        recipes
            .iter()
            .map(|recipe| hash_key(recipe.get()))
            .collect(),
    )
}

/// Sets field `name` of `value`; a field the build lacks is an error.
fn set(value: &mut Value, name: &str, new: Value) -> Result<(), Off> {
    *value
        .field_mut(name)
        .ok_or_else(|| Off::new(format!("item set: no field {name}")))? = new;
    Ok(())
}

/// The pay row of the journal `registry` that names the completion query
/// `query`, its position.
fn reward_of(registry: &Value, query: u32) -> Option<usize> {
    match registry.field(REWARDS) {
        Some(Value::List(rows)) => rows
            .iter()
            .position(|row| query_of(row, REWARD_QUERY) == Some(query)),
        _ => None,
    }
}

impl AddedSet {
    /// Adds the set to `registry` with its pay row; on an error the registry
    /// is left as it was.
    fn apply(&self, registry: &mut Value) -> Result<(), Off> {
        let new = &self.new;
        let set = new.id.guid().fnv1a();
        let ids = entry_ids(new.id, new.entries.len());
        if let Some(id) = core::iter::once(set)
            .chain(ids.iter().copied())
            .find(|&id| super::holds(registry, id))
        {
            return Err(Off::new(format!(
                "item set {}: the journal holds {id}",
                new.id
            )));
        }
        let at = categories(registry)
            .iter()
            .position(|category| key(category, CATEGORY_NAME) == Some(self.category))
            .ok_or_else(|| Off::new(format!("item set {}: its category is gone", new.id)))?;
        let entries = new
            .entries
            .iter()
            .zip(&ids)
            .map(|(entry, &id)| entry.to_value(id))
            .collect::<Result<_, _>>()?;
        let experience = hash_key(new.experience.map_or(0, ExperienceValue::get));
        let value = Value::Struct(vec![
            (SET_ID.into(), hash_key(set)),
            ("setName".into(), hash_key(new.name.fnv1a())),
            (EXPERIENCE.into(), experience.clone()),
            (ENTRIES.into(), Value::List(entries)),
            (RECIPES.into(), recipes(&new.recipe_rewards)),
            (DONE.into(), self.done.to_value()?),
            (SHOWN.into(), self.shown.to_value()?),
        ]);
        let row = Value::Struct(vec![
            (
                "rewardKnowledge".into(),
                Value::Struct(vec![("id".into(), hash_key(self.rewarded.get()))]),
            ),
            (EXPERIENCE.into(), experience),
            (REWARD_QUERY.into(), self.done.to_value()?),
        ]);
        // The pay list is checked before the set goes in, so an error leaves the journal as it was.
        list_mut(registry, REWARDS)?;
        list_mut(&mut list_mut(registry, CATEGORIES)?[at], ENTRIES)?.push(value);
        list_mut(registry, REWARDS)?.push(row);
        Ok(())
    }
}

/// Changes the vanilla set `target` in `registry` by `change`; on an error
/// the registry is left as it was.
fn change_in(registry: &mut Value, target: ItemSetId, change: &ItemSetChange) -> Result<(), Off> {
    let name = format!("item set {}", target.get());
    let (c, at) = position(registry, target)
        .ok_or_else(|| Off::new(format!("{name}: no set of the journal")))?;
    let mut held = sets_of(&categories(registry)[c])[at].clone();
    let old_done = query_of(&held, DONE);
    if let Some(title) = change.name {
        set(&mut held, "setName", hash_key(title.fnv1a()))?;
    }
    if let Some(experience) = change.experience {
        set(&mut held, EXPERIENCE, hash_key(experience.get()))?;
    }
    if let Some(rewards) = &change.recipe_rewards {
        set(&mut held, RECIPES, recipes(rewards))?;
    }
    if let Some(entries) = &change.entries {
        let ids = fresh_ids(registry, target, entries.len());
        let values = entries
            .iter()
            .zip(ids)
            .map(|(entry, id)| entry.to_value(id))
            .collect::<Result<_, _>>()?;
        set(&mut held, ENTRIES, Value::List(values))?;
    }
    for (field, query) in [(DONE, change.done_when), (SHOWN, change.shown_when)] {
        if let Some(query) = query {
            set(&mut held, field, query.to_value()?)?;
        }
    }
    let row = old_done.and_then(|query| reward_of(registry, query));
    let mut rows = match registry.field(REWARDS) {
        Some(Value::List(rows)) => rows.clone(),
        _ => Vec::new(),
    };
    if let Some(row) = row.and_then(|row| rows.get_mut(row)) {
        if let Some(experience) = change.experience {
            set(row, EXPERIENCE, hash_key(experience.get()))?;
        }
        if let Some(done) = change.done_when {
            set(row, REWARD_QUERY, done.to_value()?)?;
        }
    }
    list_mut(&mut list_mut(registry, CATEGORIES)?[c], ENTRIES)?[at] = held;
    if row.is_some() {
        *list_mut(registry, REWARDS)? = rows;
    }
    Ok(())
}

/// Fresh entry ids for `n` entries replacing those of the set `set`: the
/// first of a run of names the journal `registry` does not hold, so every
/// side that makes the same change agrees.
fn fresh_ids(registry: &Value, set: ItemSetId, n: usize) -> Vec<u32> {
    let mut ids: Vec<u32> = Vec::with_capacity(n);
    for entry in 0..n {
        let id = (0..=u32::MAX)
            .map(|k| Guid::named(&format!("journal:set-{}.entry-{entry}.{k}", set.get())).fnv1a())
            .find(|&id| id != 0 && !ids.contains(&id) && !super::holds(registry, id))
            .unwrap_or_default();
        ids.push(id);
    }
    ids
}

/// Removes the vanilla set `set` and its pay row from `registry`.
fn remove_in(registry: &mut Value, set: ItemSetId) -> Result<(), Off> {
    let (c, at) = position(registry, set)
        .ok_or_else(|| Off::new(format!("item set {}: no set of the journal", set.get())))?;
    let done = query_of(&sets_of(&categories(registry)[c])[at], DONE);
    let row = done.and_then(|query| reward_of(registry, query));
    let _removed = list_mut(&mut list_mut(registry, CATEGORIES)?[c], ENTRIES)?.remove(at);
    if let Some(row) = row {
        let _removed = list_mut(registry, REWARDS)?.remove(row);
    }
    Ok(())
}

impl SetEdit {
    /// Makes the change in `registry`, which is left as it was on an error.
    pub(super) fn apply(&self, registry: &mut Value) -> Result<(), Off> {
        match self {
            Self::Add(added) => added.apply(registry),
            Self::Change(set, change) => change_in(registry, *set, change),
            Self::Remove(set) => remove_in(registry, *set),
        }
    }
}

/// `Load`'s item set methods on the base handle, which core's forwarders
/// call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::add_item_set`.
    #[doc(hidden)]
    fn add_item_set(&mut self, new: NewItemSet) -> Result<ItemSetId, Off>;

    /// As `Load::change_item_set`.
    #[doc(hidden)]
    fn change_item_set(&mut self, set: ItemSetId, change: ItemSetChange) -> Result<(), Off>;

    /// As `Load::remove_item_set`.
    #[doc(hidden)]
    fn remove_item_set(&mut self, set: ItemSetId) -> Result<(), Off>;
}

impl LoadMethods for crate::BaseLoad {
    fn add_item_set(&mut self, new: NewItemSet) -> Result<ItemSetId, Off> {
        let owner = format!("item set {}", new.id);
        if new.entries.is_empty() {
            return Err(Off::new(format!("{owner}: no entries")));
        }
        let part = |name: &'static str| {
            new.id
                .try_part(name)
                .ok_or_else(|| Off::new(format!("{owner}: no room for part {name} in its id")))
        };
        let (done_id, shown_id, rewarded_id) = (part("done")?, part("shown")?, part("rewarded")?);
        // Every check that can refuse runs before the first declare, so a refused set leaves
        // nothing behind; a declare of the set's own parts fails only for another mod's id.
        for (entry, id) in new.entries.iter().zip(entry_ids(new.id, new.entries.len())) {
            entry.to_value(id)?;
        }
        if let Some(shown) = new.shown_when {
            shown.to_value()?;
        }
        let key = self.journal_key()?;
        let category = category_key(&self.host().game_value(key)?, &owner, new.category)?;
        self.knowledge_without(rewarded_id)?;
        let parts: Vec<KnowledgeQuery> = new.entries.iter().map(ItemSetEntry::done).collect();
        let done_query = NewQuery::new(done_id, Scope::Player, QueryForm::All, parts.clone());
        // The mod's own shown_when, or the any-of query of the entries to add for it.
        let shown_or_query = new
            .shown_when
            .ok_or_else(|| NewQuery::new(shown_id, Scope::Player, QueryForm::Any, parts));
        self.query_without(&done_query)?;
        if let Err(query) = &shown_or_query {
            self.query_without(query)?;
        }
        self.declare(new.id)?;
        let done = KnowledgeQuery::game(self.add_query(done_query)?);
        let shown = match shown_or_query {
            Ok(shown) => shown,
            Err(query) => KnowledgeQuery::game(self.add_query(query)?),
        };
        self.declare(rewarded_id)?;
        let rewarded = self.serve_knowledge(NewKnowledge::new(
            rewarded_id,
            Scope::Player,
            KnowledgeType::Bool,
        ))?;
        let set = ItemSetId::new(new.id.guid().fnv1a());
        self.edit_journal(
            key,
            Edit::ItemSet(SetEdit::Add(Box::new(AddedSet {
                new,
                category,
                done,
                shown,
                rewarded,
            }))),
        )?;
        Ok(set)
    }

    fn change_item_set(&mut self, set: ItemSetId, change: ItemSetChange) -> Result<(), Off> {
        if change.entries.as_ref().is_some_and(Vec::is_empty) {
            return Err(Off::new(format!("item set {}: no entries", set.get())));
        }
        if change.entries.is_some() && change.done_when.is_none() {
            return Err(Off::new(format!(
                "item set {}: new entries name the set's done_when, since the game's asks for the old ones",
                set.get()
            )));
        }
        let key = self.journal_key()?;
        change_in(&mut self.host().game_value(key)?, set, &change)?;
        self.edit_journal(key, Edit::ItemSet(SetEdit::Change(set, change)))
    }

    fn remove_item_set(&mut self, set: ItemSetId) -> Result<(), Off> {
        let key = self.journal_key()?;
        if position(&self.host().game_value(key)?, set).is_none() {
            return Err(Off::new(format!(
                "item set {}: no set of the game's journal",
                set.get()
            )));
        }
        self.edit_journal(key, Edit::ItemSet(SetEdit::Remove(set)))
    }
}

#[cfg(test)]
mod tests;
