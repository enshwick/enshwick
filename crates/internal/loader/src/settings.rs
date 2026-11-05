//! The loader's installation settings, `enshwick.toml` in the game's folder.

use std::collections::BTreeMap;
use std::path::Path;

use enshwick::abi::Level;
use enshwick::config::Config as _;

/// The id the loader logs and keeps settings under; no mod may take it.
pub use enshwick::mods::LOADER_ID;

/// The settings file's name in the game's folder.
pub const FILE: &str = "enshwick.toml";

/// Enshwick loader settings, read once at start.
#[enshwick::config]
#[derive(Debug, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each is a key of the settings file a person turns on or off"
)]
pub struct Settings {
    /// The most detailed lines kept in `logs/`: "error", "warn", "info" or "debug".
    pub log_level: Level,
    /// The same per mod id, over `log_level`; "enshwick" is the loader itself.
    ///
    /// For example, `log_levels = { "you.my-mod" = "debug" }`.
    pub log_levels: BTreeMap<String, Level>,
    /// Count the calls of every hooked game function for the status summary.
    /// A few instructions per call.
    pub diagnostics: bool,
    /// With diagnostics, also time every 256th call of each hooked function.
    pub diagnostics_timing: bool,
    /// On a game build without a build table yet, find the table's entries
    /// by their code patterns and run what they cover; false leaves such a
    /// build vanilla until its table arrives.
    pub build_patterns: bool,
    /// Ask GitHub once per start whether a newer Enshwick release exists, so
    /// the Mods screen can say so; a dedicated server only logs it. false
    /// never asks.
    pub release_check: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            log_level: Level::default(),
            log_levels: BTreeMap::new(),
            diagnostics: false,
            diagnostics_timing: false,
            build_patterns: true,
            release_check: true,
        }
    }
}

impl Settings {
    /// The most detailed level kept for lines from `source`.
    #[must_use]
    pub fn level(&self, source: &str) -> Level {
        self.log_levels
            .get(source)
            .copied()
            .unwrap_or(self.log_level)
    }

    /// The file written on first start: the template with every key
    /// commented out, so the file holds no value until someone sets one.
    #[must_use]
    pub fn commented() -> String {
        Self::template()
            .lines()
            .map(|line| {
                if line.is_empty() || line.starts_with('#') {
                    format!("{line}\n")
                } else {
                    format!("# {line}\n")
                }
            })
            .collect()
    }

    /// Reads `path`, writing the commented defaults there first when it is
    /// missing, with its problems for the log.
    #[must_use]
    pub fn read(path: &Path) -> (Self, Problems) {
        let (settings, mut problems) = match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (
                Self::default(),
                Problems {
                    lines: enshwick::files::write_atomically(path, Self::commented())
                        .err()
                        .map(|e| format!("the defaults could not be written there: {e}"))
                        .into_iter()
                        .collect(),
                    unread: false,
                },
            ),
            Err(e) => (Self::default(), Problems::unread(e.to_string())),
        };
        for line in &mut problems.lines {
            *line = format!("{}: {line}", path.display());
        }
        (settings, problems)
    }

    /// Parses the settings file's `text` key by key, with its problems:
    /// every valid key applies, every bad one is named and keeps its
    /// default.
    #[must_use]
    pub fn parse(text: &str) -> (Self, Problems) {
        parse_keys(text)
    }
}

/// Enshwick's rules for this world, read as the world loads.
///
/// The file is `enshwick.toml` in the loader's folder of the world's
/// settings, generated like a mod's.
#[enshwick::config(file = "enshwick")]
#[derive(Debug, PartialEq, Eq)]
pub struct WorldRules {
    /// On a server that runs mods of side "both", or in a game this client
    /// hosts, disconnect a player whose game does not run them all, in the
    /// server's versions, or runs without Enshwick, after a chat line naming
    /// what it needs; administrators too, never the host. false lets such
    /// players into this world.
    pub require_enshwick: bool,
}

