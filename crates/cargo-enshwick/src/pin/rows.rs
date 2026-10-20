//! Build rows as the sources spell them: the loader's `KnownBuild` table and
//! the `builds:` rows of each mod's `manifest!`.
//!
//! The parser keeps the span of every value, so a new row is the old row's
//! text with values replaced: comments and layout carry over.

use std::fmt::Write as _;
use std::ops::Range;

/// The game side a row is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Side {
    /// The dedicated server.
    Server,
    /// The game client.
    Client,
}

/// Which table a row belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Table {
    /// The loader's `BUILDS`.
    Loader,
    /// A mod's `manifest!` `builds:`.
    Mod,
}

/// One `"id" => value` or `("id", value)` entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    /// The symbolic id.
    pub(crate) id: String,
    /// The value.
    pub(crate) value: u64,
    /// Where the value's expression is in the source.
    pub(crate) value_span: Range<usize>,
    /// The entry, from its first token to the end of its value (a tuple's
    /// closing parenthesis included).
    pub(crate) span: Range<usize>,
    /// The entry's whole line, indentation included.
    pub(crate) line: Range<usize>,
}

/// One build row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Row {
    /// Which table.
    pub(crate) table: Table,
    /// The side.
    pub(crate) side: Side,
    /// The fingerprint GUID as lowercase hex.
    pub(crate) guid: String,
    /// The fingerprint age.
    pub(crate) age: u32,
    /// The loader row's name.
    pub(crate) name: Option<String>,
    /// The row, from its first token to its closing brace.
    pub(crate) span: Range<usize>,
    guid_span: Range<usize>,
    age_span: Range<usize>,
    name_span: Option<Range<usize>>,
    /// Systems whose entry must match (loader rows).
    pub(crate) checks: Vec<Entry>,
    /// Pinned addresses, relative to the image base.
    pub(crate) symbols: Vec<Entry>,
    /// Pinned constants.
    pub(crate) constants: Vec<Entry>,
}

/// What becomes of one entry in the new row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Edit {
    /// The value stays as written.
    Keep,
    /// The value becomes this.
    Set(u64),
    /// The entry is left out, with this note in its place.
    Drop(String),
    /// The entry stays, with this note above it.
    Note(String),
}

/// Every row in `src`, a loader build table or a mod source.
pub(crate) fn parse(src: &str) -> Result<Vec<Row>, String> {
    if let Some(table) = find_code(src, 0, src.len(), "pub static BUILDS") {
        loader_rows(src, table)
    } else if let Some(builds) = find_code(src, 0, src.len(), "builds: [") {
        mod_rows(src, builds + "builds: ".len())
    } else {
        Ok(Vec::new())
    }
}

fn loader_rows(src: &str, from: usize) -> Result<Vec<Row>, String> {
    let mut rows = Vec::new();
    let mut at = from;
    while let Some(start) = find_code(src, at, src.len(), "KnownBuild {") {
        let open = start + "KnownBuild ".len();
        let close = matching(src, open).ok_or("an unclosed KnownBuild")?;
        let name_at = field(src, open, close, "name: \"")?;
        let name_span = name_at..string_end(src, name_at)?;
        let side_at = field(src, open, close, "side: Side::")?;
        let guid_at = field(src, open, close, "guid(b\"")?;
        let guid_span = guid_at..string_end(src, guid_at)?;
        let age_from = guid_span.end + "\"), ".len();
        let age_span = age_from..age_from + digits(src, age_from);
        rows.push(Row {
            table: Table::Loader,
            side: side(src, side_at, "Side::")?,
            guid: src[guid_span.clone()].to_owned(),
            age: number(&src[age_span.clone()])?,
            name: Some(src[name_span.clone()].to_owned()),
            span: start..close + 1,
            guid_span,
            age_span,
            name_span: Some(name_span),
            checks: list(src, open, close, "checks: &[", Style::Tuple)?,
            symbols: list(src, open, close, "symbols: &[", Style::Tuple)?,
            constants: list(src, open, close, "constants: &[", Style::Tuple)?,
        });
        at = close;
    }
    Ok(rows)
}

