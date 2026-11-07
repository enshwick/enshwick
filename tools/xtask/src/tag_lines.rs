//! The install and dependency lines of the tracked Markdown that name a
//! release tag, `--tag v<version>` and `tag = "v<version>"`, on a line naming
//! the platform's GitHub address: each names the version the workspace
//! carries, the alpha the text describes. A tag of another repository stays.
//! `cargo xtask tag-lines` moves them to it after a version bump, and the
//! check fails a line naming another tag. The check also fails a line that
//! installs or depends on a crate of the workspace without a pin: the
//! platform's git without a tag or commit, or crates.io while the workspace
//! carries an alpha, which crates.io never has; a link to a crate's docs.rs
//! pages fails then too. Changelogs keep the tags of their time.

use std::path::Path;

use crate::{Package, git, packages, say, workspace_root};

const USAGE: &str = "usage: cargo xtask tag-lines";
/// What comes before a tag on an install or dependency line.
const PREFIXES: [&str; 2] = ["--tag ", "tag = \""];
const CHANGELOG: &str = "CHANGELOG.md";
/// What a line names when its tag is the platform's, as the site reads it.
const REPOSITORY: &str = "github.com/enshwick/enshwick";

pub(crate) fn run(args: &[String]) -> Result<bool, String> {
    if !args.is_empty() {
        return Err(USAGE.to_owned());
    }
    let root = workspace_root();
    let tag = tag(&packages()?)?;
    for file in markdown(&root)? {
        let path = root.join(&file);
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{file}: {e}"))?;
        let (moved, lines) = rewrite(&text, &tag);
        if !lines.is_empty() {
            std::fs::write(&path, moved).map_err(|e| format!("{file}: {e}"))?;
            say(&format!("{file}: moved {} line(s) to {tag}", lines.len()));
        }
    }
    Ok(true)
}

/// A line of the tracked Markdown under `root` that names another tag than
/// the workspace version's, or installs or depends on a crate of the
/// workspace without a pin, one problem each.
pub(crate) fn check(root: &Path, packages: &[Package]) -> Vec<String> {
    let tag = match tag(packages) {
        Ok(tag) => tag,
        Err(why) => return vec![why],
    };
    let files = match markdown(root) {
        Ok(files) => files,
        Err(why) => return vec![why],
    };
    let names: Vec<&str> = packages.iter().map(|p| p.name.as_str()).collect();
    let alpha = tag.contains('-');
    let mut problems = Vec::new();
    for file in files {
        let text = match std::fs::read_to_string(root.join(&file)) {
            Ok(text) => text,
            Err(error) => {
                problems.push(format!("{file}: {error}"));
                continue;
            }
        };
        problems.extend(rewrite(&text, &tag).1.into_iter().map(|line| {
            format!("{file}:{line}: names another tag than {tag}; `cargo xtask tag-lines` moves it")
        }));
        problems.extend(unpinned(&text, &names, alpha).into_iter().map(|(line, pin)| {
            let why = match pin {
                Pin::Git => format!("takes the platform's git without a tag; name {tag}"),
                Pin::Registry => format!(
                    "takes a crate from crates.io, which has no alpha; take it from git at {tag}"
                ),
                Pin::DocsRs => "links docs.rs, which has no alpha; link the source on GitHub".to_owned(),
            };
            format!("{file}:{line}: {why}")
        }));
    }
    problems
}

/// What an install or dependency line lacks.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Pin {
    /// It names the platform's git address but no tag or commit.
    Git,
    /// It takes the crate from crates.io while the workspace carries an alpha.
    Registry,
    /// It links the crate's docs.rs pages while the workspace carries an
    /// alpha, which docs.rs never builds.
    DocsRs,
}

/// Whether `line` links the docs.rs pages of one of `names`.
fn docs_rs(line: &str, names: &[&str]) -> Option<Pin> {
    names
        .iter()
        .any(|name| {
            line.match_indices(&format!("docs.rs/{name}"))
                .any(|(at, link)| {
                    !line[at + link.len()..]
                        .starts_with(|c: char| c.is_ascii_alphanumeric() || "-_".contains(c))
                })
        })
        .then_some(Pin::DocsRs)
}

