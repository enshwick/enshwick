//! Less CPU for the server under Wine and Proton, and the numbers to tell.
//!
//! - At start, one line in the log and the status summary names what the
//!   server runs on: the Wine version and host, the fast synchronisation
//!   available, the CPU governor, the container's CPU limit and the CPUs the
//!   game saw. Nothing changes for it.
//! - Exact waits (on by default under Wine only): the engine's event loop
//!   sleeps the last fraction of a millisecond before a deadline instead of
//!   polling its handles in a loop.
//! - Server wait cap (vanilla by default): the server's game loop may wait
//!   longer between its passes while players are online, never past the next
//!   simulation step.
//! - Measure mode: a line a minute with the CPU used, the loop and step rates
//!   and the server's "Bad Performance" warnings.
//!
//! Waits are never throttled across the board: an earlier fix that delayed
//! every wait of every thread put players under the ground and broke
//! gliding. The mod changes two sites, and the first only ever sleeps time
//! the game would have spent polling.

use std::sync::OnceLock;

use enshwick_loader_api::{Host, Load, Mod, Off, events};

mod env;
mod measure;
mod sys;
mod waits;

/// When the exact waits run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Switch {
    /// Under Wine and Proton only.
    #[default]
    Auto,
    /// Always, Windows included.
    On,
    /// Never.
    Off,
}

impl Switch {
    /// Whether the exact waits run on this system.
    #[must_use]
    pub fn applies(self, wine: bool) -> bool {
        match self {
            Self::Auto => wine,
            Self::On => true,
            Self::Off => false,
        }
    }
}

/// Proton optimizer settings.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
#[config(validate = Settings::check)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    /// Exact short waits in the engine's event loop: "auto" under Wine and
    /// Proton only, "on" everywhere, Windows included, "off" never.
    /// Turning them on takes effect at the next start when they were off at
    /// start; turning them off takes effect at once.
    pub exact_waits: Switch,
    /// The longest the server's game loop waits between its passes while
    /// players are online, in milliseconds, 1 to 8, never past the next
    /// simulation step. 1 is vanilla. 2 to 4 saves CPU, but check gliding,
    /// joining and combat before keeping it.
    pub server_wait_cap_ms: u32,
    /// A line a minute in the log and the status summary: CPU, loop and step
    /// rates, exact waits and the server's Bad Performance warnings.
    pub measure: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            exact_waits: Switch::Auto,
            server_wait_cap_ms: 1,
            measure: true,
        }
    }
}

impl Settings {
    /// Refuses a cap outside 1 to 8 ms.
    ///
    /// # Errors
    ///
    /// The reason, for the log.
    pub fn check(&self) -> Result<(), String> {
        if (1..=8).contains(&self.server_wait_cap_ms) {
            Ok(())
        } else {
            Err(format!(
                "server_wait_cap_ms is {}, not 1 to 8",
                self.server_wait_cap_ms
            ))
        }
    }
}

/// What init found, for the status summary and the settings handler.
#[derive(Debug)]
struct Found {
    wine: bool,
    summary: String,
    linked: waits::Linked,
}

static FOUND: OnceLock<Found> = OnceLock::new();

/// Applies `settings` to the running hooks.
fn apply(settings: &Settings) {
    let Some(found) = FOUND.get() else {
        return;
    };
    let exact = settings.exact_waits.applies(found.wine);
    if exact && !found.linked.exact {
        enshwick_loader_api::info!("exact waits: on at the next start");
    }
    waits::set_exact(exact && found.linked.exact);
    if settings.server_wait_cap_ms > 1 && !found.linked.server {
        enshwick_loader_api::info!("server wait cap: from the next start");
    }
    waits::set_cap(settings.server_wait_cap_ms);
    measure::set(settings.measure);
    enshwick_loader_api::info!(
        "exact waits {}, server wait cap {} ms, measure mode {}",
        if waits::exact_on() { "on" } else { "off" },
        settings.server_wait_cap_ms,
        if settings.measure { "on" } else { "off" },
    );
}

/// The status lines.
fn status() -> String {
    let Some(found) = FOUND.get() else {
        return String::new();
    };
    let mut lines = vec![
        found.summary.clone(),
        format!(
            "exact waits {}, server loop wait {}",
            if waits::exact_on() { "on" } else { "off" },
            if found.linked.server {
                "hooked"
            } else {
                "not hooked"
            },
        ),
    ];
    let last = measure::last();
    if !last.is_empty() {
        lines.push(last);
    }
    lines.join("\n")
}

