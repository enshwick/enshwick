//! Less CPU for the Enshrouded server, on Windows and under Wine or Proton,
//! and the numbers to tell.
//!
//! The mod knows where it runs, the dedicated server or the game, on
//! Windows or under Wine, and turns on by default only the changes that A/B
//! runs showed to help there (one table in `platform.rs`). A setting can force a
//! change anywhere.
//!
//! - Exact waits: the engine's event loop sleeps the last fraction of a
//!   millisecond before a deadline instead of polling its handles.
//! - Task workers: under Wine in a container with a CPU limit, the task
//!   groups start no more workers than the limit allows.
//! - Server wait cap: the server's game loop may wait longer between its
//!   passes while players are online, never past the next simulation step.
//! - Measure mode: a line a minute with the CPU used, by thread group, the
//!   loop and step rates and the server's "Bad Performance" warnings.
//! - At start, one line names what the server runs on: Wine or Windows, the
//!   fast synchronisation available, the CPU governor, the container's CPU
//!   limit and the CPUs the game saw.
//!
//! Waits are never throttled across the board: an earlier fix that delayed
//! every wait of every thread put players under the ground and broke
//! gliding. The exact waits only sleep time the game would have spent
//! polling.

use std::sync::OnceLock;

use enshwick_loader_api::abi::Side;
use enshwick_loader_api::pacing::{self, TaskWorkers};
use enshwick_loader_api::{Host, Load, Mod, Off, events};

use crate::env::CpuLimit;
use crate::platform::{Change, Platform, default_on};

mod env;
mod measure;
mod platform;
mod sys;
mod waits;

/// Whether a change runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Switch {
    /// Where it was measured to help.
    #[default]
    Auto,
    /// Everywhere.
    On,
    /// Nowhere.
    Off,
}

impl Switch {
    /// Whether the change runs where its default is `default`.
    #[must_use]
    pub fn applies(self, default: bool) -> bool {
        match self {
            Self::Auto => default,
            Self::On => true,
            Self::Off => false,
        }
    }
}

/// Optimizer settings.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    /// Exact short waits in the engine's event loop: "auto" on the dedicated
    /// server, on Windows and under Wine, where they were measured to help;
    /// "on" in the game too; "off" never. Turning them off takes effect at
    /// once; turning them on takes effect at the next start when they were
    /// off at start.
    #[config(runtime, choices = [Switch::Auto, Switch::On, Switch::Off])]
    pub exact_waits: Switch,
    /// The most worker threads each task group of the dedicated server
    /// starts, 1 to 64. 0 is automatic: under Wine in a container with a CPU
    /// limit, that limit rounded up and at least 2; elsewhere as many as the
    /// game picks.
    #[config(restart, range = 0..=64)]
    pub task_workers: u32,
    /// The longest the server's game loop waits between its passes while
    /// players are online, in milliseconds, 1 to 8, never past the next
    /// simulation step. 1 is vanilla. Check gliding, joining and combat
    /// before keeping more.
    #[config(runtime, range = 1..=8)]
    pub server_wait_cap_ms: u32,
    /// A line a minute in the log and the status summary: CPU, by thread
    /// group, loop and step rates, exact waits and the server's Bad
    /// Performance warnings.
    #[config(runtime)]
    pub measure: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            exact_waits: Switch::Auto,
            task_workers: 0,
            server_wait_cap_ms: 1,
            measure: true,
        }
    }
}

/// The task worker cap for the `task_workers` setting on `platform` under
/// `limit`: `None` leaves the game's own count.
#[must_use]
pub(crate) fn worker_cap(setting: u32, platform: Platform, limit: Option<CpuLimit>) -> Option<u8> {
    if setting > 0 {
        return u8::try_from(setting).ok();
    }
    if !default_on(Change::TaskWorkers, platform) {
        return None;
    }
    match limit? {
        CpuLimit::Cpus(cpus) => {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "clamped to 2 to 64 first"
            )]
            let cap = cpus.ceil().clamp(2.0, 64.0) as u8;
            Some(cap)
        }
        CpuLimit::Unlimited => None,
    }
}

/// What init found, for the status summary and the settings handler.
#[derive(Debug)]
struct Found {
    platform: Platform,
    summary: String,
    linked: waits::Linked,
    workers: Option<u8>,
}

static FOUND: OnceLock<Found> = OnceLock::new();

