//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::abi::ItemAction as RawItemAction;
use crate::inventory::SlotRef;
use crate::{Entity, ItemId};
use crate::{Load, Off};
use core::ffi::c_void;
use enshwick_inventory::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(items)]
impl Load<'_> {
    /// Runs `filter` on each player's action with any item in the server's
    /// world; returning `false` refuses a [`Kind::Consume`],
    /// [`Kind::Trash`], [`Kind::Equip`] or [`Kind::Unequip`] the game's own
    /// way, and is ignored for the others. It runs on the thread of the
    /// system that runs the action, once per action: keep it quick and never
    /// block.
    ///
    /// # Errors
    ///
    /// [`Off`] after init or on a build without the player input it reads.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::items::Kind;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.on_item_action(|action| {
    ///         if action.kind == Kind::Pickup {
    ///             enshwick::info!("{} items picked up", action.count);
    ///         }
    ///         true
    ///     })
    /// }
    /// ```
    #[script]
    pub fn on_item_action<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&ItemAction) -> bool + Send + Sync + 'static;

    /// Runs `handler` when a player does `kind` with `item`, as
    /// [`Load::on_item_action`] runs its filter: returning `false` refuses
    /// the action where [`Kind::can_refuse`] says it can. Actions with other
    /// items or of other kinds never reach it.
    ///
    /// # Errors
    ///
    /// As [`Load::on_item_action`].
    ///
    /// # Examples
    ///
    /// See the [module](crate::items).
    #[script]
    pub fn on_item<F>(&mut self, item: ItemId, kind: Kind, handler: F) -> Result<(), Off>
    where
        F: Fn(&ItemAction) -> bool + Send + Sync + 'static;
}