/// The lines of `text`, from 1, that install or depend on one of `names`
/// without a pin; crates.io counts as a pin only when `alpha` is false.
fn unpinned(text: &str, names: &[&str], alpha: bool) -> Vec<(usize, Pin)> {
    text.lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let pin = install_pin(line, names)
                .or_else(|| dependency_pin(line, names))
                .or_else(|| docs_rs(line, names))?;
            (pin == Pin::Git || alpha).then_some((index + 1, pin))
        })
        .collect()
}

/// What a `cargo install` of one of `names` on `line` lacks, if anything.
fn install_pin(line: &str, names: &[&str]) -> Option<Pin> {
    if !(line.contains("cargo install ") || line.contains("cargo binstall ")) {
        return None;
    }
    let words: Vec<&str> = line
        .split_whitespace()
        .map(|w| w.trim_matches('`'))
        .collect();
    let names_one = words.iter().any(|word| {
        names
            .iter()
            .any(|name| word == name || word.strip_prefix(name).is_some_and(|v| v.starts_with('@')))
    });
    if !names_one {
        return None;
    }
    let after = |flag: &str| {
        words
            .iter()
            .position(|w| *w == flag || w.starts_with(&format!("{flag}=")))
            .map(|at| {
                words[at]
                    .split_once('=')
                    .map_or(words.get(at + 1).copied(), |(_, v)| Some(v))
            })
    };
    match after("--git") {
        Some(git) if !git.unwrap_or_default().contains(REPOSITORY) => None,
        Some(_) => (after("--tag").is_none() && after("--rev").is_none()).then_some(Pin::Git),
        None if after("--path").is_some() => None,
        None => Some(Pin::Registry),
    }
}

/// What a dependency on one of `names` on `line` lacks, if anything: a
/// `name = "<version>"` or `name = { ... }` entry. A value that is no version
/// requirement, such as a log level, is no dependency.
fn dependency_pin(line: &str, names: &[&str]) -> Option<Pin> {
    names.iter().find_map(|name| {
        line.match_indices(name).find_map(|(at, _)| {
            let before = line[..at].chars().next_back();
            if before.is_some_and(|c| c.is_ascii_alphanumeric() || "-_@:./".contains(c)) {
                return None;
            }
            let value = line[at + name.len()..].trim_start().strip_prefix('=')?;
            if value.starts_with('=') {
                return None;
            }
            let value = value.trim_start();
            if let Some(text) = value.strip_prefix('"') {
                let requirement = text.split('"').next()?;
                return semver::VersionReq::parse(requirement)
                    .is_ok()
                    .then_some(Pin::Registry);
            }
            let table = value.strip_prefix('{')?;
            let table = table.split('}').next().unwrap_or(table);
            if table.contains("workspace = true") || field(table, "path").is_some() {
                return None;
            }
            match field(table, "git") {
                Some(git) if !git.contains(REPOSITORY) => None,
                Some(_) => (field(table, "tag").is_none() && field(table, "rev").is_none())
                    .then_some(Pin::Git),
                None => field(table, "version").map(|_| Pin::Registry),
            }
        })
    })
}

/// The string value of `key` in the inline table `table`.
fn field<'a>(table: &'a str, key: &str) -> Option<&'a str> {
    table.match_indices(key).find_map(|(at, _)| {
        let before = table[..at].chars().next_back();
        if before.is_some_and(|c| !(c.is_whitespace() || c == ',')) {
            return None;
        }
        let value = table[at + key.len()..].trim_start().strip_prefix('=')?;
        value.trim_start().strip_prefix('"')?.split('"').next()
    })
}

