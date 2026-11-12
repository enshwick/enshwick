//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::abi::{BuffTypeId, KnowledgeId, ModHandle, RecipeId};
use crate::game_events::{
    CraftRecipeEvent, CraftRecipeSuccessEvent, LevelUpEvent, LootInteractionEvent,
};
use crate::locks::Relock as _;
use crate::{Entity, ItemId, Load, Off};
use enshwick_knowledge::__load::*;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[enshwick_macros::forward(base)]
#[crate::script(rewards)]
impl Load<'_> {
    /// Declares `reward`: the server gives it in the server's world when a
    /// player does its action, within its limit and chance, if the mod's
    /// filters allow.
    ///
    /// # Errors
    ///
    /// [`Off`] after init, for a malformed name, a name declared before, a
    /// reward that gives nothing, a chance outside 0 to 1, or a build
    /// without what its action or the world's data need.
    ///
    /// # Examples
    ///
    /// See the [module](crate::rewards).
    #[script]
    pub fn add_reward(&mut self, reward: Reward) -> Result<(), Off>;

    /// Runs `filter` before the mod gives any of its rewards, on the game
    /// thread; returning `false` holds the grant back, and the limit stays
    /// unused. To give something else instead, give it with
    /// [`Rewards::give`] and return `false`.
    ///
    /// # Errors
    ///
    /// As [`Load::add_reward`].
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     // No rewards for the first player, entity 1.
    ///     load.on_reward(|grant| grant.player != enshwick::Entity(1))
    /// }
    /// ```
    #[script]
    pub fn on_reward<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&RewardGrant) -> bool + Send + Sync + 'static;

    /// The handle that gives the mod's declared rewards from code.
    ///
    /// # Errors
    ///
    /// As [`Load::add_reward`].
    ///
    /// # Examples
    ///
    /// See [`Rewards`].
    pub fn rewards(&mut self) -> Result<Rewards, Off>;
}
