//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::loot::Drop;
use crate::{Load, Off};
use enshwick_inventory::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(loot_drops)]
impl Load<'_> {
    /// Adds a mod item to random loot on the server, the dedicated one or a
    /// client's while it hosts: each roll of a container `drop.from` matches
    /// adds the item at `drop.chance`, on top of the vanilla loot. Containers
    /// roll once, when first opened, so loot already rolled in a save stays
    /// as it was.
    ///
    /// The item must be in the game's item registry, a vanilla item or one
    /// from [`Load::add_item`]; unknown items are skipped by the game.
    ///
    /// # Errors
    ///
    /// [`Off`] when the drop is not valid or the build lacks a piece.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::{Id, id};
    /// use enshwick::loot::{Drop, Source, Kinds};
    ///
    /// const STAFF: Id = id!("staff");
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.add_loot(Drop::new(enshwick::ItemId::from_guid(STAFF.guid()), 0.10, Source::bosses()))?;
    ///     load.add_loot(Drop::new(enshwick::ItemId::from_guid(STAFF.guid()), 0.02, Source::locked_chests()))?;
    ///     load.add_loot(Drop::new(enshwick::ItemId::from_guid(STAFF.guid()), 0.001, Source::kinds(Kinds::CORPSES)).count(1, 3))
    /// }
    /// ```
    #[script]
    pub fn add_loot(&mut self, drop: Drop) -> Result<(), Off>;
}
