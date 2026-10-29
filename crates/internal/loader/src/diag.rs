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
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use enshwick_gamedata::{Part, PartState};

use crate::locks::Relock;
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
static PARTS_OFF: Mutex<Vec<PartOff>> = Mutex::new(Vec::new());
static ANY_PART_OFF: AtomicBool = AtomicBool::new(false);
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
    let mut history = TURNED_OFF.relock();
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
    TURNED_OFF.relock().clone()
}

/// A part of the loader turned off while the game runs, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartOff {
    /// The part, as its fuse or its link names it.
    pub part: String,
    /// Why, as the log says.
    pub reason: String,
    /// When, in seconds since the Unix epoch.
    pub at: u64,
}

/// Remembers that the loader's `part` is off for `reason`, for the status
/// summary and the Mods screen. A part keeps its first reason; past
/// [`HISTORY`] parts, later ones reach only the log.
pub fn part_off(part: &str, reason: &str) {
    keep_part(&mut PARTS_OFF.relock(), part, reason, now());
    ANY_PART_OFF.store(true, Ordering::Release);
}

/// Whether any part is off, read without the lock, for a per-frame check.
#[must_use]
pub fn any_part_off() -> bool {
    ANY_PART_OFF.load(Ordering::Acquire)
}

/// Adds `part` off for `reason` at `at` to `parts`, unless it is there or
/// `parts` is full.
fn keep_part(parts: &mut Vec<PartOff>, part: &str, reason: &str, at: u64) {
    if parts.len() == HISTORY || parts.iter().any(|p| p.part == part) {
        return;
    }
    parts.push(PartOff {
        part: part.to_owned(),
        reason: reason.to_owned(),
        at,
    });
}

/// The loader's parts that are off, in the order they went off.
#[must_use]
pub fn parts_off() -> Vec<PartOff> {
    PARTS_OFF.relock().clone()
}

/// Remembers that `address` was found as `name`, a symbol or a system, so the
/// summary can name a hooked function.
pub fn name(address: u64, name: &str) {
    NAMES
        .relock()
        .entry(address)
        .or_insert_with(|| name.to_owned());
}

/// The name `address` was first found under.
#[must_use]
pub fn name_of(address: u64) -> Option<String> {
    NAMES.relock().get(&address).cloned()
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
    /// The features it reported on or off.
    pub features: Vec<crate::features::Feature>,
}

impl ModStatus {
    /// The mod's lines of the summary.
    fn render(&self, out: &mut String) {
        let off = if self.running { "" } else { " (off)" };
        let _w = if self.name.is_empty() || self.name == self.id {
            writeln!(out, "  {} {}{off}", self.id, self.version)
        } else {
            writeln!(out, "  {} ({}) {}{off}", self.name, self.id, self.version)
        };
        if !self.credits.is_empty() {
            let _w = writeln!(out, "    {}", self.credits);
        }
        for note in &self.notes {
            let _w = writeln!(out, "    note: {note}");
        }
        for feature in &self.features {
            let _w = match &feature.off {
                Some(why) => writeln!(out, "    feature {} off: {why}", feature.name),
                None => writeln!(out, "    feature {} on", feature.name),
            };
        }
        for line in &self.lines {
            let _w = writeln!(out, "    {line}");
        }
    }
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
    /// The DLLs that run beside the mods without the API, a line per folder.
    pub others: Vec<String>,
    /// Mods that present mods replace, never started, a line each.
    pub replaced: Vec<String>,
    /// Mods turned off, oldest first.
    pub turned_off: Vec<TurnedOff>,
    /// The loader's parts turned off while the game runs, oldest first.
    pub parts_off: Vec<PartOff>,
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
    /// Other world keys of the running world's save name that hold mod
    /// data, such as the world before its save moved; empty for none.
    pub world_folders: Vec<String>,
    /// What became of mod messages that did not arrive as sent, once any
    /// did not.
    pub messages: Option<String>,
    /// Each loader feature that names build table entries, with what this
    /// build lacks of them.
    pub parts: Vec<Part<'static>>,
}

