//! Bulk recipes beside the vanilla ones: for building blocks, roofs,
//! overgrowth, decor and furniture, a second recipe that makes more for the
//! same cost. Vanilla recipes stay as they are; functional props (stations,
//! chests, altars, water devices) get none.

use enshwick_api::content::recipes::{Derived, RecipeView};
use enshwick_api::{Load, Mod, Off};

/// Building yield settings.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick_api::Config)]
#[serde(deny_unknown_fields)]
#[config(validate = Settings::check)]
struct Settings {
    /// A bulk recipe makes the vanilla output times this, for the same cost.
    multiplier: u32,
    /// Bulk recipes for building blocks and roofs.
    blocks: bool,
    /// Bulk recipes for overgrowth materials.
    overgrowth: bool,
    /// Bulk recipes for decor and furniture.
    decor: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            multiplier: 2,
            blocks: true,
            overgrowth: true,
            decor: true,
        }
    }
}

impl Settings {
    fn check(&self) -> Result<(), String> {
        (2..=100)
            .contains(&self.multiplier)
            .then_some(())
            .ok_or_else(|| "multiplier is 2 to 100".into())
    }
}

/// What a recipe makes, by its debug name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    /// Building blocks and roofs.
    Blocks,
    /// Overgrowth materials.
    Overgrowth,
    /// Decor and furniture.
    Decor,
}

/// The category of the recipe named `name`, `None` for everything else.
#[must_use]
pub fn category(name: &str) -> Option<Category> {
    if name.starts_with("Block_") {
        Some(Category::Blocks)
    } else if name.starts_with("Material_") && name.contains("_Overgrowth_") {
        Some(Category::Overgrowth)
    } else if name.starts_with("Prop_Decoration_")
        // Decoration with a function: it holds water or items.
        && !["Water", "Storage", "Chest"].iter().any(|word| name.contains(word))
    {
        Some(Category::Decor)
    } else {
        None
    }
}

/// The mod's state.
#[derive(Debug)]
pub struct BuildingYield;

impl Mod for BuildingYield {
    fn manifest() -> &'static enshwick_api::abi::Manifest {
        enshwick_api::manifest!(
            id: "building-yield",
            side: BOTH,
            join = true,
            updates: github("themaiby/enshwick", "enshwick-building-yield"),
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = load.config()?;
        let derived = Derived {
            output: settings.multiplier,
            cost: 1,
        };
        // Materials first: where a workshop runs out of room, decor gives way.
        let (blocks, overgrowth, decor) = (settings.blocks, settings.overgrowth, settings.decor);
        load.derive_recipes("building-yield", move |recipe: &RecipeView<'_>| {
            match category(&recipe.debug_name())? {
                Category::Blocks => blocks,
                Category::Overgrowth => overgrowth,
                Category::Decor => false,
            }
            .then_some(derived)
        })?;
        if decor {
            load.derive_recipes("building-yield", move |recipe: &RecipeView<'_>| {
                (category(&recipe.debug_name())? == Category::Decor).then_some(derived)
            })?;
        }
        enshwick_api::info!(
            "bulk recipes x{}: blocks {}, overgrowth {}, decor {}",
            settings.multiplier,
            settings.blocks,
            settings.overgrowth,
            settings.decor
        );
        Ok(Self)
    }
}

enshwick_api::declare_mod!(BuildingYield);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipes_fall_in_their_categories() {
        assert_eq!(
            category("Block_T2_Metal_Copper_CRAFTING"),
            Some(Category::Blocks)
        );
        assert_eq!(category("Block_Roof_T6_Palmleaves"), Some(Category::Blocks));
        assert_eq!(
            category("Material_T1_Overgrowth_Ivy"),
            Some(Category::Overgrowth)
        );
        assert_eq!(
            category("Prop_Decoration_Comfort1_T0_Bench"),
            Some(Category::Decor)
        );
        assert_eq!(category("Prop_Decoration_T2_Barrel_Water"), None);
        assert_eq!(category("Prop_Storage_32_T2_Chest_Wood"), None);
        assert_eq!(category("Recipe_Overgrowth_Tool"), None);
        assert_eq!(category("Craft_Weapon_T7_1H_Lore_Axe_Ministory_10"), None);
    }

    #[test]
    fn the_default_settings_parse() {
        use enshwick_api::config::Config;
        let settings: Settings = toml::from_str(&Settings::template()).unwrap();
        assert_eq!(settings.multiplier, 2);
        assert!(settings.validate().is_ok());
    }
}
