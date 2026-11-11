//! What `enshwick-core` calls inside this crate and a mod must not: neither
//! core nor the facade re-exports it.

use crate::__load::*;
use crate::abi::StatDefData;
use crate::content::items::StatFormat;
#[cfg(feature = "typed")]
use crate::content::knowledge::{KnowledgeQuery, KnowledgeTest};
use crate::content::recipes::NewRecipe;
use crate::content::vanilla::Vanilla;
use crate::content::workshops::NewWorkshop;
use crate::content::{Guid, Id};
#[cfg(feature = "typed")]
use crate::typed::KnowledgeQueryData;
use crate::{Loading, Off};

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

/// Whether every id `new` derives fits; the reason when one does not.
///
/// # Errors
///
/// [`Off`] for a derived id that does not fit.
pub fn check_workshop(new: &NewWorkshop) -> Result<(), Off> {
    new.check()
}

/// Adds `recipe` to what the mod serves, as `Load::add_recipe` does for
/// a recipe already checked.
///
/// # Errors
///
/// [`Off`] as `Load::add_recipe`.
pub fn push_recipe(load: &mut impl Loading, recipe: NewRecipe) -> Result<(), Off> {
    load.base_mut().push_recipe(recipe)
}

/// The ABI value of `format`.
#[must_use]
pub const fn stat_format(format: StatFormat) -> u8 {
    match format {
        StatFormat::Number => StatDefData::NUMBER,
        StatFormat::Signed => StatDefData::SIGNED,
        StatFormat::Percent => StatDefData::PERCENT,
    }
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
