//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::__private::{INVENTORY_LINKS as LINKS, INVENTORY_SLOTS as SLOTS};
use crate::Entity;
use crate::Load;
use crate::Off;
use enshwick_inventory::__load::*;

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// Inventory reads and transactions in the game server, dedicated or
    /// hosted by the client; a mod asks for them during `init`.
    ///
    /// # Errors
    ///
    /// [`Off`](crate::Off) when this game build lacks a piece of them.
    ///
    /// # Examples
    ///
    /// See [`Inventories`].
    pub fn inventories(&mut self) -> Result<Inventories, crate::Off>;
}
