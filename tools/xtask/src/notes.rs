//! `cargo xtask notes`: the notes of an alpha release of the loader, the
//! version the workspace carries: the warning that anything may change, the
//! game builds the compatibility reports cover, each crate's changelog section of
//! that version and the known issues.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::{Package, packages, say, workspace_root};

const USAGE: &str = "usage: cargo xtask notes [--out <file>] [--known-issues <text>]";
/// The package whose version names the release.
const LOADER: &str = "enshwick-loader";
/// The compatibility reports of the builds the loader supports.
const REPORTS: &str = "crates/enshwick-gamedata/reports";
/// GitHub refuses a release body longer than this, in characters.
const MAX_BODY: usize = 125_000;
const WARNING: &str = "\
> [!WARNING]
> This is an alpha. Before 1.0 anything may change, including the API, the
> ABI, file layouts, settings and mod ids. Mods built for one alpha may need a
> rebuild for the next. Back up your saves.
>
> Це альфа-версія. До 1.0 може змінитися будь-що: API, ABI, розташування
> файлів, налаштування та ідентифікатори модів. Моди, зібрані для однієї
> альфа-версії, можуть потребувати перезбирання для наступної. Зробіть
> резервні копії збережень.
";
const INSTALL: &str = "Extract `enshwick.zip` into the folder that holds \
`enshrouded.exe` or `enshrouded_server.exe`. It holds the loader and no mod: \
each mod ships in its own archive.";
const NO_KNOWN_ISSUES: &str = "None reported for this alpha.";

pub(crate) fn run(args: &[String]) -> Result<bool, String> {
    let root = workspace_root();
    let (out, known_issues) = options(&root, args)?;
    let packages = packages()?;
    let version = &packages
        .iter()
        .find(|p| p.name == LOADER)
        .ok_or_else(|| format!("no package {LOADER}"))?
        .version;
    let text = notes(
        version,
        &builds(&root.join(REPORTS))?,
        &changes(&root, &packages, version)?,
        &known_issues,
    )?;
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(&out, text).map_err(|e| format!("{}: {e}", out.display()))?;
    say(&format!(
        "wrote the notes of {version} to {}",
        out.display()
    ));
    Ok(true)
}

/// The file to write, `target/notes.md` of the workspace by default, and the
/// known issues.
fn options(root: &Path, args: &[String]) -> Result<(PathBuf, String), String> {
    let mut out = root.join("target").join("notes.md");
    let mut known_issues = String::new();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--out" => out = PathBuf::from(rest.next().ok_or(USAGE)?),
            "--known-issues" => known_issues.clone_from(rest.next().ok_or(USAGE)?),
            _ => return Err(USAGE.to_owned()),
        }
    }
    Ok((out, known_issues))
}

/// One game build a compatibility report covers; a report may leave out
/// either number, as the loader's own reading allows.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct GameBuild {
    side: String,
    game_version: Option<u64>,
    steam_build: Option<u64>,
}

/// The builds of every report in `dir`, newest first on each side.
fn builds(dir: &Path) -> Result<Vec<GameBuild>, String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut builds = Vec::new();
    for entry in entries {
        let path = entry.map_err(|e| format!("{}: {e}", dir.display()))?.path();
        if path
            .extension()
            .is_none_or(|ext| !ext.eq_ignore_ascii_case("toml"))
        {
            continue;
        }
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        builds.push(build(&text).map_err(|e| format!("{}: {e}", path.display()))?);
    }
    builds.sort_by(|a, b| a.side.cmp(&b.side).then(b.cmp(a)));
    builds.dedup();
    Ok(builds)
}

fn build(table: &str) -> Result<GameBuild, String> {
    let table: toml::Table = toml::from_str(table).map_err(|e| e.to_string())?;
    let number = |key: &str| {
        table
            .get(key)
            .and_then(toml::Value::as_integer)
            .and_then(|n| u64::try_from(n).ok())
    };
    Ok(GameBuild {
        side: table
            .get("side")
            .and_then(toml::Value::as_str)
            .ok_or("no side")?
            .to_owned(),
        game_version: number("game_version"),
        steam_build: number("steam_build"),
    })
}

