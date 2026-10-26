//! Building materials x2: the vanilla recipes for building blocks, roofs,
//! overgrowth, decor and furniture make more for the same cost. Functional
//! props (stations, chests, altars, water devices) stay as they are.

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use enshwick_loader_api::content::recipes::RecipeEdit;
use enshwick_loader_api::{Load, Mod, Off};

/// The settings file of the mod this one replaces, carried over once.
const OLD_SETTINGS: &str = "building-yield.toml";

/// The output multiplier in force.
static MULTIPLIER: AtomicU32 = AtomicU32::new(1);
/// Whether each [`Category`] is scaled, in its order.
static SCALED: [AtomicBool; 3] = [
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
];

/// Building materials settings.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
#[serde(default, deny_unknown_fields)]
struct Settings {
    /// The vanilla recipes make their output times this, for the same cost.
    /// A joined player runs with the server's value.
    #[config(sync, range = 2..=100)]
    multiplier: u32,
    /// Building blocks and roofs.
    #[config(sync)]
    blocks: bool,
    /// Overgrowth materials.
    #[config(sync)]
    overgrowth: bool,
    /// Decor and furniture.
    #[config(sync)]
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
    /// Puts these settings in force for the recipe patch.
    fn apply(&self) {
        MULTIPLIER.store(self.multiplier, Ordering::Relaxed);
        for (scaled, on) in SCALED
            .iter()
            .zip([self.blocks, self.overgrowth, self.decor])
        {
            scaled.store(on, Ordering::Relaxed);
        }
        enshwick_loader_api::info!(
            "output x{}: blocks {}, overgrowth {}, decor {}",
            self.multiplier,
            self.blocks,
            self.overgrowth,
            self.decor
        );
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

/// Scales the output of `recipe` when its category is in force.
fn scale(recipe: &mut RecipeEdit<'_>) {
    let Some(category) = category(&recipe.view().debug_name()) else {
        return;
    };
    if SCALED[category as usize].load(Ordering::Relaxed) {
        recipe.scale_output(MULTIPLIER.load(Ordering::Relaxed));
    }
}

/// The settings `template` with the values of the known keys in `old`, the
/// file of the mod this one replaces; keys it no longer has are dropped.
/// `None` when `old` does not parse.
#[must_use]
fn carried_over(template: &str, old: &str) -> Option<String> {
    let old: toml::Table = toml::from_str(old).ok()?;
    let lines = template.lines().map(|line| {
        line.split_once(" = ")
            .and_then(|(key, _)| Some(format!("{key} = {}", old.get(key)?)))
            .unwrap_or_else(|| line.to_owned())
    });
    Some(lines.map(|line| line + "\n").collect())
}

/// Writes the mod's settings from the replaced mod's file, when the mod has
/// none of its own yet and that file exists.
fn carry_over_old_settings(load: &Load<'_>) -> Result<(), Off> {
    use enshwick_loader_api::config::Config;
    // `<game>/mod-data/<id>` beside `<game>/mod-configs/`.
    let data = load.data_dir()?;
    let Some(configs) = data.parent().and_then(|d| d.parent()) else {
        return Ok(());
    };
    let configs = configs.join("mod-configs");
    let new = configs.join("building-materials.toml");
    let Ok(old) = std::fs::read_to_string(configs.join(OLD_SETTINGS)) else {
        return Ok(());
    };
    if new.exists() {
        return Ok(());
    }
    match carried_over(&Settings::template(), &old) {
        Some(text) => match std::fs::write(&new, text) {
            Ok(()) => enshwick_loader_api::info!("settings carried over from {OLD_SETTINGS}"),
            Err(e) => {
                enshwick_loader_api::warn!("settings not carried over from {OLD_SETTINGS}: {e}");
            }
        },
        None => enshwick_loader_api::warn!("{OLD_SETTINGS} does not parse, the defaults apply"),
    }
    Ok(())
}

/// The mod's state.
#[derive(Debug)]
pub struct BuildingMaterials;

impl Mod for BuildingMaterials {
    fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
        enshwick_loader_api::manifest!(
            id: "building-materials",
            side: BOTH,
            name: "Building materials",
            join: true,
            replaces: ["building-yield"],
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        carry_over_old_settings(load)?;
        let settings: Settings = load.config()?;
        settings.apply();
        // Values that arrive later, an edited file or a joined server's
        // values and the file's again on leaving, rebuild the registry the
        // game already holds.
        load.on_config_change(|settings: Settings| {
            settings.apply();
            match enshwick_loader_api::content::recipes::rebuild_recipes() {
                Ok(()) => enshwick_loader_api::info!("recipe counts follow the new values"),
                Err(off) => enshwick_loader_api::warn!(
                    "recipes already built keep their counts until the next start: {off}"
                ),
            }
        })?;
        load.modify_recipes(scale)?;
        Ok(Self)
    }
}

enshwick_loader_api::declare_mod!(BuildingMaterials);

#[cfg(test)]
mod tests {
    use super::*;
    use enshwick_loader_api::config::Config;

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
        assert_eq!(category("Prop_Decoration_T1_Storage_Shelf"), None);
        assert_eq!(category("Prop_Storage_32_T2_Chest_Wood"), None);
        assert_eq!(category("Recipe_Overgrowth_Tool"), None);
        assert_eq!(category("Craft_Weapon_T7_1H_Lore_Axe_Ministory_10"), None);
    }

    #[test]
    fn the_categories_index_their_switches() {
        assert_eq!(Category::Blocks as usize, 0);
        assert_eq!(Category::Overgrowth as usize, 1);
        assert_eq!(Category::Decor as usize, 2);
    }

    #[test]
    fn the_default_settings_parse_and_sync() {
        let settings: Settings = toml::from_str(&Settings::template()).unwrap();
        assert_eq!(settings.multiplier, 2);
        assert!(settings.blocks && settings.overgrowth && settings.decor);
        assert!(settings.validate().is_ok());
        let synced = settings.synced().unwrap();
        for key in ["multiplier", "blocks", "overgrowth", "decor"] {
            assert!(synced.contains(key), "{key} in {synced}");
        }
    }

    #[test]
    fn a_joined_server_s_values_and_leaving_rebuild_the_recipes() {
        use enshwick_loader_api::abi::Status;
        use enshwick_loader_api::events::SettingsChanged;

        let mut host = enshwick_loader_api::test::FakeHost::new()
            .field_offset("keen::RecipeRegistryResource", "recipes", 8)
            .type_size("keen::RecipeInfo", 0xa8)
            .type_size("keen::RecipeInputResource", 48)
            .type_size("keen::RecipeItemStackResource", 24)
            .field_offset("keen::RecipeItemStackResource", "item", 0x10)
            .field_offset("keen::RecipeItemStackResource", "count", 0x14);
        for (field, at) in [
            ("recipeGuid", 0),
            ("recipeId", 0x10),
            ("workshopGuid", 0x14),
            ("workshopId", 0x24),
            ("input", 0x28),
            ("output", 0x30),
            ("requiredProps", 0x40),
            ("completionRequirementQuery", 0x5c),
            ("debugName", 0x78),
        ] {
            host = host.field_offset("keen::RecipeInfo", field, at);
        }
        let loader = host
            .side(enshwick_loader_api::abi::Side::CLIENT)
            .settings("multiplier = 2\nblocks = true\novergrowth = true\ndecor = true\n")
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let allowed = loader.rebuilds().len();
        assert_eq!(allowed, 1, "the registry may be rebuilt");
        loader.set_server_synced(Some("{ multiplier = 5 }"));
        assert_eq!(loader.raise::<SettingsChanged>(&()), 1);
        assert_eq!(MULTIPLIER.load(Ordering::Relaxed), 5);
        assert_eq!(loader.rebuilds().len(), allowed + 1, "joined: rebuilt");
        loader.set_server_synced(None);
        assert_eq!(loader.raise::<SettingsChanged>(&()), 1);
        assert_eq!(MULTIPLIER.load(Ordering::Relaxed), 2);
        assert_eq!(loader.rebuilds().len(), allowed + 2, "left: rebuilt");
        assert!(loader.faults().is_empty());
    }

    #[test]
    fn a_multiplier_out_of_range_is_refused() {
        for multiplier in [0, 1, 101] {
            let settings = Settings {
                multiplier,
                ..Settings::default()
            };
            assert!(settings.validate().is_err(), "{multiplier}");
        }
    }

    #[test]
    fn the_building_yield_file_carries_over_without_its_workbench() {
        let old = "# Old comment\nmultiplier = 3\nblocks = true\novergrowth = true\ndecor = false\nown_workbench = true\n";
        let text = carried_over(&Settings::template(), old).unwrap();
        let settings: Settings = toml::from_str(&text).unwrap();
        assert_eq!(settings.multiplier, 3);
        assert!(settings.blocks && settings.overgrowth);
        assert!(!settings.decor);
        assert!(!text.contains("own_workbench"));
        assert!(carried_over(&Settings::template(), "multiplier = ").is_none());
    }
}
