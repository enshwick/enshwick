//! Whether a newer `cargo enshwick` is out: asked of GitHub at most once a
//! day on a thread of its own, told at most once a day. A command never
//! waits for the answer: one that arrives after the command is done is
//! lost, and the next day asks again. Without a network nothing is said.

use std::path::PathBuf;
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};

use enshwick_gamedata::releases::{self, LIMIT, TOOL_TAGS};
use semver::Version;

/// Seconds between two questions, and between two notices.
const DAY: u64 = 24 * 60 * 60;
/// The longest the question takes before it is given up.
const SECONDS: u32 = 5;

/// What the last runs learned and said, kept in the user's cache folder.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct Seen {
    /// When GitHub was last asked, in seconds since the Unix epoch.
    #[serde(default)]
    checked: u64,
    /// When the newer release was last told.
    #[serde(default)]
    told: u64,
    /// The newest release GitHub named, when newer than the tool that asked.
    newer: Option<Newer>,
}

/// A newer release, as kept between runs.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct Newer {
    version: String,
    page: String,
}

/// A run's release check: what was seen before and, when due, the question
/// in flight.
#[derive(Debug)]
pub(crate) struct Check {
    seen: Seen,
    file: PathBuf,
    asking: Option<JoinHandle<Option<Option<Newer>>>>,
    /// `seen` as read, so an unchanged one is not written again.
    read: Seen,
}

/// Starts the run's check: reads what earlier runs saw and asks GitHub on a
/// thread when a day has passed; `None` without a cache folder.
pub(crate) fn start() -> Option<Check> {
    let file = cache_file()?;
    let mut seen: Seen = std::fs::read_to_string(&file)
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default();
    let read = seen.clone();
    let now = now();
    let asking = a_day_after(seen.checked, now).then(|| {
        seen.checked = now;
        std::thread::spawn(ask)
    });
    Some(Check {
        seen,
        file,
        asking,
        read,
    })
}

pub(crate) fn finish(check: Check) -> Option<String> {
    let Check {
        seen,
        file,
        asking,
        read,
    } = check;
    let answer = asking
        .filter(JoinHandle::is_finished)
        .and_then(|asking| asking.join().ok().flatten());
    let (seen, notice) = settle(seen, now(), answer, &running());
    if seen != read
        && let (Some(dir), Ok(text)) = (file.parent(), toml::to_string(&seen))
    {
        // A cache that cannot be written only asks again on the next run.
        let _kept = std::fs::create_dir_all(dir).and_then(|()| std::fs::write(&file, text));
    }
    notice
}

/// Whether a day has passed from `then` to `now`; a clock set back before
/// `then` counts as one.
fn a_day_after(then: u64, now: u64) -> bool {
    now.checked_sub(then).is_none_or(|since| since >= DAY)
}

/// `seen` with `answer`, the newer release GitHub named or `Some(None)` for
/// none, and the notice of a newer release than `running` when a day has
/// passed since the last one.
fn settle(
    mut seen: Seen,
    now: u64,
    answer: Option<Option<Newer>>,
    running: &Version,
) -> (Seen, Option<String>) {
    if let Some(answer) = answer {
        seen.newer = answer;
    }
    let newer = seen.newer.clone().filter(|newer| {
        Version::parse(&newer.version).is_ok_and(|v| v.cmp_precedence(running).is_gt())
    });
    let notice = newer
        .filter(|_| a_day_after(seen.told, now))
        .map(|newer| {
            seen.told = now;
            format!(
                "cargo-enshwick {} is out, this is {running}: {}",
                newer.version, newer.page
            )
        });
    (seen, notice)
}

/// GitHub's newest release of the tool newer than the running one, `None`
/// for none and `Some` of it when GitHub answered at all.
fn ask() -> Option<Option<Newer>> {
    let running = running();
    let url = format!("https://api.github.com{}", releases::path(&running));
    let answer = crate::net::get(
        &url,
        &[
            "Accept: application/vnd.github+json",
            "X-GitHub-Api-Version: 2022-11-28",
        ],
        LIMIT,
        SECONDS,
    )
    .ok()?;
    let found = releases::newer(&answer, &running, TOOL_TAGS).ok()?;
    Some(found.map(|release| Newer {
        page: release.page(),
        version: release.version.to_string(),
    }))
}

/// The running tool's version.
fn running() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).unwrap_or_else(|_| Version::new(0, 0, 0))
}

/// Seconds since the Unix epoch; 0 on a clock set before it.
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// Where the check keeps what it saw: the user's local application data on
/// Windows, the XDG cache folder elsewhere.
fn cache_file() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("XDG_CACHE_HOME"))
        .map(PathBuf::from)
        .filter(|base| base.is_absolute())
        .or_else(|| std::env::home_dir().map(|home| home.join(".cache")))?;
    Some(base.join("enshwick").join("cargo-enshwick.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000;

    fn newer(version: &str) -> Newer {
        Newer {
            version: version.into(),
            page: format!("https://github.com/enshwick/enshwick/releases/tag/v{version}"),
        }
    }

    fn running() -> Version {
        Version::parse("1.0.0-alpha.14").unwrap()
    }

    #[test]
    fn github_is_asked_at_most_once_a_day() {
        assert!(a_day_after(0, NOW));
        assert!(!a_day_after(NOW - DAY + 1, NOW));
        assert!(a_day_after(NOW - DAY, NOW));
        assert!(a_day_after(NOW + 60, NOW), "a clock set back asks again");
    }

    #[test]
    fn a_newer_release_is_told_once_a_day() {
        let found = Some(Some(newer("1.0.0-alpha.15")));
        let (seen, notice) = settle(Seen::default(), NOW, found, &running());
        assert_eq!(
            notice.as_deref(),
            Some(
                "cargo-enshwick 1.0.0-alpha.15 is out, this is 1.0.0-alpha.14: https://github.com/enshwick/enshwick/releases/tag/v1.0.0-alpha.15"
            )
        );
        assert_eq!(seen.told, NOW);
        let (seen, again) = settle(seen, NOW + 60, None, &running());
        assert_eq!(again, None);
        let (_, next_day) = settle(seen, NOW + DAY, None, &running());
        assert!(next_day.is_some());
    }

    #[test]
    fn no_answer_keeps_what_earlier_runs_learned() {
        let seen = Seen {
            newer: Some(newer("1.0.0-alpha.15")),
            ..Seen::default()
        };
        let (seen, notice) = settle(seen, NOW, None, &running());
        assert!(notice.is_some());
        assert_eq!(seen.newer, Some(newer("1.0.0-alpha.15")));
    }

    #[test]
    fn an_answer_of_none_newer_forgets_the_old_one() {
        let seen = Seen {
            newer: Some(newer("1.0.0-alpha.15")),
            ..Seen::default()
        };
        let (seen, notice) = settle(seen, NOW, Some(None), &running());
        assert_eq!((seen.newer, notice), (None, None));
    }

    #[test]
    fn a_kept_release_the_tool_caught_up_with_is_not_told() {
        let seen = Seen {
            newer: Some(newer("1.0.0-alpha.14")),
            ..Seen::default()
        };
        assert_eq!(settle(seen, NOW, None, &running()).1, None);
    }

    #[test]
    fn what_was_seen_reads_back_from_its_file() {
        let seen = Seen {
            checked: NOW,
            told: NOW - 5,
            newer: Some(newer("1.0.0-alpha.15")),
        };
        let text = toml::to_string(&seen).unwrap();
        assert_eq!(toml::from_str::<Seen>(&text).unwrap(), seen);
        assert_eq!(toml::from_str::<Seen>("").unwrap(), Seen::default());
    }
}
