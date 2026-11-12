//! The book's sentence lengths, as the house style for developer text sets
//! them: no sentence over [`MAX_WORDS`] words or [`MAX_CODE_SPANS`] code
//! spans, and a chapter's sentences averaging [`AVERAGE_WORDS`] words or
//! fewer. A code span counts as one word. Code blocks, headings and HTML
//! comments hold no sentences; list items and table cells end one. A list
//! item or table cell of code spans alone, such as a table's column of
//! paths, is a list of names, not a sentence, and table cells, mostly
//! fragments, leave the average alone.

use std::path::Path;

/// The book's sources, relative to the workspace root.
const SOURCE: &str = "book/src";
/// The most words a sentence has.
const MAX_WORDS: usize = 25;
/// The most words a chapter's sentences average.
const AVERAGE_WORDS: usize = 20;
/// The most code spans a sentence holds.
const MAX_CODE_SPANS: usize = 2;
/// The words of a sentence a problem quotes.
const QUOTED_WORDS: usize = 6;
/// Stands for a code span in flattened prose: one word, never a sentence end.
const CODE: char = '\u{E000}';
/// Abbreviations whose period never ends a sentence.
const ABBREVIATIONS: [&str; 5] = ["e.g.", "i.e.", "vs.", "cf.", "approx."];
/// An abbreviation whose period ends a sentence only before a capital.
const ET_CETERA: &str = "etc.";

/// The sentence-length problems of the book under `root`, one line each.
pub(crate) fn check(root: &Path) -> Vec<String> {
    let source = root.join(SOURCE);
    let mut chapters = Vec::new();
    if let Err(error) = super::collect(&source, "", &mut chapters) {
        return vec![format!("{SOURCE}: {error}")];
    }
    let mut found = Vec::new();
    for chapter in &chapters {
        match std::fs::read_to_string(source.join(chapter)) {
            Ok(text) => found.extend(problems(&format!("{SOURCE}/{chapter}"), &text)),
            Err(error) => found.push(format!("{SOURCE}/{chapter}: {error}")),
        }
    }
    found
}

/// A sentence of a chapter.
#[derive(Debug, PartialEq, Eq)]
struct Sentence {
    /// The line it starts on, from 1.
    line: usize,
    words: usize,
    code_spans: usize,
    /// Its first words, code spans shown as `` `…` ``.
    opening: String,
    /// Whether it sits in a table cell.
    in_cell: bool,
}

/// What a prose block of a chapter is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Block {
    Paragraph,
    Item,
    Cell,
}

/// The problems with the sentences of the chapter at `at`.
fn problems(at: &str, text: &str) -> Vec<String> {
    let sentences = sentences(text);
    let mut found = Vec::new();
    for sentence in &sentences {
        let Sentence {
            line,
            words,
            code_spans,
            opening,
            ..
        } = sentence;
        if *words > MAX_WORDS {
            found.push(format!(
                "{at}:{line}: a sentence has {words} words; keep it to {MAX_WORDS}: \"{opening} …\""
            ));
        }
        if *code_spans > MAX_CODE_SPANS {
            found.push(format!(
                "{at}:{line}: a sentence has {code_spans} code spans; keep it to {MAX_CODE_SPANS}: \"{opening} …\""
            ));
        }
    }
    let counted: Vec<usize> = sentences
        .iter()
        .filter(|sentence| !sentence.in_cell)
        .map(|sentence| sentence.words)
        .collect();
    let total: usize = counted.iter().sum();
    if total > AVERAGE_WORDS * counted.len() {
        found.push(format!(
            "{at}: {} sentences have {total} words, above an average of {AVERAGE_WORDS}",
            counted.len()
        ));
    }
    found
}

/// The sentences of a chapter, in order.
fn sentences(text: &str) -> Vec<Sentence> {
    blocks(text)
        .into_iter()
        .flat_map(|(line, kind, block)| split(line, kind, &flatten(&block)))
        .collect()
}

