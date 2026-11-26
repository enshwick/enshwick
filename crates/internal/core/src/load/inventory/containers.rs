//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::ItemId;
use crate::content::props::PropId;
use crate::inventory::containers::{NewContainer, Put};
use crate::vetoes::Verdict;
use crate::{Load, Off};
use enshwick_inventory::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(containers)]
impl Load<'_> {
    /// Adds a container type, on the server and the client alike: a prop
    /// as [`Load::add_prop`] adds it, whose template holds
    /// [`NewContainer::slots`] slots.
    ///
    /// A placed container keeps the slots it was placed with; the loader
    /// serves the vanilla template when the mod is removed, as for any prop.
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// # Errors
    ///
    /// [`Off`] for a slot count the game cannot hold (see
    /// [`NewContainer::slots`]), a vanilla item that places no container, or
    /// any refusal of [`Load::add_prop`].
    ///
    /// # Examples
    ///
    /// See the [module](crate::inventory::containers).
    #[script]
    pub fn add_container(&mut self, new: NewContainer) -> Result<PropId, Off>;

    /// Runs `rule` before a stack enters a slot of a container placed from
    /// `item`, the mod's own or a vanilla one; a refusal (`false` or a
    /// [`Verdict`] that refuses) keeps the stack out as a slot that does not take it does: it stays where it
    /// was, or goes to another slot when the game picks the slots. The mods'
    /// rules for the item are asked in load order until one keeps the stack
    /// out; the rules after it are not called for that stack.
    ///
    /// It answers for whatever moves the stack: a player's move, a swap,
    /// Take All, Deposit All or Stacks, Refill Stacks and an area deposit,
    /// one call per stack. Gives through
    /// [`Inventories`](crate::inventory::Inventories) and the game's own
    /// loot are not asked. It runs on the thread of the system that moves
    /// the items, in the server's world and in a client's own prediction:
    /// keep it quick and never block.
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// # Errors
    ///
    /// [`Off`] after init, for item 0, or on a build that lacks the put.
    ///
    /// # Examples
    ///
    /// See the [module](crate::inventory::containers).
    pub fn container_rule<F, V>(&mut self, item: ItemId, rule: F) -> Result<(), Off>
    where
        F: Fn(&Put) -> V + Send + Sync + 'static,
        V: Into<Verdict>;

    /// Runs `rule` before a stack enters a slot of a container placed from
    /// `item`, the mod's own or a vanilla one; `false` or a string keeps the
    /// stack out as a slot that does not take it does, nil or `true` lets it
    /// in, and any other value keeps it out. The mods' rules for the item
    /// are asked in load order until one keeps the stack out. It answers for
    /// whatever moves the stack, one call per stack, but not for gives
    /// through `Inventories` or the game's own loot. It runs on the thread
    /// of the system that moves the items, on both sides: keep it quick and
    /// never block.
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// # Errors
    ///
    /// As [`Load::container_rule`].
    #[script(name = "container_rule")]
    pub(crate) fn put_filter<F>(&mut self, item: ItemId, rule: F) -> Result<(), Off>
    where
        F: Fn(&Put) -> Verdict + Send + Sync + 'static;
}