/// `v` and the version of the package that names the release.
///
/// Plain `v<version>` on purpose: alpha.yml tags each alpha so, and only
/// alphas need a git tag in the text. release-plz's `<package>-v<version>`
/// tags mark real releases, which go to crates.io, so their lines name a
/// crates.io version and no tag.
fn tag(packages: &[Package]) -> Result<String, String> {
    packages
        .iter()
        .find(|p| p.name == crate::notes::LOADER)
        .map(|p| format!("v{}", p.version))
        .ok_or_else(|| format!("no package {}", crate::notes::LOADER))
}

/// The tracked Markdown files but changelogs, relative to `root`.
fn markdown(root: &Path) -> Result<Vec<String>, String> {
    let root = root.to_str().ok_or("the workspace root is not UTF-8")?;
    Ok(git(&["-C", root, "ls-files", "*.md"])
        .ok_or("git ls-files failed")?
        .lines()
        .filter(|file| !file.ends_with(CHANGELOG))
        .map(str::to_owned)
        .collect())
}

/// `text` with every tag line naming `tag`, and the lines that named
/// another, from 1.
fn rewrite(text: &str, tag: &str) -> (String, Vec<usize>) {
    let mut out = String::with_capacity(text.len());
    let mut moved = Vec::new();
    for (index, line) in text.split_inclusive('\n').enumerate() {
        if !line.contains(REPOSITORY) {
            out.push_str(line);
            continue;
        }
        let mut rest = line;
        let mut changed = false;
        while let Some((at, prefix)) = PREFIXES
            .iter()
            .filter_map(|prefix| rest.find(prefix).map(|at| (at, *prefix)))
            .min()
        {
            let start = at + prefix.len();
            let len = rest[start..]
                .find(|c: char| !(c.is_ascii_alphanumeric() || ".-+".contains(c)))
                .unwrap_or(rest.len() - start);
            // A sentence may end on the tag: its final period is prose.
            let value = rest[start..start + len].trim_end_matches(['.', '-', '+']);
            let len = value.len();
            out.push_str(&rest[..start]);
            if is_version_tag(value) && value != tag {
                out.push_str(tag);
                changed = true;
            } else {
                out.push_str(value);
            }
            rest = &rest[start + len..];
        }
        out.push_str(rest);
        if changed {
            moved.push(index + 1);
        }
    }
    (out, moved)
}

