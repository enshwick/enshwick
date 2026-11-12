//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::Entity;
use crate::abi::{AffixRoll, Header, Status};
use crate::affixes::{AffixCraft, AffixLoot};
use crate::game_events::CraftRecipeEvent;
use crate::inventory::{ItemInstance, Slot};
use crate::locks::Relock;
use crate::loot::Source;
use crate::{Load, Off};
use enshwick_inventory::__load::*;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[enshwick_macros::forward(base)]
#[crate::script(affix_rolls)]
impl Load<'_> {
    /// Rolls affixes onto the items with an entity that random loot puts
    /// into containers `rule.from` matches, each at `rule.chance`, as the
    /// container rolls its loot. Each item rolls once: an item rolled
    /// before, by this rule or another, stays as it is. On the server.
    ///
    /// # Errors
    ///
    /// [`Off`] for a chance outside 0 to 1, or when the build lacks the
    /// loot or inventory pieces.
    ///
    /// # Examples
    ///
    /// See the [module](crate::affixes).
    #[script]
    pub fn add_loot_affixes(&mut self, rule: AffixLoot) -> Result<(), Off>;

    /// Rolls affixes onto the items with an entity a player crafts, each at
    /// `rule.chance`: the items new in the crafter's inventories within a
    /// second of the craft request. Each item rolls once. On the server.
    ///
    /// # Errors
    ///
    /// [`Off`] for a chance outside 0 to 1, or when the build lacks the
    /// event or inventory pieces.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::affixes::{AffixCraft, RollRules};
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.add_craft_affixes(AffixCraft::new(0.2, RollRules::new(1, 1)))
    /// }
    /// ```
    #[script]
    pub fn add_craft_affixes(&mut self, rule: AffixCraft) -> Result<(), Off>;
}
