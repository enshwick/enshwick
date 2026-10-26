//! The loader's build tables: one TOML file per game build under
//! [`DIR`], read into rows and written for a new build with the code
//! patterns that find each entry on a build without a table.

use std::collections::HashMap;

use anyhow::{Context as _, anyhow, bail};
use enshwick_gamedata::Locate;
use toml_edit::{DocumentMut, Item, Table as TomlTable, Value};

use super::anchor::{self, Anchor};
use super::exe::Exe;
use super::patterns::{self, ConstantPattern, SymbolPattern};
use super::rows::{Edit, Entry, Row, Side, hex};

/// Where the tables live, relative to the repository.
pub(crate) const DIR: &str = "crates/enshwick-gamedata/tables";

/// The table sections, in the order a row lists them.
const SECTIONS: [&str; 3] = ["checks", "symbols", "constants"];

/// Reads a table file into a row.
pub(crate) fn parse(src: &str) -> anyhow::Result<Row> {
    let doc: DocumentMut = src.parse()?;
    let text = |key: &str| {
        doc.get(key)
            .and_then(Item::as_str)
            .map(str::to_owned)
            .with_context(|| format!("no `{key}`"))
    };
    let side = match text("side")?.as_str() {
        "server" => Side::Server,
        "client" => Side::Client,
        other => bail!("an unknown side {other:?}"),
    };
    let age = doc
        .get("age")
        .and_then(Item::as_integer)
        .and_then(|a| u32::try_from(a).ok())
        .context("no `age`")?;
    Ok(Row::loader(
        side,
        text("guid")?,
        age,
        text("name")?,
        entries(&doc, "checks")?,
        entries(&doc, "symbols")?,
        entries(&doc, "constants")?,
    ))
}

fn entries(doc: &DocumentMut, section: &str) -> anyhow::Result<Vec<Entry>> {
    let Some(table) = doc.get(section).and_then(Item::as_table) else {
        return Ok(Vec::new());
    };
    table
        .iter()
        .map(|(id, item)| {
            let value = number(item).with_context(|| format!("{section}.{id}: no value"))?;
            Ok(Entry {
                id: id.to_owned(),
                value,
                value_span: 0..0,
                span: 0..0,
                line: 0..0,
            })
        })
        .collect()
}

/// An entry's value: an integer, a hex string past `i64::MAX`, or the `rva`
/// or `value` of an inline table.
fn number(item: &Item) -> Option<u64> {
    let value = match item {
        Item::Value(Value::InlineTable(t)) => t.get("rva").or_else(|| t.get("value"))?,
        Item::Value(value) => value,
        _ => return None,
    };
    match value {
        Value::Integer(v) => u64::try_from(*v.value()).ok(),
        Value::String(s) => {
            let hex = s.value().strip_prefix("0x")?.replace('_', "");
            u64::from_str_radix(&hex, 16).ok()
        }
        _ => None,
    }
}

/// `value` as a table spells it: hex, quoted past `i64::MAX`.
fn spelled(value: u64) -> String {
    if i64::try_from(value).is_ok() {
        hex(value)
    } else {
        format!("\"{}\"", hex(value))
    }
}

/// What a rendered table holds.
#[derive(Debug, Default)]
pub(crate) struct Patterned {
    /// The symbols, and how many carry a pattern.
    pub(crate) symbols: (usize, usize),
    /// The constants, and how many carry a pattern.
    pub(crate) constants: (usize, usize),
    /// Entries whose anchor gave no pattern, and why.
    pub(crate) problems: Vec<String>,
}

/// The new identity of a rendered table.
#[derive(Debug)]
pub(crate) struct Identity<'a> {
    /// The fingerprint GUID as lowercase hex.
    pub(crate) guid: &'a str,
    /// The fingerprint age.
    pub(crate) age: u32,
    /// The table's name.
    pub(crate) name: &'a str,
    /// Steam's build id.
    pub(crate) steam_build: Option<u64>,
    /// The game's own version number.
    pub(crate) game_version: Option<u32>,
    /// Whether the table is for another build than `src` was: its revision
    /// starts again at 1, and a number given as `None` is dropped rather
    /// than kept as written.
    pub(crate) new_build: bool,
}