impl Default for WorldRules {
    fn default() -> Self {
        Self {
            require_enshwick: true,
        }
    }
}

impl WorldRules {
    /// Parses a world's rules file `text` key by key, with its problems: a
    /// key whose value does not read keeps its default.
    #[must_use]
    pub fn parse(text: &str) -> (Self, Problems) {
        parse_keys(text)
    }
}

/// `text` read key by key into `T`: every valid key applies, every bad one
/// is named and keeps its default; text that is no TOML reads as the
/// defaults.
fn parse_keys<T: serde::de::DeserializeOwned + Default>(text: &str) -> (T, Problems) {
    let table: toml::Table = match toml::from_str(text) {
        Ok(table) => table,
        Err(e) => {
            let at = match e.span() {
                Some(span) => {
                    let before = text.get(..span.start).unwrap_or(text);
                    let line = before.matches('\n').count() + 1;
                    let column = before.rsplit('\n').next().unwrap_or(before).chars().count() + 1;
                    format!("line {line}, column {column}: ")
                }
                None => String::new(),
            };
            return (
                T::default(),
                Problems::unread(format!("{at}{}", e.message())),
            );
        }
    };
    let mut good = toml::Table::new();
    let mut lines = Vec::new();
    for (key, value) in table {
        let alone = toml::Table::from_iter([(key.clone(), value.clone())]);
        match alone.try_into::<T>() {
            Ok(_) => {
                good.insert(key, value);
            }
            Err(e) => lines.push(format!("{key}: {}", e.message())),
        }
    }
    let read = good.try_into::<T>().unwrap_or_else(|e| {
        lines.push(format!("the keys do not read together: {}", e.message()));
        T::default()
    });
    (
        read,
        Problems {
            lines,
            unread: false,
        },
    )
}

/// What did not read in the settings file.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Problems {
    /// A line per problem for the log.
    pub lines: Vec<String>,
    /// The file did not read or parse at all, so the defaults apply instead
    /// of the rest of the file.
    pub unread: bool,
}

