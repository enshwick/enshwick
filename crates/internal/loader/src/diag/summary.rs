//! The status summary: what it shows of mods, hooks and content, and its text.

use std::fmt::Write as _;
use std::time::Duration;

use enshwick_gamedata::{Part, PartState};

use super::{PartOff, Patches, SAMPLE_EVERY, TurnedOff};

/// Hooked functions the summary lists, busiest first.
const HOOK_LINES: usize = 40;

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
    /// What was wrong in the loader settings file at start, a line each.
    pub settings: Vec<String>,
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
        self.render_settings(&mut out);
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

    /// What was wrong in the loader settings file, under its own heading.
    fn render_settings(&self, out: &mut String) {
        if !self.settings.is_empty() {
            let _w = writeln!(
                out,
                "\nloader settings file has problems; the protections it does not read stay closed:"
            );
            for line in &self.settings {
                let _w = writeln!(out, "  {line}");
            }
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
        lines.extend(
            self.settings
                .iter()
                .map(|problem| crate::texts::fill("status.settings", &[("problem", problem)])),
        );
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
pub(super) fn count(n: u64) -> String {
    match n {
        0..1000 => n.to_string(),
        1000..1_000_000 => format!("{:.1}k", n as f64 / 1e3),
        1_000_000..1_000_000_000 => format!("{:.1}M", n as f64 / 1e6),
        _ => format!("{:.1}G", n as f64 / 1e9),
    }
}

/// `850 ns`, `12.4 us`, `3.1 ms`.
pub(super) fn nanos(ns: f64) -> String {
    if ns < 1e3 {
        format!("{ns:.0} ns")
    } else if ns < 1e6 {
        format!("{:.1} us", ns / 1e3)
    } else {
        format!("{:.1} ms", ns / 1e6)
    }
}