/// The table `src` (read as `row`) for the build `exe` is, with `edits`
/// applied and every entry's pattern taken from `exe`; without `exe` the
/// patterns stay as written.
pub(crate) fn render(
    src: &str,
    row: &Row,
    identity: &Identity<'_>,
    edits: &[(&Entry, Edit)],
    exe: Option<&Exe<'_>>,
    anchors: &HashMap<String, Anchor>,
) -> anyhow::Result<(String, Patterned)> {
    let mut doc: DocumentMut = src.parse()?;
    doc["name"] = toml_edit::value(identity.name);
    doc["guid"] = toml_edit::value(identity.guid);
    doc["age"] = toml_edit::value(i64::from(identity.age));
    let numbers = [
        ("steam_build", identity.steam_build),
        ("game_version", identity.game_version.map(u64::from)),
    ];
    for (key, number) in numbers {
        match number {
            Some(n) => doc[key] = toml_edit::value(i64::try_from(n)?),
            None if identity.new_build => drop(doc.remove(key)),
            None => {}
        }
    }
    if identity.new_build && doc.remove("revision").is_some() {
        doc["revision"] = toml_edit::value(1);
    }
    let lists = [&row.checks, &row.symbols, &row.constants];
    for (section, list) in SECTIONS.iter().zip(lists) {
        let Some(table) = doc.get_mut(section).and_then(Item::as_table_mut) else {
            continue;
        };
        let mut dropped = Vec::new();
        for (entry, edit) in edits
            .iter()
            .filter(|(e, _)| list.iter().any(|own| core::ptr::eq(own, *e)))
        {
            match edit {
                Edit::Keep => {}
                Edit::Set(value) => set(table, &entry.id, *value)?,
                Edit::Drop(note) => {
                    table.remove(&entry.id);
                    dropped.push(format!("# pin: {note}\n"));
                }
                Edit::Note(note) => {
                    if let Some(mut key) = table.key_mut(&entry.id) {
                        let decor = key.leaf_decor_mut();
                        let before = decor.prefix().and_then(|p| p.as_str()).unwrap_or("");
                        let prefix = format!("{before}# pin: {note}\n");
                        decor.set_prefix(prefix);
                    }
                }
            }
        }
        if !dropped.is_empty() {
            let decor = table.decor_mut();
            let before = decor.prefix().and_then(|p| p.as_str()).unwrap_or("");
            let prefix = format!("{before}{}", dropped.concat());
            decor.set_prefix(prefix);
        }
    }
    let patterned = match exe {
        Some(exe) => patterns_into(&mut doc, exe, anchors)?,
        None => Patterned::default(),
    };
    Ok((doc.to_string(), patterned))
}

/// Sets entry `id` of `table` to `value`, keeping its pattern fields.
fn set(table: &mut TomlTable, id: &str, value: u64) -> anyhow::Result<()> {
    let written: Value = spelled(value).parse().with_context(|| id.to_string())?;
    let Some(item) = table.get_mut(id) else {
        return Ok(());
    };
    if number(item) == Some(value) {
        return Ok(());
    }
    match item {
        Item::Value(Value::InlineTable(inline)) => {
            let key = if inline.contains_key("rva") {
                "rva"
            } else {
                "value"
            };
            inline.insert(key, written);
        }
        other => *other = Item::Value(written.decorated(" ", "")),
    }
    Ok(())
}

/// What a pattern search gives an entry: its new text, `None` to keep it
/// plain, or why its anchor gave none.
type Written = anyhow::Result<Option<String>>;

/// Writes every symbol's and constant's pattern from `exe` into `doc`: around
/// the entry's anchor where `anchors` names one or its present pattern keeps
/// one; else a symbol's from its own code, and a constant keeps a
/// confirming pattern that still matches once, or stays plain.
fn patterns_into(
    doc: &mut DocumentMut,
    exe: &Exe<'_>,
    anchors: &HashMap<String, Anchor>,
) -> anyhow::Result<Patterned> {
    let present = enshwick_gamedata::Table::parse(&doc.to_string())?;
    let pinned = |id: &str| {
        present
            .symbols()
            .iter()
            .find(|s| s.id() == id)
            .map(enshwick_gamedata::Symbol::rva)
    };
    let mut anchors_here = anchor::of_table(&present, exe, pinned);
    anchors_here.extend(anchors.iter().map(|(id, a)| (id.clone(), a.clone())));
    let (code, _) = exe.text();
    let written = pattern_texts(doc, "constants");
    let confirmed: HashMap<&str, String> = present
        .constants()
        .iter()
        .filter(|c| c.operand().is_none())
        .filter_map(|c| {
            c.pattern()?.find_unique(code).ok()?;
            Some((c.id(), written.get(c.id())?.clone()))
        })
        .collect();
    let symbols = values(doc, "symbols");
    let constants = values(doc, "constants");
    let kept: HashMap<&str, &enshwick_gamedata::Constant> = present
        .constants()
        .iter()
        .filter(|c| c.via().is_some() || c.attribute().is_some() || !c.carried())
        .map(|c| (c.id(), c))
        .collect();
    let failed = || Err(anyhow!("the search failed"));
    let symbol_texts = parallel(
        &symbols,
        |(id, rva)| symbol_written(exe, id, *rva, anchors_here.get(id)),
        failed,
    );
    let constant_texts = parallel(
        &constants,
        |(id, value)| {
            if id.strip_suffix(".bytes").is_some() {
                return Ok(None);
            }
            if let Some(constant) = kept.get(id.as_str()) {
                return kept_written(exe, constant, &symbols, written.get(id));
            }
            match anchors_here.get(id) {
                Some(anchor) => constant_written(exe, id, *value, anchor),
                None => Ok(confirmed.get(id.as_str()).map(|text| {
                    format!("{{ value = {}, pattern = \"{text}\" }}", spelled(*value))
                })),
            }
        },
        failed,
    );
    let mut problems = Vec::new();
    let symbols = write_section(doc, "symbols", &symbols, symbol_texts, hex, &mut problems)?;
    let constants = write_section(
        doc,
        "constants",
        &constants,
        constant_texts,
        spelled,
        &mut problems,
    )?;
    problems.sort();
    Ok(Patterned {
        symbols,
        constants,
        problems,
    })
}

