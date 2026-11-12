//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::content::EngineFacts as _;
use crate::content::journal::{JOURNAL_REGISTRY, QuestReward};
use crate::content::knowledge::{ExperienceValue, KnowledgeQuery, KnowledgeTest};
use crate::knowledge::{Knowledge, KnowledgeId, Scope};
use crate::locks::Relock as _;
use crate::resources::Value;
use crate::typed::{
    JournalCollectionResource, JournalEntryResource, JournalQuestResource, JournalRegistryResource,
    Named as _, QuestSourceType, QuestType, Reflected,
};
use crate::{Entity, ItemId, Load, Off};
use enshwick_knowledge::__load::*;
use std::sync::{Arc, Mutex};

#[enshwick_macros::forward(base)]
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
    pub fn quests(&mut self) -> Quests;
}