impl Problems {
    /// The file as a whole did not read, for `why`.
    fn unread(why: String) -> Self {
        Self {
            lines: vec![why],
            unread: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_template_parses_to_the_defaults() {
        let settings: Settings = toml::from_str(&Settings::template()).unwrap();
        assert_eq!(settings, Settings::default());
        assert_eq!(settings.level("any-mod"), Level::INFO);
        assert!(!settings.diagnostics);
        assert!(settings.build_patterns);
        assert!(settings.release_check);
    }

    #[test]
    fn the_release_check_is_turned_off_in_the_file() {
        let (settings, problems) = Settings::parse("release_check = false\n");
        assert!(!settings.release_check);
        assert!(problems.lines.is_empty(), "{problems:?}");
    }

    #[test]
    fn the_written_file_holds_every_key_commented_out() {
        let file = Settings::commented();
        assert_eq!(
            Settings::parse(&file),
            (Settings::default(), Problems::default())
        );
        assert!(toml::from_str::<toml::Table>(&file).unwrap().is_empty());
        for key in [
            "log_level",
            "log_levels",
            "diagnostics",
            "diagnostics_timing",
            "build_patterns",
            "release_check",
        ] {
            assert!(file.contains(&format!("# {key} = ")), "{key}:\n{file}");
        }
    }

    #[test]
    fn diagnostics_are_turned_on_in_the_file() {
        let settings: Settings = toml::from_str("diagnostics = true\n").unwrap();
        assert!(settings.diagnostics);
        assert!(!settings.diagnostics_timing);
    }

    #[test]
    fn the_log_levels_example_in_the_template_names_a_full_mod_id() {
        let template = Settings::template();
        let example = template
            .lines()
            .find_map(|line| line.split_once("For example, `"))
            .and_then(|(_, rest)| rest.strip_suffix("`."))
            .unwrap();
        let settings: Settings = toml::from_str(example).unwrap();
        let ids: Vec<&String> = settings.log_levels.keys().collect();
        assert_eq!(ids.len(), 1);
        assert!(enshwick::abi::is_valid_id(ids[0]), "{ids:?}");
    }

    #[test]
    fn a_mod_level_overrides_the_default() {
        let settings: Settings =
            toml::from_str("log_level = \"warn\"\nlog_levels = { \"you.my-mod\" = \"debug\" }\n")
                .unwrap();
        assert_eq!(settings.level("you.my-mod"), Level::DEBUG);
        assert_eq!(settings.level(LOADER_ID), Level::WARN);
    }

    #[test]
    fn unknown_levels_and_keys_are_errors() {
        assert!(toml::from_str::<Settings>("log_level = \"loud\"\n").is_err());
        assert!(toml::from_str::<Settings>("verbose = true\n").is_err());
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri isolates the file system")]
    fn a_missing_file_is_written_and_read_back() {
        let dir = std::env::temp_dir().join(format!("enshwick-settings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE);
        let _fresh = std::fs::remove_file(&path);
        assert_eq!(
            Settings::read(&path),
            (Settings::default(), Problems::default())
        );
        assert!(path.exists());
        assert_eq!(
            Settings::read(&path),
            (Settings::default(), Problems::default())
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_bad_value_keeps_the_valid_keys_and_names_the_bad_one() {
        let text = "log_level = \"warn\"\ndiagnostics = \"yes\"\nbuild_patterns = false\n";
        let (settings, problems) = Settings::parse(text);
        assert_eq!(settings.log_level, Level::WARN);
        assert!(!settings.diagnostics);
        assert!(!settings.build_patterns);
        assert_eq!(problems.lines.len(), 1, "{problems:?}");
        assert!(
            problems.lines[0].starts_with("diagnostics: "),
            "{problems:?}"
        );
    }

    #[test]
    fn a_removed_key_is_named_and_the_rest_applies() {
        let (settings, problems) = Settings::parse("motd = \"hi\"\nlog_level = \"debug\"\n");
        assert_eq!(settings.log_level, Level::DEBUG);
        assert!(!problems.unread);
        assert!(problems.lines[0].starts_with("motd: "), "{problems:?}");
    }

    #[test]
    fn a_file_that_does_not_parse_names_where_and_reads_as_the_defaults() {
        let (settings, problems) = Settings::parse("log_level = \"debug\"\n[[[\n");
        assert_eq!(settings, Settings::default());
        assert!(problems.unread);
        assert!(
            problems.lines[0].starts_with("line 2, column "),
            "{problems:?}"
        );
    }

    #[test]
    fn a_world_requires_enshwick_unless_its_file_says_false() {
        use enshwick::config::Config as _;

        assert_eq!(WorldRules::FILE, "enshwick");
        let template = WorldRules::template();
        assert!(template.contains("require_enshwick = true"), "{template}");
        assert_eq!(
            WorldRules::parse(&template),
            (WorldRules::default(), Problems::default())
        );
        assert!(WorldRules::default().require_enshwick);
        let (rules, problems) = WorldRules::parse("require_enshwick = false\n");
        assert!(!rules.require_enshwick);
        assert!(problems.lines.is_empty());
        let (rules, problems) = WorldRules::parse("require_enshwick = \"no\"\n");
        assert!(rules.require_enshwick, "a bad value keeps the default");
        assert!(
            problems.lines[0].starts_with("require_enshwick: "),
            "{problems:?}"
        );
        assert!(WorldRules::parse("").0.require_enshwick);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri isolates the file system")]
    fn a_file_that_cannot_be_read_reads_as_the_defaults() {
        let dir =
            std::env::temp_dir().join(format!("enshwick-settings-dir-{}", std::process::id()));
        let path = dir.join(FILE);
        std::fs::create_dir_all(&path).unwrap();
        let (settings, problems) = Settings::read(&path);
        assert_eq!(settings, Settings::default());
        assert!(problems.unread);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
