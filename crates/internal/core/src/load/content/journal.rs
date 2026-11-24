//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::content::journal::{NewJournalPage, NewQuest, QuestChange};
use crate::knowledge::KnowledgeId;
use crate::{Load, Off};
use enshwick_content::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(content_journal)]
impl Load<'_> {
    /// Adds a quest to the journal beside the game's, on the server and the
    /// client alike, and returns its quest id: a player's bool the game
    /// sets when it pays the quest, as for its own. Each step shows while
    /// its `shown_when` holds and is done once its `done_when` holds; a
    /// query left `None` gets knowledge of the loader's, which
    /// [`Quests`](crate::quests::Quests) moves on, and each objective of a
    /// step a counter the loader raises on its events. When
    /// the last step is done the game gives the experience of the rewards
    /// and raises [`QuestCompleted`](crate::events::QuestCompleted); it
    /// draws an item reward and never gives it, so the mod gives its items
    /// in a handler of that event
    /// ([`give_what_fits`](crate::inventory::Inventories::give_what_fits)).
    /// A mod moves the quest on with [`Quests`](crate::quests::Quests) or by
    /// granting the knowledge its steps ask for.
    ///
    /// # Errors
    ///
    /// [`Off`] for a quest without steps, a count query from 0, an
    /// objective counting from 0 or a reach objective without finite
    /// coordinates and a radius above 0, an id too long to name the loader's parts in, an id
    /// the game's knowledge holds, or an id the loader refuses (see
    /// [`Load::declare`]); a refused quest declares nothing. An id the
    /// journal already holds is logged when the game first reads it, and
    /// the quest is left out. A kind of objective the build cannot count,
    /// such as a kill on a build without the event, is logged and only it
    /// stays at 0.
    ///
    /// # Examples
    ///
    /// See the [module](crate::content::journal).
    #[script]
    pub fn add_quest(&mut self, new: NewQuest) -> Result<KnowledgeId, Off> {
        let quest = self.base.add_quest(new)?;
        #[cfg(feature = "typed")]
        enshwick_knowledge::__internal::count_objectives(&mut self.base);
        Ok(quest)
    }

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
    /// quest (see [`QuestReward`](crate::content::journal::QuestReward)).
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

    /// Removes the vanilla quest `quest` from the journal in memory, on the
    /// server and the client alike, with its steps; its knowledge stays, so
    /// a save that holds its progress still opens. Mods change and remove a
    /// quest in load order. A removal is logged and left out when the
    /// journal first builds while anything a mod earlier in load order put
    /// in the journal asks for the quest's or one of its steps' id: a step
    /// of its own quest, a step it inserted into a vanilla quest or whose
    /// queries it changed, an entry of a lore page, or an item set. It asks
    /// by the id itself or through a game query that reads it, such as one
    /// of [`Load::add_query`], with the queries it names in turn; while the
    /// game's queries do not read, every game query counts. The game's own
    /// steps, entries and sets never hold a quest.
    ///
    /// # Errors
    ///
    /// [`Off`] when `quest` is no quest of the game's journal.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>, tutorial: enshwick::knowledge::KnowledgeId) -> Result<(), enshwick::Off> {
    ///     load.remove_quest(tutorial)
    /// }
    /// ```
    #[script]
    pub fn remove_quest(&mut self, quest: KnowledgeId) -> Result<(), Off>;

    /// Changes the vanilla lore page `page` in memory, on the server and the
    /// client alike: its category, title, priority, tutorial mark, document
    /// and entries. Mods that change one page change it in load order.
    ///
    /// # Errors
    ///
    /// [`Off`] when `page` is no page of the game's journal, a category no
    /// page of the game's journal is listed under, an entry an edit names
    /// past the page's, an inserted entry without `shown_when`, or a count
    /// query from 0.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::id;
    /// use enshwick::content::journal::{EntryId, PageChange};
    ///
    /// fn init(load: &mut enshwick::Load<'_>, page: EntryId) -> Result<(), enshwick::Off> {
    ///     load.change_journal_page(page, PageChange {
    ///         name: Some(id!("old-page.name").guid()),
    ///         ..PageChange::default()
    ///     })
    /// }
    /// ```
    #[cfg(feature = "unstable")]
    #[script]
    pub fn change_journal_page(
        &mut self,
        page: crate::content::journal::EntryId,
        change: crate::content::journal::PageChange,
    ) -> Result<(), Off>;

    /// Removes the vanilla lore page `page` from the journal in memory, on
    /// the server and the client alike, with its entries.
    ///
    /// # Errors
    ///
    /// [`Off`] when `page` is no page of the game's journal.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>, page: enshwick::content::journal::EntryId) -> Result<(), enshwick::Off> {
    ///     load.remove_journal_page(page)
    /// }
    /// ```
    #[cfg(feature = "unstable")]
    #[script]
    pub fn remove_journal_page(
        &mut self,
        page: crate::content::journal::EntryId,
    ) -> Result<(), Off>;

    /// Adds an item set to the journal's collections beside the game's, on
    /// the server and the client alike, and returns its set id. Each entry
    /// is done once its `done_when` holds, by default once a player first
    /// picks its item up, as the game's own entries are. The set shows once
    /// `shown_when` holds, by default once one entry is done, and is
    /// complete once every entry is: the loader adds those two queries to
    /// the game's query database (see [`Load::add_query`]), as the game's
    /// own sets ask theirs. The game's server walks the pay rows, at most 50
    /// a run: for a player without the set's bool, a bool of the loader's,
    /// whose completion query holds, it gives the set's experience and sets
    /// the bool, so a set pays once (server 26430253, read in the code). The
    /// set's recipes are what the collection screen lists; whether the game
    /// grants them is not read yet, so a mod that wants them granted grants
    /// them itself.
    ///
    /// # Errors
    ///
    /// [`Off`] for a set without entries, a count query from 0, an id too
    /// long to name the loader's parts in, a category no set of the game's
    /// journal is listed under, or an id the loader refuses (see
    /// [`Load::declare`]). An id the journal already holds is logged when the
    /// game first reads it, and the set is left out.
    ///
    /// # Examples
    ///
    /// See [`NewItemSet`](crate::content::journal::NewItemSet).
    #[script]
    pub fn add_item_set(
        &mut self,
        new: crate::content::journal::NewItemSet,
    ) -> Result<crate::content::journal::ItemSetId, Off>;

    /// Changes the vanilla item set `set` in memory, on the server and the
    /// client alike: its title, experience, recipes, entries and queries,
    /// and its pay row with them. Mods that change one set change it in
    /// load order; [`Quests::item_sets`](crate::quests::Quests::item_sets)
    /// lists the sets.
    ///
    /// # Errors
    ///
    /// [`Off`] when `set` is no set of the game's journal, new entries are
    /// none or come without the set's `done_when`, or a count query starts
    /// from 0.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::journal::{ItemSetChange, ItemSetId};
    /// use enshwick::content::knowledge::ExperienceValue;
    ///
    /// fn init(load: &mut enshwick::Load<'_>, set: ItemSetId, more: ExperienceValue) -> Result<(), enshwick::Off> {
    ///     load.change_item_set(set, ItemSetChange {
    ///         experience: Some(more),
    ///         ..ItemSetChange::default()
    ///     })
    /// }
    /// ```
    #[script]
    pub fn change_item_set(
        &mut self,
        set: crate::content::journal::ItemSetId,
        change: crate::content::journal::ItemSetChange,
    ) -> Result<(), Off>;

    /// Removes the vanilla item set `set` from the journal in memory, on the
    /// server and the client alike, with its entries and its pay row; the
    /// knowledge its entries ask stays.
    ///
    /// # Errors
    ///
    /// [`Off`] when `set` is no set of the game's journal.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>, set: enshwick::content::journal::ItemSetId) -> Result<(), enshwick::Off> {
    ///     load.remove_item_set(set)
    /// }
    /// ```
    #[script]
    pub fn remove_item_set(&mut self, set: crate::content::journal::ItemSetId) -> Result<(), Off>;
}