/// The platform and what runs on it, for the measure lines and the status
/// summary.
fn running(found: &Found, settings: &Settings) -> String {
    let workers = match found.workers {
        Some(cap) => format!("task workers at most {cap}"),
        None => "task workers as the game picks".to_owned(),
    };
    let cap = if settings.server_wait_cap_ms > 1 {
        format!("server wait cap {} ms", settings.server_wait_cap_ms)
    } else {
        "server wait vanilla".to_owned()
    };
    format!(
        "{}: exact waits {}, {workers}, {cap}",
        found.platform,
        if waits::exact_on() { "on" } else { "off" },
    )
}

/// Logs a change forced on where it was not measured to help.
fn note_forced(change: Change, forced: bool, platform: Platform) {
    if forced && !default_on(change, platform) {
        enshwick_loader_api::info!(
            "{change}: forced on; off by default on a {platform}, where it was not measured to help"
        );
    }
}

/// Applies `settings` to the running hooks.
fn apply(settings: &Settings) {
    let Some(found) = FOUND.get() else {
        return;
    };
    let exact = settings
        .exact_waits
        .applies(default_on(Change::ExactWaits, found.platform));
    if exact && !found.linked.exact {
        enshwick_loader_api::info!("exact waits: on at the next start");
    }
    waits::set_exact(exact && found.linked.exact);
    if settings.server_wait_cap_ms > 1 && !found.linked.server {
        enshwick_loader_api::info!("server wait cap: from the next start");
    }
    waits::set_cap(settings.server_wait_cap_ms);
    let running = running(found, settings);
    enshwick_loader_api::info!(
        "{running}; measure mode {}",
        if settings.measure { "on" } else { "off" }
    );
    measure::set(settings.measure, running);
}

/// The status lines.
fn status() -> String {
    let Some(found) = FOUND.get() else {
        return String::new();
    };
    let mut lines = vec![found.summary.clone(), measure::context()];
    let last = measure::last();
    if !last.is_empty() {
        lines.push(last);
    }
    lines.join("\n")
}

/// The mod. Its running state is in statics the hooks read.
#[derive(Debug)]
pub struct Optimizer;

impl Mod for Optimizer {
    fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
        enshwick_loader_api::manifest!(
            id: "optimizer",
            side: BOTH,
            name: "Optimizer",
            replaces: ["proton-optimizer"],
            updates: github("themaiby/enshwick", "enshwick-optimizer"),
            signed_by: ["RWTjqgtfpy3lSOLC4uaUXuvvKSLQKOFf6nUnEytoiFjSzxMEhLnoAdJF"],
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = load.config()?;
        let environment = env::Environment::probe(&measure::game_log_text());
        let wine = environment.is_wine();
        let platform = Platform::of(load.host().build().side == Side::SERVER, wine);
        let summary = environment.summary();
        enshwick_loader_api::info!("environment: {summary}");
        for advice in environment.advice() {
            enshwick_loader_api::info!("advice: {advice}");
        }
        if !wine {
            waits::tune_for_windows();
        }
        let exact = settings
            .exact_waits
            .applies(default_on(Change::ExactWaits, platform));
        note_forced(
            Change::ExactWaits,
            settings.exact_waits == Switch::On,
            platform,
        );
        note_forced(Change::TaskWorkers, settings.task_workers > 0, platform);
        let mut workers = worker_cap(settings.task_workers, platform, environment.cpu_limit);
        if let Some(cap) = workers
            && !pacing::cap_task_workers(load, TaskWorkers::at_most(cap))?
        {
            enshwick_loader_api::info!(
                "task workers: only the dedicated server starts fewer; here the game picks"
            );
            workers = None;
        }
        let server_loop = settings.measure || settings.server_wait_cap_ms > 1;
        let linked = waits::link(load, exact, server_loop)?;
        enshwick_loader_api::info!(
            "hooked: exact waits {}, message wait {}, server loop wait {}, task workers {}",
            linked.exact,
            linked.message,
            linked.server,
            workers.is_some()
        );
        load.on::<events::Ticked>(|_| waits::stepped())?;
        let _first = FOUND.set(Found {
            platform,
            summary: format!("environment: {summary}"),
            linked,
            workers,
        });
        load.add_status(status)?;
        load.on_config_change(|settings: Settings| apply(&settings))?;
        apply(&settings);
        Ok(Self)
    }

    fn ready(&'static self, _host: Host) {
        let wine = FOUND.get().is_some_and(|found| {
            matches!(found.platform, Platform::WineServer | Platform::WineClient)
        });
        let started = std::thread::Builder::new()
            .name("enshwick-optimizer".into())
            .spawn(move || measure::run(wine));
        if let Err(e) = started {
            enshwick_loader_api::warn!("measure mode does not run: {e}");
        }
    }
}

