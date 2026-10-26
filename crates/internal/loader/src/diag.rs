//! Diagnostics: what the loader keeps for its status summary, and the
//! summary's text.
//!
//! Mods turned off and why, the names hooked functions were found under and
//! the code patches written are kept as they happen, all of it off the game's
//! hot paths. Hook call counters live beside the hook thunks (see
//! `patch.rs`) and exist only with `diagnostics = true`; without it a hooked
//! call runs exactly the instructions it ran before.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::settings::Settings;

/// With diagnostics timing on, one call in this many of each hooked
/// function is timed. A power of two: the thunk tests the counter's low byte.
pub const SAMPLE_EVERY: u64 = 256;
/// Mods turned off that the summary remembers; older ones are dropped.
const HISTORY: usize = 64;
/// Hooked functions the summary lists, busiest first.
const HOOK_LINES: usize = 40;

/// Counting and timing, fixed by the settings read at start.
static COUNTING: OnceLock<(bool, bool)> = OnceLock::new();
static STATUS_MINUTES: AtomicU32 = AtomicU32::new(crate::settings::STATUS_MINUTES);
static TURNED_OFF: Mutex<Vec<TurnedOff>> = Mutex::new(Vec::new());
static NAMES: Mutex<BTreeMap<u64, String>> = Mutex::new(BTreeMap::new());
static PATCHES: [AtomicUsize; 3] = [const { AtomicUsize::new(0) }; 3];

/// Takes the diagnostics settings: the period at every call, counting and
/// timing only at the first, since thunks are written once.
pub fn configure(settings: &Settings) {
    STATUS_MINUTES.store(settings.status_minutes, Ordering::Relaxed);
    crate::migrations::set_dry_run(settings.migrations_dry_run);
    let _first = COUNTING.set((
        settings.diagnostics,
        settings.diagnostics && settings.diagnostics_timing,
    ));
}

/// Whether hook calls are counted in this session.
#[must_use]
pub fn counting() -> bool {
    COUNTING.get().is_some_and(|&(on, _)| on)
}

/// Whether every [`SAMPLE_EVERY`]th hook call is timed in this session.
#[must_use]
pub fn timing() -> bool {
    COUNTING.get().is_some_and(|&(_, timed)| timed)
}

/// Minutes between status summaries; 0 for on demand only.
#[must_use]
pub fn status_minutes() -> u32 {
    STATUS_MINUTES.load(Ordering::Relaxed)
}

/// Seconds since the Unix epoch.
#[must_use]
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// A mod the loader turned off, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurnedOff {
    /// The mod id.
    pub id: String,
    /// Why, as the log says.
    pub reason: String,
    /// When, in seconds since the Unix epoch.
    pub at: u64,
}

/// Remembers that mod `id` was turned off for `reason`.
pub fn turned_off(id: &str, reason: &str) {
    let mut history = TURNED_OFF.lock().unwrap_or_else(PoisonError::into_inner);
    if history.len() == HISTORY {
        history.remove(0);
    }
    history.push(TurnedOff {
        id: id.to_owned(),
        reason: reason.to_owned(),
        at: now(),
    });
}

/// The mods turned off so far, oldest first.
#[must_use]
pub fn history() -> Vec<TurnedOff> {
    TURNED_OFF
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// Remembers that `address` was found as `name`, a symbol or a system, so the
/// summary can name a hooked function.
pub fn name(address: u64, name: &str) {
    NAMES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .entry(address)
        .or_insert_with(|| name.to_owned());
}

/// The name `address` was first found under.
#[must_use]
pub fn name_of(address: u64) -> Option<String> {
    NAMES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&address)
        .cloned()
}

/// What became of the code patches: written, left out for a mod that is off,
/// skipped.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Patches {
    /// Written into the game.
    pub written: usize,
    /// Left out because their mod was off.
    pub off: usize,
    /// Skipped: their bytes did not match or they clashed.
    pub skipped: usize,
}

/// Records what became of the code patches at arming.
pub fn record_patches(patches: Patches) {
    PATCHES[0].store(patches.written, Ordering::Relaxed);
    PATCHES[1].store(patches.off, Ordering::Relaxed);
    PATCHES[2].store(patches.skipped, Ordering::Relaxed);
}

