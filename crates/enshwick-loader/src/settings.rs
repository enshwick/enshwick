//! The loader's own settings, `mod-configs/enshwick.toml`.

use std::collections::BTreeMap;
use std::path::Path;

use enshwick_api::abi::Level;
use enshwick_api::config::Config as _;

/// The id the loader logs and keeps settings under; no mod may take it.
pub const LOADER_ID: &str = "enshwick";

/// Enshwick loader settings.
#[derive(
    Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize, enshwick_api::Config,
)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// The most detailed lines kept in `logs/enshwick`: "error", "warn", "info" or "debug".
    pub log_level: Level,
    /// The same per mod id, over `log_level`; "enshwick" is the loader itself.
    /// For example: `log_levels = { survivors-per-altar = "debug" }`
    pub log_levels: BTreeMap<String, Level>,
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

    /// Reads `path`, writing the commented defaults there first when it is
    /// missing; the defaults and a reason for the log when it cannot be read.
    #[must_use]
    pub fn read(path: &Path) -> (Self, Option<String>) {
        match std::fs::read_to_string(path) {
            Ok(text) => match toml::from_str(&text) {
                Ok(settings) => (settings, None),
                Err(e) => (
                    Self::default(),
                    Some(format!("{}: {}", path.display(), e.message())),
                ),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let written = std::fs::write(path, Self::template());
                (
                    Self::default(),
                    written.err().map(|e| format!("{}: {e}", path.display())),
                )
            }
            Err(e) => (Self::default(), Some(format!("{}: {e}", path.display()))),
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
    }

    #[test]
    fn a_mod_level_overrides_the_default() {
        let settings: Settings = toml::from_str(
            "log_level = \"warn\"\nlog_levels = { survivors-per-altar = \"debug\" }\n",
        )
        .unwrap();
        assert_eq!(settings.level("survivors-per-altar"), Level::DEBUG);
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
        let path = dir.join("enshwick.toml");
        let _fresh = std::fs::remove_file(&path);
        assert_eq!(Settings::read(&path), (Settings::default(), None));
        assert!(path.exists());
        assert_eq!(Settings::read(&path), (Settings::default(), None));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