fn mod_rows(src: &str, open: usize) -> Result<Vec<Row>, String> {
    let close = matching(src, open).ok_or("an unclosed builds list")?;
    let mut rows = Vec::new();
    let mut at = skip_trivia(src, open + 1);
    while at < close {
        if src.as_bytes().get(at) == Some(&b',') {
            at = skip_trivia(src, at + 1);
            continue;
        }
        if src.as_bytes().get(at) != Some(&b'{') {
            return Err(format!("a build row expected at byte {at}"));
        }
        let end = matching(src, at).ok_or("an unclosed build row")?;
        let side_at = field(src, at, end, "side: ")?;
        let guid_at = field(src, at, end, "guid: \"")?;
        let guid_span = guid_at..string_end(src, guid_at)?;
        let age_at = field(src, at, end, "age: ")?;
        let age_span = age_at..age_at + digits(src, age_at);
        rows.push(Row {
            table: Table::Mod,
            side: side(src, side_at, "")?,
            guid: src[guid_span.clone()].to_owned(),
            age: number(&src[age_span.clone()])?,
            name: None,
            span: at..end + 1,
            guid_span,
            age_span,
            name_span: None,
            checks: Vec::new(),
            symbols: optional_list(src, at, end, "symbols: [")?,
            constants: optional_list(src, at, end, "constants: [")?,
        });
        at = skip_trivia(src, end + 1);
    }
    Ok(rows)
}

impl Row {
    /// The row's text for a new build: `guid`/`age`, `name` for a loader row,
    /// and each entry edited as `edits` says (by id; missing ids are kept).
    pub(crate) fn render(
        &self,
        src: &str,
        guid: &str,
        age: u32,
        name: Option<&str>,
        edits: &[(&Entry, Edit)],
    ) -> String {
        let mut changes: Vec<(Range<usize>, String)> = vec![
            (self.guid_span.clone(), guid.to_owned()),
            (self.age_span.clone(), age.to_string()),
        ];
        if let (Some(span), Some(name)) = (&self.name_span, name) {
            changes.push((span.clone(), name.to_owned()));
        }
        for (entry, edit) in edits {
            let indent = &src[entry.line.clone()]
                [..src[entry.line.clone()].len() - src[entry.line.clone()].trim_start().len()];
            match edit {
                Edit::Keep => {}
                Edit::Set(value) => {
                    if number(&src[entry.value_span.clone()]).ok() != Some(*value) {
                        changes.push((entry.value_span.clone(), hex(*value)));
                    }
                }
                Edit::Drop(note) => {
                    let mut own = entry.span.clone();
                    if src.as_bytes().get(own.end) == Some(&b',') {
                        own.end += 1;
                    }
                    if src[entry.line.clone()].trim() == src[own.clone()].trim() {
                        changes.push((entry.line.clone(), format!("{indent}// pin: {note}")));
                    } else {
                        changes.push((own, format!("/* pin: {note} */")));
                    }
                }
                Edit::Note(note) => {
                    let at = entry.line.start;
                    changes.push((at..at, format!("{indent}// pin: {note}\n")));
                }
            }
        }
        changes.sort_by_key(|(span, _)| std::cmp::Reverse((span.start, span.end)));
        let mut text = src[self.span.clone()].to_owned();
        for (span, with) in changes {
            text.replace_range(
                span.start - self.span.start..span.end - self.span.start,
                &with,
            );
        }
        text
    }

    /// Where a row placed after this one goes in the source, and the
    /// indentation it takes.
    pub(crate) fn insertion(&self, src: &str) -> (usize, String) {
        let line_start = src[..self.span.start].rfind('\n').map_or(0, |i| i + 1);
        let indent = src[line_start..self.span.start].to_owned();
        let mut at = self.span.end;
        if src.as_bytes().get(at) == Some(&b',') {
            at += 1;
        }
        (at, indent)
    }
}

/// `value` as the sources spell addresses: hex, `_` every four digits.
pub(crate) fn hex(value: u64) -> String {
    let digits = format!("{value:x}");
    let mut out = String::from("0x");
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(4) {
            out.push('_');
        }
        out.push(c);
    }
    out
}