/// Each package's changelog section of `version`, as `(package, section)`;
/// a package without a changelog, or with an empty section, is left out.
fn changes(
    root: &Path,
    packages: &[Package],
    version: &str,
) -> Result<Vec<(String, String)>, String> {
    let mut changes = Vec::new();
    for package in packages {
        let path = root.join(&package.dir).join("CHANGELOG.md");
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        if let Some(section) =
            closed(&text, version).map_err(|e| format!("{}: {e}", path.display()))?
        {
            changes.push((package.name.clone(), section.to_owned()));
        }
    }
    if changes.is_empty() {
        return Err(format!("no changelog has a change in {version}"));
    }
    Ok(changes)
}

/// The section of `version` in a changelog closed as it: one with changes
/// still under Unreleased, or without the version's heading, is refused, so
/// an alpha never ships changes its notes leave out.
fn closed<'a>(changelog: &'a str, version: &str) -> Result<Option<&'a str>, String> {
    if section(changelog, "Unreleased").is_some() {
        return Err(format!(
            "changes under Unreleased; close them as `## [{version}]` first"
        ));
    }
    let heading = format!("## [{version}]");
    if !changelog.lines().any(|line| line.starts_with(&heading)) {
        return Err(format!("no section `{heading}`"));
    }
    Ok(section(changelog, version))
}

/// The body of the `## [version]` section of a Keep a Changelog file, up to
/// the next section, trimmed; `None` without one or when it is empty.
fn section<'a>(changelog: &'a str, version: &str) -> Option<&'a str> {
    let heading = format!("## [{version}]");
    let mut offset = 0;
    let mut start = None;
    for line in changelog.split_inclusive('\n') {
        if let Some(start) = start {
            if line.starts_with("## ") {
                return Some(changelog[start..offset].trim()).filter(|s| !s.is_empty());
            }
        } else if line.starts_with(&heading) {
            start = Some(offset + line.len());
        }
        offset += line.len();
    }
    start
        .map(|start| changelog[start..].trim())
        .filter(|s| !s.is_empty())
}

