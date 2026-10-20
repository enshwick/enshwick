//! What the server runs on, reported once at start: Wine or Windows, the
//! fast synchronisation Wine may use, the CPU governor and the container's
//! CPU limit, and the CPUs the game saw.
//!
//! Under Wine the host's files are on drive `Z:`, so `/sys` and `/dev` read
//! as `Z:\sys` and `Z:\dev`. Nothing here changes anything.

use std::fmt::Write as _;
use std::path::Path;

use crate::sys;

/// Environment variables that turn Wine's fast synchronisation on or off,
/// reported as found.
const SYNC_VARIABLES: [&str; 7] = [
    "WINEESYNC",
    "WINEFSYNC",
    "WINENTSYNC",
    "PROTON_NO_ESYNC",
    "PROTON_NO_FSYNC",
    "PROTON_USE_NTSYNC",
    "PROTON_NO_NTSYNC",
];
/// Environment variables Proton sets; their presence is reported, never
/// acted on.
const PROTON_VARIABLES: [&str; 2] = ["STEAM_COMPAT_DATA_PATH", "PROTON_VERSION"];

/// A container's CPU limit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum CpuLimit {
    /// No quota.
    Unlimited,
    /// A quota worth this many CPUs.
    Cpus(f64),
}

/// What the game logged about the CPUs at start.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GameCpus {
    /// The logical CPUs the engine counted (`LCpus:`).
    pub(crate) logical: Option<u32>,
    /// The task worker groups, such as `Default 10, Background 8`.
    pub(crate) workers: Vec<(String, u32)>,
}

/// The environment, as read at start.
#[derive(Clone, Debug, Default)]
pub(crate) struct Environment {
    /// The Wine version; `None` on Windows.
    pub(crate) wine: Option<String>,
    /// The host system and release under Wine.
    pub(crate) host: Option<(String, String)>,
    /// Whether Proton's variables are set.
    pub(crate) proton: bool,
    /// Whether `/dev/ntsync` exists.
    pub(crate) ntsync_device: bool,
    /// The synchronisation variables that are set, with their values.
    pub(crate) sync_variables: Vec<(String, String)>,
    /// The CPU governor of CPU 0.
    pub(crate) governor: Option<String>,
    /// The cgroup CPU limit.
    pub(crate) cpu_limit: Option<CpuLimit>,
    /// The logical CPUs Windows or Wine reports.
    pub(crate) logical_cpus: u32,
    /// What the game logged about its CPUs.
    pub(crate) game: GameCpus,
}

/// Parses cgroup v2 `cpu.max`: `max 100000` or `400000 100000`.
#[must_use]
pub(crate) fn parse_cpu_max(text: &str) -> Option<CpuLimit> {
    let mut words = text.split_ascii_whitespace();
    let quota = words.next()?;
    let period = words
        .next()
        .map_or(Some(100_000.0), |p| p.parse::<f64>().ok())?;
    if quota == "max" {
        return Some(CpuLimit::Unlimited);
    }
    cpus(quota.parse().ok()?, period)
}

/// Parses cgroup v1 `cpu.cfs_quota_us` and `cpu.cfs_period_us`; a quota of
/// -1 is no limit.
#[must_use]
pub(crate) fn parse_cfs(quota: &str, period: &str) -> Option<CpuLimit> {
    let quota: f64 = quota.trim().parse().ok()?;
    if quota < 0.0 {
        return Some(CpuLimit::Unlimited);
    }
    cpus(quota, period.trim().parse().ok()?)
}

fn cpus(quota: f64, period: f64) -> Option<CpuLimit> {
    (quota > 0.0 && period > 0.0).then(|| CpuLimit::Cpus(quota / period))
}

/// Reads what the game logged about its CPUs: `LCpus: 32` and each
/// `Group 'Default'  Worker:10` line.
#[must_use]
pub(crate) fn parse_game_log(text: &str) -> GameCpus {
    let mut game = GameCpus::default();
    for line in text.lines() {
        if game.logical.is_none()
            && let Some(rest) = line.split("LCpus:").nth(1)
        {
            game.logical = rest.trim().parse().ok();
        }
        if let Some(rest) = line.split("Group '").nth(1)
            && let Some((name, rest)) = rest.split_once('\'')
            && let Some(count) = rest.split("Worker:").nth(1)
            && let Some(count) = count.split_ascii_whitespace().next()
            && let Ok(count) = count.parse()
            && !game.workers.iter().any(|(known, _)| known == name)
        {
            game.workers.push((name.to_owned(), count));
        }
    }
    game
}

/// The first line of a small host file, trimmed.
fn first_line(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let line = text.lines().next()?.trim();
    (!line.is_empty()).then(|| line.to_owned())
}

