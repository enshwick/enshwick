//! Flame Altar kinds from mods: a prop copied from a vanilla altar whose
//! altars take zones and a maximum level of their own.

#[cfg(feature = "unstable")]
use crate::RecipeId;
use crate::content::items::NewItem;
use crate::content::props::{NewProp, PropId};
#[cfg(feature = "unstable")]
use crate::content::{props::RECIPES, recipes::RECIPE_REGISTRY, types};
#[cfg(feature = "unstable")]
use crate::resources::Value;
use crate::{Load, Off};

use super::template_of;

/// The game's names of the recipe fields the level limit reads
/// (`RecipeInfo`).
#[cfg(feature = "unstable")]
const RECIPE_ID: &str = "recipeId";
#[cfg(feature = "unstable")]
const WORKSHOP_ID: &str = "workshopId";
#[cfg(feature = "unstable")]
const LEVEL: &str = "level";
#[cfg(feature = "unstable")]
const IS_UPGRADE: &str = "isUpgrade";
#[cfg(feature = "unstable")]
const ID_VALUE: &str = "value";

/// A Flame Altar kind's zone at one level, as an altar zone filter sets it:
/// see [`AltarZone`](super::AltarZone).
///
/// # Examples
///
/// See [`Load::add_flame_altar`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
pub struct KindZone {
    /// Cells added to the vanilla zone on every horizontal side, at least 0.
    pub grow: i32,
    /// The zone's height in cells.
    pub height: i32,
}

/// A new Flame Altar kind for [`Load::add_flame_altar`]: a prop copied from
/// a vanilla altar, with its own zone at each level.
///
/// # Examples
///
/// See [`Load::add_flame_altar`].
#[derive(Clone, Debug, crate::Script)]
pub struct NewFlameAltar {
    /// The altar's item, a copy of a vanilla altar's item; its other ids
    /// derive from it as a prop's do.
    pub item: NewItem,
    /// Its zone at levels 1, 2 and on; a level past the list takes the last
    /// one, and an empty list keeps the vanilla zones.
    #[script(default)]
    pub zones: Vec<KindZone>,
    /// With a cost, the altar is built by a copy of the vanilla altar's
    /// recipe whose ingredient counts are times this.
    pub recipe_cost: Option<u32>,
}

/// The ids of the altar upgrade recipes in `registry` that raise an altar
/// from `max` or above. An upgrade recipe's level is the one it upgrades
/// from (server 26430253); the Flame's own upgrades are upgrades too, in a
/// workshop of their own (client 26430625).
///
/// # Errors
///
/// [`Off`] when the registry holds no upgrade recipe at level 0, the claim:
/// its fields do not read as this build lays them out.
#[cfg(feature = "unstable")]
fn upgrades_from(registry: &Value, max: u8) -> Result<Vec<RecipeId>, Off> {
    let unread = || Off::new("altar level limit: the recipe registry has no altar claim recipe");
    let Some(Value::List(recipes)) = registry.field(RECIPES) else {
        return Err(unread());
    };
    let level = |recipe: &Value| match recipe.field(LEVEL) {
        Some(&Value::Uint(level)) => Some(level),
        _ => None,
    };
    let upgrades = || {
        recipes
            .iter()
            .filter(|recipe| recipe.field(IS_UPGRADE) == Some(&Value::Bool(true)))
    };
    // shortcut: the altar's workshop is the one with a level-0 upgrade, as only the claim is on
    // the builds seen; upgrade to a catalogue workshop id once one names the altar's.
    let altar = upgrades()
        .find(|recipe| level(recipe) == Some(0))
        .and_then(|claim| claim.field(WORKSHOP_ID))
        .ok_or_else(unread)?;
    Ok(upgrades()
        .filter(|recipe| recipe.field(WORKSHOP_ID) == Some(altar))
        .filter(|recipe| level(recipe).is_some_and(|level| level >= u64::from(max)))
        .filter_map(|recipe| match recipe.field(RECIPE_ID)?.field(ID_VALUE)? {
            &Value::Uint(id) => u32::try_from(id).ok().map(RecipeId),
            _ => None,
        })
        .collect())
}

#[crate::script(flame_altars)]
impl Load<'_> {
    /// Adds a kind of Flame Altar, on the server and the client alike: a
    /// prop copied from the vanilla altar whose item is `item.like`, as
    /// [`Load::add_prop`] adds one, whose altars take the kind's zones
    /// through an altar zone filter of this mod, chained with other mods'
    /// filters in load order (see [`Load::on_altar_zone`]).
    /// [`Altar::template`](super::Altar::template) tells the kind's altars
    /// apart.
    ///
    /// # Errors
    ///
    /// As [`Load::add_prop`] and [`Load::on_altar_zone`].
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
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
    pub fn add_flame_altar(&mut self, new: NewFlameAltar) -> Result<PropId, Off> {
        let prop = self.add_prop(NewProp {
            recipe_cost: new.recipe_cost,
            ..NewProp::new(new.item)
        })?;
        if let Some(&last) = new.zones.last() {
            let (host, kind, zones) = (self.host(), prop.template.guid(), new.zones);
            self.on_altar_zone(move |zone| {
                if template_of(host, zone.altar) != Some(kind) {
                    return;
                }
                let level = usize::from(zone.level.max(1)) - 1;
                let sized = zones.get(level).copied().unwrap_or(last);
                zone.grow = sized.grow.max(0).into();
                zone.height = sized.height.into();
            })?;
        }
        Ok(prop)
    }

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
    /// # extern crate enshwick_core as enshwick;
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
    pub fn limit_altar_level(&mut self, kind: PropId, max_level: u8) -> Result<(), Off> {
        let registry = types::RECIPE_REGISTRY_RESOURCE.key(RECIPE_REGISTRY)?;
        let refused = upgrades_from(&self.host().game_value(registry)?, max_level)?;
        let (host, kind) = (self.host(), kind.template.guid());
        self.on_craft(move |craft| {
            !(refused.contains(&craft.recipe_id)
                && craft
                    .crafting_station_id
                    .and_then(|at| template_of(host, at))
                    == Some(kind))
        })
    }
}