/// What became of the code patches at arming.
#[must_use]
pub fn patches() -> Patches {
    Patches {
        written: PATCHES[0].load(Ordering::Relaxed),
        off: PATCHES[1].load(Ordering::Relaxed),
        skipped: PATCHES[2].load(Ordering::Relaxed),
    }
}

/// One hooked game function's calls.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HookStat {
    /// The name it was found under, or its address in the game.
    pub name: String,
    /// The mods linked on it, `enshwick` for the loader.
    pub owners: Vec<String>,
    /// Calls since the hooks were armed.
    pub calls: u64,
    /// Timed calls.
    pub samples: u64,
    /// Time-stamp counter ticks the timed calls took, the hook chain and the
    /// original together.
    pub ticks: u64,
}

impl HookStat {
    /// The mean time of a timed call in nanoseconds, given the time-stamp
    /// counter's rate.
    #[must_use]
    #[expect(
        clippy::cast_precision_loss,
        reason = "a mean for people to read; f64 keeps 15 digits"
    )]
    pub fn mean_ns(&self, tsc_hz: f64) -> Option<f64> {
        (self.samples > 0 && tsc_hz > 0.0)
            .then(|| self.ticks as f64 / self.samples as f64 / tsc_hz * 1e9)
    }
}

/// A mod in the summary.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModStatus {
    /// The mod id.
    pub id: String,
    /// The name players read; empty for none.
    pub name: String,
    /// Its version.
    pub version: String,
    /// Authors, licence and homepage, as `Meta::credits` writes them.
    pub credits: String,
    /// Notes on its relations, such as a recommended mod that is missing.
    pub notes: Vec<String>,
    /// Whether it runs.
    pub running: bool,
    /// The lines its status sources gave.
    pub lines: Vec<String>,
}

/// What mods serve to the game.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Served {
    /// Resources mods serve, new or changed.
    pub resources: usize,
    /// Of those, built so far.
    pub built: usize,
    /// Of those, failed to build; the game read the base.
    pub failed: usize,
    /// Texts mods added.
    pub texts: usize,
}

/// Everything the status summary shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Summary {
    /// The loader's version.
    pub version: String,
    /// The game build's name.
    pub game: String,
    /// Whether this process runs a game server now.
    pub hosting: bool,
    /// Since the loader started.
    pub uptime: Duration,
    /// How the session before ended, when it crashed: the crash line.
    pub last_crash: Option<String>,
    /// When the summary was taken, in seconds since the Unix epoch.
    pub at: u64,
    /// Every loaded mod, in load order.
    pub mods: Vec<ModStatus>,
    /// Mods turned off in the mod list, by id.
    pub disabled: Vec<String>,
    /// Mods that present mods replace, never started, a line each.
    pub replaced: Vec<String>,
    /// Mods turned off, oldest first.
    pub turned_off: Vec<TurnedOff>,
    /// The services mods publish and the events they define, a line each.
    pub links: Vec<String>,
    /// What mods serve.
    pub served: Served,
    /// What became of the code patches.
    pub patches: Patches,
    /// Hooked functions armed.
    pub targets: usize,
    /// Each hooked function's calls, with diagnostics on.
    pub hooks: Option<Vec<HookStat>>,
    /// Whether calls are timed.
    pub timed: bool,
    /// The time-stamp counter's rate, once measured.
    pub tsc_hz: Option<f64>,
    /// What each mod costs, a header and a line per mod.
    pub costs: Vec<String>,
    /// Hooks, features and resources mods share, a line each.
    pub conflicts: Vec<String>,
    /// What the knowledge guard does and has counted, once it runs.
    pub knowledge_guard: Option<String>,
}

