//! The loader's own settings, `mod-configs/enshwick.toml`.

use std::collections::BTreeMap;
use std::path::Path;

use enshwick_loader_api::abi::Level;
use enshwick_loader_api::config::Config as _;

/// The id the loader logs and keeps settings under; no mod may take it.
pub use enshwick_loader_api::mods::LOADER_ID;

/// Minutes between status summaries by default.
pub const STATUS_MINUTES: u32 = 10;

/// Enshwick loader settings.
#[derive(
    Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config,
)]
#[serde(default, deny_unknown_fields)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each is a switch of its own in the settings file"
)]
pub struct Settings {
    /// The most detailed lines kept in `logs/enshwick`: "error", "warn", "info" or "debug".
    pub log_level: Level,
    /// The same per mod id, over `log_level`; "enshwick" is the loader itself.
    /// For example: `log_levels = { survivors-per-altar = "debug" }`
    pub log_levels: BTreeMap<String, Level>,
    /// Count the calls of every hooked game function for the status summary.
    /// A few instructions per call; read at start.
    pub diagnostics: bool,
    /// With diagnostics, also time every 256th call of each hooked function.
    /// Read at start.
    pub diagnostics_timing: bool,
    /// Minutes between status summaries in `logs/enshwick/status.log`;
    /// 0 writes one only for `/enshwick status`.
    pub status_minutes: u32,
    /// Only log what mods' migrations would change, without changing their
    /// files or recording them in mod-data/enshwick/migrations.toml.
    pub migrations_dry_run: bool,
    /// On a game build without a build table yet, find the table's entries
    /// by their code patterns and run what they cover; false leaves such a
    /// build vanilla until its table arrives. Read at start.
    pub build_patterns: bool,
    /// Watch the mod DLLs in mods/ and reload one that changes, when the mod
    /// allows it (`reload = true` in its manifest). For developing mods, on
    /// the dedicated server only. Read at start.
    pub dev_reload: bool,
    /// Refuse knowledge a client asks the server for in its input that a
    /// vanilla client never asks for: ids of the world's, counters, flag
    /// bits other than the seen bit, ids that give experience or skill
    /// points, and quests. A vanilla server grants whatever a client asks,
    /// so a modified client could grant itself any of it. Read at start.
    pub knowledge_guard: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            log_level: Level::default(),
            log_levels: BTreeMap::new(),
            diagnostics: false,
            diagnostics_timing: false,
            status_minutes: STATUS_MINUTES,
            migrations_dry_run: false,
            build_patterns: true,
            dev_reload: false,
            knowledge_guard: true,
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
        assert!(!settings.diagnostics);
        assert_eq!(settings.status_minutes, STATUS_MINUTES);
    }

    #[test]
    fn diagnostics_are_turned_on_in_the_file() {
        let settings: Settings = toml::from_str(
            "diagnostics = true
status_minutes = 0
",
        )
        .unwrap();
        assert!(settings.diagnostics);
        assert!(!settings.diagnostics_timing);
        assert_eq!(settings.status_minutes, 0);
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
