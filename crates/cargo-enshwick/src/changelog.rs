//! Hand-written changelogs in the Keep a Changelog form: a `CHANGELOG.md`
//! of `## [version]` sections read for release notes and closed for a
//! release, and the change files a pull request adds instead of editing the
//! shared `## [Unreleased]` section, joined into the changelog when it
//! closes.
//!
//! ```
//! use cargo_enshwick::changelog::{Fragment, join};
//!
//! let fragment = Fragment::parse("### Fixed\n\n- A crash on load.\n")?;
//! let changelog = "# Changelog\n\n## [Unreleased]\n\n## [0.1.0]\n\n- First.\n";
//! let joined = join(changelog, "0.1.1", "2026-11-13", &[fragment])?;
//! assert!(joined.contains("## [0.1.1] - 2026-11-13\n\n### Fixed\n\n- A crash on load.\n"));
//! # Ok::<(), cargo_enshwick::changelog::Error>(())
//! ```

/// A package's changelog, beside its `Cargo.toml`.
pub const CHANGELOG: &str = "CHANGELOG.md";
/// The folder of a package's change files, beside its `Cargo.toml`: one
/// Markdown file per pull request.
pub const CHANGES: &str = "changes";
/// The section of the changes no release carries yet.
pub const UNRELEASED: &str = "Unreleased";
/// The Keep a Changelog headings, in the order a section lists them.
pub const HEADINGS: [&str; 6] = [
    "Added",
    "Changed",
    "Deprecated",
    "Removed",
    "Fixed",
    "Security",
];
/// The warning every alpha's release notes open with.
pub const ALPHA_WARNING: &str = "\
> [!WARNING]
> This is an alpha. Before 1.0 anything may change, including the API, the
> ABI, file layouts, settings and mod ids. Mods built for one alpha may need a
> rebuild for the next. Back up your saves.
";

/// Why a changelog or a change file does not read or close.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// Changes wait under Unreleased.
    #[error("changes under Unreleased; close them as `## [{0}]` first")]
    Open(String),
    /// The version has no heading.
    #[error("no section `## [{0}]`")]
    NoSection(String),
    /// The version has a section and more changes wait under Unreleased.
    #[error(
        "{0} is closed already and has changes under Unreleased; move them into its section by hand"
    )]
    ClosedAlready(String),
    /// The changelog has no Unreleased section.
    #[error("no `## [Unreleased]` section")]
    NoUnreleased,
    /// Changes were written under Unreleased by hand.
    #[error("changes under Unreleased; a pull request adds them as a file in `changes/`")]
    HandEdited,
    /// A change file breaks its form.
    #[error("line {line}: {why}")]
    Fragment {
        /// The line, from 1.
        line: usize,
        /// What is wrong there.
        why: &'static str,
    },
}

/// Where the body of the `## [version]` section starts and ends: after its
/// heading's line, up to the next section or the link definitions at the
/// file's foot.
#[must_use]
pub fn span(changelog: &str, version: &str) -> Option<(usize, usize)> {
    heading(changelog, version).map(|(_, start, end)| (start, end))
}

/// Where the `## [version]` heading's line starts, and its section's body
/// as [`span`] gives it.
fn heading(changelog: &str, version: &str) -> Option<(usize, usize, usize)> {
    let heading = format!("## [{version}]");
    let mut offset = 0;
    let mut at = None;
    for line in changelog.split_inclusive('\n') {
        if let Some((at, start)) = at {
            let definition = line.starts_with('[') && line.contains("]: ");
            if line.starts_with("## ") || definition {
                return Some((at, start, offset));
            }
        } else if line.starts_with(&heading) {
            at = Some((offset, offset + line.len()));
        }
        offset += line.len();
    }
    at.map(|(at, start)| (at, start, changelog.len()))
}

/// The body of the `## [version]` section, trimmed; `None` without one or
/// when it is empty.
#[must_use]
pub fn section<'a>(changelog: &'a str, version: &str) -> Option<&'a str> {
    span(changelog, version)
        .map(|(start, end)| changelog[start..end].trim())
        .filter(|s| !s.is_empty())
}

/// The section of `version` in a changelog closed as it, `None` when it is
/// empty, so a release never ships changes its notes leave out.
///
/// # Errors
///
/// When changes wait under Unreleased or the version has no heading.
pub fn closed<'a>(changelog: &'a str, version: &str) -> Result<Option<&'a str>, Error> {
    if section(changelog, UNRELEASED).is_some() {
        return Err(Error::Open(version.to_owned()));
    }
    if heading(changelog, version).is_none() {
        return Err(Error::NoSection(version.to_owned()));
    }
    Ok(section(changelog, version))
}

