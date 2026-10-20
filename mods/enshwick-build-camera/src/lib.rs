//! Airborne Architect's building fly mode for every player.
//!
//! The game opens fly mode near a building zone to a character that may edit
//! the base, is alive, not gliding, and unlocked the Airborne Architect node.
//! This mod answers "unlocked" at the fly mode gate alone, so skill points,
//! prerequisites and the skill tree stay vanilla. `enabled` is a join value: a
//! joined player predicts with the server's.

use core::sync::atomic::{AtomicBool, Ordering};

use enshwick_loader_api::progress::{AIRBORNE_ARCHITECT, SkillGate};
use enshwick_loader_api::{Load, Mod, Off};

static ENABLED: AtomicBool = AtomicBool::new(false);

/// Build camera settings.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
#[serde(deny_unknown_fields)]
struct Settings {
    /// Every player may use the building fly mode near a base without the
    /// Airborne Architect skill. The vanilla limits stay: near a building
    /// zone, allowed to edit the base, alive and not gliding.
    /// A joined player runs with the server's value.
    #[config(sync)]
    enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// The answer to a check of the Airborne Architect node.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::progress::SkillGate;
///
/// assert_eq!(enshwick_build_camera::lifts(SkillGate::FlyMode, true), Some(true));
/// assert_eq!(enshwick_build_camera::lifts(SkillGate::FlyMode, false), None);
/// assert_eq!(enshwick_build_camera::lifts(SkillGate::Other, true), None);
/// ```
#[must_use]
pub fn lifts(gate: SkillGate, enabled: bool) -> Option<bool> {
    (enabled && gate == SkillGate::FlyMode).then_some(true)
}

/// The mod.
#[derive(Debug)]
pub struct BuildCamera;

fn apply(settings: &Settings) {
    let was = ENABLED.swap(settings.enabled, Ordering::Relaxed);
    if was != settings.enabled {
        enshwick_loader_api::info!(
            "fly mode without the skill: {}",
            if settings.enabled { "on" } else { "off" }
        );
    }
}

impl Mod for BuildCamera {
    fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
        enshwick_loader_api::manifest!(
            id: "build-camera",
            side: BOTH,
            join = true,
            updates: github("themaiby/enshwick", "enshwick-build-camera"),
            signed_by: ["RWTjqgtfpy3lSOLC4uaUXuvvKSLQKOFf6nUnEytoiFjSzxMEhLnoAdJF"],
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = load.config()?;
        load.on_skill(AIRBORNE_ARCHITECT, |check| {
            lifts(check.gate, ENABLED.load(Ordering::Relaxed))
        })?;
        load.on_config_change(|settings: Settings| apply(&settings))?;
        apply(&settings);
        Ok(Self)
    }
}

enshwick_loader_api::declare_mod!(BuildCamera);

#[cfg(test)]
mod tests {
    use enshwick_loader_api::abi::{Header, SkillCheck as RawCheck, Status};
    use enshwick_loader_api::config::Config as _;
    use enshwick_loader_api::events;

    use super::*;

    #[test]
    fn enabled_is_the_join_value_and_on_by_default() {
        let settings: Settings = toml::from_str(&Settings::template()).unwrap();
        assert!(settings.enabled);
        let synced = settings.synced().unwrap();
        assert_eq!(synced, "{ enabled = true }");
        let mut local: Settings = toml::from_str("enabled = true").unwrap();
        local.merge_synced("{ enabled = false }").unwrap();
        assert!(!local.enabled);
    }

    #[test]
    fn the_gate_follows_the_settings_and_the_servers_join_value() {
        let loader = enshwick_loader_api::test::FakeHost::new()
            .settings("enabled = true\n")
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let ask = |node, gate| {
            loader.skill(&RawCheck {
                header: Header::of::<RawCheck>(),
                node,
                gate,
                unlocked: 0,
                reserved: [0; 3],
            })
        };

        assert_eq!(
            ask(AIRBORNE_ARCHITECT, RawCheck::FLY_MODE),
            RawCheck::UNLOCKED
        );
        assert_eq!(
            ask(AIRBORNE_ARCHITECT, RawCheck::OTHER),
            RawCheck::VANILLA,
            "another caller stays vanilla"
        );
        assert_eq!(
            ask(0xa667_75b7, RawCheck::FLY_MODE),
            RawCheck::VANILLA,
            "another node stays vanilla"
        );

        loader.set_server_synced(Some("{ enabled = false }"));
        assert_eq!(loader.raise::<events::SettingsChanged>(&()), 1);
        assert_eq!(
            ask(AIRBORNE_ARCHITECT, RawCheck::FLY_MODE),
            RawCheck::VANILLA,
            "the server turned it off"
        );

        loader.set_server_synced(None);
        assert_eq!(loader.raise::<events::SettingsChanged>(&()), 1);
        assert_eq!(
            ask(AIRBORNE_ARCHITECT, RawCheck::FLY_MODE),
            RawCheck::UNLOCKED,
            "the file's value again"
        );
        assert!(loader.faults().is_empty());
    }
}
