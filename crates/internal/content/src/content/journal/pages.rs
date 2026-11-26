//! Lore pages of a mod's own, for `Load::add_journal_page`.

use super::{Guid, Id, KnowledgeQuery};

/// One entry of a journal page, shown once `shown_when` holds.
///
/// # Examples
///
/// See `Load::add_journal_page`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct PageEntry {
    /// Its title's text tag.
    pub name: Guid,
    /// Its text's tag.
    pub text: Guid,
    /// When the journal shows it.
    pub shown_when: KnowledgeQuery,
}

impl PageEntry {
    /// A `PageEntry` of its parts.
    ///
    /// # Examples
    ///
    /// See [`PageEntry`].
    #[must_use]
    pub const fn new(name: Guid, text: Guid, shown_when: KnowledgeQuery) -> Self {
        Self {
            name,
            text,
            shown_when,
        }
    }
}

/// A new lore page for `Load::add_journal_page`.
///
/// The name is enshwick's; the game's is `JournalCollectionResource`.
///
/// # Examples
///
/// See `Load::add_journal_page`.
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
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

impl NewJournalPage {
    /// A `NewJournalPage` of its parts.
    ///
    /// # Examples
    ///
    /// See [`NewJournalPage`].
    #[must_use]
    pub const fn new(id: Id, category: Guid, name: Guid, entries: Vec<PageEntry>) -> Self {
        Self {
            id,
            category,
            name,
            entries,
        }
    }
}
