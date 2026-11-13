//! The modding book's structure: every chapter `SUMMARY.md` links exists,
//! every chapter is in `SUMMARY.md`, and every chapter is included by a
//! doctest harness of the API crate, so its Rust examples compile. Every
//! chapter opens with its page type and description and keeps its intro
//! short, a chapter that names an id links the page on how ids are
//! namespaced, some chapter names each public module of the API crate, and
//! the ABI coverage chapter lists every host table entry in order.
//! Its links and its sentence lengths are [`mod@links`] and [`mod@prose`].

pub(crate) mod links;
pub(crate) mod prose;

use std::path::Path;

use crate::FACADE;
use crate::commands::header;

/// The book's sources, relative to the workspace root.
const SOURCE: &str = "book/src";
/// The files that include chapters as doctests.
const HARNESSES: [&str; 1] = ["crates/enshwick/src/book.rs"];

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
    let mut texts = Vec::new();
    for chapter in &chapters {
        match std::fs::read_to_string(source.join(chapter)) {
            Ok(text) => {
                found.extend(page_problems(chapter, &text));
                found.extend(intro_problem(chapter, &text));
                found.extend(ids_link_problem(chapter, &text));
                texts.push(text);
            }
            Err(error) => found.push(format!("{SOURCE}/{chapter}: {error}")),
        }
    }
    match std::fs::read_to_string(root.join(API_ROOT)) {
        Ok(lib) => found.extend(unnamed_modules(&lib, &texts)),
        Err(error) => found.push(format!("{API_ROOT}: {error}")),
    }
    found
}

/// The chapter whose entries table audits every host table entry's tests.
const COVERAGE_CHAPTER: &str = "abi-coverage.md";
/// The heading of that table's section.
const COVERAGE_ENTRIES: &str = "## The entries";

/// Why the coverage chapter under `root` does not list the fields of
/// `HostApi` in the golden layout once each, in the table's order, so an
/// appended entry cannot skip the audit.
pub(crate) fn coverage_problem(root: &Path) -> Option<String> {
    let read = |path: &str| {
        std::fs::read_to_string(root.join(path)).map_err(|error| format!("{path}: {error}"))
    };
    let chapter_path = format!("{SOURCE}/{COVERAGE_CHAPTER}");
    let fields = match read(header::LAYOUT).and_then(|text| header::host_api_fields(&text)) {
        Ok(fields) => fields,
        Err(error) => return Some(error),
    };
    let chapter = match read(&chapter_path) {
        Ok(text) => text,
        Err(error) => return Some(error),
    };
    entries_mismatch(&chapter, &fields).map(|at| {
        format!(
            "{chapter_path}: row {} of its entries table should be `{}`, the next HostApi field of {}; list every field once, in order",
            at + 1,
            fields.get(at).map_or("(none)", String::as_str),
            header::LAYOUT
        )
    })
}

/// The first row of `chapter`'s entries table that is not the field of
/// `fields` at its place, or the row past its end when rows are missing.
fn entries_mismatch(chapter: &str, fields: &[String]) -> Option<usize> {
    let mut rows = chapter
        .lines()
        .skip_while(|line| *line != COVERAGE_ENTRIES)
        .skip(1)
        .take_while(|line| !line.starts_with("## "))
        .filter_map(|line| line.strip_prefix("| `")?.split_once('`'))
        .map(|(entry, _)| entry);
    (0..=fields.len()).find(|&at| rows.next() != fields.get(at).map(String::as_str))
}

/// The API crate's root, which declares its public modules.
const API_ROOT: &str = "crates/enshwick/src/lib.rs";

/// The public modules of `lib` that no chapter of `texts` names. The
/// developer site builds a page per chapter and fails on a module none
/// names, so this holds the same rule where the module is added.
fn unnamed_modules(lib: &str, texts: &[String]) -> Vec<String> {
    public_modules(lib)
        .into_iter()
        .filter(|name| !texts.iter().any(|text| names_module(text, name)))
        .map(|name| {
            format!(
                "{API_ROOT}: no chapter names the public module `{name}`; name `{FACADE}::{name}` in the chapter that documents it"
            )
        })
        .collect()
}