/// Formats a fingerprint GUID the way the rows spell it.
pub(crate) fn guid_hex(guid: [u8; 16]) -> String {
    guid.iter().fold(String::new(), |mut out, b| {
        let _infallible = write!(out, "{b:02x}");
        out
    })
}

#[derive(Clone, Copy)]
enum Style {
    Tuple,
    Arrow,
}

fn optional_list(src: &str, from: usize, to: usize, name: &str) -> Result<Vec<Entry>, String> {
    if find_code(src, from, to, name).is_some() {
        list(src, from, to, name, Style::Arrow)
    } else {
        Ok(Vec::new())
    }
}

fn list(src: &str, from: usize, to: usize, name: &str, style: Style) -> Result<Vec<Entry>, String> {
    let open = field(src, from, to, name)? - 1;
    let close = matching(src, open).ok_or_else(|| format!("an unclosed {name}"))?;
    let mut entries = Vec::new();
    let mut at = skip_trivia(src, open + 1);
    while at < close {
        let bytes = src.as_bytes();
        let entry_start = at;
        if let Style::Tuple = style {
            if bytes.get(at) != Some(&b'(') {
                return Err(format!("{name}: an entry expected at byte {at}"));
            }
            at += 1;
        }
        if bytes.get(at) != Some(&b'"') {
            return Err(format!("{name}: an id expected at byte {at}"));
        }
        let id_end = string_end(src, at + 1)?;
        let id = src[at + 1..id_end].to_owned();
        at = skip_trivia(src, id_end + 1);
        let separator = match style {
            Style::Tuple => ",",
            Style::Arrow => "=>",
        };
        if !src[at..].starts_with(separator) {
            return Err(format!("{name}: {id} has no value"));
        }
        let value_start = skip_trivia(src, at + separator.len());
        let mut end = value_start;
        let mut depth = 0usize;
        while end < close {
            match bytes.get(end) {
                Some(b'(') => depth += 1,
                Some(b')' | b',') if depth == 0 => break,
                Some(b')') => depth -= 1,
                _ => {}
            }
            end += 1;
        }
        let value_span = value_start..value_start + src[value_start..end].trim_end().len();
        let value = evaluate(&src[value_span.clone()]).map_err(|e| format!("{id}: {e}"))?;
        if let Style::Tuple = style {
            end += 1;
        }
        let line_start = src[..entry_start].rfind('\n').map_or(0, |i| i + 1);
        let line_end = src[end..].find('\n').map_or(src.len(), |i| end + i);
        entries.push(Entry {
            id,
            value,
            value_span,
            span: entry_start..end,
            line: line_start..line_end,
        });
        at = end;
        if bytes.get(skip_trivia(src, at)) == Some(&b',') {
            at = skip_trivia(src, at) + 1;
        }
        at = skip_trivia(src, at);
    }
    Ok(entries)
}

/// Evaluates the constant expressions the rows use: literals, `<<` and `|`.
fn evaluate(expr: &str) -> Result<u64, String> {
    let expr = expr.trim();
    let expr = expr
        .strip_prefix('(')
        .and_then(|e| e.strip_suffix(')'))
        .unwrap_or(expr);
    expr.split('|').try_fold(0u64, |acc, part| {
        let mut terms = part.split("<<");
        let first: u64 = number(terms.next().unwrap_or_default())?;
        let shifted = terms.try_fold(first, |value, shift| {
            let by: u32 = number(shift)?;
            value
                .checked_shl(by)
                .ok_or_else(|| format!("a shift by {by} overflows"))
        })?;
        Ok(acc | shifted)
    })
}

fn number<T: TryFrom<u64>>(text: &str) -> Result<T, String> {
    let clean: String = text.trim().chars().filter(|&c| c != '_').collect();
    let value = match clean.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16),
        None => clean.parse(),
    }
    .map_err(|e| format!("`{}`: {e}", text.trim()))?;
    T::try_from(value).map_err(|_| format!("`{}` is out of range", text.trim()))
}

fn side(src: &str, at: usize, prefix: &str) -> Result<Side, String> {
    let rest = &src[at..];
    if rest.starts_with("SERVER") {
        Ok(Side::Server)
    } else if rest.starts_with("CLIENT") {
        Ok(Side::Client)
    } else {
        Err(format!("an unknown side after `{prefix}` at byte {at}"))
    }
}