/// The entries of a section with their values.
fn values(doc: &DocumentMut, section: &str) -> Vec<(String, u64)> {
    doc.get(section)
        .and_then(Item::as_table)
        .map(|t| {
            t.iter()
                .filter_map(|(id, item)| Some((id.to_owned(), number(item)?)))
                .collect()
        })
        .unwrap_or_default()
}

/// The pattern of the symbol `id` at `rva`: through its anchor, or from its
/// own code.
fn symbol_written(exe: &Exe<'_>, id: &str, rva: u64, anchor: Option<&Anchor>) -> Written {
    match anchor {
        Some(Anchor::Through(site)) => match patterns::symbol_through(exe, *site) {
            Ok((pattern, target)) if target == rva => Ok(Some(symbol_text(rva, &pattern))),
            Ok((_, target)) => Err(anyhow!(
                "{id}: {site:#x} reaches {target:#x}, not {}",
                hex(rva)
            )),
            Err(why) => Err(anyhow!("{id}: {why}")),
        },
        _ => Ok(patterns::symbol(exe, rva).map(|p| symbol_text(rva, &p))),
    }
}

/// The pattern of the constant `id` around its anchor.
fn constant_written(exe: &Exe<'_>, id: &str, value: u64, anchor: &Anchor) -> Written {
    match anchor {
        Anchor::Read(site, carrier, plus) => {
            let read = anchor::read(exe, anchor);
            if read != Some(value) {
                bail!(
                    "{id}: its anchor reads {read:x?}, the table pins {}",
                    hex(value)
                );
            }
            let pattern =
                patterns::constant_from(exe, *site, *carrier).with_context(|| id.to_string())?;
            Ok(Some(constant_text(value, &pattern, *plus)))
        }
        Anchor::Confirm(site) => {
            let pattern =
                patterns::constant_confirmed(exe, *site).with_context(|| id.to_string())?;
            Ok(Some(constant_text(value, &pattern, 0)))
        }
        other => Err(anyhow!("{id}: a constant cannot take {other:?}")),
    }
}

/// A constant only its own build holds, read from the game's data, or read
/// via a symbol in its code at the symbol's address in `exe`.
fn kept_written(
    exe: &Exe<'_>,
    constant: &enshwick_gamedata::Constant,
    symbols: &[(String, u64)],
    pattern: Option<&String>,
) -> Written {
    let id = constant.id();
    if !constant.carried() {
        return Ok(Some(format!(
            "{{ value = {}, carried = false }}",
            spelled(constant.value())
        )));
    }
    if let Some(name) = constant.attribute() {
        return Ok(Some(format!(
            "{{ value = {}, attribute = \"{name}\" }}",
            spelled(constant.value())
        )));
    }
    let (Some(via), Some(text)) = (constant.via(), pattern) else {
        bail!("{id}: no symbol or pattern to read it via");
    };
    let Some((_, rva)) = symbols.iter().find(|(symbol, _)| symbol == via) else {
        bail!("{id}: the table has no {via}");
    };
    let (code, code_rva) = exe.text();
    let value = rva
        .checked_sub(code_rva)
        .and_then(|at| constant.find_at(code, at))
        .with_context(|| format!("{id}: the code at {via} does not match its pattern"))?;
    let pattern = ConstantPattern {
        text: text.clone(),
        operand: constant.operand().map(|o| (o.at(), o.size())),
        negative: constant
            .operand()
            .is_some_and(enshwick_gamedata::Operand::is_negative),
        shift: constant
            .operand()
            .is_some_and(enshwick_gamedata::Operand::is_shift),
    };
    let plus = constant
        .operand()
        .map_or(0, enshwick_gamedata::Operand::addend);
    let written = constant_text(value, &pattern, plus);
    let within = constant.within().map_or_else(String::new, |n| {
        format!(", within = {}", spelled(u64::from(n)))
    });
    Ok(Some(written.replacen(
        ", pattern",
        &format!(", via = \"{via}\"{within}, pattern"),
        1,
    )))
}

