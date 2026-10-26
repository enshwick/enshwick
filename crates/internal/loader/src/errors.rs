//! The codes the loader's messages carry, each with a page on the
//! documentation site that says what went wrong and how to fix it. The codes,
//! their texts and the steps of "the game won't start" live in `errors.toml`
//! beside this file; the site builds its pages from it.

use core::fmt;

/// Every code with its texts, and the steps of the decision tree.
pub const SOURCE: &str = include_str!("errors.toml");

/// Where the page of a code lives, before the code in lower case.
pub const PAGES: &str = "https://enshwick.github.io/docs/errors/";

/// A code of `errors.toml`, shown at the end of a loader message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Code(&'static str);

impl Code {
    /// The code as `errors.toml` spells it, such as `E1003`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for Code {
    /// `E1003, see https://.../errors/e1003/`, for the end of a message.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}, see {PAGES}{}/", self.0, self.0.to_ascii_lowercase())
    }
}

/// Another program loaded the stub.
pub const NOT_THE_GAME: Code = Code("E1001");
/// The game's image does not read.
pub const UNREADABLE_GAME: Code = Code("E1002");
/// No table and no patterns serve this build.
pub const UNKNOWN_BUILD: Code = Code("E1003");
/// The build's own table does not match the running executable.
pub const TABLE_MISMATCH: Code = Code("E1004");
/// A build without its own table, served by patterns, lacks entries.
pub const FEATURES_OFF: Code = Code("E1005");
/// The system `powrprof.dll` did not load.
pub const NO_SYSTEM_POWRPROF: Code = Code("E1006");
/// The loader's settings file does not read.
pub const BAD_SETTINGS: Code = Code("E1007");
/// A mod is built for another ABI.
pub const OTHER_ABI: Code = Code("E2001");
/// A mod's relations to other mods keep it from starting.
pub const UNMET_RELATIONS: Code = Code("E2002");
/// A mod turned itself off, at init or later.
pub const MOD_OFF: Code = Code("E2003");
/// A DLL in `mods/` is no working mod.
pub const BROKEN_DLL: Code = Code("E2004");
/// One mod in two files.
pub const DUPLICATE_MOD: Code = Code("E2005");
/// The mod list does not read.
pub const BAD_MOD_LIST: Code = Code("E2006");
/// A mod's hold stops the game.
pub const HELD: Code = Code("E2007");
/// The server runs join mods the client lacks.
pub const JOIN_MISSING: Code = Code("E3001");

/// Every code a loader message carries.
pub const ALL: &[Code] = &[
    NOT_THE_GAME,
    UNREADABLE_GAME,
    UNKNOWN_BUILD,
    TABLE_MISMATCH,
    FEATURES_OFF,
    NO_SYSTEM_POWRPROF,
    BAD_SETTINGS,
    OTHER_ABI,
    UNMET_RELATIONS,
    MOD_OFF,
    BROKEN_DLL,
    DUPLICATE_MOD,
    BAD_MOD_LIST,
    HELD,
    JOIN_MISSING,
];

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    use super::*;

    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Source {
        error: Vec<Error>,
        step: Vec<Step>,
    }

    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Error {
        code: String,
        title: String,
        cause: String,
        fix: String,
    }

    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Step {
        id: String,
        question: String,
        yes: String,
        no: String,
    }

    fn source() -> Source {
        toml::from_str(SOURCE).unwrap()
    }

    /// Every source file of the loader but this one.
    fn sources() -> Vec<PathBuf> {
        let mut dirs = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
        let this = Path::new(file!()).file_name().unwrap();
        let mut files = Vec::new();
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    dirs.push(path);
                } else if path.extension().is_some_and(|e| e == "rs")
                    && path.file_name() != Some(this)
                {
                    files.push(path);
                }
            }
        }
        files
    }

    #[test]
    fn every_code_is_unique_well_formed_and_explained() {
        let source = source();
        let mut seen = BTreeSet::new();
        for error in &source.error {
            let code = error.code.as_str();
            assert!(seen.insert(code), "{code} twice");
            assert!(
                code.len() == 5
                    && code.starts_with('E')
                    && code[1..].bytes().all(|b| b.is_ascii_digit()),
                "{code}"
            );
            for text in [&error.title, &error.cause, &error.fix] {
                assert!(!text.trim().is_empty(), "{code}");
            }
        }
    }

    #[test]
    fn every_code_of_a_message_is_in_the_source_and_every_code_there_is_reached() {
        let source = source();
        let listed: BTreeSet<&str> = source.error.iter().map(|e| e.code.as_str()).collect();
        let carried: BTreeSet<&str> = ALL.iter().map(|c| c.as_str()).collect();
        assert_eq!(carried.len(), ALL.len(), "a code is named twice");
        let reached: BTreeSet<&str> = source
            .step
            .iter()
            .flat_map(|s| [s.yes.as_str(), s.no.as_str()])
            .filter(|target| listed.contains(target))
            .collect();
        assert!(
            carried.is_subset(&listed),
            "{:?}",
            carried.difference(&listed)
        );
        let unused: Vec<&&str> = listed
            .iter()
            .filter(|code| !carried.contains(*code) && !reached.contains(*code))
            .collect();
        assert!(unused.is_empty(), "{unused:?}");
    }

    #[test]
    fn every_code_a_message_carries_is_named_at_a_call_site() {
        let code: String = sources()
            .iter()
            .map(|path| std::fs::read_to_string(path).unwrap())
            .collect();
        let named: BTreeSet<&str> = code
            .split("errors::")
            .skip(1)
            .map(|rest| {
                let end = rest
                    .find(|c: char| !(c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit()))
                    .unwrap_or(rest.len());
                &rest[..end]
            })
            .filter(|name| !name.is_empty())
            .collect();
        let this = include_str!("errors.rs");
        for name in this
            .lines()
            .filter_map(|l| l.strip_prefix("pub const "))
            .filter_map(|l| l.split_once(": Code ="))
            .map(|(name, _)| name)
        {
            assert!(named.contains(name), "errors::{name} carries no message");
        }
    }

    #[test]
    fn the_tree_starts_at_start_and_every_answer_leads_somewhere() {
        let source = source();
        let codes: BTreeSet<&str> = source.error.iter().map(|e| e.code.as_str()).collect();
        let ids: BTreeSet<&str> = source.step.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids.len(), source.step.len(), "a step id twice");
        assert!(ids.contains("start"));
        for step in &source.step {
            assert!(step.question.ends_with('?'), "{}", step.id);
            for target in [&step.yes, &step.no] {
                assert!(
                    ids.contains(target.as_str()) || codes.contains(target.as_str()),
                    "{} leads to {target}",
                    step.id
                );
            }
        }
    }

    #[test]
    fn a_code_reads_as_its_page() {
        assert_eq!(
            UNKNOWN_BUILD.to_string(),
            "E1003, see https://enshwick.github.io/docs/errors/e1003/"
        );
    }
}