/// The `changelog` with what is under Unreleased moved under
/// `## [version] - date`, Unreleased left empty above it; `None` when
/// nothing is under Unreleased.
///
/// An empty heading of the version is dropped, so a stray one never blocks
/// the round that releases the version.
///
/// # Errors
///
/// When the version has a section with changes already: a second heading
/// would hide the first from its notes.
pub fn close(changelog: &str, version: &str, date: &str) -> Result<Option<String>, Error> {
    if section(changelog, UNRELEASED).is_none() {
        return Ok(None);
    }
    let stripped;
    let changelog = match heading(changelog, version) {
        Some((at, start, end)) if changelog[start..end].trim().is_empty() => {
            stripped = format!("{}{}", &changelog[..at], &changelog[end..]);
            stripped.as_str()
        }
        _ => changelog,
    };
    if heading(changelog, version).is_some() {
        return Err(Error::ClosedAlready(version.to_owned()));
    }
    let Some((start, end)) = span(changelog, UNRELEASED) else {
        return Ok(None);
    };
    let body = changelog[start..end].trim();
    Ok(Some(format!(
        "{}\n## [{version}] - {date}\n\n{body}\n\n{}",
        &changelog[..start],
        &changelog[end..]
    )))
}

/// The `changelog` with the `fragments` joined under `## [version] - date`,
/// each heading's entries in the order of the fragments; without fragments
/// the heading stands alone, as for a release with no change of the
/// package's own.
///
/// # Errors
///
/// When the changelog has no Unreleased section, holds changes there
/// written by hand, or has a section of the version with changes already.
pub fn join(
    changelog: &str,
    version: &str,
    date: &str,
    fragments: &[Fragment],
) -> Result<String, Error> {
    let (start, end) = span(changelog, UNRELEASED).ok_or(Error::NoUnreleased)?;
    if !changelog[start..end].trim().is_empty() {
        return Err(Error::HandEdited);
    }
    let body = Fragment::merged(fragments);
    if !body.is_empty() {
        let open = format!("{}\n{body}\n{}", &changelog[..start], &changelog[end..]);
        return Ok(close(&open, version, date)?.unwrap_or(open));
    }
    if heading(changelog, version).is_some() {
        return Ok(changelog.to_owned());
    }
    Ok(format!(
        "{}\n## [{version}] - {date}\n\n{}",
        &changelog[..start],
        &changelog[end..]
    ))
}

/// Whether `text` is a date as `YYYY-MM-DD`, the form a closed section's
/// heading gives.
///
/// ```
/// use cargo_enshwick::changelog::is_date;
///
/// assert!(is_date("2026-11-13"));
/// assert!(!is_date("13.11.2026"));
/// ```
#[must_use]
pub fn is_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 10
        && bytes.iter().enumerate().all(|(at, byte)| match at {
            4 | 7 => *byte == b'-',
            _ => byte.is_ascii_digit(),
        })
}

/// Whether `changelog` holds changes under Unreleased, which a pull request
/// adds as a change file instead.
#[must_use]
pub fn hand_edited(changelog: &str) -> bool {
    section(changelog, UNRELEASED).is_some()
}

/// One change file: entries under Keep a Changelog headings, as the
/// section of a release lists them.
///
/// ```
/// use cargo_enshwick::changelog::Fragment;
///
/// assert!(Fragment::parse("### Added\n\n- A call.\n").is_ok());
/// assert!(Fragment::parse("- A change without a heading.\n").is_err());
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fragment {
    /// Each entry as its index in [`HEADINGS`] and its text, continuation
    /// lines included.
    entries: Vec<(usize, String)>,
}

impl Fragment {
    /// Reads a change file: `### <heading>` lines of [`HEADINGS`], each
    /// once, each followed by `- ` entries whose further lines are indented,
    /// and blank lines.
    ///
    /// # Errors
    ///
    /// When a line breaks that form, a heading is unknown or repeated, or
    /// the file holds no entry.
    pub fn parse(text: &str) -> Result<Self, Error> {
        let mut entries: Vec<(usize, String)> = Vec::new();
        let mut current = None;
        let mut seen = Vec::new();
        let mut in_entry = false;
        let mut last = 0;
        for (index, line) in text.lines().enumerate() {
            let fail = |why| Error::Fragment {
                line: index + 1,
                why,
            };
            last = index + 1;
            if line.trim().is_empty() {
                in_entry = false;
            } else if let Some(name) = line.strip_prefix("### ") {
                let at = HEADINGS
                    .iter()
                    .position(|h| *h == name.trim())
                    .ok_or_else(|| {
                        fail("a heading is one of Added, Changed, Deprecated, Removed, Fixed and Security")
                    })?;
                if seen.contains(&at) {
                    return Err(fail("a heading comes once"));
                }
                seen.push(at);
                current = Some(at);
                in_entry = false;
            } else if line.starts_with("- ") {
                let at = current.ok_or_else(|| fail("an entry stands under a heading"))?;
                entries.push((at, line.to_owned()));
                in_entry = true;
            } else if line.starts_with("  ") && in_entry {
                if let Some((_, entry)) = entries.last_mut() {
                    entry.push('\n');
                    entry.push_str(line);
                }
            } else {
                return Err(fail(
                    "a line is a `### ` heading, a `- ` entry, an indented line of one, or blank",
                ));
            }
        }
        if entries.is_empty() {
            return Err(Error::Fragment {
                line: last.max(1),
                why: "a change file holds an entry",
            });
        }
        Ok(Self { entries })
    }

