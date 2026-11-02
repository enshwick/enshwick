//! Diagnostics: what the loader keeps for its status summary, and the
//! summary's text.
//!
//! Mods turned off and why, the names hooked functions were found under and
//! the code patches written are kept as they happen, all of it off the game's
//! hot paths. Hook call counters live beside the hook thunks (see
//! `patch.rs`) and exist only with `diagnostics = true`; without it a hooked
//! call runs exactly the instructions it ran before.

mod menu;
mod summary;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

pub use menu::{KeptLine, MenuNote, menu_line, menu_line_off};
pub(crate) use summary::clock;
pub use summary::{HookStat, ModStatus, Served, Summary, duration, messages_line, part_line};

use crate::locks::Relock;
use crate::settings::Settings;

/// With diagnostics timing on, one call in this many of each hooked
/// function is timed. A power of two: the thunk tests the counter's low byte.
pub const SAMPLE_EVERY: u64 = 256;
/// Mods turned off that the summary remembers; older ones are dropped.
const HISTORY: usize = 64;

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use enshwick_gamedata::Part;

    use super::summary::{count, nanos};

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
        assert_eq!(menu_line("1.0.0", 6, 0, &[]), "Enshwick 1.0.0 · mods: 6");
        assert_eq!(
            menu_line("1.0.0", 6, 2, &[]),
            "Enshwick 1.0.0 · mods: 6, off: 2"
        );
        assert_eq!(
            menu_line("1.0.0", 0, 0, &[MenuNote::Lacking]),
            "Enshwick 1.0.0 · mods: 0 · some features are off on this game version"
        );
    }

    #[test]
    fn the_menu_line_says_developer_tools_are_on_last() {
        assert_eq!(
            menu_line("1.0.0", 2, 0, &[MenuNote::Developer, MenuNote::BadSettings]),
            "Enshwick 1.0.0 · mods: 2 · enshwick.toml has errors, see the log · DEVELOPER TOOLS ON"
        );
    }

    #[test]
    fn the_menu_line_turns_off_alone_without_its_site_or_its_look() {
        assert_eq!(menu_line_off(true, true, true), None);
        let no_site = menu_line_off(false, false, true).unwrap();
        assert!(no_site.contains("has no ui.legal_line_site"), "{no_site}");
        let moved = menu_line_off(false, true, true).unwrap();
        assert!(moved.contains("does not match"), "{moved}");
        let no_look = menu_line_off(true, true, false).unwrap();
        assert!(no_look.contains("legalText"), "{no_look}");
        assert_eq!(menu_line_off(false, false, false), Some(no_site));
    }

    #[test]
    fn the_menu_line_is_made_again_only_when_the_count_off_changes() {
        let mut kept = KeptLine::new();
        let mut made = 0;
        let mut get = |kept: &mut KeptLine, off: usize| {
            kept.get(off, || {
                made += 1;
                format!("off: {off}")
            })
            .to_owned()
        };
        assert_eq!(get(&mut kept, 0), "off: 0");
        assert_eq!(get(&mut kept, 0), "off: 0");
        assert_eq!(get(&mut kept, 2), "off: 2");
        assert_eq!(get(&mut kept, 2), "off: 2");
        assert_eq!(get(&mut kept, 0), "off: 0");
        drop(get);
        assert_eq!(made, 3);
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
            settings: Vec::new(),
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
    fn a_broken_settings_file_shows_in_the_summary_and_the_menu_line() {
        let mut summary = summary();
        summary.settings = vec!["enshwick.toml: allowlist: invalid type".into()];
        assert!(
            summary
                .render()
                .contains("enshwick.toml: allowlist: invalid type")
        );
        assert!(
            summary
                .short()
                .contains("enshwick.toml: allowlist: invalid type")
        );
        let good = menu_line("0.1.5", 2, 0, &[]);
        let broken = menu_line("0.1.5", 2, 0, &[MenuNote::BadSettings]);
        assert!(
            broken.starts_with(&good) && broken.len() > good.len(),
            "{broken}"
        );
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
