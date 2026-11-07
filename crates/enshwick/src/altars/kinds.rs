//! Flame Altar kinds from mods: a prop copied from a vanilla altar whose
//! altars take zones and a maximum level of their own.

use crate::content::items::NewItem;
use crate::content::props::{NewProp, PropId};
use crate::content::recipes::RECIPE_REGISTRY;
use crate::content::types;
use crate::resources::Value;
use crate::{Load, Off, RecipeId};

use super::template_of;

/// The game's names of the recipe registry's list and of the recipe fields
/// the level limit reads (`RecipeRegistryResource`, `RecipeInfo`).
const RECIPES: &str = "recipes";
const RECIPE_ID: &str = "recipeId";
const LEVEL: &str = "level";
const IS_UPGRADE: &str = "isUpgrade";
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
    /// The highest level the kind's altars reach, below the game's own:
    /// the server refuses an upgrade past it as it refuses a craft, so the
    /// altar keeps its level and the player their items; 0 refuses even
    /// the claim. `None` keeps the game's limit.
    pub max_level: Option<u8>,
}

/// The ids of the upgrade recipes in `registry` that raise an altar from
/// `max` or above: an upgrade recipe's level is the one it upgrades from
/// (server 26430253).
fn upgrades_from(registry: &Value, max: u8) -> Vec<RecipeId> {
    let Some(Value::List(recipes)) = registry.field(RECIPES) else {
        return Vec::new();
    };
    recipes
        .iter()
        .filter(|recipe| recipe.field(IS_UPGRADE) == Some(&Value::Bool(true)))
        .filter(|recipe| {
            matches!(recipe.field(LEVEL), Some(&Value::Uint(level)) if level >= u64::from(max))
        })
        .filter_map(|recipe| match recipe.field(RECIPE_ID)?.field(ID_VALUE)? {
            &Value::Uint(id) => u32::try_from(id).ok().map(RecipeId),
            _ => None,
        })
        .collect()
}

#[crate::script(flame_altars)]
impl Load<'_> {
    /// Adds a kind of Flame Altar, on the server and the client alike: a
    /// prop copied from the vanilla altar whose item is `item.like`, as
    /// [`Load::add_prop`] adds one, whose altars take the kind's zones
    /// through an altar zone filter of this mod, chained with other mods'
    /// filters in load order (see [`Load::on_altar_zone`]).
    /// [`Altar::template`](super::Altar::template) tells the kind's altars
    /// apart. With a [`max_level`](NewFlameAltar::max_level), a craft filter
    /// of this mod (see [`Load::on_craft`]) refuses the vanilla upgrade
    /// recipes past it at the kind's altars, on the server.
    ///
    /// # Errors
    ///
    /// As [`Load::add_prop`], [`Load::on_altar_zone`] and
    /// [`Load::on_craft`], and [`Off`] with a maximum level when the
    /// vanilla recipe registry does not read.
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
    ///         max_level: Some(2),
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
        if let Some(max) = new.max_level {
            let registry = RECIPE_REGISTRY.key(types::RECIPE_REGISTRY_RESOURCE)?;
            let refused = upgrades_from(&self.host().game_value(registry)?, max);
            let (host, kind) = (self.host(), prop.template.guid());
            self.on_craft(move |craft| {
                !(refused.contains(&craft.recipe_id)
                    && craft
                        .crafting_station_id
                        .and_then(|at| template_of(host, at))
                        == Some(kind))
            })?;
        }
        Ok(prop)
    }
}
