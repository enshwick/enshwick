//! The loader's own settings, `mod-configs/enshwick.toml`.

use std::collections::BTreeMap;
use std::path::Path;

use enshwick::abi::Level;
use enshwick::config::Config as _;

/// The id the loader logs and keeps settings under; no mod may take it.
pub use enshwick::mods::LOADER_ID;

/// Minutes between status summaries by default.
pub const STATUS_MINUTES: u32 = 10;
/// Seconds a joining player's game has to say hello by default.
pub const JOIN_HELLO_SECONDS: u32 = 30;
/// Seconds between the join note and the disconnect by default.
pub const JOIN_NOTE_SECONDS: u32 = 10;

/// Enshwick loader settings.
#[derive(Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize, enshwick::Config)]
#[serde(default, deny_unknown_fields)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each is a switch of its own in the settings file"
)]
pub struct Settings {
    /// The most detailed lines kept in `logs/enshwick`: "error", "warn", "info" or "debug".
    pub log_level: Level,
    /// The same per mod id, over `log_level`; "enshwick" is the loader itself.
    ///
    /// For example, `log_levels = { "you.my-mod" = "debug" }`.
    pub log_levels: BTreeMap<String, Level>,
    /// Count the calls of every hooked game function for the status summary.
    /// A few instructions per call; read at start.
    pub diagnostics: bool,
    /// With diagnostics, also time every 256th call of each hooked function.
    /// Read at start.
    pub diagnostics_timing: bool,
    /// Minutes between status summaries in `logs/enshwick/status.log`;
    /// 0 writes one only when the console asks with enshwick status.
    pub status_minutes: u32,
    /// Only log what mods' migrations would change, without changing their
    /// files or recording them in mod-data/enshwick/migrations.toml.
    pub migrations_dry_run: bool,
    /// On a game build without a build table yet, find the table's entries
    /// by their code patterns and run what they cover; false leaves such a
    /// build vanilla until its table arrives. Read at start.
    pub build_patterns: bool,
    /// Watch the mod DLLs in mods/ and reload one that changes, when the mod
    /// allows it (`reload: true` in its manifest). For developing mods, on
    /// the dedicated server or the client. Read at start.
    pub dev_reload: bool,
    /// What the server does with knowledge a client asks for in its input
    /// that a vanilla client never asks for: ids of the world's, counters,
    /// flag bits other than the seen bit, ids that give experience or skill
    /// points, and quests. A vanilla server grants whatever a client asks,
    /// so a modified client could grant itself any of it. "watch" lets it
    /// through and logs it, "enforce" refuses it, "off" does neither. Read
    /// at start.
    #[config(choices = [KnowledgeGuard::Watch, KnowledgeGuard::Enforce, KnowledgeGuard::Off])]
    pub knowledge_guard: KnowledgeGuard,
    /// The key that opens a mod's actions where the HUD hints at one: "F1"
    /// to "F12", a letter or a digit; "off" for none. Read at start.
    pub actions_key: String,
    /// On a server that runs join mods, or in a game this client hosts,
    /// disconnect a player whose game does not run them all, in the server's
    /// versions, or runs without Enshwick, after a chat line naming what it
    /// needs; administrators too, never the host. false lets such players
    /// in. Read at start.
    pub require_enshwick: bool,
    /// Seconds a joining player's game has to say which join mods it runs
    /// before the server takes it for a game without Enshwick. Read at start.
    pub join_hello_seconds: u32,
    /// Seconds between the chat line and the disconnect, for the player to
    /// read it. Read at start.
    pub join_note_seconds: u32,
    /// On a dedicated server, let only the accounts in
    /// mod-data/enshwick/allowlist.txt, and administrators, stay in the
    /// game; the console's enshwick allowlist add and remove change the
    /// file, and changes apply at the next join. Read at start.
    pub allowlist: bool,
    /// On a dedicated server, the language of the loader's texts where no
    /// player's game names its own, such as the console's answers: a code
    /// such as "de", "pt-br" or "uk"; empty for English. A player with
    /// Enshwick, and a game's own client, always read the loader's texts in
    /// their game's language. Read at start.
    pub language: String,
}