enshwick_loader_api::declare_mod!(Optimizer);

#[cfg(test)]
mod tests {
    use enshwick_loader_api::abi::Status;
    use enshwick_loader_api::config::Config as _;

    use super::*;

    #[test]
    fn auto_follows_the_table() {
        assert!(Switch::Auto.applies(true));
        assert!(!Switch::Auto.applies(false));
        assert!(Switch::On.applies(false));
        assert!(!Switch::Off.applies(true));
    }

    #[test]
    fn task_workers_follow_the_cpu_limit_under_wine_only() {
        let four = Some(CpuLimit::Cpus(3.5));
        assert_eq!(worker_cap(0, Platform::WineServer, four), Some(4));
        assert_eq!(
            worker_cap(0, Platform::WineServer, Some(CpuLimit::Cpus(0.5))),
            Some(2)
        );
        assert_eq!(
            worker_cap(0, Platform::WineServer, Some(CpuLimit::Unlimited)),
            None
        );
        assert_eq!(worker_cap(0, Platform::WineServer, None), None);
        assert_eq!(worker_cap(0, Platform::WindowsServer, four), None);
        assert_eq!(worker_cap(6, Platform::WindowsServer, None), Some(6));
    }

    #[test]
    fn the_defaults_parse_and_ranges_hold() {
        let settings: Settings = toml::from_str(&Settings::template()).unwrap();
        assert_eq!(settings.exact_waits, Switch::Auto);
        assert_eq!(settings.task_workers, 0);
        assert_eq!(settings.server_wait_cap_ms, 1);
        assert!(settings.measure);
        assert!(settings.validate().is_ok());
        let wide: Settings = toml::from_str(
            "exact_waits = \"on\"\ntask_workers = 0\nserver_wait_cap_ms = 9\nmeasure = false",
        )
        .unwrap();
        assert!(wide.validate().is_err());
        assert!(
            toml::from_str::<Settings>(
                "exact_waits = \"maybe\"\ntask_workers = 0\nserver_wait_cap_ms = 1\nmeasure = true"
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
            .settings(
                "exact_waits = \"on\"\ntask_workers = 0\nserver_wait_cap_ms = 4\nmeasure = true\n",
            )
            .install();
        // SAFETY: the export of `declare_mod!` above. Without `ready` no
        // measuring thread sets the slack to the timer resolution.
        let status = unsafe { loader.init(enshwick_mod_init) };
        waits::set_slack(0);
        waits::set_wake(0);
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(loader.mid_hooks(), [wait, server]);

        // A zero wait with 0.3 ms left sleeps them and keeps the zero wait.
        let slept = waits::SLEEPS.load(core::sync::atomic::Ordering::Relaxed);
        let mut registers = enshwick_loader_api::abi::Registers {
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
        let mut registers = enshwick_loader_api::abi::Registers {
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
        let mut registers = enshwick_loader_api::abi::Registers {
            r9: 1_000_000,
            ..Default::default()
        };
        assert_eq!(loader.run_at(server, &mut registers), 1);
        assert_eq!(registers.r9, 1_000_000);
        assert_eq!(
            loader.raise::<events::Ticked>(&enshwick_loader_api::abi::TickData {
                header: enshwick_loader_api::abi::Header::of::<enshwick_loader_api::abi::TickData>(
                ),
                step: 1
            }),
            1
        );
        assert_eq!(loader.run_at(server, &mut registers), 1);
        assert_eq!(registers.r9, 4_000_000);
        let mut registers = enshwick_loader_api::abi::Registers {
            r9: 40_000_000,
            ..Default::default()
        };
        assert_eq!(loader.run_at(server, &mut registers), 1);
        assert_eq!(registers.r9, 40_000_000);

        // Off at once.
        loader.set_settings(
            "exact_waits = \"off\"\ntask_workers = 0\nserver_wait_cap_ms = 1\nmeasure = true\n",
        );
        assert_eq!(loader.raise::<events::SettingsChanged>(&()), 1);
        let mut registers = enshwick_loader_api::abi::Registers {
            r9: 1_000_000,
            ..Default::default()
        };
        assert_eq!(loader.run_at(server, &mut registers), 1);
        assert_eq!(registers.r9, 1_000_000);
        let mut registers = enshwick_loader_api::abi::Registers {
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