/// The prose blocks of a chapter with the line each starts on: paragraphs,
/// list items and table cells, outside code fences, headings and comments.
fn blocks(text: &str) -> Vec<(usize, Block, String)> {
    let mut found = Vec::new();
    let mut current: Option<(usize, Block, String)> = None;
    let mut fence: Option<(char, usize)> = None;
    let mut comment = false;
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let trimmed = raw.trim_start().trim_start_matches(['>', ' ']);
        if comment {
            comment = !raw.contains("-->");
            continue;
        }
        let marker = fence_marker(trimmed);
        match (fence, marker) {
            (None, Some((mark, run, _))) => {
                found.extend(current.take());
                fence = Some((mark, run));
                continue;
            }
            (Some((open, length)), Some((mark, run, true))) if open == mark && run >= length => {
                fence = None;
                continue;
            }
            (Some(_), _) => continue,
            (None, None) => {}
        }
        if let Some(rest) = trimmed.strip_prefix("<!--") {
            found.extend(current.take());
            comment = !rest.contains("-->");
            continue;
        }
        if trimmed.is_empty()
            || is_heading(trimmed)
            || trimmed.starts_with("{{#")
            || is_reference_definition(trimmed)
        {
            found.extend(current.take());
            continue;
        }
        if trimmed.starts_with('|') {
            found.extend(current.take());
            if !trimmed.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ')) {
                found.extend(cells(trimmed).map(|cell| (line, Block::Cell, cell.to_owned())));
            }
            continue;
        }
        let paragraph = matches!(current, Some((_, Block::Paragraph, _)));
        if let Some(item) = list_item(trimmed, paragraph) {
            found.extend(current.take());
            current = Some((line, Block::Item, item.to_owned()));
            continue;
        }
        match &mut current {
            Some((_, _, block)) => {
                block.push('\n');
                block.push_str(trimmed);
            }
            None => current = Some((line, Block::Paragraph, trimmed.to_owned())),
        }
    }
    found.extend(current);
    found
}

/// The character, length and bareness of the code fence `line` starts
/// with, if any: a run of three or more backticks or tildes, bare when
/// nothing but spaces follows, as a closing fence must be.
fn fence_marker(line: &str) -> Option<(char, usize, bool)> {
    let mark = line.chars().next().filter(|c| matches!(c, '`' | '~'))?;
    let run = line.chars().take_while(|&c| c == mark).count();
    (run >= 3).then(|| (mark, run, line[run..].trim().is_empty()))
}

/// Whether `line` is an ATX heading: one to six `#` and then a space or
/// the line's end.
fn is_heading(line: &str) -> bool {
    let hashes = line.chars().take_while(|&c| c == '#').count();
    (1..=6).contains(&hashes) && line[hashes..].chars().next().is_none_or(|c| c == ' ')
}

/// Whether `line` defines a link reference, `[name]: target`.
fn is_reference_definition(line: &str) -> bool {
    line.starts_with('[') && line.contains("]: ")
}

/// The text after a list item's marker (`-`, `*`, `+`, `1.`, `1)`), if
/// `line` starts one. Inside a paragraph only a bullet or `1` starts one,
/// as Markdown has it, so a wrapped line opening on `255.` stays prose.
fn list_item(line: &str, in_paragraph: bool) -> Option<&str> {
    if let Some(rest) = line
        .strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))
        .or_else(|| line.strip_prefix("+ "))
    {
        return Some(rest);
    }
    let digits = line.find(|c: char| !c.is_ascii_digit())?;
    let rest = &line[digits..];
    (digits > 0 && (!in_paragraph || &line[..digits] == "1"))
        .then(|| rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")))
        .flatten()
}

