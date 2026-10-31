//! The book's links into files of this repository and of enshwick/mods:
//! each `tree/main/<path>` or `blob/main/<path>` link names a path that
//! exists, so a renamed or split file shows here instead of on the site.
//! Nothing goes over the network: this repository's own links are checked
//! against the workspace, the mods' against a local clone of enshwick/mods
//! as checked out, and they are skipped with a note without one.

use std::path::{Path, PathBuf};

/// The book's sources, relative to the workspace root.
const SOURCE: &str = "book/src";
/// The environment variable that names a local clone of enshwick/mods.
pub(crate) const MODS_DIR: &str = "ENSH_MODS_DIR";

/// A repository the book links files of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Repo {
    /// This repository, enshwick/enshwick.
    Own,
    /// enshwick/mods.
    Mods,
}

impl Repo {
    /// Every repository, with the prefix its file links start with.
    const ALL: [(Self, &'static str); 2] = [
        (Self::Own, "https://github.com/enshwick/enshwick/"),
        (Self::Mods, "https://github.com/enshwick/mods/"),
    ];

    /// The repository's name in a problem.
    const fn name(self) -> &'static str {
        match self {
            Self::Own => "enshwick/enshwick",
            Self::Mods => "enshwick/mods",
        }
    }
}

/// A link to a file or folder on `main` of a repository.
#[derive(Debug, PartialEq, Eq)]
struct FileLink {
    /// The line it is on, from 1.
    line: usize,
    repo: Repo,
    /// The path it names, `/`-separated, without an anchor.
    path: String,
}

/// What the check found: the broken links, one line each, and a note when
/// the mods' links went unchecked.
#[derive(Debug, Default)]
pub(crate) struct Found {
    pub(crate) problems: Vec<String>,
    pub(crate) skipped: Option<String>,
}

/// The broken file links of the book under `root`; the mods' links are
/// checked against `mods`, a clone of enshwick/mods, and skipped without it.
pub(crate) fn check(root: &Path, mods: Option<&Path>) -> Found {
    let source = root.join(SOURCE);
    let mut chapters = Vec::new();
    if let Err(error) = crate::book::collect(&source, "", &mut chapters) {
        return Found {
            problems: vec![format!("{SOURCE}: {error}")],
            skipped: None,
        };
    }
    let mut found = Found::default();
    let mut mods_links = 0usize;
    for chapter in &chapters {
        let text = match std::fs::read_to_string(source.join(chapter)) {
            Ok(text) => text,
            Err(error) => {
                found.problems.push(format!("{SOURCE}/{chapter}: {error}"));
                continue;
            }
        };
        for link in file_links(&text) {
            let tree = match (link.repo, mods) {
                (Repo::Own, _) => root,
                (Repo::Mods, Some(mods)) => mods,
                (Repo::Mods, None) => {
                    mods_links += 1;
                    continue;
                }
            };
            if !tree.join(&link.path).exists() {
                found.problems.push(format!(
                    "{SOURCE}/{chapter}:{}: links {}, which {} does not have",
                    link.line,
                    link.path,
                    link.repo.name()
                ));
            }
        }
    }
    if mods_links > 0 {
        found.skipped = Some(format!(
            "{mods_links} book links into enshwick/mods not checked: set {MODS_DIR} to a clone of enshwick/mods"
        ));
    }
    found
}

/// The clone of enshwick/mods to check against: [`MODS_DIR`], else a
/// `mods` folder beside the workspace, as a clone of both repositories
/// lays them out; `None` when neither holds a `Cargo.toml`.
pub(crate) fn mods_clone(root: &Path) -> Option<PathBuf> {
    std::env::var_os(MODS_DIR)
        .map(PathBuf::from)
        .or_else(|| Some(root.join("../mods")))
        .filter(|dir| dir.join("Cargo.toml").is_file())
}

/// Every link in `text` to a file or folder on `main` of a repository of
/// [`Repo::ALL`]; links elsewhere, such as releases, are left out.
fn file_links(text: &str) -> Vec<FileLink> {
    let mut found = Vec::new();
    for (index, line) in text.lines().enumerate() {
        for (repo, prefix) in Repo::ALL {
            for (at, _) in line.match_indices(prefix) {
                let rest = &line[at + prefix.len()..];
                let end = rest
                    .find(|c: char| c.is_whitespace() || matches!(c, ')' | '>' | '"' | '\''))
                    .unwrap_or(rest.len());
                let target = &rest[..end];
                let Some(path) = ["tree/main/", "blob/main/"]
                    .into_iter()
                    .find_map(|kind| target.strip_prefix(kind))
                else {
                    continue;
                };
                let path = path.split('#').next().unwrap_or("").trim_end_matches('/');
                if !path.is_empty() {
                    found.push(FileLink {
                        line: index + 1,
                        repo,
                        path: path.to_owned(),
                    });
                }
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_links_are_the_tree_and_blob_links_on_main() {
        let text = "See [a](https://github.com/enshwick/mods/tree/main/area-loot)\n\
            and [b](https://github.com/enshwick/mods/blob/main/area-loot/src/take.rs#L4),\n\
            <https://github.com/enshwick/enshwick/tree/main/examples/waypoints/> but not\n\
            [zip](https://github.com/enshwick/enshwick/releases/download/dev/enshwick.zip).\n";
        assert_eq!(
            file_links(text),
            [
                FileLink {
                    line: 1,
                    repo: Repo::Mods,
                    path: "area-loot".to_owned()
                },
                FileLink {
                    line: 2,
                    repo: Repo::Mods,
                    path: "area-loot/src/take.rs".to_owned()
                },
                FileLink {
                    line: 3,
                    repo: Repo::Own,
                    path: "examples/waypoints".to_owned()
                },
            ]
        );
    }

    #[test]
    fn the_repository_book_links_only_files_that_exist() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let found = check(&root, mods_clone(&root).as_deref());
        assert!(found.problems.is_empty(), "{:?}", found.problems);
    }

    #[test]
    fn a_link_to_a_missing_file_is_named_with_its_chapter_and_line() {
        let dir = std::env::temp_dir().join(format!("xtask-book-links-{}", std::process::id()));
        let source = dir.join(SOURCE);
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(dir.join("mods/kept")).unwrap();
        std::fs::write(
            source.join("hooks.md"),
            "# Hooks\n\n[kept](https://github.com/enshwick/mods/tree/main/kept)\n\
             [gone](https://github.com/enshwick/mods/blob/main/gone/src/lib.rs)\n",
        )
        .unwrap();
        let found = check(&dir, Some(&dir.join("mods")));
        let skipped = check(&dir, None);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            found.problems,
            ["book/src/hooks.md:4: links gone/src/lib.rs, which enshwick/mods does not have"]
        );
        assert!(skipped.problems.is_empty());
        assert!(
            skipped
                .skipped
                .is_some_and(|note| note.starts_with("2 book links"))
        );
    }
}