/// Writes `texts` over the entries of `section`, the value spelled plain
/// where an entry has none; how many entries there are and how many carry a
/// pattern.
fn write_section(
    doc: &mut DocumentMut,
    section: &str,
    entries: &[(String, u64)],
    texts: Vec<Written>,
    plain: fn(u64) -> String,
    problems: &mut Vec<String>,
) -> anyhow::Result<(usize, usize)> {
    let found: HashMap<&str, _> = entries
        .iter()
        .zip(texts)
        .map(|((id, value), text)| (id.as_str(), (*value, text)))
        .collect();
    let mut counts = (0, 0);
    let Some(table) = doc.get_mut(section).and_then(Item::as_table_mut) else {
        return Ok(counts);
    };
    for (id, item) in table.iter_mut() {
        let Some((value, text)) = found.get(id.get()) else {
            continue;
        };
        counts.0 += 1;
        let text = match text {
            Ok(Some(text)) => {
                counts.1 += 1;
                text.clone()
            }
            Ok(None) => plain(*value),
            Err(why) => {
                problems.push(format!("{why:#}"));
                plain(*value)
            }
        };
        let value: Value = text.parse().with_context(|| id.get().to_string())?;
        *item = Item::Value(value.decorated(" ", ""));
    }
    Ok(counts)
}

/// A symbol entry with its pattern, as a table spells it.
fn symbol_text(rva: u64, pattern: &SymbolPattern) -> String {
    let locate = match pattern.locate {
        Locate::At(0) => String::new(),
        Locate::At(at) => format!(", at = {at}"),
        Locate::Rel { at, end } if end == at + 4 => format!(", rel = {at}"),
        Locate::Rel { at, end } => format!(", rel = {at}, end = {end}"),
        _ => String::new(),
    };
    format!(
        "{{ rva = {}, pattern = \"{}\"{locate} }}",
        hex(rva),
        pattern.text
    )
}

/// A constant entry with its pattern, as a table spells it.
fn constant_text(value: u64, pattern: &ConstantPattern, plus: i64) -> String {
    let operand = match pattern.operand {
        None => String::new(),
        Some((at, size)) => {
            let neg = match (pattern.negative, pattern.shift) {
                (true, _) => ", neg = true",
                (false, true) => ", shl = true",
                (false, false) => "",
            };
            let plus = if plus == 0 {
                String::new()
            } else {
                format!(", plus = {plus}")
            };
            format!(", at = {at}, size = {size}{neg}{plus}")
        }
    };
    format!(
        "{{ value = {}, pattern = \"{}\"{operand} }}",
        spelled(value),
        pattern.text
    )
}

