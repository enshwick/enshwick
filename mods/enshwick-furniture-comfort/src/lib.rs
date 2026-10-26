//! More comfort from furniture, so every house reaches the rested bonus.
//!
//! Each tick every comfort provider in a base registers its comfort for its
//! category, and the base keeps the best per category; a player's comfort is
//! the sum. The mod scales what each provider registers. It runs on the
//! server, which decides comfort, so players without the mod see the higher
//! comfort and rested time too; only item tooltips keep vanilla numbers. A
//! client loads it for the games it hosts, where its built-in server decides.

use std::sync::atomic::{AtomicU32, Ordering};

use enshwick_loader_api::{Load, Mod, Off};

/// Furniture comfort settings.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
#[serde(deny_unknown_fields)]
struct Settings {
    /// Comfort each piece of furniture gives inside a base, times this factor,
    /// from 0 to 100.
    #[config(runtime, range = 0.0..=MAX_MULTIPLIER)]
    multiplier: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { multiplier: 2.0 }
    }
}

/// The largest factor: the game sums the comfort of every category into one
/// number, which a larger factor could overflow.
const MAX_MULTIPLIER: f32 = 100.0;

/// The factor, as `f32` bits, read on every registration.
static MULTIPLIER: AtomicU32 = AtomicU32::new(0x3f80_0000);

/// `comfort` scaled by `multiplier`, rounded and kept in range.
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    reason = "clamped to the i32 range first"
)]
pub fn scaled(comfort: i32, multiplier: f32) -> i32 {
    let value = (f64::from(comfort) * f64::from(multiplier)).round();
    value.clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
}

/// The mod's state.
#[derive(Debug)]
pub struct FurnitureComfort;

impl Mod for FurnitureComfort {
    fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
        enshwick_loader_api::manifest!(
            id: "furniture-comfort",
            side: SERVER,
            name: "Furniture comfort",
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = load.config()?;
        MULTIPLIER.store(settings.multiplier.to_bits(), Ordering::Relaxed);
        load.on_comfort(|comfort| {
            scaled(
                comfort.value,
                f32::from_bits(MULTIPLIER.load(Ordering::Relaxed)),
            )
        })?;
        load.on_config_change(|settings: Settings| {
            MULTIPLIER.store(settings.multiplier.to_bits(), Ordering::Relaxed);
            enshwick_loader_api::info!("furniture comfort x{}", settings.multiplier);
        })?;
        enshwick_loader_api::info!("furniture comfort x{}", settings.multiplier);
        Ok(Self)
    }
}

enshwick_loader_api::declare_mod!(FurnitureComfort);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comfort_scales_rounds_and_stays_in_range() {
        assert_eq!(scaled(7, 2.0), 14);
        assert_eq!(scaled(3, 1.5), 5);
        assert_eq!(scaled(i32::MAX, 2.0), i32::MAX);
        assert_eq!(scaled(5, 0.0), 0);
    }

    #[test]
    fn the_default_settings_parse() {
        use enshwick_loader_api::config::Config;
        let settings: Settings = toml::from_str(&Settings::template()).unwrap();
        assert!((settings.multiplier - 2.0).abs() < f32::EPSILON);
        assert!(settings.validate().is_ok());
        assert!(Settings { multiplier: -1.0 }.validate().is_err());
        assert!(Settings { multiplier: 100.0 }.validate().is_ok());
        assert!(Settings { multiplier: 1e9 }.validate().is_err());
        assert!(
            Settings {
                multiplier: f32::INFINITY
            }
            .validate()
            .is_err()
        );
        assert!(
            Settings {
                multiplier: f32::NAN
            }
            .validate()
            .is_err()
        );
    }
}
