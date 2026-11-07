//! The book's sentence lengths, as the house style for developer text sets
//! them: no sentence over [`MAX_WORDS`] words or [`MAX_CODE_SPANS`] code
//! spans, and a chapter's sentences averaging [`AVERAGE_WORDS`] words or
//! fewer. A code span counts as one word. Code blocks, headings and HTML
//! comments hold no sentences; list items and table cells end one.

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
    if let Err(error) = crate::book::collect(&source, "", &mut chapters) {
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
    let total: usize = sentences.iter().map(|sentence| sentence.words).sum();
    if total > AVERAGE_WORDS * sentences.len() {
        found.push(format!(
            "{at}: {} sentences have {total} words, above an average of {AVERAGE_WORDS}",
            sentences.len()
        ));
    }
    found
}

/// The sentences of a chapter, in order.
fn sentences(text: &str) -> Vec<Sentence> {
    blocks(text)
        .into_iter()
        .flat_map(|(line, block)| split(line, &flatten(&block)))
        .collect()
}

/// The prose blocks of a chapter with the line each starts on: paragraphs,
/// list items and table cells, outside code fences, headings and comments.
fn blocks(text: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    let mut current: Option<(usize, String)> = None;
    let mut fence: Option<&str> = None;
    let mut comment = false;
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let trimmed = raw.trim_start();
        if comment {
            comment = !raw.contains("-->");
            continue;
        }
        let marker = ["```", "~~~"]
            .into_iter()
            .find(|marker| trimmed.starts_with(marker));
        match (fence, marker) {
            (None, Some(marker)) => {
                found.extend(current.take());
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
        let trimmed = trimmed.trim_start_matches(['>', ' ']);
        if let Some(rest) = trimmed.strip_prefix("<!--") {
            found.extend(current.take());
            comment = !rest.contains("-->");
            continue;
        }
        if trimmed.is_empty()
            || trimmed.starts_with('#')
            || trimmed.starts_with("{{#")
            || is_reference_definition(trimmed)
        {
            found.extend(current.take());
            continue;
        }
        if trimmed.starts_with('|') {
            found.extend(current.take());
            if !trimmed.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ')) {
                found.extend(cells(trimmed).map(|cell| (line, cell.to_owned())));
            }
            continue;
        }
        if let Some(item) = list_item(trimmed) {
            found.extend(current.take());
            current = Some((line, item.to_owned()));
            continue;
        }
        match &mut current {
            Some((_, block)) => {
                block.push('\n');
                block.push_str(trimmed);
            }
            None => current = Some((line, trimmed.to_owned())),
        }
    }
    found.extend(current);
    found
}

/// Whether `line` defines a link reference, `[name]: target`.
fn is_reference_definition(line: &str) -> bool {
    line.starts_with('[') && line.contains("]: ")
}

/// The text after a list item's marker (`-`, `*`, `+`, `1.`, `1)`), if
/// `line` starts one.
fn list_item(line: &str) -> Option<&str> {
    if let Some(rest) = line
        .strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))
        .or_else(|| line.strip_prefix("+ "))
    {
        return Some(rest);
    }
    let digits = line.find(|c: char| !c.is_ascii_digit())?;
    let rest = &line[digits..];
    (digits > 0)
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
            ('<', Some(next)) if next.is_ascii_alphabetic() || matches!(next, '/' | '!') => {
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
fn split(line: usize, prose: &str) -> Vec<Sentence> {
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
                found.extend(sentence(line, &chars, start, end));
                start = end;
            }
            at = end;
        } else {
            at += 1;
        }
    }
    found.extend(sentence(line, &chars, start, chars.len()));
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

/// The sentence `chars[start..end]` of a block starting on `line`, if it
/// has a word.
fn sentence(line: usize, chars: &[char], start: usize, end: usize) -> Option<Sentence> {
    let text: String = chars[start..end].iter().collect();
    let words: Vec<&str> = text
        .split_whitespace()
        .filter(|word| word.chars().any(|c| c.is_alphanumeric() || c == CODE))
        .collect();
    if words.is_empty() {
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
        let text = "<!--\ntype: how-to\ndescription: a b c.\n-->\n\n# A long title here\n\n```rust\n// One two. Three four.\nlet x = 1;\n```\n\n~~~\n``` not a fence end\n~~~\n\n<!-- one -->\nWord.\n";
        assert_eq!(counts(text), [(1, 0)]);
    }

    #[test]
    fn list_items_and_table_cells_end_a_sentence() {
        let text = "Pick one:\n\n- a first item\n  wrapped on\n- `second`\n1. third\n\n| Name | What it does |\n| --- | :-: |\n| `a|b` | runs once |\n";
        assert_eq!(
            counts(text),
            [
                (2, 0),
                (5, 0),
                (1, 1),
                (1, 0),
                (1, 0),
                (3, 0),
                (1, 1),
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
        let text = format!("{long}\n`a` `b` `c`.\n");
        assert_eq!(
            problems("book/src/x.md", &text),
            [
                "book/src/x.md:1: a sentence has 26 words; keep it to 25: \"word word word word word word …\"",
                "book/src/x.md:2: a sentence has 3 code spans; keep it to 2: \"`…` `…` `…`. …\"",
            ]
        );
    }

    #[test]
    fn a_chapter_averaging_over_twenty_words_is_named() {
        let sentence = format!("{}.", ["word"; 21].join(" "));
        let text = format!("{sentence} {sentence} Two words.");
        assert!(problems("book/src/x.md", &text).is_empty());
        assert_eq!(
            problems("book/src/x.md", &format!("{sentence} {sentence}")),
            ["book/src/x.md: 2 sentences have 42 words, above an average of 20"]
        );
    }

    #[test]
    fn sentences_at_the_limits_pass() {
        let most = format!("{}.", ["word"; 25].join(" "));
        let text = format!("{most} {}. `a` and `b`.", ["word"; 13].join(" "));
        assert!(problems("book/src/x.md", &text).is_empty());
    }

    #[test]
    fn the_repository_book_passes() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let found = check(&root);
        assert!(found.is_empty(), "{found:?}");
    }
}