/// Where the text after `name` starts, inside `from..to`.
fn field(src: &str, from: usize, to: usize, name: &str) -> Result<usize, String> {
    find_code(src, from, to, name)
        .map(|at| at + name.len())
        .ok_or_else(|| format!("no `{name}` in the row at byte {from}"))
}

fn digits(src: &str, at: usize) -> usize {
    src[at..].bytes().take_while(u8::is_ascii_digit).count()
}

fn string_end(src: &str, from: usize) -> Result<usize, String> {
    let bytes = src.as_bytes();
    let mut at = from;
    while let Some(&c) = bytes.get(at) {
        match c {
            b'\\' => at += 2,
            b'"' => return Ok(at),
            _ => at += 1,
        }
    }
    Err(format!("an unclosed string at byte {from}"))
}

/// Skips whitespace and line comments.
fn skip_trivia(src: &str, mut at: usize) -> usize {
    loop {
        let rest = &src[at.min(src.len())..];
        let trimmed = rest.trim_start();
        at += rest.len() - trimmed.len();
        if trimmed.starts_with("//") {
            at += trimmed.find('\n').unwrap_or(trimmed.len());
        } else {
            return at;
        }
    }
}

/// The first `needle` in `from..to` outside comments and strings.
fn find_code(src: &str, from: usize, to: usize, needle: &str) -> Option<usize> {
    let bytes = src.as_bytes();
    let mut at = from;
    while at < to {
        if src[at..].starts_with(needle) {
            return Some(at);
        }
        match bytes.get(at) {
            Some(b'/') if bytes.get(at + 1) == Some(&b'/') => {
                at += src[at..].find('\n').unwrap_or(src.len() - at);
            }
            Some(b'"') => at = string_end(src, at + 1).ok()? + 1,
            _ => at += src[at..].chars().next().map_or(1, char::len_utf8),
        }
    }
    None
}

/// The bracket that closes the one at `open`.
fn matching(src: &str, open: usize) -> Option<usize> {
    let bytes = src.as_bytes();
    let mut depth = 0usize;
    let mut at = open;
    while let Some(&c) = bytes.get(at) {
        match c {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(at);
                }
            }
            b'/' if bytes.get(at + 1) == Some(&b'/') => {
                at += src[at..].find('\n')?;
                continue;
            }
            b'"' => {
                at = string_end(src, at + 1).ok()? + 1;
                continue;
            }
            _ => {}
        }
        at += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOADER: &str = r#"
pub static BUILDS: &[KnownBuild] = &[
    KnownBuild {
        name: "1.0 server (1269579)",
        side: Side::SERVER,
        fingerprint: Fingerprint::new(guid(b"1c129660cd6efc4f9f0b21652d5ff2bf"), 387),
        checks: &[("upgrade_building_range", 0x90a60)],
        symbols: &[
            ("ecs.iterator_init", 0x65_b8c0),
            // A comment with symbols: &[ in it.
            ("npc.move", 0x1e_ca40),
        ],
        constants: &[
            ("ecs.context_row", 8),
            ("inventory.copy_a8", 0x18 << 32 | 0xa8),
        ],
    },
];
"#;

    const MOD: &str = r#"
        enshwick_loader_api::manifest!(
            id: "safes",
            side: BOTH,
            builds: [
                {
                    side: SERVER,
                    guid: "1c129660cd6efc4f9f0b21652d5ff2bf",
                    age: 387,
                    symbols: [
                        "safes.offer_test" => 0x9_e270,
                        "safes.sort" => 0x19_8cf0,
                    ],
                    constants: ["safes.state_input" => 0x10],
                },
                {
                    // The client's twin.
                    side: CLIENT,
                    guid: "69f0e9573b32c24a8838d3fff550824c",
                    age: 410,
                    symbols: ["safes.offer_test" => 0x2d_8295],
                },
            ],
        )
