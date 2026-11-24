//! The journal's item sets, mods' included, as the game built the journal.

use crate::content::journal::{ItemSetCategory, ItemSetId};
use crate::content::knowledge::{ExperienceValue, KnowledgeQuery};
use crate::knowledge::KnowledgeId;
use crate::typed::{
    JournalItemSetEntryHintResource, JournalItemSetEntryResource, JournalItemSetResource,
    JournalItemSetRewardResource,
};
use crate::{ItemId, RecipeId};

use super::{EntryId, Quests, TextKey};

/// The query the game's query data `data` asks, `None` for none.
fn query(data: &crate::typed::KnowledgeQueryData) -> Option<KnowledgeQuery> {
    enshwick_content::__internal::query_from_data(data)
}

/// A hint of an item set entry, as the journal holds it.
///
/// The name is enshwick's; the game's is `JournalItemSetEntryHintResource`.
///
/// # Examples
///
/// See [`ItemSetInfo`].
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct ItemSetHintInfo {
    /// When the collection screen shows it.
    pub shown_when: Option<KnowledgeQuery>,
    /// When it is ticked.
    pub done_when: Option<KnowledgeQuery>,
    /// Its objective's text.
    pub objective: Option<TextKey>,
    /// Its description.
    pub text: Option<TextKey>,
    /// The text tag of the map marker it points at.
    pub marker: Option<TextKey>,
    /// The level it suits; 0 for none.
    pub level: u8,
}

impl ItemSetHintInfo {
    fn of(hint: &JournalItemSetEntryHintResource) -> Self {
        Self {
            shown_when: query(&hint.hint_state_knowledge_query),
            done_when: query(&hint.completion_state_knowledge_query),
            objective: TextKey::of_key(hint.objective.value),
            text: TextKey::of_key(hint.hint_description.value),
            marker: TextKey::of_key(hint.map_marker_reference.value),
            level: hint.recommended_level,
        }
    }
}

/// An entry of an item set, as the journal holds it.
///
/// The name is enshwick's; the game's is `JournalItemSetEntryResource`.
///
/// # Examples
///
/// See [`ItemSetInfo`].
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct ItemSetEntryInfo {
    /// Its entry id.
    pub entry: EntryId,
    /// The item it shows.
    pub item: ItemId,
    /// When it is done; the game's own entries ask their item's pickup
    /// flag.
    pub done_when: Option<KnowledgeQuery>,
    /// The quest step the screen points at.
    pub quest: Option<EntryId>,
    /// The recipe the screen names beside it.
    pub recipe: Option<RecipeId>,
    /// Its hints, in order.
    pub hints: Vec<ItemSetHintInfo>,
}

impl ItemSetEntryInfo {
    fn of(entry: &JournalItemSetEntryResource) -> Self {
        Self {
            entry: EntryId::new(entry.entry_id.value),
            item: ItemId::new(entry.item_id.value),
            done_when: query(&entry.completion_knowledge_query),
            quest: (entry.quest_reference.value != 0)
                .then(|| EntryId::new(entry.quest_reference.value)),
            recipe: (entry.recipe_id.value != 0).then(|| RecipeId::new(entry.recipe_id.value)),
            hints: entry.hints.iter().map(ItemSetHintInfo::of).collect(),
        }
    }
}

/// An item set of the journal, the game's or a mod's: a collection the
/// game pays once every entry is done.
///
/// The name is enshwick's; the game's is `JournalItemSetResource`.
///
/// # Examples
///
/// ```
/// fn sizes(quests: &enshwick::quests::Quests) -> Vec<usize> {
///     quests.item_sets().iter().map(|set| set.entries.len()).collect()
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct ItemSetInfo {
    /// Its set id.
    pub id: ItemSetId,
    /// The category the collection screen lists it under.
    pub category: ItemSetCategory,
    /// Its title's text.
    pub name: Option<TextKey>,
    /// Its entries, in order.
    pub entries: Vec<ItemSetEntryInfo>,
    /// The recipes the collection screen lists as its reward.
    pub recipe_rewards: Vec<RecipeId>,
    /// The experience the game gives when it is complete.
    pub experience: Option<ExperienceValue>,
    /// When it is complete: the game's own sets ask an all-of query of
    /// their entries.
    pub done_when: Option<KnowledgeQuery>,
    /// When the screen shows it: the game's own sets ask an any-of query
    /// of their entries.
    pub shown_when: Option<KnowledgeQuery>,
    /// The player's knowledge its pay row names, `None` for a set without
    /// one.
    pub rewarded: Option<KnowledgeId>,
}

impl ItemSetInfo {
    /// The set `set` of the category `category`, its pay row found among
    /// `rewards` by its completion query.
    pub(super) fn of(
        category: u32,
        set: &JournalItemSetResource,
        rewards: &[JournalItemSetRewardResource],
    ) -> Self {
        let done = set.completion_knowledge_query.knowledge_or_query_id.value;
        let rewarded = rewards
            .iter()
            .find(|row| row.knowledge_query_data.knowledge_or_query_id.value == done && done != 0)
            .map(|row| KnowledgeId::new(row.reward_knowledge.id.value));
        Self {
            id: ItemSetId::new(set.set_id.value),
            category: ItemSetCategory::new(category),
            name: TextKey::of_key(set.set_name.value),
            entries: set.entries.iter().map(ItemSetEntryInfo::of).collect(),
            recipe_rewards: set
                .recipe_rewards
                .iter()
                .map(|recipe| RecipeId::new(recipe.value))
                .collect(),
            experience: (set.experience_value_id.value != 0)
                .then(|| ExperienceValue::new(set.experience_value_id.value)),
            done_when: query(&set.completion_knowledge_query),
            shown_when: query(&set.unlock_knowledge_query),
            rewarded,
        }
    }
}

#[crate::script(item_sets)]
impl Quests {
    /// Every item set of the journal, the game's and mods', by category;
    /// empty until the journal reads, such as during init.
    ///
    /// # Examples
    ///
    /// See [`ItemSetInfo`].
    #[must_use]
    #[script]
    pub fn item_sets(&self) -> Vec<ItemSetInfo> {
        self.journal().item_sets.clone()
    }
}