/// The mod. Its running state is in statics the hooks read.
#[derive(Debug)]
pub struct ProtonOptimizer;

impl Mod for ProtonOptimizer {
    fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
        enshwick_loader_api::manifest!(
            id: "proton-optimizer",
            side: BOTH,
            updates: github("themaiby/enshwick", "enshwick-proton-optimizer"),
            signed_by: ["RWTjqgtfpy3lSOLC4uaUXuvvKSLQKOFf6nUnEytoiFjSzxMEhLnoAdJF"],
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = load.config()?;
        let environment = env::Environment::probe(&measure::game_log_text());
        let wine = environment.is_wine();
        let summary = environment.summary();
        enshwick_loader_api::info!("environment: {summary}");
        if !wine {
            waits::set_slack(sys::timer_resolution_ns().unwrap_or(waits::MILLISECOND_NS));
        }
        let exact = settings.exact_waits.applies(wine);
        let server_loop = settings.measure || settings.server_wait_cap_ms > 1;
        let linked = waits::link(load, exact, server_loop)?;
        enshwick_loader_api::info!(
            "hooked: exact waits {}, message wait {}, server loop wait {}",
            linked.exact,
            linked.message,
            linked.server
        );
        load.on::<events::Tick>(|_| waits::stepped())?;
        let _first = FOUND.set(Found {
            wine,
            summary: format!("environment: {summary}"),
            linked,
        });
        load.status(status)?;
        load.on_config_change(|settings: Settings| apply(&settings))?;
        apply(&settings);
        Ok(Self)
    }

    fn ready(&'static self, _host: Host) {
        let wine = FOUND.get().is_some_and(|found| found.wine);
        let started = std::thread::Builder::new()
            .name("enshwick-proton-optimizer".into())
            .spawn(move || measure::run(wine));
        if let Err(e) = started {
            enshwick_loader_api::warn!("measure mode does not run: {e}");
        }
    }
}

enshwick_loader_api::declare_mod!(ProtonOptimizer);

#[cfg(test)]
mod tests {
    use enshwick_loader_api::abi::Status;
    use enshwick_loader_api::config::Config as _;

    use super::*;

    #[test]
    fn auto_follows_wine() {
        assert!(Switch::Auto.applies(true));
        assert!(!Switch::Auto.applies(false));
        assert!(Switch::On.applies(false));
        assert!(!Switch::Off.applies(true));
    }

    #[test]
    fn the_defaults_are_vanilla_on_windows() {
        let settings: Settings = toml::from_str(&Settings::template()).unwrap();
        assert_eq!(settings.exact_waits, Switch::Auto);
        assert_eq!(settings.server_wait_cap_ms, 1);
        assert!(settings.measure);
        assert!(settings.check().is_ok());
        let wide: Settings =
            toml::from_str("exact_waits = \"on\"\nserver_wait_cap_ms = 9\nmeasure = false")
                .unwrap();
        assert!(wide.check().is_err());
        assert!(
            toml::from_str::<Settings>(
                "exact_waits = \"maybe\"\nserver_wait_cap_ms = 1\nmeasure = true"
            )
            .is_err()
        );
    }