/// The pattern texts written in a section, by id.
fn pattern_texts(doc: &DocumentMut, section: &str) -> HashMap<String, String> {
    doc.get(section)
        .and_then(Item::as_table)
        .map(|t| {
            t.iter()
                .filter_map(|(id, item)| {
                    let Item::Value(Value::InlineTable(inline)) = item else {
                        return None;
                    };
                    Some((id.to_owned(), inline.get("pattern")?.as_str()?.to_owned()))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Runs `f` over `items` on all cores, in order; a part whose thread
/// panicked gives `failed` for each of its items.
fn parallel<T: Sync, R: Send>(
    items: &[T],
    f: impl Fn(&T) -> R + Sync,
    failed: impl Fn() -> R,
) -> Vec<R> {
    let threads = std::thread::available_parallelism().map_or(1, usize::from);
    let chunk = items.len().div_ceil(threads).max(1);
    std::thread::scope(|scope| {
        let workers: Vec<_> = items
            .chunks(chunk)
            .map(|part| {
                (
                    part.len(),
                    scope.spawn(|| part.iter().map(&f).collect::<Vec<_>>()),
                )
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|(len, w)| {
                w.join()
                    .unwrap_or_else(|_| core::iter::repeat_with(&failed).take(len).collect())
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &str = r#"# A table.

name = "1.0 server (1)"
side = "server"
guid = "1c129660cd6efc4f9f0b21652d5ff2bf"
age = 387
steam_build = 1
game_version = 5
revision = 3

[checks]
"upgrade_building_range" = 0x90a60

# Symbols.
[symbols]
"ecs.iterator_init" = 0x65_b8c0
# A comment the new table keeps.
"npc.move" = { rva = 0x1e_ca40, pattern = "48 8B", at = 2 }

[constants]
"ecs.context_row" = 8
"inventory.copy_a8" = 0x18_0000_00a8
"#;

    #[test]
    fn a_table_reads_into_a_row() {
        let row = parse(TABLE).unwrap();
        assert_eq!(row.side, Side::Server);
        assert_eq!(row.age, 387);
        assert_eq!(row.name.as_deref(), Some("1.0 server (1)"));
        assert_eq!(row.checks[0].value, 0x90a60);
        let ids: Vec<&str> = row.symbols.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["ecs.iterator_init", "npc.move"]);
        assert_eq!(row.symbols[1].value, 0x1e_ca40);
        assert_eq!(row.constants[1].value, 0x18 << 32 | 0xa8);
    }

    #[test]
    fn edits_keep_comments_and_drop_with_a_note() {
        let row = parse(TABLE).unwrap();
        let mut doc: DocumentMut = TABLE.parse().unwrap();
        let table = doc["symbols"].as_table_mut().unwrap();
        set(table, "ecs.iterator_init", 0x66_0000).unwrap();
        set(table, "npc.move", 0x1f_0000).unwrap();
        let text = doc.to_string();
        assert!(text.contains("\"ecs.iterator_init\" = 0x66_0000\n"));
        assert!(
            text.contains("# A comment the new table keeps.\n\"npc.move\" = { rva = 0x1f_0000")
        );
        assert_eq!(row.symbols.len(), 2);
    }

    #[test]
    fn a_rendered_table_carries_its_new_identity_and_edits() {
        let row = parse(TABLE).unwrap();
        let edits = [
            (&row.symbols[0], Edit::Set(0x66_0000)),
            (
                &row.symbols[1],
                Edit::Drop("npc.move missing: gone; was 0x1e_ca40".into()),
            ),
            (
                &row.constants[0],
                Edit::Note("ecs.context_row flagged".into()),
            ),
            (&row.checks[0], Edit::Set(0x90a60)),
        ];
        let identity = Identity {
            guid: &"ab".repeat(16),
            age: 400,
            name: "1.0 server (2)",
            steam_build: Some(2),
            game_version: None,
            new_build: true,
        };
        let (text, _) = render(TABLE, &row, &identity, &edits, None, &HashMap::new()).unwrap();
        assert!(text.contains(
            "name = \"1.0 server (2)\"
"
        ));
        assert!(text.contains(&format!(
            "guid = \"{}\"
",
            "ab".repeat(16)
        )));
        assert!(text.contains(
            "age = 400
steam_build = 2
revision = 1
"
        ));
        assert!(text.contains(
            "\"ecs.iterator_init\" = 0x66_0000
"
        ));
        assert!(!text.contains("\"npc.move\""));
        assert!(text.contains(
            "# Symbols.
# pin: npc.move missing: gone; was 0x1e_ca40
[symbols]"
        ));
        assert!(text.contains(
            "# pin: ecs.context_row flagged
\"ecs.context_row\" = 8"
        ));
        let reread = parse(&text).unwrap();
        assert_eq!(reread.age, 400);
        assert_eq!(reread.symbols.len(), 1);
    }

    #[test]
    fn a_new_build_takes_both_numbers_given_and_drops_the_old_ones() {
        let row = parse(TABLE).unwrap();
        let identity = |steam_build, game_version| Identity {
            guid: &row.guid,
            age: 400,
            name: "1.0 server (2)",
            steam_build,
            game_version,
            new_build: true,
        };
        let text = |src: &str, identity| {
            render(src, &row, &identity, &[], None, &HashMap::new())
                .unwrap()
                .0
        };
        let both = text(TABLE, identity(Some(2), Some(6)));
        assert!(both.contains("steam_build = 2\ngame_version = 6\nrevision = 1\n"));
        let added = text(
            &TABLE.replace("game_version = 5\n", ""),
            identity(Some(2), Some(6)),
        );
        assert!(added.contains("steam_build = 2\ngame_version = 6\nrevision = 1\n"));
        let neither = text(TABLE, identity(None, None));
        assert!(!neither.contains("steam_build"));
        assert!(!neither.contains("game_version"));
    }
}
