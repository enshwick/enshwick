//! Bulk recipes beside the vanilla ones: for building blocks, roofs,
//! overgrowth, decor and furniture, a second recipe that makes more for the
//! same cost, at a Bulk Workbench of the mod's own. Vanilla recipes stay as
//! they are; functional props (stations, chests, altars, water devices) get
//! none.

use enshwick_api::content::Guid;
use enshwick_api::content::recipes::{Derived, RecipeView};
use enshwick_api::content::workshops::{NewWorkshop, StationRecipe, stations};
use enshwick_api::{Load, Mod, Off};

/// The Bulk Workbench item; its game id is FNV-1a of these bytes.
pub const BULK_WORKBENCH: Guid = Guid::parse("0851e93d-8883-42e4-a807-c30fd644e7c3");
/// The template the Bulk Workbench places.
pub const BULK_WORKBENCH_TEMPLATE: Guid = Guid::parse("77ff6155-1365-4a4a-8987-507a1bdd32a2");
/// The Bulk Workbench's workshop, where the bulk recipes are made.
pub const BULK_WORKSHOP: Guid = Guid::parse("5b828914-9b48-40f8-b541-72e9b942cee6");
/// The recipe that makes the Bulk Workbench.
pub const BULK_WORKBENCH_RECIPE: Guid = Guid::parse("94cf68a3-11e0-4c05-a793-526f7e1d7693");
/// The Bulk Workbench's name text.
pub const BULK_WORKBENCH_NAME: Guid = Guid::parse("41b222ea-cefa-4774-a040-c43aa9f0b227");
/// The Bulk Workbench's description text.
pub const BULK_WORKBENCH_DESCRIPTION: Guid = Guid::parse("6bc87387-936a-40a5-8e62-5aa5dbf933eb");

/// The mod's texts, per language.
pub const TEXTS: &str = include_str!("texts.toml");

/// The Bulk Workbench: the vanilla workbench with a workshop of its own,
/// made like the workbench.
#[must_use]
pub fn bulk_workbench() -> NewWorkshop {
    NewWorkshop {
        like: stations::WORKBENCH,
        item: BULK_WORKBENCH,
        template: BULK_WORKBENCH_TEMPLATE,
        workshop: BULK_WORKSHOP,
        name: Some(BULK_WORKBENCH_NAME),
        description: Some(BULK_WORKBENCH_DESCRIPTION),
        station_recipe: Some(StationRecipe {
            guid: BULK_WORKBENCH_RECIPE,
            cost: 1,
        }),
        debug_name: "Craftingstation_BulkWorkbench".into(),
    }
}

/// Building yield settings.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick_api::Config)]
#[serde(default, deny_unknown_fields)]
#[config(validate = Settings::check)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "settings a server owner switches"
)]
struct Settings {
    /// A bulk recipe makes the vanilla output times this, for the same cost.
    multiplier: u32,
    /// Bulk recipes for building blocks and roofs.
    blocks: bool,
    /// Bulk recipes for overgrowth materials.
    overgrowth: bool,
    /// Bulk recipes for decor and furniture.
    decor: bool,
    /// Bulk recipes at a Bulk Workbench of their own; off puts them beside
    /// the vanilla recipes at each vanilla workshop.
    own_workbench: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            multiplier: 2,
            blocks: true,
            overgrowth: true,
            decor: true,
            own_workbench: true,
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
            signed_by: ["RWTjqgtfpy3lSOLC4uaUXuvvKSLQKOFf6nUnEytoiFjSzxMEhLnoAdJF"],
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = load.config()?;
        let workshop = if settings.own_workbench {
            let texts = load.texts(TEXTS)?;
            load.add_texts(
                &texts,
                &[
                    ("bulk_workbench.name", BULK_WORKBENCH_NAME),
                    ("bulk_workbench.description", BULK_WORKBENCH_DESCRIPTION),
                ],
            )?;
            Some(load.add_workshop(bulk_workbench())?)
        } else {
            None
        };
        let derived = Derived {
            output: settings.multiplier,
            cost: 1,
            workshop,
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
            "bulk recipes x{}: blocks {}, overgrowth {}, decor {}, at {}",
            settings.multiplier,
            settings.blocks,
            settings.overgrowth,
            settings.decor,
            if settings.own_workbench {
                "the Bulk Workbench"
            } else {
                "the vanilla workshops"
            }
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
        assert!(settings.own_workbench);
        assert!(settings.validate().is_ok());
    }

    #[test]
    fn a_settings_file_from_before_the_workbench_still_parses() {
        let settings: Settings = toml::from_str(
            "multiplier = 3
blocks = true
overgrowth = true
decor = false
",
        )
        .unwrap();
        assert_eq!(settings.multiplier, 3);
        assert!(settings.own_workbench);
    }

    #[test]
    fn the_texts_name_and_describe_the_bulk_workbench_in_every_language() {
        use enshwick_api::i18n::{Language, Texts};
        let texts = Texts::parse(TEXTS).unwrap();
        for key in ["bulk_workbench.name", "bulk_workbench.description"] {
            for language in ["en", "uk", "de", "fr", "es", "pl"] {
                let language = Language::from_code(language).unwrap();
                assert!(texts.get(key, language).is_some(), "{key} {language:?}");
            }
        }
    }

    #[test]
    fn the_bulk_workbench_is_a_workbench_with_its_own_identity() {
        let bench = bulk_workbench();
        assert_eq!(bench.like, stations::WORKBENCH);
        assert_ne!(bench.item, stations::WORKBENCH.item);
        assert_ne!(bench.template, stations::WORKBENCH.template);
    }
}