    /// Code holding the server's wait helper and game loop, as the patterns
    /// see them.
    fn code() -> &'static [u8] {
        let mut code = vec![0xCC; 0x40];
        // The division of rbx, then the clamp and the wait.
        code.extend([0x48, 0xB8, 0xDB, 0x34, 0xB6, 0xD7, 0x82, 0xDE, 0x1B, 0x43]);
        code.extend([
            0xBF, 0xFF, 0xFF, 0xFF, 0xFF, 0x48, 0xF7, 0xEB, 0x48, 0xC1, 0xFA, 0x12,
        ]);
        code.extend([0x48, 0x3B, 0xD7, 0x44, 0x8B, 0xCF, 0x44, 0x0F, 0x4C, 0xCA]);
        code.extend([0x48, 0x8B, 0x55, 0xD8, 0x45, 0x33, 0xC0, 0x8B, 0x4D, 0xE0]);
        code.extend([
            0xC7, 0x44, 0x24, 0x20, 0x01, 0x00, 0x00, 0x00, 0xFF, 0x15, 0, 0, 0, 0,
        ]);
        code.extend([0xCC; 0x10]);
        // The game loop's wait.
        code.extend([
            0x44, 0x39, 0xBB, 0x3C, 0x05, 0x00, 0x00, 0x75, 0x08, 0x41, 0xB9, 0x00,
        ]);
        code.extend([
            0x5A, 0x62, 0x02, 0xEB, 0x12, 0x48, 0x81, 0x7D, 0x67, 0x80, 0x9F, 0xD5,
        ]);
        code.extend([
            0x00, 0x41, 0xB9, 0x40, 0x42, 0x0F, 0x00, 0x4C, 0x0F, 0x4F, 0xCF, 0x4D,
        ]);
        code.extend([
            0x8B, 0xC4, 0x48, 0x8D, 0x55, 0xC7, 0x48, 0x8D, 0x4D, 0x77, 0xE8, 0, 0, 0, 0,
        ]);
        code.extend([0xCC; 0x10]);
        code.leak()
    }

    #[test]
    fn forced_on_the_waits_sleep_exactly_and_the_cap_stays_under_the_step() {
        let code = code();
        let base = code.as_ptr().addr() as u64;
        let wait = base + 0x40 + 22 + 10;
        let server = base + 0x40 + 22 + 34 + 0x10 + 35;
        let loader = enshwick_loader_api::test::FakeHost::new()
            .code(code)
            .settings("exact_waits = \"on\"\nserver_wait_cap_ms = 4\nmeasure = true\n")
            .install();
        // SAFETY: the export of `declare_mod!` above. Without `ready` no
        // measuring thread sets the slack to the timer resolution.
        let status = unsafe { loader.init(enshwick_mod_init) };
        waits::set_slack(0);
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(loader.mid_hooks(), [wait, server]);

        // A zero wait with 0.3 ms left sleeps them and keeps the zero wait.
        let slept = waits::SLEEPS.load(core::sync::atomic::Ordering::Relaxed);
        let mut registers = enshwick_loader_api::Registers {
            r9: 0,
            rbx: 300_000,
            ..Default::default()
        };
        let started = std::time::Instant::now();
        assert_eq!(loader.run_at(wait, &mut registers), 1);
        assert!(started.elapsed() >= std::time::Duration::from_micros(100));
        assert_eq!(registers.r9, 0);
        assert_eq!(
            waits::SLEEPS.load(core::sync::atomic::Ordering::Relaxed),
            slept + 1
        );
        // A timed wait is left alone.
        let mut registers = enshwick_loader_api::Registers {
            r9: 16,
            rbx: 16_600_000,
            ..Default::default()
        };
        assert_eq!(loader.run_at(wait, &mut registers), 1);
        assert_eq!(
            waits::SLEEPS.load(core::sync::atomic::Ordering::Relaxed),
            slept + 1
        );

        // Before any step the server wait stays vanilla; right after one it
        // grows to the cap; the idle wait never changes.
        let mut registers = enshwick_loader_api::Registers {
            r9: 1_000_000,
            ..Default::default()
        };
        assert_eq!(loader.run_at(server, &mut registers), 1);
        assert_eq!(registers.r9, 1_000_000);
        assert_eq!(
            loader.raise::<events::Tick>(&enshwick_loader_api::abi::TickData {
                header: enshwick_loader_api::abi::Header::of::<enshwick_loader_api::abi::TickData>(
                ),
                step: 1
            }),
            1
        );
        assert_eq!(loader.run_at(server, &mut registers), 1);
        assert_eq!(registers.r9, 4_000_000);
        let mut registers = enshwick_loader_api::Registers {
            r9: 40_000_000,
            ..Default::default()
        };
        assert_eq!(loader.run_at(server, &mut registers), 1);
        assert_eq!(registers.r9, 40_000_000);

        // Off at once.
        loader.set_settings("exact_waits = \"off\"\nserver_wait_cap_ms = 1\nmeasure = true\n");
        assert_eq!(loader.raise::<events::SettingsChanged>(&()), 1);
        let mut registers = enshwick_loader_api::Registers {
            r9: 1_000_000,
            ..Default::default()
        };
        assert_eq!(loader.run_at(server, &mut registers), 1);
        assert_eq!(registers.r9, 1_000_000);
        let mut registers = enshwick_loader_api::Registers {
            r9: 0,
            rbx: 300_000,
            ..Default::default()
        };
        assert_eq!(loader.run_at(wait, &mut registers), 1);
        assert_eq!(
            waits::SLEEPS.load(core::sync::atomic::Ordering::Relaxed),
            slept + 1
        );
        assert!(
            loader
                .status()
                .iter()
                .any(|line| line.contains("environment: "))
        );
        assert!(loader.faults().is_empty());
    }
}
