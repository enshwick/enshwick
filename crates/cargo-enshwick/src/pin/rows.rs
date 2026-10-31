//! Build rows: the loader's tables (read by [`super::table`]) and the
//! `[[package.metadata.enshwick.builds]]` rows of each mod's `Cargo.toml`
//! as the files spell them.
//!
//! The parser keeps the span of every value in a mod's `Cargo.toml`, so a
//! new row is the old row's text with values replaced: comments and layout
//! carry over.

use std::fmt::Write as _;
use std::ops::Range;

use anyhow::{Context as _, anyhow, bail};

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
    /// A loader build table.
    Loader,
    /// A mod's `[[package.metadata.enshwick.builds]]`.
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
    side_span: Range<usize>,
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

/// The header of a mod's build row.
const ROW: &str = "[[package.metadata.enshwick.builds]]";
/// The header of a row's symbols.
const SYMBOLS: &str = "[package.metadata.enshwick.builds.symbols]";
/// The header of a row's constants.
const CONSTANTS: &str = "[package.metadata.enshwick.builds.constants]";

/// Which part of a row a line belongs to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Part {
    Fields,
    Symbols,
    Constants,
}

/// Every build row in a mod's `Cargo.toml`, each a
/// `[[package.metadata.enshwick.builds]]` table with its `side`, `guid` and
/// `age`, then its `.symbols` and `.constants` tables of `"id" = value`
/// lines.
pub(crate) fn parse(src: &str) -> anyhow::Result<Vec<Row>> {
    let _toml: toml::Table = toml::from_str(src).context("not TOML")?;
    let mut rows: Vec<Row> = Vec::new();
    let mut open: Option<(Row, Part)> = None;
    let mut at = 0;
    for (index, line) in src.split_inclusive('\n').enumerate() {
        let start = at;
        at += line.len();
        let body = line.trim_end_matches(['\n', '\r']);
        let code = code_of(body);
        let indent = body.len() - body.trim_start().len();
        if code.starts_with('[') {
            let part = match code {
                ROW => {
                    rows.extend(finished(open.take(), index)?);
                    open = Some((mod_row(start + indent), Part::Fields));
                    continue;
                }
                SYMBOLS => Part::Symbols,
                CONSTANTS => Part::Constants,
                _ => {
                    rows.extend(finished(open.take(), index)?);
                    continue;
                }
            };
            match open.as_mut() {
                Some((row, now)) => {
                    *now = part;
                    row.span.end = start + body.len();
                }
                None => bail!("line {}: `{code}` outside a build row", index + 1),
            }
            continue;
        }
        let Some((row, part)) = open.as_mut() else {
            continue;
        };
        if code.is_empty() {
            continue;
        }
        let at_line = |what: &str| format!("line {}: {what}", index + 1);
        let equals = code
            .find('=')
            .with_context(|| at_line("a `key = value` expected"))?;
        let key = code[..equals].trim();
        let value_text = code[equals + 1..].trim();
        let value_start = start
            + indent
            + equals
            + 1
            + (code[equals + 1..].len() - code[equals + 1..].trim_start().len());
        let value_span = value_start..value_start + value_text.len();
        match part {
            Part::Fields => field(row, key, value_text, value_span)
                .map_err(|why| anyhow!("line {}: {why}", index + 1))?,
            Part::Symbols | Part::Constants => {
                let id = key
                    .strip_prefix('"')
                    .and_then(|k| k.strip_suffix('"'))
                    .with_context(|| at_line("an entry's id is quoted"))?;
                let entry = Entry {
                    id: id.to_owned(),
                    value: number(value_text).with_context(|| at_line(id))?,
                    value_span: value_span.clone(),
                    span: start + indent..value_span.end,
                    line: start..start + body.len(),
                };
                if *part == Part::Symbols {
                    row.symbols.push(entry);
                } else {
                    row.constants.push(entry);
                }
            }
        }
        row.span.end = start + body.len();
    }
    rows.extend(finished(open, usize::MAX)?);
    Ok(rows)
}