    /// The section body of `fragments`: each heading of [`HEADINGS`] that
    /// has entries, in that order, with its entries in the order given.
    fn merged(fragments: &[Self]) -> String {
        let mut out = String::new();
        for (at, heading) in HEADINGS.iter().enumerate() {
            let entries: Vec<&str> = fragments
                .iter()
                .flat_map(|f| &f.entries)
                .filter(|(h, _)| *h == at)
                .map(|(_, entry)| entry.as_str())
                .collect();
            if entries.is_empty() {
                continue;
            }
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str("### ");
            out.push_str(heading);
            out.push_str("\n\n");
            for entry in entries {
                out.push_str(entry);
                out.push('\n');
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "# Changelog\n\nIntro.\n\n## [Unreleased]\n";
    const RELEASED: &str = "## [0.1.0] - 2026-11-05\n\n### Added\n\n- First.\n";

    #[test]
    fn closing_moves_the_unreleased_changes_under_the_version() {
        let changelog = format!("{HEAD}\n### Fixed\n\n- A bug.\n\n{RELEASED}");
        assert_eq!(
            close(&changelog, "0.1.1", "2026-11-08").unwrap().unwrap(),
            format!("{HEAD}\n## [0.1.1] - 2026-11-08\n\n### Fixed\n\n- A bug.\n\n{RELEASED}")
        );
        let foot = "## [Unreleased]\n\n- A.\n[0.1.0]: https://example.com\n";
        assert_eq!(
            close(foot, "0.1.1", "2026-11-08").unwrap().unwrap(),
            "## [Unreleased]\n\n## [0.1.1] - 2026-11-08\n\n- A.\n\n[0.1.0]: https://example.com\n"
        );
        assert_eq!(
            close(&format!("{HEAD}\n{RELEASED}"), "0.1.1", "2026-11-08"),
            Ok(None)
        );
    }

    #[test]
    fn a_version_closed_already_is_never_closed_twice() {
        let changelog = format!("{HEAD}\n### Fixed\n\n- Late.\n\n{RELEASED}");
        assert_eq!(
            close(&changelog, "0.1.0", "2026-11-08"),
            Err(Error::ClosedAlready("0.1.0".to_owned()))
        );
    }

    #[test]
    fn an_empty_heading_of_the_version_is_replaced_when_it_is_closed() {
        let stray = format!("{HEAD}\n### Fixed\n\n- A bug.\n\n## [0.1.1]\n\n{RELEASED}");
        assert_eq!(
            close(&stray, "0.1.1", "2026-11-09").unwrap().unwrap(),
            format!("{HEAD}\n## [0.1.1] - 2026-11-09\n\n### Fixed\n\n- A bug.\n\n{RELEASED}")
        );
        let nothing_new = format!("{HEAD}\n## [0.1.1]\n\n{RELEASED}");
        assert_eq!(close(&nothing_new, "0.1.1", "2026-11-09"), Ok(None));
        let twice = "## [Unreleased]\n\n- A.\n\n## [0.1.1]\n\n## [0.1.1]\n\n- B.\n";
        assert!(close(twice, "0.1.1", "2026-11-09").is_err());
    }

    #[test]
    fn only_a_link_definition_at_a_line_start_ends_the_section() {
        let changelog = "## [Unreleased]\n\n- Rates: [x]: y.\n[ref] at the start.\n";
        assert_eq!(
            close(changelog, "0.1.1", "2026-11-08").unwrap().unwrap(),
            "## [Unreleased]\n\n## [0.1.1] - 2026-11-08\n\n- Rates: [x]: y.\n[ref] at the start.\n\n"
        );
    }

    #[test]
    fn notes_take_the_closed_section_of_the_version() {
        let changelog = "# Changelog\n\n## [Unreleased]\n\n## [0.2.0] - 2026-11-05\n\n### Added\n\n- More loot.\n\n## [0.1.0]\n\n- First, see [docs].\n\n[docs]: https://example.com/docs\n";
        assert_eq!(
            closed(changelog, "0.2.0"),
            Ok(Some("### Added\n\n- More loot."))
        );
        assert_eq!(closed(changelog, "0.1.0"), Ok(Some("- First, see [docs].")));
    }

    #[test]
    fn notes_refuse_open_changes_and_a_missing_section_and_pass_an_empty_one() {
        let open = "## [Unreleased]\n\n- Next.\n\n## [0.2.0]\n\n- A.\n";
        assert_eq!(closed(open, "0.2.0"), Err(Error::Open("0.2.0".to_owned())));
        let empty = "## [Unreleased]\n\n## [0.2.0]\n\n## [0.1.0]\n\n- A.\n";
        assert_eq!(closed(empty, "0.2.0"), Ok(None));
        assert_eq!(
            closed(empty, "0.3.0"),
            Err(Error::NoSection("0.3.0".to_owned()))
        );
    }

    #[test]
    fn a_change_file_reads_its_headings_and_entries() {
        let fragment =
            Fragment::parse("### Fixed\n\n- A bug,\n  on two lines.\n\n### Added\n- A call.\n")
                .unwrap();
        assert_eq!(
            fragment.entries,
            [
                (4, "- A bug,\n  on two lines.".to_owned()),
                (0, "- A call.".to_owned())
            ]
        );
    }

    #[test]
    fn a_malformed_change_file_names_its_line() {
        let why = |text: &str| match Fragment::parse(text) {
            Err(Error::Fragment { line, why }) => (line, why),
            other => panic!("{other:?}"),
        };
        assert_eq!(why("- No heading.\n").0, 1);
        assert_eq!(why("### Improved\n\n- A.\n").0, 1);
        assert_eq!(why("### Added\n\n- A.\n\n### Added\n\n- B.\n").0, 5);
        assert_eq!(why("### Added\n\nProse.\n").0, 3);
        assert_eq!(why("### Added\n\n- A.\n\n  Loose.\n").0, 5);
        assert_eq!(why("## [Unreleased]\n").0, 1);
        assert_eq!(why("### Added\n").1, "a change file holds an entry");
        assert_eq!(why("").1, "a change file holds an entry");
    }

    #[test]
    fn joined_change_files_list_each_heading_once_in_order() {
        let a = Fragment::parse("### Fixed\n\n- Fix a.\n\n### Added\n\n- Add a.\n").unwrap();
        let b = Fragment::parse("### Added\n\n- Add b,\n  more.\n\n### Removed\n\n- Drop b.\n")
            .unwrap();
        let changelog = format!("{HEAD}\n{RELEASED}");
        assert_eq!(
            join(&changelog, "0.1.1", "2026-11-13", &[a, b]).unwrap(),
            format!(
                "{HEAD}\n## [0.1.1] - 2026-11-13\n\n### Added\n\n- Add a.\n- Add b,\n  more.\n\n\
                 ### Removed\n\n- Drop b.\n\n### Fixed\n\n- Fix a.\n\n{RELEASED}"
            )
        );
    }

    #[test]
    fn a_package_without_change_files_gets_the_version_s_heading_alone() {
        let changelog = format!("{HEAD}\n{RELEASED}");
        let joined = join(&changelog, "0.1.1", "2026-11-13", &[]).unwrap();
        assert_eq!(
            joined,
            format!("{HEAD}\n## [0.1.1] - 2026-11-13\n\n{RELEASED}")
        );
        assert_eq!(closed(&joined, "0.1.1"), Ok(None));
        assert_eq!(join(&joined, "0.1.1", "2026-11-13", &[]).unwrap(), joined);
    }

    #[test]
    fn only_a_calendar_shaped_date_is_a_date() {
        assert!(is_date("2026-11-13"));
        for not in ["2026-1-13", "2026/11/13", "2026-11-13 ", "", "abcd-ef-gh"] {
            assert!(!is_date(not), "{not}");
        }
    }

    #[test]
    fn joining_refuses_a_hand_edited_unreleased_section_and_a_closed_version() {
        let fragment = Fragment::parse("### Added\n\n- A.\n").unwrap();
        let edited = format!("{HEAD}\n- By hand.\n\n{RELEASED}");
        assert!(hand_edited(&edited));
        assert_eq!(
            join(&edited, "0.1.1", "d", std::slice::from_ref(&fragment)),
            Err(Error::HandEdited)
        );
        let released = format!("{HEAD}\n{RELEASED}");
        assert!(!hand_edited(&released));
        assert_eq!(
            join(&released, "0.1.0", "d", std::slice::from_ref(&fragment)),
            Err(Error::ClosedAlready("0.1.0".to_owned()))
        );
        assert_eq!(
            join(RELEASED, "0.1.1", "d", &[fragment]),
            Err(Error::NoUnreleased)
        );
    }
}
