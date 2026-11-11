//! `Load`'s methods of a module below core, forwarded to the base handle.

#[cfg(feature = "unstable")]
use crate::RecipeId;
use crate::content::items::NewItem;
use crate::content::props::{NewProp, PropId};
#[cfg(feature = "unstable")]
use crate::content::{recipes::RECIPE_REGISTRY, types};
#[cfg(feature = "unstable")]
use crate::resources::Value;
use crate::{Load, Off};
#[cfg(feature = "unstable")]
use enshwick_content::__internal::PROP_RECIPES as RECIPES;
use enshwick_world::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(flame_altars)]
impl Load<'_> {
    /// Adds a kind of Flame Altar, on the server and the client alike: a
    /// prop copied from the vanilla altar whose item is `item.like`, as
    /// [`Load::add_prop`] adds one, whose altars take the kind's zones
    /// through an altar zone filter of this mod, chained with other mods'
    /// filters in load order (see [`Load::on_altar_zone`]).
    /// [`Altar::template`](crate::altars::Altar::template) tells the kind's altars
    /// apart.
    ///
    /// # Errors
    ///
    /// As [`Load::add_prop`] and [`Load::on_altar_zone`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::altars::{KindZone, NewFlameAltar};
    /// use enshwick::content::{Guid, id};
    /// use enshwick::content::items::NewItem;
    ///
    /// fn init(load: &mut enshwick::Load<'_>, altar: Guid) -> Result<(), enshwick::Off> {
    ///     let wide = load.add_flame_altar(NewFlameAltar {
    ///         item: NewItem::new(id!("wide-altar"), altar),
    ///         zones: vec![KindZone { grow: 20, height: 80 }],
    ///         recipe_cost: Some(2),
    ///     })?;
    ///     enshwick::info!("wide altars are {}", wide.template.guid());
    ///     Ok(())
    /// }
    /// ```
    #[script]
    pub fn add_flame_altar(&mut self, new: NewFlameAltar) -> Result<PropId, Off>;

    /// Keeps the altars of the kind `kind` at `max_level` or below, under
    /// the game's own limit: a craft filter of this mod (see
    /// [`Load::on_craft`]) refuses, on the server, the vanilla altar
    /// upgrade recipes from `max_level` up crafted at an altar of the kind,
    /// so the altar keeps its level and the player their items. A
    /// `max_level` of 0 refuses even the claim. The Flame's own upgrades,
    /// upgrade recipes added by mods and other kinds keep the game's limit.
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// # Errors
    ///
    /// As [`Load::on_craft`], and [`Off`] when the vanilla recipe registry
    /// does not read or holds no altar claim recipe.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::altars::NewFlameAltar;
    /// use enshwick::content::{Guid, id};
    /// use enshwick::content::items::NewItem;
    ///
    /// fn init(load: &mut enshwick::Load<'_>, altar: Guid) -> Result<(), enshwick::Off> {
    ///     let small = load.add_flame_altar(NewFlameAltar {
    ///         item: NewItem::new(id!("small-altar"), altar),
    ///         zones: Vec::new(),
    ///         recipe_cost: None,
    ///     })?;
    ///     load.limit_altar_level(small, 2)
    /// }
    /// ```
    #[cfg(feature = "unstable")]
    #[script]
    pub fn limit_altar_level(&mut self, kind: PropId, max_level: u8) -> Result<(), Off>;
}