/// The modules `lib` declares as `pub mod <name>;` or re-exports whole from
/// an internal crate as `pub use enshwick_<crate>::<name>;`, without those
/// whose attribute lines right above hide them from the docs.
fn public_modules(lib: &str) -> Vec<&str> {
    let mut modules = Vec::new();
    let mut hidden = false;
    for line in lib.lines() {
        let line = line.trim();
        if line.starts_with("#[") {
            hidden |= line == "#[doc(hidden)]";
            continue;
        }
        let declared = line.strip_prefix("pub mod ");
        // The macro crate's lowercase items are macros, not modules.
        let reexported = line
            .strip_prefix("pub use ")
            .and_then(|rest| rest.split_once("::"))
            .filter(|(from, _)| from.starts_with("enshwick_") && *from != "enshwick_macros")
            .map(|(_, name)| name)
            .filter(|name| {
                name.bytes()
                    .all(|b| b.is_ascii_lowercase() || b == b'_' || b == b';')
            });
        if let Some(name) = declared
            .or(reexported)
            .and_then(|rest| rest.strip_suffix(';'))
            && !hidden
        {
            modules.push(name);
        }
        hidden = false;
    }
    modules
}

/// Whether `text` names the module `name` as the site reads it: a link to
/// its source, `enshwick::<name>` or a path `<name>::` through it.
fn names_module(text: &str, name: &str) -> bool {
    let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let bounded = |needle: &str, right: bool| {
        text.match_indices(needle).any(|(at, _)| {
            let before = text[..at].chars().next_back();
            let after = text[at + needle.len()..].chars().next();
            !(before.is_some_and(word) || right && after.is_some_and(word))
        })
    };
    text.contains(&format!("src/{name}.rs"))
        || text.contains(&format!("src/{name}/"))
        || bounded(&format!("{FACADE}::{name}"), true)
        || bounded(&format!("{name}::"), false)
}

/// The Diátaxis page types a chapter may declare.
const PAGE_TYPES: [&str; 4] = ["tutorial", "how-to", "reference", "explanation"];
/// The opening comment's key for the page type.
const TYPE_KEY: &str = "type";
/// The opening comment's key for the description.
const DESCRIPTION_KEY: &str = "description";
/// The characters a description has: the site's search snippet, cut by
/// search engines past the upper bound.
const DESCRIPTION_CHARS: std::ops::RangeInclusive<usize> = 120..=160;

/// The problems with the comment a chapter opens with, which mdBook hides
/// and the site reads for the page's type and description, with the same
/// grammar as the site's reader:
///
/// ```text
/// <!--
/// type: how-to
/// description: One or two sentences, 120 to 160 characters.
/// -->
/// ```
fn page_problems(chapter: &str, text: &str) -> Vec<String> {
    let at = format!("{SOURCE}/{chapter}");
    if text.starts_with("<!--\r\n") {
        return vec![format!(
            "{at}: has CRLF line endings; the site reads LF only"
        )];
    }
    let Some(block) = text
        .strip_prefix("<!--\n")
        .and_then(|rest| rest.split_once("\n-->\n"))
        .map(|(block, _)| block)
    else {
        return vec![format!(
            "{at}: opens with no <!-- type: … description: … --> comment on lines of their own"
        )];
    };
    if block.contains('\r') {
        return vec![format!(
            "{at}: has CRLF line endings; the site reads LF only"
        )];
    }
    let mut found = Vec::new();
    let (mut kind, mut description) = (None, None);
    for line in block.lines() {
        let (key, value) = line.split_once(": ").unwrap_or_default();
        let slot = match key {
            TYPE_KEY => &mut kind,
            DESCRIPTION_KEY => &mut description,
            _ => {
                found.push(format!(
                    "{at}: the opening comment has an unknown line `{line}`"
                ));
                continue;
            }
        };
        if slot.replace(value.trim()).is_some() {
            found.push(format!("{at}: the opening comment has `{key}` twice"));
        }
    }
    match kind {
        None => found.push(format!("{at}: the opening comment has no {TYPE_KEY}")),
        Some(kind) if !PAGE_TYPES.contains(&kind) => found.push(format!(
            "{at}: {TYPE_KEY} `{kind}` is none of {}",
            PAGE_TYPES.join(", ")
        )),
        Some(_) => {}
    }
    match description {
        None => found.push(format!("{at}: the opening comment has no {DESCRIPTION_KEY}")),
        Some(text) if text.contains(['`', '[', ']']) => found.push(format!(
            "{at}: the {DESCRIPTION_KEY} is plain text for search snippets, without backticks or links"
        )),
        Some(text) if !DESCRIPTION_CHARS.contains(&text.chars().count()) => {
            found.push(format!(
                "{at}: the {DESCRIPTION_KEY} has {} characters; keep it within {} to {}",
                text.chars().count(),
                DESCRIPTION_CHARS.start(),
                DESCRIPTION_CHARS.end()
            ));
        }
        Some(_) => {}
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
            "{SOURCE}/{chapter}: the intro has {words} words; keep it under {INTRO_WORDS} before the first ## heading"
        )
    })
}

