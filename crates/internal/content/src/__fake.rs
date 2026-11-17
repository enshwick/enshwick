//! What the fake loader of `enshwick-fake` and core's tests read inside
//! this crate; only the `fake` feature, which core's turns on, builds it.

#[cfg(feature = "typed")]
use crate::Off;
use crate::content::Guid;
#[cfg(feature = "typed")]
use crate::content::journal::NewQuest;
#[cfg(feature = "typed")]
use crate::content::knowledge::KnowledgeQuery;
use crate::content::vanilla::{Vanilla, fixtures};
pub use crate::rates::{Rate, scale_whole};
#[cfg(feature = "typed")]
use crate::resources::Value;

/// A field a mod built without `unstable` cannot name (today a stack size or
/// container slots, both items'), whose raw changes the fake host takes as
/// the loader, always built with the feature, does.
#[derive(Clone, Copy, Debug)]
pub struct UnnamedField(crate::content::changes::rules::Rules);

impl UnnamedField {
    /// The field `raw` changes, when this build has no [`Field`] for it but
    /// the loader has.
    ///
    /// [`Field`]: crate::content::changes::Field
    #[must_use]
    pub fn of(raw: &crate::abi::DataChange) -> Option<Self> {
        if crate::content::changes::Field::from_abi(raw.field).is_some() {
            return None;
        }
        crate::content::changes::rules::Rules::of(raw.field).map(Self)
    }

    /// Whether the loader takes `raw`, a change of this field: its target
    /// and its value.
    #[must_use]
    pub fn takes(self, raw: &crate::abi::DataChange) -> bool {
        self.0.takes(raw)
    }

    /// Whether the field takes `value`, as a later set of a live change.
    #[must_use]
    pub fn takes_value(self, value: crate::content::changes::Value) -> bool {
        self.0.check(value).is_ok()
    }
}

/// The fields of an image resource the game's catalogue names, in the order
/// an icon reads them.
pub const IMAGE_FIELDS: [&str; 6] = crate::content::icons::IMAGE_FIELDS;

/// A vanilla game with the items `named`, a forge station of its own
/// workshop, a survivor's workshop and an item that places a plain template.
#[must_use]
pub fn game(named: &[(Guid, &str)]) -> Vanilla {
    fixtures::game(named)
}

/// [`game`] with a recipe registry: each recipe's GUID, debug name and the
/// item it makes.
#[must_use]
pub fn game_with_recipes(named: &[(Guid, &str)], recipes: &[(Guid, &str, Guid)]) -> Vanilla {
    fixtures::game_with_recipes(named, recipes)
}

/// [`game`] with the item `chest`, named `name`, which places a container
/// of `slots` slots.
#[must_use]
pub fn game_with_chest(chest: Guid, name: &str, slots: u8) -> Vanilla {
    fixtures::game_with_chest(chest, name, slots)
}

/// A vanilla game whose templates are the containers `containers`: name
/// and fill table each.
#[must_use]
pub fn game_with_tables(containers: &[(&str, Guid)]) -> Vanilla {
    fixtures::game_with_tables(containers)
}

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