impl Summary {
    /// The summary for `status.log`.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::with_capacity(2048);
        let _w = writeln!(
            out,
            "Enshwick {} status at {}",
            self.version,
            clock(self.at)
        );
        let _w = writeln!(
            out,
            "game: {}; {}; up {}",
            self.game,
            if self.hosting {
                "a game server runs here"
            } else {
                "no game server runs here"
            },
            duration(self.uptime)
        );
        if let Some(crash) = &self.last_crash {
            let _w = writeln!(out, "the last session crashed: {crash}");
        }
        let running = self.mods.iter().filter(|m| m.running).count();
        let _w = writeln!(out, "\nmods: {running} of {} running", self.mods.len());
        for m in &self.mods {
            let off = if m.running { "" } else { " (off)" };
            let _w = if m.name.is_empty() || m.name == m.id {
                writeln!(out, "  {} {}{off}", m.id, m.version)
            } else {
                writeln!(out, "  {} ({}) {}{off}", m.name, m.id, m.version)
            };
            if !m.credits.is_empty() {
                let _w = writeln!(out, "    {}", m.credits);
            }
            for note in &m.notes {
                let _w = writeln!(out, "    note: {note}");
            }
            for line in &m.lines {
                let _w = writeln!(out, "    {line}");
            }
        }
        if !self.disabled.is_empty() {
            let _w = writeln!(
                out,
                "  off in {}: {}",
                crate::modlist::LIST_PATH,
                self.disabled.join(", ")
            );
        }
        for line in &self.replaced {
            let _w = writeln!(out, "  not started: {line}");
        }
        if !self.turned_off.is_empty() {
            let _w = writeln!(out, "\nturned off:");
            for t in &self.turned_off {
                let _w = writeln!(out, "  {} {}: {}", clock(t.at), t.id, t.reason);
            }
        }
        if !self.links.is_empty() {
            let _w = writeln!(
                out,
                "
services and events:"
            );
            for line in &self.links {
                let _w = writeln!(out, "  {line}");
            }
        }
        let s = self.served;
        let p = self.patches;
        let _w = writeln!(
            out,
            "\ncontent: {} resources served ({} built, {} failed), {} texts",
            s.resources, s.built, s.failed, s.texts
        );
        let _w = writeln!(
            out,
            "code patches: {} written, {} left out (mod off), {} skipped",
            p.written, p.off, p.skipped
        );
        if let Some(guard) = &self.knowledge_guard {
            let _w = writeln!(out, "knowledge guard: {guard}");
        }
        let _w = writeln!(out, "hooks: {} functions armed", self.targets);
        match &self.hooks {
            None => {
                let _w = writeln!(
                    out,
                    "  calls are not counted; set diagnostics = true in mod-configs/enshwick.toml"
                );
            }
            Some(hooks) => self.render_hooks(&mut out, hooks),
        }
        self.render_costs_and_conflicts(&mut out);
        out
    }

    fn render_costs_and_conflicts(&self, out: &mut String) {
        if let Some((header, lines)) = self.costs.split_first() {
            let _w = writeln!(
                out,
                "
{header}"
            );
            for line in lines {
                let _w = writeln!(out, "  {line}");
            }
        }
        if self.conflicts.is_empty() {
            let _w = writeln!(
                out,
                "
conflicts: none, no two mods share a hook, feature or resource"
            );
        } else {
            let _w = writeln!(
                out,
                "
conflicts:"
            );
            for line in &self.conflicts {
                let _w = writeln!(out, "  {line}");
            }
        }
    }

    fn render_hooks(&self, out: &mut String, hooks: &[HookStat]) {
        let mut sorted: Vec<&HookStat> = hooks.iter().collect();
        sorted.sort_by(|a, b| b.calls.cmp(&a.calls).then_with(|| a.name.cmp(&b.name)));
        let timing = if self.timed {
            format!(", one in {SAMPLE_EVERY} timed")
        } else {
            String::new()
        };
        let _w = writeln!(out, "  calls since start{timing}:");
        for hook in sorted.iter().take(HOOK_LINES) {
            let mean = match self.tsc_hz.and_then(|hz| hook.mean_ns(hz)) {
                Some(ns) => format!("{:>10}", nanos(ns)),
                None => format!("{:>10}", "-"),
            };
            let _w = writeln!(
                out,
                "  {:>12} {mean}  {} ({})",
                hook.calls,
                hook.name,
                hook.owners.join(", ")
            );
        }
        if sorted.len() > HOOK_LINES {
            let _w = writeln!(out, "  and {} more", sorted.len() - HOOK_LINES);
        }
    }

    /// A few lines for the chat.
    #[must_use]
    pub fn short(&self) -> String {
        let running = self.mods.iter().filter(|m| m.running).count();
        let mut lines = vec![format!(
            "Enshwick {}, up {}{}: {running} of {} mods running.",
            self.version,
            duration(self.uptime),
            if self.hosting { ", server running" } else { "" },
            self.mods.len()
        )];
        lines.extend(self.replaced.iter().map(|line| format!("{line}.")));
        for t in self.turned_off.iter().rev().take(3) {
            lines.push(format!("{} off: {}", t.id, t.reason));
        }
        let busiest = self
            .hooks
            .as_ref()
            .and_then(|hooks| hooks.iter().max_by_key(|h| h.calls))
            .filter(|h| h.calls > 0)
            .map(|h| format!(", busiest {} with {} calls", h.name, count(h.calls)))
            .unwrap_or_default();
        lines.push(format!(
            "{} hooked functions{busiest}; {} resources served; {} code patches.",
            self.targets, self.served.resources, self.patches.written
        ));
        lines.push("Full summary: logs/enshwick/status.log".to_owned());
        lines.join("\n")
    }
}