/// The cells of a table row, split at pipes outside code spans.
fn cells(row: &str) -> impl Iterator<Item = &str> {
    let mut cells = Vec::new();
    let mut start = 0;
    let mut in_code = false;
    for (at, c) in row.char_indices() {
        match c {
            '`' => in_code = !in_code,
            '|' if !in_code => {
                cells.push(&row[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    cells.push(&row[start..]);
    cells
        .into_iter()
        .map(str::trim)
        .filter(|cell| !cell.is_empty())
}

/// A block's prose with Markdown removed: each code span becomes [`CODE`],
/// a link its text, an image and an HTML tag nothing, an autolink one word.
fn flatten(block: &str) -> String {
    let chars: Vec<char> = block.chars().collect();
    let mut out = String::new();
    let mut at = 0;
    while let Some(&c) = chars.get(at) {
        let next = chars.get(at + 1).copied();
        at = match (c, next) {
            ('`', _) => {
                let run = chars[at..].iter().take_while(|&&c| c == '`').count();
                if let Some(end) = closing_run(&chars, at + run, run) {
                    out.push(CODE);
                    end + run
                } else {
                    out.extend(&chars[at..at + run]);
                    at + run
                }
            }
            ('!', Some('[')) => skip_link(&chars, at + 1),
            (']', Some('(')) => skip_past(&chars, at + 1, '(', ')'),
            (']', Some('[')) => skip_past(&chars, at + 1, '[', ']'),
            ('[' | ']', _) => at + 1,
            ('<', Some(next))
                if (next.is_ascii_alphabetic() || matches!(next, '/' | '!'))
                    && chars[at..].contains(&'>') =>
            {
                let end = chars[at..]
                    .iter()
                    .position(|&c| c == '>')
                    .map_or(chars.len(), |offset| at + offset + 1);
                if chars[at + 1..end].starts_with(&['h', 't', 't', 'p']) {
                    out.push_str("link");
                }
                end
            }
            _ => {
                out.push(c);
                at + 1
            }
        };
    }
    out
}

/// The index of the backtick run of exactly `run` that closes a code span
/// opened before `from`.
fn closing_run(chars: &[char], from: usize, run: usize) -> Option<usize> {
    let mut at = from;
    while at < chars.len() {
        if chars[at] == '`' {
            let length = chars[at..].iter().take_while(|&&c| c == '`').count();
            if length == run {
                return Some(at);
            }
            at += length;
        } else {
            at += 1;
        }
    }
    None
}

/// The index past an image, `[alt](target)`, its `!` already behind `at`.
fn skip_link(chars: &[char], at: usize) -> usize {
    let close = skip_past(chars, at, '[', ']');
    if chars.get(close) == Some(&'(') {
        skip_past(chars, close, '(', ')')
    } else {
        close
    }
}

/// The index past the bracket that closes the `open` at `at`, nesting
/// counted; the end of `chars` when nothing closes it.
fn skip_past(chars: &[char], at: usize, open: char, close: char) -> usize {
    let mut depth = 0usize;
    for (offset, &c) in chars[at..].iter().enumerate() {
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return at + offset + 1;
            }
        }
    }
    chars.len()
}

/// The sentences of a flattened block that starts on `line`.
fn split(line: usize, kind: Block, prose: &str) -> Vec<Sentence> {
    let chars: Vec<char> = prose.chars().collect();
    let mut found = Vec::new();
    let mut start = 0;
    let mut at = 0;
    while at < chars.len() {
        if matches!(chars[at], '.' | '!' | '?') {
            let mut end = at + 1;
            while chars
                .get(end)
                .is_some_and(|&c| matches!(c, ')' | '"' | '\'' | '*' | '_' | '”' | '’'))
            {
                end += 1;
            }
            let at_space = chars.get(end).is_none_or(|c| c.is_whitespace());
            if at_space && ends_sentence(&chars, start, at, end) {
                found.extend(sentence(line, kind, &chars, start, end));
                start = end;
            }
            at = end;
        } else {
            at += 1;
        }
    }
    found.extend(sentence(line, kind, &chars, start, chars.len()));
    found
}

/// Whether the mark at `mark`, closed at `end`, ends the sentence begun at
/// `start` rather than an abbreviation.
fn ends_sentence(chars: &[char], start: usize, mark: usize, end: usize) -> bool {
    let word_start = chars[start..mark]
        .iter()
        .rposition(|c| c.is_whitespace())
        .map_or(start, |offset| start + offset + 1);
    let word: String = chars[word_start..=mark]
        .iter()
        .skip_while(|c| matches!(c, '(' | '"' | '*' | '_'))
        .flat_map(|c| c.to_lowercase())
        .collect();
    if ABBREVIATIONS.contains(&word.as_str()) {
        return false;
    }
    if word == ET_CETERA {
        return chars[end..]
            .iter()
            .find(|c| !c.is_whitespace())
            .is_none_or(|c| c.is_uppercase());
    }
    true
}

/// The sentence `chars[start..end]` of a block of `kind` starting on
/// `line`, if it has a word; in a list item or table cell, a prose word.
fn sentence(
    line: usize,
    kind: Block,
    chars: &[char],
    start: usize,
    end: usize,
) -> Option<Sentence> {
    let text: String = chars[start..end].iter().collect();
    let words: Vec<&str> = text
        .split_whitespace()
        .filter(|word| word.chars().any(|c| c.is_alphanumeric() || c == CODE))
        .collect();
    let names_only = words
        .iter()
        .all(|word| !word.chars().any(char::is_alphanumeric));
    if words.is_empty() || (names_only && kind != Block::Paragraph) {
        return None;
    }
    let leading = chars[..start].iter().filter(|&&c| c == '\n').count()
        + text
            .chars()
            .take_while(|c| c.is_whitespace())
            .filter(|&c| c == '\n')
            .count();
    let opening = words[..words.len().min(QUOTED_WORDS)]
        .join(" ")
        .replace(CODE, "`…`");
    Some(Sentence {
        line: line + leading,
        words: words.len(),
        code_spans: text.chars().filter(|&c| c == CODE).count(),
        opening,
        in_cell: kind == Block::Cell,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(text: &str) -> Vec<(usize, usize)> {
        sentences(text)
            .into_iter()
            .map(|sentence| (sentence.words, sentence.code_spans))
            .collect()
    }

    #[test]
    fn periods_question_and_exclamation_marks_end_sentences() {
        assert_eq!(
            counts("One two three. Four five? Six!\nSeven eight."),
            [(3, 0), (2, 0), (1, 0), (2, 0)]
        );
    }

    #[test]
    fn a_code_span_is_one_word_and_its_periods_end_nothing() {
        assert_eq!(
            counts("Call `world.time(). Then` once. Use ``a ` b`` too."),
            [(3, 1), (3, 1)]
        );
    }

    #[test]
    fn abbreviations_and_version_numbers_do_not_end_a_sentence() {
        assert_eq!(
            counts("Pin a version, e.g. 1.2.3, i.e. the newest. A list, etc. ends here."),
            [(8, 0), (5, 0)]
        );
        assert_eq!(counts("Hooks, timers etc. Then more."), [(3, 0), (2, 0)]);
    }

    #[test]
    fn code_blocks_headings_and_comments_hold_no_sentences() {
        let text = "<!--\ntype: how-to\ndescription: a b c.\n-->\n\n# A long title here\n\n```rust\n// One two. Three four.\nlet x = 1;\n```\n\n~~~\n``` not a fence end\n~~~\n\n> ```text\n> One two. Three.\n> ```\n\n<!-- one -->\nWord.\n";
        assert_eq!(counts(text), [(1, 0)]);
    }

    #[test]
    fn list_items_and_table_cells_end_a_sentence() {
        let text = "Pick one:\n\n- a first item\n  wrapped on\n- `second` item\n1. third\n\n| Name | What it does |\n| --- | :-: |\n| `a|b` cell | runs once |\n";
        assert_eq!(
            counts(text),
            [
                (2, 0),
                (5, 0),
                (2, 1),
                (1, 0),
                (1, 0),
                (3, 0),
                (2, 1),
                (2, 0)
            ]
        );
    }

    #[test]
    fn links_count_their_text_and_images_count_nothing() {
        assert_eq!(
            counts(
                "See [the `Hook` page](hooks.md#a.b) and [ref][r]. ![A b. C](x.png) Read <https://a.b/c> or <kbd>Ctrl</kbd>."
            ),
            [(6, 1), (4, 0)]
        );
    }

    #[test]
    fn a_sentence_wrapped_over_lines_starts_on_its_first_line() {
        let found = sentences("Intro.\n\nOne two.\nThree\nfour.\n");
        let lines: Vec<usize> = found.iter().map(|sentence| sentence.line).collect();
        assert_eq!(lines, [1, 3, 4]);
        let found = sentences("One two. Three\nfour. Five.\n");
        let lines: Vec<usize> = found.iter().map(|sentence| sentence.line).collect();
        assert_eq!(lines, [1, 1, 2]);
    }

    #[test]
    fn long_sentences_and_many_code_spans_are_named() {
        let long = format!("{}.", ["word"; 26].join(" "));
        let text = format!("{long}\nUse `a`, `b` and `c`.\n");
        assert_eq!(
            problems("book/src/x.md", &text),
            [
                "book/src/x.md:1: a sentence has 26 words; keep it to 25: \"word word word word word word …\"",
                "book/src/x.md:2: a sentence has 3 code spans; keep it to 2: \"Use `…`, `…` and `…`. …\"",
            ]
        );
    }

    #[test]
    fn a_chapter_averaging_over_twenty_words_is_named() {
        let sentence = format!("{}.", ["word"; 21].join(" "));
        let text = format!("{sentence} {sentence} Two words.");
        assert!(problems("book/src/x.md", &text).is_empty());
        let average = format!("{}. {sentence}", ["word"; 19].join(" "));
        assert!(problems("book/src/x.md", &average).is_empty());
        assert_eq!(
            problems("book/src/x.md", &format!("{sentence} {sentence}")),
            ["book/src/x.md: 2 sentences have 42 words, above an average of 20"]
        );
    }

    #[test]
    fn link_titles_reference_targets_and_tag_attributes_count_nothing() {
        assert_eq!(
            counts(
                "See [the page](x.md \"A long title\") and [the ref][some long ref] or <a href=\"x.md\">this one</a>."
            ),
            [(9, 0)]
        );
    }

    #[test]
    fn a_lone_backtick_or_angle_bracket_stays_prose() {
        assert_eq!(counts("It costs 5` more, and a <= b."), [(7, 0)]);
        assert_eq!(counts("Ends on a tick `"), [(4, 0)]);
    }

    #[test]
    fn a_fence_ends_only_at_its_own_marker() {
        assert_eq!(counts("```\n~~~\nIn code.\n```\n\nWord.\n"), [(1, 0)]);
    }

    #[test]
    fn links_and_numbers_starting_a_line_are_prose() {
        assert_eq!(
            counts("[Hooks](hooks.md) run first,\n) then more.\n\n[r]: https://example.com/a.b\n"),
            [(5, 0)]
        );
    }

    #[test]
    fn a_book_with_a_long_sentence_fails_the_check() {
        let root = std::env::temp_dir().join(format!("xtask-prose-{}", std::process::id()));
        let source = root.join(SOURCE);
        std::fs::create_dir_all(&source).unwrap();
        let long = format!("{}.", ["word"; 26].join(" "));
        std::fs::write(source.join("hooks.md"), format!("# Hooks\n\n{long}\n")).unwrap();
        let found = check(&root);
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(
            found,
            [
                "book/src/hooks.md:3: a sentence has 26 words; keep it to 25: \"word word word word word word …\"",
                "book/src/hooks.md: 1 sentences have 26 words, above an average of 20",
            ]
        );
    }

    #[test]
    fn a_list_of_code_spans_is_no_sentence() {
        assert_eq!(
            counts("| `a::*`, `b::c`, `d`, `e` | Why `a` and `b` differ. |\n- `x`, `y`, `z`.\n"),
            [(5, 2)]
        );
        assert_eq!(counts("Use `a`, `b` and `c`."), [(5, 3)]);
    }

    #[test]
    fn sentences_at_the_limits_pass() {
        let most = format!("{}.", ["word"; 25].join(" "));
        let text = format!("{most} {}. `a` and `b`.", ["word"; 13].join(" "));
        assert!(problems("book/src/x.md", &text).is_empty());
    }

    #[test]
    fn a_fence_closes_only_on_a_bare_marker_as_long_as_its_opening() {
        assert_eq!(
            counts("````\n```rust\nlet a = b. c = d.\n```\n````\n\nWord.\n"),
            [(1, 0)]
        );
        assert_eq!(counts("```\n```rust\nOne two.\n```\n\nWord.\n"), [(1, 0)]);
    }

    #[test]
    fn a_wrapped_line_opening_on_a_hash_or_a_number_stays_prose() {
        assert_eq!(
            counts("Fixed in pull request\n#960 and more words here.\n"),
            [(9, 0)]
        );
        assert_eq!(
            counts("It allows up to 64, at most\n255. The game groups them.\n"),
            [(8, 0), (4, 0)]
        );
        assert_eq!(
            counts("Intro.\n1. first\n2. second\n"),
            [(1, 0), (1, 0), (1, 0)]
        );
    }

    #[test]
    fn code_spans_alone_in_a_paragraph_are_a_sentence() {
        assert_eq!(counts("Use these. `a`, `b`, `c`."), [(2, 0), (3, 3)]);
    }

    #[test]
    fn table_cells_leave_the_average_alone() {
        let sentence = format!("{}.", ["word"; 25].join(" "));
        let text = format!("{sentence} {sentence} {sentence}\n\n| a | 1 |\n| b | 2 |\n");
        assert_eq!(
            problems("book/src/x.md", &text),
            ["book/src/x.md: 3 sentences have 75 words, above an average of 20"]
        );
    }

    #[test]
    fn an_angle_bracket_that_never_closes_stays_prose() {
        assert_eq!(
            counts("When x <y the rest one. Four five."),
            [(6, 0), (2, 0)]
        );
    }

    #[test]
    fn the_repository_book_passes() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let found = check(&root);
        assert!(found.is_empty(), "{found:?}");
    }
}