fn notes(
    version: &str,
    builds: &[GameBuild],
    changes: &[(String, String)],
    known_issues: &str,
) -> Result<String, String> {
    let semver = semver::Version::parse(version).map_err(|e| format!("{version}: {e}"))?;
    if semver.pre.is_empty() {
        return Err(format!(
            "{version} is not a prerelease; alpha notes need one"
        ));
    }
    let mut text = format!("{WARNING}\n{INSTALL}\n\n## Supported game builds\n\n");
    text.push_str("| Side | Game version | Steam build |\n| --- | --- | --- |\n");
    let known = |n: Option<u64>| n.map_or_else(|| "unknown".to_owned(), |n| n.to_string());
    for build in builds {
        let _infallible = writeln!(
            text,
            "| {} | {} | {} |",
            build.side,
            known(build.game_version),
            known(build.steam_build)
        );
    }
    text.push_str("\n## What changed\n");
    for (package, section) in changes {
        let _infallible = write!(text, "\n### `{package}`\n\n");
        // Headings one level down, under the package's; code blocks as they are.
        let mut fenced = false;
        for line in section.lines() {
            let start = line.trim_start();
            if start.starts_with("```") || start.starts_with("~~~") {
                fenced = !fenced;
            } else if !fenced
                && line.trim_start_matches('#').starts_with(' ')
                && line.starts_with('#')
            {
                text.push('#');
            }
            text.push_str(line);
            text.push('\n');
        }
    }
    let known_issues = Some(known_issues.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or(NO_KNOWN_ISSUES);
    let _infallible = writeln!(text, "\n## Known issues\n\n{known_issues}");
    let length = text.chars().count();
    if length > MAX_BODY {
        return Err(format!(
            "the notes of {version} have {length} characters; GitHub takes {MAX_BODY} at most"
        ));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHANGELOG: &str = "# Changelog\n\n## [Unreleased]\n\n### Added\n\n- next\n\n\
        ## [1.0.0-alpha.12] - 2026-11-05\n\n### Added\n\n- controls (#930)\n\n\
        ## [1.0.0-alpha.11] - 2026-10-28\n\n### Changed\n\n- older\n";

    #[test]
    fn a_section_ends_at_the_next_version() {
        assert_eq!(
            section(CHANGELOG, "1.0.0-alpha.12"),
            Some("### Added\n\n- controls (#930)")
        );
    }

    #[test]
    fn the_last_section_runs_to_the_end_of_the_file() {
        assert_eq!(
            section(CHANGELOG, "1.0.0-alpha.11"),
            Some("### Changed\n\n- older")
        );
    }

    #[test]
    fn a_version_without_a_section_or_with_an_empty_one_has_none() {
        assert_eq!(section(CHANGELOG, "1.0.0-alpha.13"), None);
        assert_eq!(
            section(
                "## [1.0.0-alpha.2]\n\n## [1.0.0-alpha.1]\n- a\n",
                "1.0.0-alpha.2"
            ),
            None
        );
    }

    #[test]
    fn a_version_is_not_matched_by_its_prefix() {
        assert_eq!(section("## [1.0.0-alpha.12]\n- a\n", "1.0.0-alpha.1"), None);
    }

    fn sample() -> Result<String, String> {
        notes(
            "1.0.0-alpha.12",
            &[GameBuild {
                side: "server".to_owned(),
                game_version: Some(1_287_369),
                steam_build: Some(26_430_253),
            }],
            &[(
                "enshwick".to_owned(),
                "### Added\n\n- controls (#930)".to_owned(),
            )],
            "",
        )
    }

    #[test]
    fn the_notes_open_with_the_warning_in_english_and_ukrainian() -> Result<(), String> {
        let text = sample()?;
        assert!(text.starts_with("> [!WARNING]\n> This is an alpha."));
        assert!(text.contains("> Це альфа-версія."));
        Ok(())
    }

    #[test]
    fn the_notes_list_builds_changes_one_level_down_and_known_issues() -> Result<(), String> {
        let text = sample()?;
        assert!(text.contains("| server | 1287369 | 26430253 |"));
        assert!(text.contains("### `enshwick`\n\n#### Added\n\n- controls (#930)\n"));
        assert!(text.ends_with(&format!("## Known issues\n\n{NO_KNOWN_ISSUES}\n")));
        Ok(())
    }

    #[test]
    fn the_notes_may_fill_a_github_release_body_but_not_overflow_it() -> Result<(), String> {
        let pick = |issues: &str| {
            notes(
                "1.0.0-alpha.12",
                &[],
                &[("enshwick".to_owned(), "- a".to_owned())],
                issues,
            )
        };
        let base = pick("x")?.chars().count();
        let fits = "x".repeat(MAX_BODY - base + 1);
        assert_eq!(pick(&fits)?.chars().count(), MAX_BODY);
        assert!(pick(&format!("{fits}x")).is_err());
        Ok(())
    }

    #[test]
    fn code_blocks_in_a_section_keep_their_lines() -> Result<(), String> {
        let section = "### Added\n\n```rust\n#[hook]\n# hidden\n```\n#hashtag";
        let text = notes(
            "1.0.0-alpha.12",
            &[],
            &[("enshwick".to_owned(), section.to_owned())],
            "",
        )?;
        assert!(text.contains("#### Added\n\n```rust\n#[hook]\n# hidden\n```\n#hashtag\n"));
        Ok(())
    }

    #[test]
    fn a_build_without_a_number_shows_it_unknown() -> Result<(), String> {
        let text = notes(
            "1.0.0-alpha.12",
            &[build("side = \"client\"\n")?],
            &[("enshwick".to_owned(), "- a".to_owned())],
            "",
        )?;
        assert!(text.contains("| client | unknown | unknown |"));
        Ok(())
    }

    fn package(name: &str, dir: &str) -> Package {
        Package {
            name: name.to_owned(),
            version: String::new(),
            dir: dir.to_owned(),
            deps: Vec::new(),
            windows_only: false,
            windows_tests: false,
        }
    }

    /// A workspace of its own for this test process, with `changelogs` as
    /// `(package dir, CHANGELOG.md)`.
    fn workspace(changelogs: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("xtask-notes-{}", std::process::id()));
        for (dir, text) in changelogs {
            std::fs::create_dir_all(root.join(dir)).unwrap();
            std::fs::write(root.join(dir).join("CHANGELOG.md"), text).unwrap();
        }
        root
    }

    #[test]
    fn the_changes_take_each_closed_changelog_and_skip_packages_without_one() -> Result<(), String>
    {
        let root = workspace(&[
            ("a", "## [Unreleased]\n\n## [1.0.0-alpha.12]\n- a\n"),
            (
                "b",
                "## [Unreleased]\n\n## [1.0.0-alpha.12]\n\n## [1.0.0-alpha.11]\n- old\n",
            ),
        ]);
        let packages = [package("a", "a"), package("b", "b"), package("c", "c")];
        let changes = changes(&root, &packages, "1.0.0-alpha.12")?;
        assert_eq!(changes, [("a".to_owned(), "- a".to_owned())]);
        Ok(())
    }

    #[test]
    fn a_changelog_with_open_changes_or_without_the_version_is_refused() {
        assert!(
            closed(
                "## [Unreleased]\n- next\n## [1.0.0-alpha.12]\n- a\n",
                "1.0.0-alpha.12"
            )
            .is_err()
        );
        assert!(
            closed(
                "## [Unreleased]\n\n## [1.0.0-alpha.11]\n- a\n",
                "1.0.0-alpha.12"
            )
            .is_err()
        );
        assert_eq!(
            closed(
                "## [Unreleased]\n\n## [1.0.0-alpha.12]\n- a\n",
                "1.0.0-alpha.12"
            ),
            Ok(Some("- a"))
        );
    }

    #[test]
    fn an_unreadable_changelog_is_refused_not_skipped() {
        let root = workspace(&[("f", "## [1.0.0-alpha.12]\n- f\n")]);
        std::fs::create_dir_all(root.join("d").join("CHANGELOG.md")).unwrap();
        let packages = [package("d", "d"), package("f", "f")];
        let refused = changes(&root, &packages, "1.0.0-alpha.12").unwrap_err();
        assert!(refused.contains("CHANGELOG.md"), "{refused}");
    }

    #[test]
    fn a_version_without_a_change_anywhere_is_refused() {
        let root = workspace(&[("e", "## [1.0.0-alpha.12]\n\n## [1.0.0-alpha.11]\n- a\n")]);
        assert!(changes(&root, &[package("e", "e")], "1.0.0-alpha.12").is_err());
    }

    #[test]
    fn a_release_version_gets_no_alpha_notes() {
        assert!(notes("1.0.0", &[], &[], "").is_err());
        assert!(notes("1.0.0+build-1", &[], &[], "").is_err());
    }

    #[test]
    fn the_notes_go_to_the_workspace_target_unless_told_otherwise() -> Result<(), String> {
        let root = Path::new("root");
        assert_eq!(
            options(root, &[])?,
            (root.join("target").join("notes.md"), String::new())
        );
        let args = ["--out", "x.md", "--known-issues", "- crash"].map(str::to_owned);
        assert_eq!(
            options(root, &args)?,
            (PathBuf::from("x.md"), "- crash".to_owned())
        );
        assert!(options(root, &["--out".to_owned()]).is_err());
        assert!(options(root, &["--other".to_owned()]).is_err());
        Ok(())
    }

    #[test]
    fn a_table_without_a_steam_build_still_names_its_game_version() -> Result<(), String> {
        let build =
            build("name = \"1.0 server (1269579)\"\nside = \"server\"\ngame_version = 1269579\n")?;
        assert_eq!(build.game_version, Some(1_269_579));
        assert_eq!(build.steam_build, None);
        Ok(())
    }

    #[test]
    fn every_report_of_the_loader_reads() -> Result<(), String> {
        let builds = builds(&workspace_root().join(REPORTS))?;
        assert!(builds.iter().any(|b| b.side == "client"));
        assert!(builds.iter().any(|b| b.side == "server"));
        Ok(())
    }
}