/// Reads the row field `key` with its value `value` at `span` into `row`.
fn field(row: &mut Row, key: &str, value: &str, span: Range<usize>) -> anyhow::Result<()> {
    match key {
        "side" => {
            row.side = match value {
                "\"server\"" => Side::Server,
                "\"client\"" => Side::Client,
                _ => bail!("`side` is \"server\" or \"client\""),
            };
            row.side_span = span;
        }
        "guid" => {
            let inner = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .context("`guid` is a string")?;
            inner.clone_into(&mut row.guid);
            row.guid_span = span.start + 1..span.end - 1;
        }
        "age" => {
            row.age = number(value).context("`age`")?;
            row.age_span = span;
        }
        other => match other.split_once('.').map_or(other, |(head, _)| head) {
            table @ ("symbols" | "constants") => bail!(
                "pin reads `{table}` only as a `[package.metadata.enshwick.builds.{table}]` table"
            ),
            _ => bail!("a build row has no `{other}`"),
        },
    }
    Ok(())
}

/// A mod row starting at `start`, its fields still to read.
fn mod_row(start: usize) -> Row {
    Row {
        table: Table::Mod,
        side: Side::Server,
        guid: String::new(),
        age: 0,
        name: None,
        span: start..start,
        side_span: 0..0,
        guid_span: 0..0,
        age_span: 0..0,
        name_span: None,
        checks: Vec::new(),
        symbols: Vec::new(),
        constants: Vec::new(),
    }
}

/// The row that ends before the line at `index`, once it has its fields.
fn finished(open: Option<(Row, Part)>, index: usize) -> anyhow::Result<Option<Row>> {
    let Some((row, _)) = open else {
        return Ok(None);
    };
    if row.side_span.is_empty() || row.guid_span.is_empty() || row.age_span.is_empty() {
        let at = if index == usize::MAX {
            "the end".to_owned()
        } else {
            format!("line {}", index + 1)
        };
        bail!("the build row before {at} needs `side`, `guid` and `age`");
    }
    Ok(Some(row))
}

/// The line without its comment, trimmed; a `#` inside a string stays.
fn code_of(line: &str) -> &str {
    let mut quoted = false;
    for (at, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '#' if !quoted => return line[..at].trim(),
            _ => {}
        }
    }
    line.trim()
}