/// What the knowledge guard does with a request a vanilla client never makes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum KnowledgeGuard {
    /// Lets it through and logs it.
    #[default]
    Watch,
    /// Refuses it.
    Enforce,
    /// Leaves it alone.
    Off,
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
            knowledge_guard: KnowledgeGuard::Watch,
            actions_key: "F7".to_owned(),
            require_enshwick: true,
            join_hello_seconds: JOIN_HELLO_SECONDS,
            join_note_seconds: JOIN_NOTE_SECONDS,
            allowlist: false,
            language: String::new(),
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
    /// missing, with a line per problem for the log.
    ///
    /// A file that cannot be read or parsed holds every protection closed,
    /// as [`Settings::parse`] describes; `listed` says whether the
    /// allowlist's file exists.
    #[must_use]
    pub fn read(path: &Path, listed: bool) -> (Self, Vec<String>) {
        let (settings, problems) = match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text, listed),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (
                Self::default(),
                std::fs::write(path, Self::template())
                    .err()
                    .map(|e| e.to_string())
                    .into_iter()
                    .collect(),
            ),
            Err(e) => Self::closed(listed, e.to_string()),
        };
        let problems = problems
            .into_iter()
            .map(|problem| format!("{}: {problem}", path.display()))
            .collect();
        (settings, problems)
    }

    /// Parses the settings file's `text` key by key, with a line per problem.
    ///
    /// Every valid key applies and every bad one is named. A protection
    /// ([`PROTECTIONS`]) that is bad or missing in a file with problems is
    /// held closed until the file is fixed: the allowlist on when its file
    /// exists (`listed`), the migrations dry run on, the join check on and
    /// the knowledge guard enforcing. Text that is not TOML holds them all.
    #[must_use]
    pub fn parse(text: &str, listed: bool) -> (Self, Vec<String>) {
        let table: toml::Table = match toml::from_str(text) {
            Ok(table) => table,
            Err(e) => return Self::closed(listed, e.message().to_owned()),
        };
        let mut good = toml::Table::new();
        let mut problems = Vec::new();
        for (key, value) in table {
            let alone = toml::Table::from_iter([(key.clone(), value.clone())]);
            match alone.try_into::<Self>() {
                Ok(_) => {
                    good.insert(key, value);
                }
                Err(e) => problems.push(format!("{key}: {}", e.message())),
            }
        }
        let Ok(mut settings) = good.clone().try_into::<Self>() else {
            return Self::closed(listed, "the keys do not read together".to_owned());
        };
        if problems.is_empty() {
            return (settings, problems);
        }
        let held: Vec<&str> = PROTECTIONS
            .into_iter()
            .filter(|key| !good.contains_key(*key))
            .collect();
        for key in &held {
            settings.close(key, listed);
        }
        if !held.is_empty() {
            problems.push(format!("held closed until fixed: {}", held.join(", ")));
        }
        (settings, problems)
    }

    /// The defaults with every protection closed, and `problem` naming why.
    fn closed(listed: bool, problem: String) -> (Self, Vec<String>) {
        let mut settings = Self::default();
        for key in PROTECTIONS {
            settings.close(key, listed);
        }
        (
            settings,
            vec![
                problem,
                format!("held closed until fixed: {}", PROTECTIONS.join(", ")),
            ],
        )
    }

    /// Sets the protection `key` to its closed state.
    fn close(&mut self, key: &str, listed: bool) {
        match key {
            ALLOWLIST => self.allowlist = listed,
            MIGRATIONS_DRY_RUN => self.migrations_dry_run = true,
            REQUIRE_ENSHWICK => self.require_enshwick = true,
            KNOWLEDGE_GUARD => self.knowledge_guard = KnowledgeGuard::Enforce,
            _ => {}
        }
    }
}

const ALLOWLIST: &str = "allowlist";
const MIGRATIONS_DRY_RUN: &str = "migrations_dry_run";
const REQUIRE_ENSHWICK: &str = "require_enshwick";
const KNOWLEDGE_GUARD: &str = "knowledge_guard";

