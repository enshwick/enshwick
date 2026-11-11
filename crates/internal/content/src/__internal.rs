//! What `enshwick-core` calls inside this crate and a mod must not: neither
//! core nor the facade re-exports it.

use crate::Off;
#[cfg(feature = "typed")]
use crate::content::knowledge::{KnowledgeQuery, KnowledgeTest};
use crate::content::vanilla::Vanilla;
use crate::content::{Guid, Id};
#[cfg(feature = "typed")]
use crate::typed::KnowledgeQueryData;

/// The fields of an image resource the fake host fills.
pub const IMAGE_FIELDS: [&str; 6] = crate::content::icons::IMAGE_FIELDS;

/// The part of a crafting prop that lists its recipes.
pub const PROP_RECIPES: &str = crate::content::props::RECIPES;

/// `id`, once the loader lets this mod name it.
///
/// # Errors
///
/// [`Off`] when the loader refuses the id.
pub fn named(id: Id) -> Result<Id, Off> {
    crate::content::named(id)
}

/// Whether `value` passes `test`; `None` for a test this API cannot run.
#[cfg(feature = "typed")]
#[must_use]
pub const fn passes(test: KnowledgeTest, value: u32) -> Option<bool> {
    test.passes(value)
}

/// The query a game's knowledge query data states; `None` for one this API
/// does not name.
#[cfg(feature = "typed")]
#[must_use]
pub fn query_from_data(data: &KnowledgeQueryData) -> Option<KnowledgeQuery> {
    KnowledgeQuery::from_data(data)
}

/// Whether `guid` is one of `vanilla`'s items.
#[must_use]
pub fn is_item(vanilla: &Vanilla, guid: Guid) -> bool {
    vanilla.is_item(guid)
}

/// Whether `guid` is one of `vanilla`'s recipes.
#[must_use]
pub fn is_recipe(vanilla: &Vanilla, guid: Guid) -> bool {
    vanilla.is_recipe(guid)
}

/// The vanilla item of the item id `id`.
#[must_use]
pub fn item_with_id(vanilla: &Vanilla, id: u32) -> Option<Guid> {
    vanilla.item_with_id(id)
}