impl Row {
    /// A loader table's row; its text is a table file, not a span.
    pub(crate) fn loader(
        side: Side,
        guid: String,
        age: u32,
        name: String,
        checks: Vec<Entry>,
        symbols: Vec<Entry>,
        constants: Vec<Entry>,
    ) -> Self {
        Self {
            table: Table::Loader,
            side,
            guid,
            age,
            name: Some(name),
            span: 0..0,
            side_span: 0..0,
            guid_span: 0..0,
            age_span: 0..0,
            name_span: None,
            checks,
            symbols,
            constants,
        }
    }

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
                    changes.push((entry.line.clone(), format!("{indent}# pin: {note}")));
                }
                Edit::Note(note) => {
                    let at = entry.line.start;
                    changes.push((at..at, format!("{indent}# pin: {note}\n")));
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

    /// Where a row placed after this one goes in the file, and what goes
    /// before it there: a blank line and the indentation of the line this
    /// row starts on.
    pub(crate) fn insertion(&self, src: &str) -> (usize, String) {
        let line_start = src[..self.span.start].rfind('\n').map_or(0, |i| i + 1);
        let indent = &src[line_start..self.span.start];
        (self.span.end, format!("\n\n{indent}"))
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

fn number<T: TryFrom<u64>>(text: &str) -> anyhow::Result<T> {
    let clean: String = text.trim().chars().filter(|&c| c != '_').collect();
    let value = match clean.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16),
        None => clean.parse(),
    }
    .with_context(|| format!("`{}`", text.trim()))?;
    T::try_from(value).map_err(|_| anyhow!("`{}` is out of range", text.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOD: &str = r#"[package]
name = "trading-post"

[package.metadata.enshwick]
id = "you.trading-post"
side = "both"

[[package.metadata.enshwick.builds]]
side = "server"
guid = "1c129660cd6efc4f9f0b21652d5ff2bf"
age = 387

[package.metadata.enshwick.builds.symbols]
"trading-post.offer_test" = 0x9_e270
"trading-post.sort" = 0x19_8cf0 # the sort

[package.metadata.enshwick.builds.constants]
"trading-post.state_input" = 0x10

# The client's twin.
[[package.metadata.enshwick.builds]]
side = "client"
guid = "69f0e9573b32c24a8838d3fff550824c"
age = 410

[package.metadata.enshwick.builds.symbols]
"trading-post.offer_test" = 0x2d_8295

[lints]
workspace = true
"#;

    #[test]
    fn mod_rows_parse_with_and_without_constants() {
        let rows = parse(MOD).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].guid, "1c129660cd6efc4f9f0b21652d5ff2bf");
        assert_eq!(rows[0].age, 387);
        assert_eq!(rows[0].symbols[1].value, 0x19_8cf0);
        assert_eq!(&MOD[rows[0].symbols[1].value_span.clone()], "0x19_8cf0");
        assert_eq!(rows[0].constants[0].value, 0x10);
        assert_eq!(rows[1].side, Side::Client);
        assert!(rows[1].constants.is_empty());
        assert_eq!(rows[1].symbols[0].value, 0x2d_8295);
        assert!(MOD[rows[1].span.clone()].ends_with("0x2d_8295"));
    }

    #[test]
    fn a_dropped_entry_leaves_a_note_in_its_place() {
        let row = &parse(MOD).unwrap()[0];
        let edits = [(&row.symbols[1], Edit::Drop("gone".into()))];
        let text = row.render(MOD, &row.guid, row.age, None, &edits);
        assert!(text.contains("\n# pin: gone\n"), "{text}");
        let dropped = MOD.replace(&MOD[row.span.clone()], &text);
        assert_eq!(parse(&dropped).unwrap()[0].symbols.len(), 1);
    }

    #[test]
    fn a_row_inserted_after_another_parses_back() {
        for which in 0..2 {
            let rows = parse(MOD).unwrap();
            let row = &rows[which];
            let text = row.render(MOD, &"ab".repeat(16), 1, None, &[]);
            let (at, lead) = row.insertion(MOD);
            let mut out = MOD.to_owned();
            out.insert_str(at, &format!("{lead}{text}"));
            let again = parse(&out).unwrap_or_else(|e| panic!("{e}\n{out}"));
            assert_eq!(again.len(), 3, "{out}");
            assert_eq!(again[which + 1].age, 1);
            assert_eq!(again[which + 1].guid, "ab".repeat(16));
            let ids = |row: &Row| row.symbols.iter().map(|s| s.id.clone()).collect::<Vec<_>>();
            assert_eq!(ids(&again[which + 1]), ids(row));
        }
    }

    #[test]
    fn a_row_without_its_fields_is_an_error() {
        let src = "[[package.metadata.enshwick.builds]]\nside = \"server\"\nage = 1\n";
        assert!(parse(src).is_err());
        let src = "[[package.metadata.enshwick.builds]]\nside = \"both\"\n";
        assert!(parse(src).is_err());
    }

    #[test]
    fn a_row_without_a_side_is_an_error_never_a_server_row() {
        let src = "[[package.metadata.enshwick.builds]]\nguid = \"ab\"\nage = 1\n";
        let why = parse(src).unwrap_err().to_string();
        assert!(why.contains("needs `side`"), "{why}");
    }

    #[test]
    fn inline_symbols_are_refused_with_the_table_form_named() {
        for symbols in ["symbols = { \"a.b\" = 0x10 }", "symbols.\"a.b\" = 0x10"] {
            let src = format!(
                "[[package.metadata.enshwick.builds]]\nside = \"server\"\nguid = \"ab\"\nage = 1\n{symbols}\n"
            );
            let why = parse(&src).unwrap_err().to_string();
            assert!(
                why.contains("[package.metadata.enshwick.builds.symbols]"),
                "{why}"
            );
        }
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
    fn numbers_read_in_hex_or_decimal_with_separators() {
        assert_eq!(number::<u64>("0x10_0000_00d8").ok(), Some(0x10_0000_00d8));
        assert_eq!(number::<u64>("32").ok(), Some(32));
        assert!(number::<u64>("SIZE").is_err());
    }

    #[test]
    fn a_manifest_without_rows_has_none() {
        assert_eq!(parse("[package]\nname = \"a\"\n").ok(), Some(Vec::new()));
    }
}