/// The summary's line on mod messages that did not arrive as sent, `None`
/// when all did.
///
/// It counts the `messages` a side dropped whole, the frame `lines` the
/// server's queue dropped, and the lines for one player that reached
/// others (`strayed`).
///
/// # Examples
///
/// ```
/// use enshwick_loader::diag::messages_line;
///
/// assert_eq!(messages_line(0, 0, 0), None);
/// assert!(messages_line(0, 1, 0).is_some_and(|line| line.starts_with("1 line for one player dropped")));
/// ```
#[must_use]
pub fn messages_line(messages: u64, lines: u64, strayed: u64) -> Option<String> {
    let counted =
        |n: u64, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let mut parts = Vec::new();
    if messages > 0 {
        parts.push(format!(
            "{} (over a rate or unreadable)",
            counted(messages, "message dropped", "messages dropped")
        ));
    }
    if lines > 0 {
        parts.push(format!(
            "{} (queue full, player gone or such lines off)",
            counted(
                lines,
                "line for one player dropped",
                "lines for one player dropped"
            )
        ));
    }
    if strayed > 0 {
        parts.push(format!(
            "{}, lines for one player off",
            counted(
                strayed,
                "line for one player reached others",
                "lines for one player reached others"
            )
        ));
    }
    (!parts.is_empty()).then(|| parts.join(", "))
}