/// `hh:mm:ssZ` of a Unix time.
fn clock(at: u64) -> String {
    format!("{:02}:{:02}:{:02}Z", at / 3600 % 24, at / 60 % 60, at % 60)
}

/// `3d 4h`, `2h 03m`, `5m 07s` or `9s`.
#[must_use]
pub fn duration(d: Duration) -> String {
    let s = d.as_secs();
    match s {
        0..60 => format!("{s}s"),
        60..3600 => format!("{}m {:02}s", s / 60, s % 60),
        3600..86_400 => format!("{}h {:02}m", s / 3600, s / 60 % 60),
        _ => format!("{}d {}h", s / 86_400, s / 3600 % 24),
    }
}

/// `950`, `12.3k`, `4.5M`, `1.2G`.
#[expect(
    clippy::cast_precision_loss,
    reason = "a rounded count for people to read"
)]
fn count(n: u64) -> String {
    match n {
        0..1000 => n.to_string(),
        1000..1_000_000 => format!("{:.1}k", n as f64 / 1e3),
        1_000_000..1_000_000_000 => format!("{:.1}M", n as f64 / 1e6),
        _ => format!("{:.1}G", n as f64 / 1e9),
    }
}

/// `850 ns`, `12.4 us`, `3.1 ms`.
fn nanos(ns: f64) -> String {
    if ns < 1e3 {
        format!("{ns:.0} ns")
    } else if ns < 1e6 {
        format!("{:.1} us", ns / 1e3)
    } else {
        format!("{:.1} ms", ns / 1e6)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary() -> Summary {
        Summary {
            version: "0.1.5".into(),
            game: "1.0 server".into(),
            hosting: true,
            uptime: Duration::from_mins(123),
            last_crash: Some("fault 0xc0000005, blamed on you.trading-post: in its code".into()),
            at: 45_000,
            mods: vec![
                ModStatus {
                    id: "you.lockers".into(),
                    name: "Flame Altar Zones".into(),
                    version: "0.1.4".into(),
                    credits: "by themaiby; MIT".into(),
                    notes: vec!["recommends you.lanterns, which is not loaded".into()],
                    running: true,
                    lines: vec!["3 zones merged".into()],
                },
                ModStatus {
                    id: "you.trading-post".into(),
                    name: String::new(),
                    version: "0.1.1".into(),
                    credits: String::new(),
                    notes: Vec::new(),
                    running: false,
                    lines: Vec::new(),
                },
            ],
            disabled: vec!["you.my-mod".into()],
            replaced: vec![
                "you.stalls is replaced by you.market-stalls; delete mods/stalls.dll".into(),
            ],
            turned_off: vec![TurnedOff {
                id: "you.trading-post".into(),
                reason: "panic in an action".into(),
                at: 44_000,
            }],
            links: vec!["event trading-post:sold from trading-post 0.3.0 to market-stalls".into()],
            served: Served {
                resources: 4,
                built: 4,
                failed: 0,
                texts: 12,
            },
            patches: Patches {
                written: 1,
                off: 0,
                skipped: 0,
            },
            targets: 2,
            hooks: Some(vec![
                HookStat {
                    name: "chat.post".into(),
                    owners: vec!["enshwick".into()],
                    calls: 10,
                    samples: 0,
                    ticks: 0,
                },
                HookStat {
                    name: "building.max_altars".into(),
                    owners: vec!["you.lanterns".into()],
                    calls: 1_234_567,
                    samples: 4,
                    ticks: 9_000,
                },
            ]),
            timed: true,
            tsc_hz: Some(3e9),
            costs: vec![
                "per-mod costs over 2h 03m (since start), work per second of time:".into(),
                "you.trading-post 0.20 ms/s: interaction filter 0.20 ms/s in 100 calls".into(),
            ],
            conflicts: vec![
                "interaction filter: you.trading-post, you.market-stalls, chained".into(),
            ],
            knowledge_guard: Some("watching; 3 requests it would refuse let through".into()),
        }
    }

    #[test]
    fn the_summary_lists_mods_faults_content_and_the_busiest_hooks_first() {
        let text = summary().render();
        assert!(text.starts_with("Enshwick 0.1.5 status at 12:30:00Z\n"));
        assert!(text.contains("up 2h 03m"));
        assert!(text.contains(
            "\nthe last session crashed: fault 0xc0000005, blamed on you.trading-post: in its code\n"
        ));
        assert!(text.contains("mods: 1 of 2 running"));
        assert!(text.contains(
            "  Flame Altar Zones (you.lockers) 0.1.4\n    by themaiby; MIT\n    \
             note: recommends you.lanterns, which is not loaded\n    3 zones merged\n"
        ));
        assert!(text.contains(
            "  you.trading-post 0.1.1 (off)\n  off in mods/enshwick/mods.toml: you.my-mod\n"
        ));
        assert!(text.contains(
            "  not started: you.stalls is replaced by you.market-stalls; delete mods/stalls.dll\n"
        ));
        assert!(text.contains("12:13:20Z you.trading-post: panic in an action"));
        assert!(text.contains(
            "services and events:
  event trading-post:sold from trading-post 0.3.0 to market-stalls
"
        ));
        assert!(text.contains("4 resources served (4 built, 0 failed), 12 texts"));
        assert!(text.contains(
            "
knowledge guard: watching; 3 requests it would refuse"
        ));
        assert!(text.contains(
            "
per-mod costs over 2h 03m (since start), work per second of time:
  you.trading-post 0.20 ms/s"
        ));
        assert!(text.contains(
            "
conflicts:
  interaction filter: you.trading-post, you.market-stalls, chained
"
        ));
        assert!(text.contains("one in 256 timed"));
        let busy = text.find("building.max_altars").unwrap();
        let quiet = text.find("chat.post").unwrap();
        assert!(busy < quiet);
        // 9000 ticks over 4 calls at 3 GHz.
        assert!(text.contains("750 ns  building.max_altars (you.lanterns)"));
    }

    #[test]
    fn without_diagnostics_the_summary_says_how_to_turn_them_on() {
        let mut summary = summary();
        summary.hooks = None;
        assert!(summary.render().contains("set diagnostics = true"));
        assert!(!summary.short().contains("busiest"));
    }

    #[test]
    fn the_chat_summary_is_short() {
        let short = summary().short();
        assert_eq!(short.lines().count(), 5);
        assert!(short.contains("you.stalls is replaced by you.market-stalls; delete"));
        assert!(short.starts_with("Enshwick 0.1.5, up 2h 03m, server running: 1 of 2 mods"));
        assert!(short.contains("you.trading-post off: panic in an action"));
        assert!(short.contains("busiest building.max_altars with 1.2M calls"));
    }

    #[test]
    fn numbers_read_well() {
        assert_eq!(duration(Duration::from_secs(9)), "9s");
        assert_eq!(duration(Duration::from_secs(307)), "5m 07s");
        assert_eq!(duration(Duration::from_hours(76)), "3d 4h");
        assert_eq!(count(950), "950");
        assert_eq!(count(12_345), "12.3k");
        assert_eq!(count(4_500_000_000), "4.5G");
        assert_eq!(nanos(850.0), "850 ns");
        assert_eq!(nanos(12_400.0), "12.4 us");
        assert_eq!(nanos(3_100_000.0), "3.1 ms");
    }

    #[test]
    fn the_history_keeps_the_latest_turn_offs() {
        for i in 0..HISTORY + 2 {
            turned_off("a", &format!("reason {i}"));
        }
        let history = history();
        assert_eq!(history.len(), HISTORY);
        assert_eq!(history[0].reason, "reason 2");
        name(0x10, "chat.post");
        name(0x10, "later");
        assert_eq!(name_of(0x10).as_deref(), Some("chat.post"));
    }
}