/// Whether `value` is a version's tag: `v` and a SemVer version without a
/// capital, since a placeholder such as `v1.0.0-alpha.N` names none.
fn is_version_tag(value: &str) -> bool {
    value.strip_prefix('v').is_some_and(|version| {
        !version.contains(|c: char| c.is_ascii_uppercase())
            && semver::Version::parse(version).is_ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TAG: &str = "v1.0.0-alpha.13";

    fn loader(version: &str) -> Vec<Package> {
        vec![Package {
            name: crate::notes::LOADER.to_owned(),
            version: version.to_owned(),
            dir: String::new(),
            deps: Vec::new(),
            windows_only: false,
            windows_tests: false,
        }]
    }

    /// A fresh git repository of its own for this test process, with `files`
    /// as `(path, text)`, each added to its index; a reused process id finds
    /// no index of an earlier run.
    fn repository(files: &[(&str, &str)]) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("xtask-tag-lines-{}", std::process::id()));
        if root.exists() {
            std::fs::remove_dir_all(&root).unwrap();
        }
        std::fs::create_dir_all(root.join("book")).unwrap();
        let root_text = root.to_str().unwrap();
        assert!(git(&["-C", root_text, "init", "--quiet"]).is_some());
        for (path, text) in files {
            std::fs::write(root.join(path), text).unwrap();
            assert!(git(&["-C", root_text, "add", path]).is_some());
        }
        root
    }

    #[test]
    fn the_check_names_each_tracked_line_of_another_tag_but_changelogs() {
        let line = "cargo install --git https://github.com/enshwick/enshwick --tag v1.0.0-alpha.12 cargo-enshwick\n";
        let root = repository(&[
            ("README.md", line),
            ("book/a.md", &format!("a\n{line}")),
            ("book/ok.md", &line.replace("alpha.12", "alpha.13")),
            ("CHANGELOG.md", line),
            ("notes.txt", line),
        ]);
        let problems = check(&root, &loader("1.0.0-alpha.13"));
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(
            problems,
            [
                "README.md:1: names another tag than v1.0.0-alpha.13; `cargo xtask tag-lines` moves it",
                "book/a.md:2: names another tag than v1.0.0-alpha.13; `cargo xtask tag-lines` moves it",
            ]
        );
    }

    const NAMES: [&str; 2] = ["enshwick", "cargo-enshwick"];

    #[test]
    fn the_check_names_an_unpinned_line_of_a_tracked_file() {
        let root = repository(&[("README.md", "a\n```toml\nenshwick = \"1\"\n```\n")]);
        let mut packages = loader("1.0.0-alpha.13");
        let mut api = packages[0].clone();
        api.name = "enshwick".to_owned();
        packages.push(api);
        let problems = check(&root, &packages);
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(
            problems,
            [
                "README.md:3: takes a crate from crates.io, which has no alpha; take it from git at v1.0.0-alpha.13"
            ]
        );
    }

    #[test]
    fn crates_io_lines_are_unpinned_only_while_the_workspace_carries_an_alpha() {
        let text = "cargo install cargo-enshwick\n\
                    enshwick = \"1\"\n\
                    `enshwick = { version = \"1.0\", features = [\"test\"] }`\n\
                    cargo binstall cargo-enshwick@1.0.0\n";
        assert_eq!(
            unpinned(text, &NAMES, true),
            [
                (1, Pin::Registry),
                (2, Pin::Registry),
                (3, Pin::Registry),
                (4, Pin::Registry)
            ]
        );
        assert!(unpinned(text, &NAMES, false).is_empty());
    }

    #[test]
    fn a_docs_rs_link_of_a_workspace_crate_fails_only_while_the_workspace_carries_an_alpha() {
        let text = "[API](https://docs.rs/enshwick)\n\
                    [Load](https://docs.rs/enshwick/latest/enshwick/struct.Load.html)\n\
                    [cli](https://docs.rs/cargo-enshwick/)\n\
                    [macros](https://docs.rs/enshwick-macros)\n\
                    [arc-swap](https://docs.rs/arc-swap)\n";
        assert_eq!(
            unpinned(text, &NAMES, true),
            [(1, Pin::DocsRs), (2, Pin::DocsRs), (3, Pin::DocsRs)]
        );
        assert!(unpinned(text, &NAMES, false).is_empty());
    }

    #[test]
    fn a_line_of_the_platform_git_without_a_tag_or_commit_is_unpinned() {
        let text = "cargo install --git https://github.com/enshwick/enshwick cargo-enshwick\n\
                    enshwick = { git = \"https://github.com/enshwick/enshwick\" }\n\
                    enshwick = { git = \"https://github.com/enshwick/enshwick\", branch = \"main\" }\n";
        assert_eq!(
            unpinned(text, &NAMES, false),
            [(1, Pin::Git), (2, Pin::Git), (3, Pin::Git)]
        );
    }

    #[test]
    fn pinned_inherited_local_and_unrelated_lines_pass() {
        let text = [
            "cargo install --git https://github.com/enshwick/enshwick --tag v1.0.0-alpha.13 cargo-enshwick",
            "cargo install --git https://github.com/enshwick/enshwick --rev abc cargo-enshwick",
            "cargo install --path crates/cargo-enshwick",
            "cargo install --path crates/cargo-enshwick cargo-enshwick",
            "cargo install --git https://github.com/someone/fork cargo-enshwick",
            "enshwick = { git = \"https://github.com/someone/fork\" }",
            "enshwick = { path = \"../enshwick\", version = \"1\" }",
            "enshwick = { version = \"1\",path = \"../enshwick\" }",
            "cargo install --git https://github.com/rust-cross/cargo-xwin cargo-xwin",
            "cargo install cargo-nextest",
            "enshwick = { git = \"https://github.com/enshwick/enshwick\", tag = \"v1.0.0-alpha.N\" }",
            "enshwick = { git = \"https://github.com/enshwick/enshwick\", rev = \"<commit>\" }",
            "  enshwick = { workspace = true, features = [\"unstable\"] }",
            "enshwick = { path = \"../enshwick\" }",
            "enshwick.workspace = true",
            "log_levels = { enshwick = \"debug\" }",
            "luau-lsp analyze --definitions:@enshwick=types/enshwick.d.luau main.luau",
            "if enshwick == 1",
            "my-enshwick = \"1\"",
            "other = { git = \"https://github.com/someone/else\" }",
        ]
        .join("\n");
        assert!(unpinned(&text, &NAMES, true).is_empty());
    }

    #[test]
    fn a_workspace_without_the_loader_is_a_problem() {
        let mut packages = loader("1.0.0");
        packages[0].name = "other".to_owned();
        assert_eq!(
            tag(&packages),
            Err(format!("no package {}", crate::notes::LOADER))
        );
    }

    #[test]
    fn an_install_line_of_an_older_alpha_moves_to_the_workspace_tag() {
        let text = "a\ncargo install --git https://github.com/enshwick/enshwick --tag v1.0.0-alpha.12 cargo-enshwick\n";
        let (moved, lines) = rewrite(text, TAG);
        assert_eq!(lines, [2]);
        assert_eq!(
            moved,
            "a\ncargo install --git https://github.com/enshwick/enshwick --tag v1.0.0-alpha.13 cargo-enshwick\n"
        );
    }

    #[test]
    fn a_dependency_line_keeps_what_follows_its_tag() {
        let text = "enshwick = { git = \"https://github.com/enshwick/enshwick\", tag = \"v1.0.0-alpha.9\", features = [\"typed\"] }";
        let (moved, lines) = rewrite(text, TAG);
        assert_eq!(lines, [1]);
        assert_eq!(
            moved,
            "enshwick = { git = \"https://github.com/enshwick/enshwick\", tag = \"v1.0.0-alpha.13\", features = [\"typed\"] }"
        );
    }

    #[test]
    fn lines_naming_the_workspace_tag_or_no_version_stay_as_they_are() {
        let text = [
            "--tag v1.0.0-alpha.13",
            "tag = \"dev\"",
            "use `--tag` here",
            "tag = \"v\"",
            "(`tag = \"v1.0.0-alpha.N\"`)",
            "--tag v1.0",
        ]
        .map(|line| format!("{REPOSITORY} {line}\n"))
        .concat();
        let (moved, lines) = rewrite(&text, TAG);
        assert!(lines.is_empty());
        assert_eq!(moved, text);
    }

    #[test]
    fn a_tag_of_another_repository_stays() {
        let text = "cargo install --git https://github.com/rust-cross/cargo-xwin --tag v0.18.4 cargo-xwin\n\
                    run `--tag v1.0.0-alpha.12` without an address\n";
        let (moved, lines) = rewrite(text, TAG);
        assert!(lines.is_empty());
        assert_eq!(moved, text);
    }

    #[test]
    fn a_tag_ending_a_sentence_moves_and_keeps_the_period() {
        let (moved, lines) = rewrite(
            &format!("From {REPOSITORY} with `--tag v1.0.0-alpha.12`. Or --tag v1.0.0-alpha.12."),
            TAG,
        );
        assert_eq!(lines, [1]);
        assert_eq!(
            moved,
            format!("From {REPOSITORY} with `--tag {TAG}`. Or --tag {TAG}.")
        );
    }

    #[test]
    fn every_tag_of_a_line_moves() {
        let (moved, lines) = rewrite(
            &format!("{REPOSITORY} --tag v1.0.0 and tag = \"v0.9.0\""),
            TAG,
        );
        assert_eq!(lines, [1]);
        assert_eq!(
            moved,
            format!("{REPOSITORY} --tag {TAG} and tag = \"{TAG}\"")
        );
    }
}