/// What the log and the summary say of a loader feature this build does
/// not serve whole; `None` when it does.
#[must_use]
pub fn part_line(part: &Part<'_>) -> Option<String> {
    let how = match part.state() {
        PartState::Works => return None,
        PartState::Off => "off",
        _ => "partly off",
    };
    Some(format!(
        "{} {how}: missing {}",
        part.name,
        part.missing.join(", ")
    ))
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
        self.render_parts(&mut out);
        let running = self.mods.iter().filter(|m| m.running).count();
        let _w = writeln!(out, "\nmods: {running} of {} running", self.mods.len());
        for m in &self.mods {
            m.render(&mut out);
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
        for line in &self.others {
            let _w = writeln!(out, "  {line}");
        }
        if !self.turned_off.is_empty() {
            let _w = writeln!(out, "\nturned off:");
            for t in &self.turned_off {
                let _w = writeln!(out, "  {} {}: {}", clock(t.at), t.id, t.reason);
            }
        }
        self.render_parts_off(&mut out);
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
        if let Some(messages) = &self.messages {
            let _w = writeln!(out, "mod messages: {messages}");
        }
        if !self.world_folders.is_empty() {
            let _w = writeln!(
                out,
                "mod data of other worlds of this save name: {}; a moved world takes it over \
                 only when one is left (see mod-data/<mod>/worlds/)",
                self.world_folders.join(", ")
            );
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

    fn render_parts(&self, out: &mut String) {
        if self.parts.is_empty() {
            return;
        }
        let lines: Vec<String> = self.parts.iter().filter_map(part_line).collect();
        let total = self.parts.len();
        let _w = if lines.is_empty() {
            writeln!(out, "\nloader features: all {total} work on this build")
        } else {
            writeln!(
                out,
                "\nloader features: {} of {total} work on this build",
                total - lines.len()
            )
        };
        for line in &lines {
            let _w = writeln!(out, "  {line}");
        }
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

    /// The loader's parts that are off, under their own heading.
    fn render_parts_off(&self, out: &mut String) {
        if !self.parts_off.is_empty() {
            let _w = writeln!(out, "\nloader parts off:");
            for p in &self.parts_off {
                let _w = writeln!(out, "  {} {}: {}", clock(p.at), p.part, p.reason);
            }
        }
    }

    /// A few lines for the console, in the reader's language.
    #[must_use]
    pub fn short(&self) -> String {
        let running = self.mods.iter().filter(|m| m.running).count().to_string();
        let mods = self.mods.len().to_string();
        let uptime = duration(self.uptime);
        let head = if self.hosting {
            "status.short.server"
        } else {
            "status.short"
        };
        let mut lines = vec![crate::texts::fill(
            head,
            &[
                ("version", &self.version),
                ("uptime", &uptime),
                ("running", &running),
                ("mods", &mods),
            ],
        )];
        lines.extend(self.replaced.iter().map(|line| format!("{line}.")));
        for t in self.turned_off.iter().rev().take(3) {
            lines.push(crate::texts::fill(
                "status.off",
                &[("id", &t.id), ("reason", &t.reason)],
            ));
        }
        for p in self.parts_off.iter().take(3) {
            lines.push(crate::texts::fill(
                "status.off",
                &[("id", &p.part), ("reason", &p.reason)],
            ));
        }
        let hooks = self.targets.to_string();
        let resources = self.served.resources.to_string();
        let patches = self.patches.written.to_string();
        let mut args = vec![
            ("hooks", hooks.as_str()),
            ("resources", resources.as_str()),
            ("patches", patches.as_str()),
        ];
        let busiest = self
            .hooks
            .as_ref()
            .and_then(|hooks| hooks.iter().max_by_key(|h| h.calls))
            .filter(|h| h.calls > 0)
            .map(|h| (h.name.as_str(), count(h.calls)));
        let line = if let Some((hook, calls)) = &busiest {
            args.extend([("hook", *hook), ("calls", calls.as_str())]);
            "status.hooks.busiest"
        } else {
            "status.hooks"
        };
        lines.push(crate::texts::fill(line, &args));
        lines.push(crate::texts::fill("status.full", &[]));
        lines.join("\n")
    }
}

/// `hh:mm:ssZ` of a Unix time.
pub(crate) fn clock(at: u64) -> String {
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

/// The main menu's line under the game's version, in the reader's language.
///
/// It names the loader's version and how many `mods` load, how many of them
/// are `off`, and whether this game version leaves some of the loader's
/// features off (`lacking`).
#[must_use]
pub fn menu_line(version: &str, mods: usize, off: usize, lacking: bool) -> String {
    let mods = mods.to_string();
    let off = off.to_string();
    let line = if off == "0" {
        crate::texts::fill("menu.line", &[("version", version), ("mods", &mods)])
    } else {
        crate::texts::fill(
            "menu.line.off",
            &[("version", version), ("mods", &mods), ("off", &off)],
        )
    };
    if lacking {
        crate::texts::fill("menu.line.lacking", &[("line", &line)])
    } else {
        line
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mods() -> Vec<ModStatus> {
        vec![
            ModStatus {
                id: "you.lockers".into(),
                name: "Flame Altar Zones".into(),
                version: "0.1.4".into(),
                credits: "by themaiby; MIT".into(),
                notes: vec!["recommends you.lanterns, which is not loaded".into()],
                running: true,
                lines: vec!["3 zones merged".into()],
                features: vec![
                    crate::features::Feature {
                        name: "glow".into(),
                        off: Some("symbol lantern.glow: missing on this game build".into()),
                    },
                    crate::features::Feature {
                        name: "light".into(),
                        off: None,
                    },
                ],
            },
            ModStatus {
                id: "you.trading-post".into(),
                name: String::new(),
                version: "0.1.1".into(),
                credits: String::new(),
                notes: Vec::new(),
                running: false,
                lines: Vec::new(),
                features: Vec::new(),
            },
        ]
    }

    #[test]
    fn the_menu_line_counts_mods_and_warns_of_what_is_off() {
        assert_eq!(menu_line("1.0.0", 6, 0, false), "Enshwick 1.0.0 · mods: 6");
        assert_eq!(
            menu_line("1.0.0", 6, 2, false),
            "Enshwick 1.0.0 · mods: 6, off: 2"
        );
        assert_eq!(
            menu_line("1.0.0", 0, 0, true),
            "Enshwick 1.0.0 · mods: 0 · some features are off on this game version"
        );
    }

    fn summary() -> Summary {
        Summary {
            version: "0.1.5".into(),
            game: "1.0 server".into(),
            hosting: true,
            uptime: Duration::from_mins(123),
            last_crash: Some("fault 0xc0000005, blamed on you.trading-post: in its code".into()),
            at: 45_000,
            mods: mods(),
            disabled: vec!["you.my-mod".into()],
            others: vec!["native mods in mods/, without Enshwick's API: fps-unlock.dll".into()],
            replaced: vec![
                "you.stalls is replaced by you.market-stalls; delete mods/stalls.dll".into(),
            ],
            turned_off: vec![TurnedOff {
                id: "you.trading-post".into(),
                reason: "panic in an action".into(),
                at: 44_000,
            }],
            parts_off: Vec::new(),
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
            world_folders: Vec::new(),
            messages: messages_line(0, 3, 1),
            parts: vec![
                Part {
                    name: "weather",
                    named: 2,
                    missing: vec!["weather.set", "weather.get"],
                },
                Part {
                    name: "altars",
                    named: 3,
                    missing: vec!["altars.radius"],
                },
                Part {
                    name: "chat",
                    named: 1,
                    missing: Vec::new(),
                },
            ],
        }
    }

    #[test]
    fn the_summary_counts_mod_messages_lost_on_the_way() {
        let mut quiet = summary();
        quiet.messages = None;
        assert!(!quiet.render().contains("mod messages:"));
        let text = summary().render();
        assert!(text.contains(
            "\nmod messages: 3 lines for one player dropped (queue full, player gone or such lines off), \
             1 line for one player reached others, lines for one player off\n"
        ));
    }

    #[test]
    fn the_summary_names_other_worlds_of_the_save_name_with_mod_data() {
        let mut summary = summary();
        assert!(!summary.render().contains("other worlds"));
        summary.world_folders = vec!["world0-aaaaaaaa".into(), "world0-cccccccc".into()];
        assert!(summary.render().contains(
            "mod data of other worlds of this save name: world0-aaaaaaaa, world0-cccccccc;"
        ));
    }

    #[test]
    fn the_messages_line_names_only_what_happened() {
        assert_eq!(messages_line(0, 0, 0), None);
        assert_eq!(
            messages_line(2, 0, 0).as_deref(),
            Some("2 messages dropped (over a rate or unreadable)")
        );
        assert_eq!(
            messages_line(1, 300, 0).as_deref(),
            Some(
                "1 message dropped (over a rate or unreadable), \
                 300 lines for one player dropped (queue full, player gone or such lines off)"
            )
        );
        assert_eq!(
            messages_line(0, 0, 4).as_deref(),
            Some("4 lines for one player reached others, lines for one player off")
        );
    }

    #[test]
    fn the_summary_names_the_loader_features_this_build_lacks() {
        let text = summary().render();
        assert!(text.contains(
            "\nloader features: 1 of 3 work on this build\n  \
             weather off: missing weather.set, weather.get\n  \
             altars partly off: missing altars.radius\n"
        ));
        let mut whole = summary();
        whole.parts.clear();
        assert!(!whole.render().contains("loader features"));
        whole.parts.push(summary().parts.swap_remove(2));
        assert!(
            whole
                .render()
                .contains("\nloader features: all 1 work on this build\n")
        );
    }

    #[test]
    fn a_part_the_build_serves_whole_gives_no_line() {
        let parts = summary().parts;
        assert_eq!(
            part_line(&parts[0]).as_deref(),
            Some("weather off: missing weather.set, weather.get")
        );
        assert_eq!(part_line(&parts[2]), None);
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
             note: recommends you.lanterns, which is not loaded\n    \
             feature glow off: symbol lantern.glow: missing on this game build\n    \
             feature light on\n    3 zones merged\n"
        ));
        assert!(text.contains(
            "  you.trading-post 0.1.1 (off)\n  off in mods/enshwick/mods.toml: you.my-mod\n"
        ));
        assert!(text.contains(
            "  not started: you.stalls is replaced by you.market-stalls; delete mods/stalls.dll\n  \
             native mods in mods/, without Enshwick's API: fps-unlock.dll\n"
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
    #[cfg_attr(
        windows,
        ignore = "the snapshots are held on the host; under Wine their paths do not resolve"
    )]
    fn the_status_report_reads_as_its_snapshot() {
        insta::assert_snapshot!("status", summary().render());
        insta::assert_snapshot!("status_short", summary().short());
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
    fn a_part_off_keeps_its_first_reason_and_the_list_stays_bounded() {
        let mut off = Vec::new();
        keep_part(
            &mut off,
            "map markers",
            "the loader panicked in the marker pass: boom",
            1,
        );
        keep_part(&mut off, "map markers", "a later reason", 2);
        for i in 0..HISTORY + 2 {
            keep_part(&mut off, &format!("part {i}"), "off", 3);
        }
        assert_eq!(off.len(), HISTORY);
        let markers = off.iter().find(|p| p.part == "map markers");
        assert_eq!(
            markers.map(|p| p.reason.as_str()),
            Some("the loader panicked in the marker pass: boom")
        );
    }

    #[test]
    fn the_summary_lists_the_loader_s_parts_that_are_off() {
        let mut s = summary();
        s.parts_off = vec![PartOff {
            part: "map markers".into(),
            reason: "the loader panicked in the marker pass: boom".into(),
            at: 44_100,
        }];
        let text = s.render();
        assert!(
            text.contains(
                "loader parts off:\n  12:15:00Z map markers: the loader panicked in the marker pass: boom"
            ),
            "{text}"
        );
        assert!(
            s.short()
                .contains("map markers off: the loader panicked in the marker pass: boom"),
            "{}",
            s.short()
        );
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
