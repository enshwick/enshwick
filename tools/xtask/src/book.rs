//! The modding book's structure: every chapter `SUMMARY.md` links exists,
//! every chapter is in `SUMMARY.md`, and every chapter is included by a
//! doctest harness of the API crate, so its Rust examples compile. Every
//! chapter's intro stays short.

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
    let mut found = problems(&summary, &chapters, &harnessed, |link| {
        source.join(link).is_file()
    });
    for chapter in &chapters {
        match std::fs::read_to_string(source.join(chapter)) {
            Ok(text) => found.extend(intro_problem(chapter, &text)),
            Err(error) => found.push(format!("{SOURCE}/{chapter}: {error}")),
        }
    }
    found
}

/// A chapter's intro, the prose between its title and its first section,
/// stays under this many words: on the site it is the page's lead.
const INTRO_WORDS: usize = 40;

/// The problem with a chapter's intro, if it is not under [`INTRO_WORDS`].
fn intro_problem(chapter: &str, text: &str) -> Option<String> {
    let words = intro_words(text);
    (words >= INTRO_WORDS).then(|| {
        format!(
            "{chapter}: the intro has {words} words; keep it under {INTRO_WORDS} before the first ## heading"
        )
    })
}

/// The words between the `# ` title and the first deeper heading, outside
/// code fences and HTML comments. A word is a run of non-space characters
/// with a letter or digit in it.
fn intro_words(text: &str) -> usize {
    let mut prose = String::new();
    let mut fence: Option<&str> = None;
    let mut titled = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        let marker = ["```", "~~~"]
            .into_iter()
            .find(|marker| trimmed.starts_with(marker));
        match (fence, marker) {
            (None, Some(marker)) => {
                fence = Some(marker);
                continue;
            }
            (Some(open), Some(marker)) if open == marker => {
                fence = None;
                continue;
            }
            (Some(_), _) => continue,
            (None, None) => {}
        }
        if !titled {
            titled = trimmed.starts_with("# ");
            continue;
        }
        if trimmed.starts_with("##") {
            break;
        }
        prose.push_str(line);
        prose.push('\n');
    }
    let mut words = 0;
    let mut rest = prose.as_str();
    while !rest.is_empty() {
        let (text, after) = match rest.split_once("<!--") {
            Some((text, comment)) => (
                text,
                comment.split_once("-->").map_or("", |(_, after)| after),
            ),
            None => (rest, ""),
        };
        words += text
            .split_whitespace()
            .filter(|word| word.chars().any(char::is_alphanumeric))
            .count();
        rest = after;
    }
    words
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
    fn the_intro_counts_prose_between_the_title_and_the_first_section() {
        let text = "Before the title.\n\n# Title\n\nOne two three.\n\n```rust\nlet code = 1;\n```\n\n<!-- a note\nover lines -->\n- four, five\n| six | --- |\n\n## Section\n\nSeven eight.\n";
        assert_eq!(intro_words(text), 6);
    }

    #[test]
    fn a_heading_inside_code_does_not_end_the_intro() {
        let text = "# Title\n\nOne.\n\n```toml\n## not a heading\n```\n\nTwo.\n\n## Section\n";
        assert_eq!(intro_words(text), 2);
    }

    #[test]
    fn an_intro_of_forty_words_is_named() {
        let long = format!("# Title\n\n{}\n\n## Next\n", ["word"; 40].join(" "));
        let short = format!("# Title\n\n{}\n\n## Next\n", ["word"; 39].join(" "));
        assert_eq!(
            intro_problem("hooks.md", &long).as_deref(),
            Some("hooks.md: the intro has 40 words; keep it under 40 before the first ## heading")
        );
        assert_eq!(intro_problem("hooks.md", &short), None);
    }

    #[test]
    fn the_repository_book_passes() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let found = check(&root);
        assert!(found.is_empty(), "{found:?}");
    }
}
