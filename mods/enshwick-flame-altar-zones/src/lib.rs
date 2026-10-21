//! Flame altar zones that touch form one cluster: every member grows by the
//! same amount until the cluster has the area of its members' zones without
//! losing any to overlap, no zone ever shrinks, and pieces can be built across
//! the seams between members.
//!
//! The zones are solved on the server, which the game trusts for them, and by
//! a client's built-in server while it hosts a game; the seam check runs on
//! both sides, since the client predicts placement. The platform follows the
//! altars and writes the sizes; this mod only decides them.

pub mod geometry;
mod seam;
mod solver;

use std::sync::atomic::{AtomicBool, Ordering};

use enshwick_loader_api::events::{AltarsChanged, ShutdownStarted};
use enshwick_loader_api::{Load, Mod, Off};

/// Whether pieces may be placed across seams, from the settings in force:
/// the server's on a joined client.
static SEAMS: AtomicBool = AtomicBool::new(true);

/// Flame altar zones settings, in `mod-configs/flame-altar-zones.toml`.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
#[serde(deny_unknown_fields)]
struct Settings {
    /// Altars whose zones touch grow together and share their area. Each
    /// world's own value applies when it loads.
    #[config(startup)]
    clusters: bool,
    /// Pieces may be placed across the seams between the zones of one cluster.
    /// A joined player runs with the server's value.
    #[config(sync)]
    building_across_seams: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            clusters: true,
            building_across_seams: true,
        }
    }
}

/// The mod's state.
#[derive(Debug)]
pub struct FlameAltarZones;

impl Mod for FlameAltarZones {
    fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
        enshwick_loader_api::manifest!(
            id: "flame-altar-zones",
            side: BOTH,
            name: "Flame altar zones",
            join: true,
            updates: github("themaiby/enshwick", "enshwick-flame-altar-zones"),
            signed_by: ["RWTjqgtfpy3lSOLC4uaUXuvvKSLQKOFf6nUnEytoiFjSzxMEhLnoAdJF"],
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = load.config()?;
        SEAMS.store(settings.building_across_seams, Ordering::Relaxed);
        // Always linked: the server's value may turn seams on for a joined
        // client, and a change applies while the game runs.
        load.on_zone_check(|check| SEAMS.load(Ordering::Relaxed) && seam::accepts(check))?;
        // Taken whatever the settings say: each world's own value turns
        // clusters on or off as it loads.
        let control = load.control_altar_zones()?;
        solver::switch(control, settings.clusters);
        load.on_config_change(move |settings: Settings| {
            let on = settings.building_across_seams;
            if SEAMS.swap(on, Ordering::Relaxed) != on {
                enshwick_loader_api::info!("building across seams {on}");
            }
            solver::switch(control, settings.clusters);
        })?;
        load.on::<AltarsChanged>(move |()| solver::request(control))?;
        // The loader drops the sizes with a hosted game; the next game,
        // the same world again included, is solved afresh.
        load.on::<ShutdownStarted>(|()| solver::forget())?;
        Ok(Self)
    }

    fn ready(&'static self, _host: enshwick_loader_api::Host) {
        enshwick_loader_api::info!("building across seams {}", SEAMS.load(Ordering::Relaxed));
    }
}

enshwick_loader_api::declare_mod!(FlameAltarZones);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_settings_parse() {
        use enshwick_loader_api::config::Config as _;

        let settings: Settings = toml::from_str(&Settings::template()).unwrap();
        assert!(settings.clusters && settings.building_across_seams);
    }

    #[test]
    fn it_sizes_the_zones_of_touching_altars_and_accepts_a_box_across_them() {
        use enshwick_loader_api::abi::{AltarInfo, Bounds, Status};

        let loader = enshwick_loader_api::test::FakeHost::new()
            .settings(&<Settings as enshwick_loader_api::config::Config>::template())
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(loader.altars_opened(), (true, true));

        let altar = |entity, x| {
            let zone = Bounds {
                min: [x, 0, 0],
                max: [x + 80, 80, 80],
            };
            let mut info = AltarInfo::EMPTY;
            info.entity = entity;
            info.level = 1;
            info.level_count = 1;
            info.zone = zone;
            info.levels[0] = zone;
            info
        };
        loader.set_altars(&[altar(1, 0), altar(2, 80)]);
        assert_eq!(loader.raise::<AltarsChanged>(&()), 1);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while loader.zone_sizes().len() < 2 && std::time::Instant::now() < deadline {
            std::thread::yield_now();
        }
        let sizes = loader.zone_sizes();
        assert_eq!(sizes.len(), 2);
        assert_eq!(sizes[0].grow, sizes[1].grow);

        let zones = [
            Bounds {
                min: [0, 0, 0],
                max: [10, 10, 10],
            },
            Bounds {
                min: [10, 0, 0],
                max: [20, 10, 10],
            },
        ];
        let across = Bounds {
            min: [5, 1, 1],
            max: [15, 9, 9],
        };
        assert!(loader.zone_check(&zones, across, 0));
        assert!(!loader.zone_check(&zones[..1], across, 0));
        SEAMS.store(false, Ordering::Relaxed);
        assert!(!loader.zone_check(&zones, across, 0), "seams off: vanilla");
        SEAMS.store(true, Ordering::Relaxed);
        assert!(loader.faults().is_empty());
    }

    #[test]
    fn a_world_s_own_clusters_value_applies_when_it_loads() {
        use enshwick_loader_api::abi::{AltarInfo, Bounds, Status};
        use enshwick_loader_api::events::SettingsReplaced;

        let loader = enshwick_loader_api::test::FakeHost::new()
            .settings(
                "clusters = false
building_across_seams = true
",
            )
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let altar = |entity, x| {
            let zone = Bounds {
                min: [x, 0, 0],
                max: [x + 80, 80, 80],
            };
            let mut info = AltarInfo::EMPTY;
            info.entity = entity;
            info.level = 1;
            info.level_count = 1;
            info.zone = zone;
            info.levels[0] = zone;
            info
        };
        loader.set_altars(&[altar(1, 0), altar(2, 80)]);
        assert_eq!(loader.raise::<AltarsChanged>(&()), 1);
        assert!(loader.zone_sizes().is_empty(), "clusters off: vanilla");
        loader.set_settings(
            "clusters = true
building_across_seams = true
",
        );
        assert!(loader.raise::<SettingsReplaced>(&()) >= 1);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while loader.zone_sizes().len() < 2 && std::time::Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert_eq!(loader.zone_sizes().len(), 2, "the world turned clusters on");
        loader.set_settings(
            "clusters = false
building_across_seams = true
",
        );
        assert!(loader.raise::<SettingsReplaced>(&()) >= 1);
        assert!(
            loader.zone_sizes().is_empty(),
            "the next world turned them off"
        );
        assert!(loader.faults().is_empty());
    }

    #[test]
    fn a_joined_player_takes_the_server_s_seams() {
        use enshwick_loader_api::config::Config as _;

        let server = Settings {
            building_across_seams: false,
            ..Settings::default()
        };
        let synced = server.synced().unwrap();
        assert_eq!(synced, "{ building_across_seams = false }");
        let mut joined = Settings::default();
        joined.merge_synced(&synced).unwrap();
        assert!(!joined.building_across_seams);
    }
}
