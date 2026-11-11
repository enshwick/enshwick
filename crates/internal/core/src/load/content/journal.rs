//! `Load`'s methods of a module below core, forwarded to the base handle.

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
use crate::{Host, ItemId, Load, Off};
use enshwick_content::__load::*;
use std::sync::{Arc, Mutex};

#[enshwick_macros::forward(base)]
#[crate::script(content_journal)]
impl Load<'_> {
    /// Adds a quest to the journal beside the game's, on the server and the
    /// client alike, and returns its quest id: a player's bool the game
    /// sets when it pays the quest, as for its own. Each step shows while
    /// its `shown_when` holds and is done once its `done_when` holds. When
    /// the last step is done the game gives the experience of the rewards
    /// and raises [`QuestCompleted`](crate::events::QuestCompleted); it
    /// draws an item reward and never gives it, so the mod gives its items
    /// in a handler of that event
    /// ([`give_what_fits`](crate::inventory::Inventories::give_what_fits)).
    /// A mod moves the quest on by granting the knowledge its steps ask for.
    ///
    /// # Errors
    ///
    /// [`Off`] for a quest without steps, a count query from 0, an id the
    /// game's knowledge holds, or an id the loader refuses (see
    /// [`Load::declare`]). An id the journal already
    /// holds is logged when the game first reads it, and the quest is left
    /// out.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[script]
    pub fn add_quest(&mut self, new: NewQuest) -> Result<KnowledgeId, Off>;

    /// Adds a lore page to the journal, on the server and the client alike:
    /// its entries show once their knowledge holds, on the card of its
    /// category on the journal's world information screen. The card counts
    /// the page from the start and lists it once one entry shows.
    ///
    /// # Errors
    ///
    /// [`Off`] for a page without entries, a count query from 0, a category
    /// no page of the game's journal is listed under, or an id the loader
    /// refuses (see [`Load::declare`]).
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::id;
    /// use enshwick::content::Guid;
    /// use enshwick::content::journal::{NewJournalPage, PageEntry};
    /// use enshwick::content::knowledge::KnowledgeQuery;
    /// use enshwick::knowledge::{KnowledgeId, Scope};
    ///
    /// fn init(load: &mut enshwick::Load<'_>, read: KnowledgeId, general: Guid) -> Result<(), enshwick::Off> {
    ///     load.add_journal_page(NewJournalPage {
    ///         id: id!("shrine-lore"),
    ///         category: general,
    ///         name: id!("shrine-lore.name").guid(),
    ///         entries: vec![PageEntry {
    ///             name: id!("shrine-lore.first").guid(),
    ///             text: id!("shrine-lore.first.text").guid(),
    ///             shown_when: KnowledgeQuery::is_set(read, Scope::Player),
    ///         }],
    ///     })
    /// }
    /// ```
    #[cfg(feature = "unstable")]
    #[script]
    pub fn add_journal_page(&mut self, new: NewJournalPage) -> Result<(), Off>;

    /// Changes the vanilla quest `quest` in memory, on the server and the
    /// client alike: its rewards, whether it unlocks for every player, and
    /// its steps' knowledge. Mods that change one quest change it in load
    /// order, each on the one before's result; [`Knowledge::quests`]
    /// lists the quest ids. The game gives an added reward's experience and
    /// only draws an added item, which the mod gives on
    /// [`QuestCompleted`](crate::events::QuestCompleted), as for its own
    /// quest (see [`QuestReward`]).
    ///
    /// [`Knowledge::quests`]: crate::knowledge::Knowledge::quests
    ///
    /// # Errors
    ///
    /// [`Off`] when `quest` is no quest of the game's journal, a step it
    /// names is past the quest's, or a count query starts from 0.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::Guid;
    /// use enshwick::content::journal::{QuestChange, QuestReward};
    ///
    /// fn init(load: &mut enshwick::Load<'_>, quest: enshwick::knowledge::KnowledgeId, torch: Guid) -> Result<(), enshwick::Off> {
    ///     load.change_quest(quest, QuestChange {
    ///         add_rewards: vec![QuestReward::Item(torch)],
    ///         ..QuestChange::default()
    ///     })
    /// }
    /// ```
    #[script]
    pub fn change_quest(&mut self, quest: KnowledgeId, change: QuestChange) -> Result<(), Off>;
}