"#;

    #[test]
    fn the_loader_table_parses_with_its_expressions() {
        let rows = parse(LOADER).unwrap();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.table, Table::Loader);
        assert_eq!(row.side, Side::Server);
        assert_eq!(row.age, 387);
        assert_eq!(row.name.as_deref(), Some("1.0 server (1269579)"));
        assert_eq!(row.checks[0].value, 0x90a60);
        let ids: Vec<&str> = row.symbols.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["ecs.iterator_init", "npc.move"]);
        assert_eq!(row.constants[1].value, 0x18 << 32 | 0xa8);
        assert_eq!(&LOADER[row.symbols[1].value_span.clone()], "0x1e_ca40");
    }

    #[test]
    fn mod_rows_parse_with_and_without_constants() {
        let rows = parse(MOD).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].symbols[1].value, 0x19_8cf0);
        assert_eq!(rows[0].constants[0].value, 0x10);
        assert_eq!(rows[1].side, Side::Client);
        assert!(rows[1].constants.is_empty());
        assert_eq!(rows[1].symbols[0].value, 0x2d_8295);
    }

    #[test]
    fn an_entry_sharing_its_line_is_dropped_in_place() {
        let row = &parse(MOD).unwrap()[1];
        let edits = [(&row.symbols[0], Edit::Drop("gone".into()))];
        let text = row.render(MOD, &row.guid, row.age, None, &edits);
        assert!(text.contains("symbols: [/* pin: gone */],"));
    }

    #[test]
    fn a_rendered_row_keeps_its_layout_and_comments() {
        let row = &parse(LOADER).unwrap()[0];
        let edits = [
            (&row.symbols[0], Edit::Set(0x66_0000)),
            (
                &row.symbols[1],
                Edit::Drop("npc.move: missing; was 0x1e_ca40".into()),
            ),
            (&row.constants[0], Edit::Note("unverified".into())),
            (&row.checks[0], Edit::Set(0x90a60)),
        ];
        let text = row.render(LOADER, "ab".repeat(16).as_str(), 400, Some("x (2)"), &edits);
        assert!(text.starts_with("KnownBuild {\n        name: \"x (2)\""));
        assert!(text.contains(&format!("guid(b\"{}\"), 400)", "ab".repeat(16))));
        assert!(text.contains("(\"ecs.iterator_init\", 0x66_0000),"));
        assert!(text.contains("            // pin: npc.move: missing; was 0x1e_ca40\n"));
        assert!(!text.contains("0x1e_ca40)"));
        assert!(text.contains("// pin: unverified\n            (\"ecs.context_row\", 8)"));
        assert!(
            text.contains("(\"upgrade_building_range\", 0x90a60)"),
            "an equal value stays as written"
        );
        assert!(text.contains("// A comment with symbols: &[ in it."));
        assert!(
            parse(&format!(
                "pub static BUILDS: &[KnownBuild] = &[\n    {text},\n];"
            ))
            .is_ok()
        );
    }

    #[test]
    fn a_row_goes_after_the_last_one_with_its_indentation() {
        let rows = parse(MOD).unwrap();
        let (at, indent) = rows[1].insertion(MOD);
        assert_eq!(&MOD[at - 2..at], "},");
        assert_eq!(indent, " ".repeat(16));
    }

    #[test]
    fn addresses_are_spelled_as_the_sources_spell_them() {
        assert_eq!(hex(0x65_b8c0), "0x65_b8c0");
        assert_eq!(hex(0x131_6740), "0x131_6740");
        assert_eq!(hex(0xe90e0), "0xe_90e0");
        assert_eq!(hex(0x10), "0x10");
        assert_eq!(guid_hex([0xab; 16]), "ab".repeat(16));
    }

    #[test]
    fn expressions_evaluate_and_bad_ones_are_errors() {
        assert_eq!(evaluate("0x10 << 32 | 0xd8"), Ok(0x10_0000_00d8));
        assert_eq!(evaluate("0x7095_8d48_b065_8b4c"), Ok(0x7095_8d48_b065_8b4c));
        assert_eq!(evaluate("32"), Ok(32));
        assert!(evaluate("SIZE * 2").is_err());
    }

    #[test]
    fn sources_without_rows_have_none() {
        assert_eq!(parse("fn main() {}"), Ok(Vec::new()));
    }
}