impl Environment {
    /// Reads the environment; `game_log` is the text the game logged so far.
    #[must_use]
    pub(crate) fn probe(game_log: &str) -> Self {
        let wine = sys::wine_version();
        let host = sys::wine_host();
        let linux = host.as_ref().is_some_and(|(name, _)| name == "Linux");
        let mut env = Self {
            proton: PROTON_VARIABLES
                .iter()
                .any(|name| std::env::var_os(name).is_some()),
            sync_variables: SYNC_VARIABLES
                .iter()
                .filter_map(|&name| Some((name.to_owned(), std::env::var(name).ok()?)))
                .collect(),
            logical_cpus: sys::logical_cpus(),
            game: parse_game_log(game_log),
            ..Self::default()
        };
        if linux {
            let root = Path::new(r"Z:\");
            env.ntsync_device = root.join(r"dev\ntsync").exists();
            env.governor =
                first_line(&root.join(r"sys\devices\system\cpu\cpu0\cpufreq\scaling_governor"));
            env.cpu_limit = first_line(&root.join(r"sys\fs\cgroup\cpu.max"))
                .and_then(|text| parse_cpu_max(&text))
                .or_else(|| {
                    let v1 = root.join(r"sys\fs\cgroup\cpu");
                    parse_cfs(
                        &first_line(&v1.join("cpu.cfs_quota_us"))?,
                        &first_line(&v1.join("cpu.cfs_period_us"))?,
                    )
                });
        }
        env.wine = wine;
        env.host = host;
        env
    }

    /// Whether the process runs under Wine, Proton included.
    #[must_use]
    pub(crate) fn is_wine(&self) -> bool {
        self.wine.is_some()
    }

    /// The report, one line.
    #[must_use]
    pub(crate) fn summary(&self) -> String {
        let mut line = match (&self.wine, &self.host) {
            (Some(version), Some((name, release))) => format!("Wine {version} on {name} {release}"),
            (Some(version), None) => format!("Wine {version}"),
            (None, _) => "Windows, not Wine".to_owned(),
        };
        if self.is_wine() {
            let _ = write!(
                line,
                ", Proton variables {}",
                if self.proton { "set" } else { "not set" }
            );
            let mut sync: Vec<String> = Vec::new();
            if self.ntsync_device {
                sync.push("/dev/ntsync present".to_owned());
            }
            sync.extend(self.sync_variables.iter().map(|(k, v)| format!("{k}={v}")));
            let sync = if sync.is_empty() {
                "no ntsync device, no esync or fsync variables".to_owned()
            } else {
                sync.join(", ")
            };
            let _ = write!(line, "; sync: {sync}");
            let _ = write!(
                line,
                "; governor: {}",
                self.governor.as_deref().unwrap_or("unknown")
            );
            let limit = match self.cpu_limit {
                Some(CpuLimit::Unlimited) => "none".to_owned(),
                Some(CpuLimit::Cpus(cpus)) => format!("{cpus:.2} CPUs"),
                None => "unknown".to_owned(),
            };
            let _ = write!(line, "; cgroup CPU limit: {limit}");
        }
        let _ = write!(line, "; logical CPUs: {}", self.logical_cpus);
        if let Some(logical) = self.game.logical {
            let _ = write!(line, ", the game saw {logical}");
        }
        if !self.game.workers.is_empty() {
            let workers: Vec<String> = self
                .game
                .workers
                .iter()
                .map(|(name, count)| format!("{name} {count}"))
                .collect();
            let _ = write!(line, " (task workers: {})", workers.join(", "));
        }
        line
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cgroup_v2_limits_parse() {
        assert_eq!(parse_cpu_max("max 100000"), Some(CpuLimit::Unlimited));
        assert_eq!(parse_cpu_max("400000 100000\n"), Some(CpuLimit::Cpus(4.0)));
        assert_eq!(parse_cpu_max("150000 100000"), Some(CpuLimit::Cpus(1.5)));
        assert_eq!(parse_cpu_max("200000"), Some(CpuLimit::Cpus(2.0)));
        assert_eq!(parse_cpu_max(""), None);
        assert_eq!(parse_cpu_max("lots 100000"), None);
        assert_eq!(parse_cpu_max("100 0"), None);
    }

    #[test]
    fn cgroup_v1_limits_parse() {
        assert_eq!(parse_cfs("-1", "100000"), Some(CpuLimit::Unlimited));
        assert_eq!(parse_cfs("200000\n", "100000\n"), Some(CpuLimit::Cpus(2.0)));
        assert_eq!(parse_cfs("x", "100000"), None);
    }

    #[test]
    fn the_game_log_gives_cpus_and_workers() {
        let log = "\
[I 00:00:00,006]     LCpus:      32
[I 00:00:00,006] [task]   mainWorkerCount: 2
[I 00:00:00,006] [task]   Group 'Default'  Worker:10  Main:2
[I 00:00:00,006] [task]      0:  Task-Default00     00000000
[I 00:00:00,006] [task]   Group 'Background'  Worker:8
[I 00:00:00,006] [task]   Group 'Default'  Worker:3
";
        let game = parse_game_log(log);
        assert_eq!(game.logical, Some(32));
        assert_eq!(
            game.workers,
            [("Default".to_owned(), 10), ("Background".to_owned(), 8)]
        );
        assert_eq!(parse_game_log("nothing here"), GameCpus::default());
    }

    #[test]
    fn the_summary_names_what_was_found() {
        let windows = Environment {
            logical_cpus: 32,
            game: GameCpus {
                logical: Some(32),
                workers: vec![("Default".into(), 10)],
            },
            ..Environment::default()
        };
        assert_eq!(
            windows.summary(),
            "Windows, not Wine; logical CPUs: 32, the game saw 32 (task workers: Default 10)"
        );
        let wine = Environment {
            wine: Some("10.16".into()),
            host: Some(("Linux".into(), "6.12.1".into())),
            ntsync_device: true,
            sync_variables: vec![("WINEFSYNC".into(), "1".into())],
            governor: Some("performance".into()),
            cpu_limit: Some(CpuLimit::Cpus(4.0)),
            logical_cpus: 32,
            ..Environment::default()
        };
        assert!(wine.is_wine());
        assert_eq!(
            wine.summary(),
            "Wine 10.16 on Linux 6.12.1, Proton variables not set; sync: /dev/ntsync present, \
             WINEFSYNC=1; governor: performance; cgroup CPU limit: 4.00 CPUs; logical CPUs: 32"
        );
    }
}
