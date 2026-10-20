//! Area loot: when a player empties an enemy's loot, the loot of the other
//! enemies around comes along. Corpses and enemy loot bags within the radius
//! are emptied into the same player, nearest first, until one does not fit;
//! containers another player has open stay as they are. Items on the ground
//! keep the game's own pickup.

mod take;
mod tracker;

use enshwick_loader_api::{Load, Mod, Off};

/// Area loot settings.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
#[serde(deny_unknown_fields)]
struct Settings {
    /// Metres across the ground from the emptied container.
    #[config(runtime, range = 0.1..=50.0)]
    radius: f32,
    /// Metres above or below the emptied container.
    #[config(runtime, range = 0.1..=20.0)]
    height: f32,
    /// Take the loot around; false leaves looting vanilla.
    #[config(runtime)]
    enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            radius: 12.0,
            height: 4.0,
            enabled: true,
        }
    }
}

fn describe(settings: &Settings) {
    if settings.enabled {
        enshwick_loader_api::info!(
            "area loot within {} m, {} m up or down",
            settings.radius,
            settings.height
        );
    } else {
        enshwick_loader_api::info!("area loot disabled");
    }
}

/// The mod's state.
#[derive(Debug)]
pub struct AreaLoot;

impl Mod for AreaLoot {
    fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
        enshwick_loader_api::manifest!(
            id: "area-loot",
            side: SERVER,
            updates: github("themaiby/enshwick", "enshwick-area-loot"),
            signed_by: ["RWTjqgtfpy3lSOLC4uaUXuvvKSLQKOFf6nUnEytoiFjSzxMEhLnoAdJF"],
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = load.config()?;
        take::apply(&settings);
        take::link(load)?;
        load.on_config_change(|settings: Settings| {
            take::apply(&settings);
            describe(&settings);
        })?;
        describe(&settings);
        Ok(Self)
    }
}

enshwick_loader_api::declare_mod!(AreaLoot);

#[cfg(test)]
mod tests {
    use enshwick_loader_api::config::Config;

    use super::*;

    #[test]
    fn the_default_settings_parse() {
        let settings: Settings = toml::from_str(&Settings::template()).unwrap();
        assert!((settings.radius - 12.0).abs() < f32::EPSILON);
        assert!((settings.height - 4.0).abs() < f32::EPSILON);
        assert!(settings.enabled);
        assert!(settings.validate().is_ok());
    }

    #[test]
    fn out_of_range_settings_are_refused() {
        let with = |radius, height| Settings {
            radius,
            height,
            enabled: true,
        };
        assert!(with(0.0, 4.0).validate().is_err());
        assert!(with(51.0, 4.0).validate().is_err());
        assert!(with(f32::NAN, 4.0).validate().is_err());
        assert!(with(12.0, 0.0).validate().is_err());
        assert!(with(12.0, 21.0).validate().is_err());
        assert!(with(50.0, 20.0).validate().is_ok());
    }
}
