//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::content::items::NewItem;
use crate::{Load, Off};
use enshwick_content::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(content_items)]
impl Load<'_> {
    /// Adds an item to the game, on the server and the client alike: the
    /// item data cloned from a vanilla item, listed in the item registry
    /// beside items other mods add, and with a [`FuelSetup`](crate::content::items::FuelSetup) its entity
    /// template, listed in the template collection. Players see it once a
    /// recipe, loot or an action gives it to them.
    ///
    /// # Errors
    ///
    /// [`Off`] when the item layout is not reflected,
    /// the meter's maximum is 0, or the loader refuses one of its content ids
    /// (see [`Load::declare`]).
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::{Guid, Id, id};
    /// use enshwick::content::items::{FuelSetup, NewItem};
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let mut item = NewItem::new(
    ///             id!("charged-locker"),
    ///             Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d"),
    ///         );
    ///     item.meter = Some(FuelSetup::new(id!("charges"), 3, 3));
    ///     load.add_item(item)
    /// }
    /// ```
    #[script]
    pub fn add_item(&mut self, mut item: NewItem) -> Result<(), Off>;
}
