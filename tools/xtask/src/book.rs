//! The modding book's structure: every chapter `SUMMARY.md` links exists,
//! every chapter is in `SUMMARY.md`, and every chapter is included by a
//! doctest harness of the API crate, so its Rust examples compile.

use std::path::Path;

/// The book's sources, relative to the workspace root.
const SOURCE: &str = "book/src";
/// The files that include chapters as doctests.
const HARNESSES: [&str; 2] = ["crates/enshwick/src/book.rs", "crates/enshwick/src/test.rs"];

/// The problems with the book under `root`, one line each.
pub(crate) fn check(root: &Path) -> Vec<String> {
    let source = root.join(SOURCE);
    let summary = match std::fs::read_to_string(source.join("SUMMARY.md")) {
        Ok(text) => text,
        Err(error) => return vec![format!("{SOURCE}/SUMMARY.md: {error}")],
    };
    let mut harnessed = String::new();
    for harness in HARNESSES {
        match std::fs::read_to_string(root.join(harness)) {
            Ok(text) => harnessed.push_str(&text),
            Err(error) => return vec![format!("{harness}: {error}")],
        }
    }
    let mut chapters = Vec::new();
    if let Err(error) = collect(&source, "", &mut chapters) {
        return vec![format!("{SOURCE}: {error}")];
    }
    problems(&summary, &chapters, &harnessed, |link| {
        source.join(link).is_file()
    })
}

/// The problems, given the summary, the chapters on disk (paths relative to
/// the book's sources, `/`-separated), the harnesses' text and whether a
/// linked path exists.
fn problems(
    summary: &str,
    chapters: &[String],
    harnessed: &str,
    exists: impl Fn(&str) -> bool,
) -> Vec<String> {
    let linked = links(summary);
    let mut found = Vec::new();
    for link in &linked {
        if !exists(link) {
            found.push(format!("SUMMARY.md links {link}, which does not exist"));
        }
    }
    for chapter in chapters {
        if !linked.contains(chapter) {
            found.push(format!("{chapter} is not in SUMMARY.md"));
        }
        if !harnessed.contains(&format!("\"{chapter}\""))
            && !harnessed.contains(&format!("{SOURCE}/{chapter}\""))
        {
            found.push(format!("{chapter} is in no doctest harness"));
        }
    }
    found
}

/// The `.md` targets of the summary's Markdown links, `](path.md)`.
fn links(summary: &str) -> Vec<String> {
    summary
        .split("](")
        .skip(1)
        .filter_map(|rest| rest.split_once(')').map(|(target, _)| target))
        .filter(|target| is_markdown(target))
        .map(str::to_owned)
        .collect()
}

/// Whether `path` names a Markdown file.
fn is_markdown(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
}

/// Every `.md` file under `dir` but `SUMMARY.md`, as `prefix` plus its
/// `/`-separated path.
fn collect(dir: &Path, prefix: &str, into: &mut Vec<String>) -> std::io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = format!("{prefix}{name}");
        if entry.file_type()?.is_dir() {
            collect(&entry.path(), &format!("{path}/"), into)?;
        } else if is_markdown(&name) && name != "SUMMARY.md" {
            into.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUMMARY: &str = "# Summary\n\n- [Intro](introduction.md)\n- [Hooks](concepts/hooks.md)\n";

    fn chapters(list: &[&str]) -> Vec<String> {
        list.iter().map(|chapter| (*chapter).to_owned()).collect()
    }

    #[test]
    fn links_are_the_md_targets_of_the_summary() {
        assert_eq!(links(SUMMARY), ["introduction.md", "concepts/hooks.md"]);
        assert!(links("[site](https://example.com) and [x](y.png)").is_empty());
    }

    #[test]
    fn a_whole_book_has_no_problems() {
        let harness = r#"include_str!("../../../book/src/introduction.md")] "concepts/hooks.md""#;
        let found = problems(
            SUMMARY,
            &chapters(&["introduction.md", "concepts/hooks.md"]),
            harness,
            |_| true,
        );
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn missing_links_unlisted_chapters_and_unharnessed_chapters_are_named() {
        let harness = r#""introduction.md""#;
        let found = problems(
            SUMMARY,
            &chapters(&["introduction.md", "extra.md"]),
            harness,
            |link| link != "concepts/hooks.md",
        );
        assert_eq!(
            found,
            [
                "SUMMARY.md links concepts/hooks.md, which does not exist",
                "extra.md is not in SUMMARY.md",
                "extra.md is in no doctest harness",
            ]
        );
    }

    #[test]
    fn the_repository_book_passes() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let found = check(&root);
        assert!(found.is_empty(), "{found:?}");
    }
}
