//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::content::workshops::*;
use crate::engine_ids as ids;
use crate::{Load, Off};
use enshwick_content::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(content_workshops)]
impl Load<'_> {
    /// Adds a crafting station with a workshop of its own, on the server and
    /// the client alike: the vanilla station's template and item cloned, the
    /// template crafting at the new workshop, the item placing the new
    /// template, both listed beside what other mods add, a crafting menu
    /// for the workshop on the client, and optionally a copy of the vanilla
    /// station's recipe that makes the new one. Recipes
    /// target the workshop with [`NewRecipe::workshop`] or
    /// [`Derived::workshop`](crate::content::recipes::Derived::workshop).
    ///
    /// When the mod is removed, the loader serves the vanilla template under
    /// the new one's GUID, so placed stations load as the vanilla station.
    ///
    /// # Errors
    ///
    /// [`Off`] when a layout is not reflected, the id takes no derived part
    /// (see [`Id::try_part`]), the vanilla station's template lacks a
    /// crafting component (see [`at_workshop`]) or the loader refuses one of
    /// the ids (see [`Load::declare`]); a station item that does not place
    /// the station's template turns the mod off when the game first reads it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::{Id, id};
    /// use enshwick::content::workshops::{NewWorkshop, stations};
    ///
    /// const BENCH: Id = id!("bench");
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let bench = load.add_workshop(NewWorkshop {
    ///         recipe_cost: Some(1),
    ///         ..NewWorkshop::new(BENCH, stations::WORKBENCH)
    ///     })?;
    ///     assert_eq!(bench.guid(), BENCH.part("workshop").guid());
    ///     Ok(())
    /// }
    /// ```
    #[script]
    pub fn add_workshop(&mut self, new: NewWorkshop) -> Result<WorkshopId, Off>;
}
