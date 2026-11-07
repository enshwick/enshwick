//! The install and dependency lines of the tracked Markdown that name a
//! release tag, `--tag v<version>` and `tag = "v<version>"`: each names the
//! version the workspace carries, the alpha the text describes.
//! `cargo xtask tag-lines` moves them to it after a version bump, and the
//! check fails a line naming another tag. Changelogs keep the tags of their
//! time.

use std::path::Path;

use crate::{Package, git, packages, say, workspace_root};

const USAGE: &str = "usage: cargo xtask tag-lines";
/// What comes before a tag on an install or dependency line.
const PREFIXES: [&str; 2] = ["--tag ", "tag = \""];
const CHANGELOG: &str = "CHANGELOG.md";

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
/// the workspace version's, one problem each.
pub(crate) fn check(root: &Path, packages: &[Package]) -> Vec<String> {
    let tag = match tag(packages) {
        Ok(tag) => tag,
        Err(why) => return vec![why],
    };
    let files = match markdown(root) {
        Ok(files) => files,
        Err(why) => return vec![why],
    };
    let mut problems = Vec::new();
    for file in files {
        match std::fs::read_to_string(root.join(&file)) {
            Ok(text) => problems.extend(rewrite(&text, &tag).1.into_iter().map(|line| {
                format!(
                    "{file}:{line}: names another tag than {tag}; `cargo xtask tag-lines` moves it"
                )
            })),
            Err(error) => problems.push(format!("{file}: {error}")),
        }
    }
    problems
}

/// `v` and the version of the package that names the release.
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

    #[test]
    fn an_install_line_of_an_older_alpha_moves_to_the_workspace_tag() {
        let text = "a\ncargo install --git https://x --tag v1.0.0-alpha.12 cargo-enshwick\n";
        let (moved, lines) = rewrite(text, TAG);
        assert_eq!(lines, [2]);
        assert_eq!(
            moved,
            "a\ncargo install --git https://x --tag v1.0.0-alpha.13 cargo-enshwick\n"
        );
    }

    #[test]
    fn a_dependency_line_keeps_what_follows_its_tag() {
        let text =
            "enshwick = { git = \"https://x\", tag = \"v1.0.0-alpha.9\", features = [\"typed\"] }";
        let (moved, lines) = rewrite(text, TAG);
        assert_eq!(lines, [1]);
        assert_eq!(
            moved,
            "enshwick = { git = \"https://x\", tag = \"v1.0.0-alpha.13\", features = [\"typed\"] }"
        );
    }

    #[test]
    fn lines_naming_the_workspace_tag_or_no_version_stay_as_they_are() {
        let text = "--tag v1.0.0-alpha.13\ntag = \"dev\"\nuse `--tag` here\ntag = \"v\"\n\
                    (`tag = \"v1.0.0-alpha.N\"`)\n--tag v1.0\n";
        let (moved, lines) = rewrite(text, TAG);
        assert!(lines.is_empty());
        assert_eq!(moved, text);
    }

    #[test]
    fn a_tag_ending_a_sentence_moves_and_keeps_the_period() {
        let (moved, lines) = rewrite(
            "Run it with `--tag v1.0.0-alpha.12`. Or --tag v1.0.0-alpha.12.",
            TAG,
        );
        assert_eq!(lines, [1]);
        assert_eq!(moved, format!("Run it with `--tag {TAG}`. Or --tag {TAG}."));
    }

    #[test]
    fn every_tag_of_a_line_moves() {
        let (moved, lines) = rewrite("--tag v1.0.0 and tag = \"v0.9.0\"", TAG);
        assert_eq!(lines, [1]);
        assert_eq!(moved, format!("--tag {TAG} and tag = \"{TAG}\""));
    }
}