/// The settings that keep players out or data unchanged; a broken file
/// never opens them.
pub const PROTECTIONS: [&str; 4] = [
    ALLOWLIST,
    MIGRATIONS_DRY_RUN,
    REQUIRE_ENSHWICK,
    KNOWLEDGE_GUARD,
];

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
    fn the_knowledge_guard_watches_unless_told_otherwise() {
        assert_eq!(Settings::default().knowledge_guard, KnowledgeGuard::Watch);
        let settings: Settings = toml::from_str(
            "knowledge_guard = \"enforce\"
",
        )
        .unwrap();
        assert_eq!(settings.knowledge_guard, KnowledgeGuard::Enforce);
        assert!(
            toml::from_str::<Settings>(
                "knowledge_guard = true
"
            )
            .is_err()
        );
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
        let path = dir.join("enshwick.toml");
        let _fresh = std::fs::remove_file(&path);
        assert_eq!(
            Settings::read(&path, true),
            (Settings::default(), Vec::new())
        );
        assert!(path.exists());
        assert_eq!(
            Settings::read(&path, true),
            (Settings::default(), Vec::new())
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn closed(listed: bool) -> Settings {
        Settings {
            allowlist: listed,
            migrations_dry_run: true,
            require_enshwick: true,
            knowledge_guard: KnowledgeGuard::Enforce,
            ..Settings::default()
        }
    }

    #[test]
    fn the_template_parses_without_problems() {
        assert_eq!(
            Settings::parse(&Settings::template(), true),
            (Settings::default(), Vec::new())
        );
    }

    #[test]
    fn a_file_that_does_not_parse_holds_every_protection_closed() {
        let (settings, problems) =
            Settings::parse("allowlist = = true\nlog_level = \"debug\"\n", true);
        assert_eq!(settings, closed(true));
        assert!(!problems.is_empty());
        assert!(
            problems
                .iter()
                .any(|p| p.contains("allowlist") && p.contains("migrations_dry_run")),
            "{problems:?}"
        );
    }

    #[test]
    fn without_an_allowlist_file_a_broken_file_leaves_the_allowlist_off() {
        let (settings, _) = Settings::parse("[[[", false);
        assert_eq!(settings, closed(false));
    }

    #[test]
    fn a_bad_value_keeps_the_valid_keys_and_names_the_bad_one() {
        let text = "log_level = \"warn\"\nstatus_minutes = \"ten\"\nallowlist = true\n\
                    migrations_dry_run = false\nrequire_enshwick = false\nknowledge_guard = \"off\"\n";
        let (settings, problems) = Settings::parse(text, false);
        assert_eq!(settings.log_level, Level::WARN);
        assert_eq!(settings.status_minutes, STATUS_MINUTES);
        assert!(settings.allowlist);
        assert!(!settings.migrations_dry_run);
        assert!(!settings.require_enshwick);
        assert_eq!(settings.knowledge_guard, KnowledgeGuard::Off);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("status_minutes"), "{problems:?}");
    }

    #[test]
    fn a_bad_protection_value_holds_that_protection_closed() {
        let text = "allowlist = \"yes\"\nmigrations_dry_run = 1\nrequire_enshwick = false\n\
                    knowledge_guard = \"loud\"\n";
        let (settings, problems) = Settings::parse(text, true);
        assert!(settings.allowlist);
        assert!(settings.migrations_dry_run);
        assert!(!settings.require_enshwick);
        assert_eq!(settings.knowledge_guard, KnowledgeGuard::Enforce);
        for key in ["allowlist", "migrations_dry_run", "knowledge_guard"] {
            assert!(
                problems.iter().any(|p| p.contains(key)),
                "{key}: {problems:?}"
            );
        }
    }

    #[test]
    fn a_misspelled_protection_holds_it_closed() {
        let text = "alowlist = false\nmigrations_dry_run = false\nrequire_enshwick = true\n\
                    knowledge_guard = \"watch\"\n";
        let (settings, problems) = Settings::parse(text, true);
        assert!(settings.allowlist);
        assert!(!settings.migrations_dry_run);
        assert_eq!(settings.knowledge_guard, KnowledgeGuard::Watch);
        assert!(
            problems.iter().any(|p| p.contains("alowlist")),
            "{problems:?}"
        );
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri isolates the file system")]
    fn a_file_that_cannot_be_read_holds_every_protection_closed() {
        let dir =
            std::env::temp_dir().join(format!("enshwick-settings-dir-{}", std::process::id()));
        let path = dir.join("enshwick.toml");
        std::fs::create_dir_all(&path).unwrap();
        let (settings, problems) = Settings::read(&path, true);
        assert_eq!(settings, closed(true));
        assert!(!problems.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
