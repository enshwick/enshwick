//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::content::props::{NewProp, PropId};
use crate::{Load, Off};
use enshwick_content::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(content_props)]
impl Load<'_> {
    /// Adds a prop type, on the server and the client alike: its item a
    /// copy of the vanilla prop's, placing `<id>.template`, a copy of the
    /// template the vanilla item places with [`NewProp::template`] applied;
    /// with a cost, a copy of the vanilla prop's recipe that builds it; and
    /// with [`NewProp::crafting_prop`], a crafting prop the given recipes
    /// require.
    ///
    /// When the mod is removed, the loader serves the vanilla template under
    /// the copy's GUID, so placed props load as the vanilla one.
    ///
    /// # Errors
    ///
    /// [`Off`] when the vanilla item places nothing, the id takes no derived
    /// part (see [`Id::try_part`](crate::content::Id::try_part)), a template change does not fit (see
    /// [`ComponentChange`](crate::content::templates::ComponentChange)), the crafting prop mask is full or the loader
    /// refuses one of the ids (see [`Load::declare`]).
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::{Guid, id};
    /// use enshwick::content::items::NewItem;
    /// use enshwick::content::props::{CraftingPropSetup, NewProp};
    ///
    /// const MORTAR: Guid = Guid::parse("61575880-3db5-4ce8-b9bc-1291402a9fb4");
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let mortar = load.add_prop(NewProp {
    ///         crafting_prop: Some(CraftingPropSetup::default()),
    ///         ..NewProp::new(NewItem::new(id!("mortar"), MORTAR))
    ///     })?;
    ///     assert_eq!(mortar.crafting_prop, Some(id!("mortar").part("crafting-prop").guid()));
    ///     Ok(())
    /// }
    /// ```
    #[script]
    pub fn add_prop(&mut self, new: NewProp) -> Result<PropId, Off>;
}
