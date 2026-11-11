//! What the fake loader of `enshwick-fake` and core's tests read inside
//! this crate; only the `fake` feature, which core's turns on, builds it.

#[cfg(feature = "typed")]
use crate::Off;
#[cfg(feature = "typed")]
use crate::content::journal::NewQuest;
#[cfg(feature = "typed")]
use crate::content::knowledge::KnowledgeQuery;
pub use crate::content::vanilla::fixtures::{
    game, game_with_chest, game_with_recipes, game_with_tables,
};
pub use crate::rates::{Rate, scale_whole};
#[cfg(feature = "typed")]
use crate::resources::Value;

/// The fields of an image resource the game's catalogue names, in the order
/// an icon reads them.
pub const IMAGE_FIELDS: [&str; 6] = crate::content::icons::IMAGE_FIELDS;

/// Whether `factor` is one a mod may set for a rate: finite, from 0 to the
/// most the loader takes.
#[must_use]
pub fn is_valid_factor(factor: f32) -> bool {
    crate::rates::is_valid_factor(factor)
}

/// The factor in force for a rate whose declaring mods set `factors`: their
/// product, held to the range a mod may set; 1 when there are none.
#[must_use]
pub fn combine(factors: impl IntoIterator<Item = f32>) -> f32 {
    crate::rates::combine(factors)
}

/// The game's value of `query`, as a quest step holds it.
///
/// # Errors
///
/// [`Off`] for a query the game cannot hold.
#[cfg(feature = "typed")]
pub fn query_value(query: KnowledgeQuery) -> Result<Value, Off> {
    query.to_value()
}

/// The journal's value of the quest `new`, under the knowledge key `key`.
///
/// # Errors
///
/// [`Off`] for a quest the game cannot hold.
#[cfg(feature = "typed")]
pub fn quest_value(new: &NewQuest, key: u32) -> Result<Value, Off> {
    crate::content::journal::quest_value(new, crate::content::knowledge::hash_key(key))
}