/// The chapter that states how ids are namespaced, relative to [`SOURCE`].
const IDS_CHAPTER: &str = "concepts/ids.md";
/// What marks a chapter as naming an id: a full id's form or the term.
const ID_MARKS: [&str; 4] = ["`<mod>:", "`<id>:", "`<mod id>:", "content id"];

fn ids_link_problem(chapter: &str, text: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    if chapter == IDS_CHAPTER || !ID_MARKS.iter().any(|mark| lower.contains(mark)) {
        return None;
    }
    let folder = chapter.rsplit_once('/').map_or("", |(dir, _)| dir);
    let linked = link_targets(text)
        .map(|target| target.split_once('#').map_or(target, |(path, _)| path))
        .any(|target| resolve(folder, target).as_deref() == Some(IDS_CHAPTER));
    (!linked).then(|| format!("{SOURCE}/{chapter}: names an id; link {IDS_CHAPTER} where it does"))
}

/// The path, relative to [`SOURCE`], that the relative link `target` in a
/// chapter of `folder` points to; `None` when it climbs out of the book.
fn resolve(folder: &str, target: &str) -> Option<String> {
    let mut parts: Vec<&str> = folder.split('/').filter(|part| !part.is_empty()).collect();
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    Some(parts.join("/"))
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

fn links(summary: &str) -> Vec<String> {
    link_targets(summary)
        .filter(|target| is_markdown(target))
        .map(str::to_owned)
        .collect()
}

/// The targets of the Markdown links in `text`, `](target)`.
fn link_targets(text: &str) -> impl Iterator<Item = &str> {
    text.split("](")
        .skip(1)
        .filter_map(|rest| rest.split_once(')').map(|(target, _)| target))
}

/// Whether `path` names a Markdown file.
fn is_markdown(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
}

/// Every `.md` file under `dir` but `SUMMARY.md`, as `prefix` plus its
/// `/`-separated path.
pub(crate) fn collect(dir: &Path, prefix: &str, into: &mut Vec<String>) -> std::io::Result<()> {
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
            Some(
                "book/src/hooks.md: the intro has 40 words; keep it under 40 before the first ## heading"
            )
        );
        assert_eq!(intro_problem("hooks.md", &short), None);
    }

    fn page(block: &str) -> String {
        format!("<!--\n{block}\n-->\n\n# Title\n")
    }

    #[test]
    fn a_chapter_naming_an_id_without_the_ids_link_is_named() {
        assert!(ids_link_problem("a.md", "Its content id is fixed.").is_some());
        assert!(ids_link_problem("a.md", "The name is `<mod>:x`.").is_some());
        assert!(ids_link_problem("a.md", "A [content id](concepts/ids.md).").is_none());
        assert!(ids_link_problem("b/a.md", "A [content id](../concepts/ids.md).").is_none());
        assert!(ids_link_problem(IDS_CHAPTER, "A content id.").is_none());
        assert!(ids_link_problem("a.md", "No id here.").is_none());
        assert!(ids_link_problem("a.md", "Content ID, see `ids.md`.").is_some());
        assert!(ids_link_problem("a.md", "A [content id](uuids.md).").is_some());
        assert!(ids_link_problem("a.md", "A [content id](ids.md).").is_some());
        assert!(ids_link_problem("concepts/a.md", "A [content id](ids.md#x).").is_none());
        assert!(ids_link_problem("b/c/a.md", "A [content id](../../concepts/ids.md).").is_none());
    }

    #[test]
    fn an_ids_link_at_the_wrong_depth_does_not_count() {
        assert!(ids_link_problem("b/a.md", "A [content id](concepts/ids.md).").is_some());
        assert!(ids_link_problem("a.md", "A [content id](../concepts/ids.md).").is_some());
        assert!(ids_link_problem("b/c/a.md", "A [content id](../concepts/ids.md).").is_some());
    }

    #[test]
    fn a_chapter_with_a_type_and_a_fitting_description_passes() {
        let text = page(&format!("type: how-to\ndescription: {}", "d".repeat(120)));
        assert!(page_problems("hooks.md", &text).is_empty());
    }

    #[test]
    fn a_chapter_without_the_opening_comment_is_named() {
        assert_eq!(
            page_problems("hooks.md", "# Title\n"),
            [
                "book/src/hooks.md: opens with no <!-- type: … description: … --> comment on lines of their own"
            ]
        );
    }

    #[test]
    fn an_unknown_type_a_long_description_and_stray_lines_are_named() {
        let text = page(&format!(
            "type: guide\ndescription: {}\nowner: me",
            "d".repeat(161)
        ));
        assert_eq!(
            page_problems("hooks.md", &text),
            [
                "book/src/hooks.md: the opening comment has an unknown line `owner: me`",
                "book/src/hooks.md: type `guide` is none of tutorial, how-to, reference, explanation",
                "book/src/hooks.md: the description has 161 characters; keep it within 120 to 160",
            ]
        );
    }

    #[test]
    fn a_comment_missing_its_type_and_description_names_both() {
        let text = page("type:how-to");
        assert_eq!(
            page_problems("hooks.md", &text),
            [
                "book/src/hooks.md: the opening comment has an unknown line `type:how-to`",
                "book/src/hooks.md: the opening comment has no type",
                "book/src/hooks.md: the opening comment has no description",
            ]
        );
    }

    #[test]
    fn a_comment_the_site_cannot_read_is_named() {
        let fitting = format!("type: how-to\ndescription: {}", "d".repeat(120));
        let closed_mid_line = format!("<!--\n{fitting}\n--> # Title\n");
        let crlf = format!("<!--\r\n{fitting}\r\n-->\r\n");
        let one_crlf = format!(
            "<!--\ntype: how-to\r\ndescription: {}\n-->\n",
            "d".repeat(120)
        );
        assert_eq!(
            page_problems("hooks.md", &one_crlf),
            ["book/src/hooks.md: has CRLF line endings; the site reads LF only"]
        );
        assert_eq!(
            page_problems("hooks.md", &closed_mid_line),
            [
                "book/src/hooks.md: opens with no <!-- type: … description: … --> comment on lines of their own"
            ]
        );
        assert_eq!(
            page_problems("hooks.md", &crlf),
            ["book/src/hooks.md: has CRLF line endings; the site reads LF only"]
        );
    }

    #[test]
    fn a_repeated_key_markdown_and_padding_are_named() {
        let twice = page(&format!(
            "type: how-to\ndescription: {}\ndescription: {}",
            "d".repeat(130),
            "d".repeat(140)
        ));
        let marked = page(&format!(
            "type: how-to\ndescription: `Load` {}",
            "d".repeat(120)
        ));
        let padded = page(&format!("type: how-to\ndescription: {} ", "d".repeat(119)));
        assert_eq!(
            page_problems("hooks.md", &twice),
            ["book/src/hooks.md: the opening comment has `description` twice"]
        );
        assert_eq!(
            page_problems("hooks.md", &marked),
            [
                "book/src/hooks.md: the description is plain text for search snippets, without backticks or links"
            ]
        );
        assert_eq!(
            page_problems("hooks.md", &padded),
            ["book/src/hooks.md: the description has 119 characters; keep it within 120 to 160"]
        );
    }

    #[test]
    fn a_book_with_an_untyped_chapter_fails_the_check() {
        let root = std::env::temp_dir().join(format!("xtask-book-{}", std::process::id()));
        let source = root.join(SOURCE);
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(root.join("crates/enshwick/src")).unwrap();
        for harness in HARNESSES {
            std::fs::write(root.join(harness), r#""hooks.md""#).unwrap();
        }
        std::fs::write(root.join(API_ROOT), "").unwrap();
        std::fs::write(source.join("SUMMARY.md"), "- [Hooks](hooks.md)\n").unwrap();
        std::fs::write(source.join("hooks.md"), "# Hooks\n\nA hook runs first.\n").unwrap();
        let found = check(&root);
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(
            found,
            [
                "book/src/hooks.md: opens with no <!-- type: … description: … --> comment on lines of their own"
            ]
        );
    }

    #[test]
    fn a_public_module_no_chapter_names_is_named() {
        let lib = "pub mod abi;\n#[cfg(feature = \"typed\")]\npub mod quests;\nmod hook;\n#[doc(hidden)]\npub mod __private;\n";
        let texts = [String::from("Read `enshwick::abi` first.")];
        assert_eq!(
            unnamed_modules(lib, &texts),
            [
                "crates/enshwick/src/lib.rs: no chapter names the public module `quests`; name `enshwick::quests` in the chapter that documents it"
            ]
        );
    }

    #[test]
    fn a_module_is_named_by_its_source_its_crate_path_or_a_path_through_it() {
        assert!(names_module("See [it](src/quests.rs).", "quests"));
        assert!(names_module("Under src/quests/state.rs.", "quests"));
        assert!(names_module("`enshwick::quests`.", "quests"));
        assert!(names_module("use quests::Quests;", "quests"));
        assert!(!names_module("`enshwick::questsx`.", "quests"));
        assert!(!names_module("my_quests::all", "quests"));
        assert!(!names_module("enshwick.quests.all()", "quests"));
    }

    #[test]
    fn a_hidden_module_needs_no_chapter_and_the_hiding_ends_at_the_next_item() {
        let lib = "#[doc(hidden)]\n#[cfg(test)]\npub mod a;\npub mod b;\n";
        assert_eq!(public_modules(lib), ["b"]);
    }

    #[test]
    fn a_module_reexported_whole_from_an_internal_crate_is_public_and_a_macro_is_not() {
        let lib = "#[doc(inline)]\npub use enshwick_values::shape;\npub use enshwick_values::Off;\n\
                   pub use enshwick_macros::texts;\n#[doc(hidden)]\npub use enshwick_values::abi;\n";
        assert_eq!(public_modules(lib), ["shape"]);
    }

    #[test]
    fn the_repository_book_passes() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let found = check(&root);
        assert!(found.is_empty(), "{found:?}");
        assert_eq!(coverage_problem(&root), None);
    }

    fn fields(names: &[&str]) -> Vec<String> {
        names.iter().map(|&name| name.to_owned()).collect()
    }

    const ENTRIES: &str = "# ABI coverage\n\n## The entries\n\n| Entry | Test |\n| --- | --- |\n\
                           | `header` | a.rs |\n| `log` | b.rs |\n\n## Gaps\n\n| `stray` | c.rs |\n";

    #[test]
    fn an_entries_table_listing_every_field_in_order_passes() {
        assert_eq!(entries_mismatch(ENTRIES, &fields(&["header", "log"])), None);
    }

    #[test]
    fn a_missing_a_swapped_or_an_extra_entry_is_named_by_its_row() {
        assert_eq!(
            entries_mismatch(ENTRIES, &fields(&["header", "log", "chat"])),
            Some(2)
        );
        assert_eq!(
            entries_mismatch(ENTRIES, &fields(&["log", "header"])),
            Some(0)
        );
        assert_eq!(entries_mismatch(ENTRIES, &fields(&["header"])), Some(1));
    }

    #[test]
    fn the_golden_layout_gives_the_host_table_fields_in_order() {
        let layout = "abi 5.0\nHeader size 8 align 4\n    size at 0 size 4 type u32\n\
                      HostApi size 16 align 8\n    header at 0 size 8 type Header\n    log at 8 size 8 type fn\n";
        assert_eq!(
            header::host_api_fields(layout),
            Ok(fields(&["header", "log"]))
        );
    }
}
