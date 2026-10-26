//! Reading a pack's changes of vanilla values.

use std::collections::HashMap;

use super::ITEMS_FILE;
use super::entries::{ItemsFile, ValueEntry};
use super::files::PackError;
use super::pack::Reader;
use crate::content::Guid;
use crate::content::changes::{Change, Field, Value};

/// The value a pack entry gives.
fn value(entry: ValueEntry) -> Value {
    match entry {
        ValueEntry::Set(v) => Value::Set(v),
        ValueEntry::Scale(scale) => Value::Times(scale.times),
    }
}

/// The checked changes of `items_file`, each with its source,
/// `items.toml:<line>`, followed by the name the pack wrote when it is no
/// GUID: `items.toml:12 (Wood_Log)`.
pub(super) fn item_changes(
    reader: &mut Reader<'_>,
    items_file: &ItemsFile,
    text: &str,
) -> Vec<(Change, String)> {
    let file = ITEMS_FILE;
    let mut out = Vec::new();
    let mut seen: HashMap<(Guid, Field), usize> = HashMap::new();
    for (index, entry) in items_file.change.iter().enumerate() {
        let at = entry.span().start;
        let line = PackError::at(file, text, at, "").line;
        let e = entry.get_ref();
        let number = index + 1;
        let fields: Vec<(Field, Value)> = e
            .max_stack
            .map(|v| (Field::MaxStack, value(v)))
            .into_iter()
            .collect();
        if e.items.is_empty() {
            reader.error(
                file,
                text,
                at,
                format!("change {number}: items names no item"),
            );
        }
        if fields.is_empty() {
            let names: Vec<&str> = Field::ALL.iter().map(|f| f.name()).collect();
            reader.error(
                file,
                text,
                at,
                format!(
                    "change {number}: changes nothing; give {}",
                    names.join(", ")
                ),
            );
        }
        let mut valid = Vec::new();
        for &(field, value) in &fields {
            match field.check(value) {
                Ok(()) => valid.push((field, value)),
                Err(why) => reader.error(file, text, at, format!("change {number}: {why}")),
            }
        }
        for name in &e.items {
            let Some((item, _)) = reader.reference(file, text, at, name, "item", "") else {
                continue;
            };
            for &(field, value) in &valid {
                if let Some(&first) = seen.get(&(item, field)) {
                    reader.error(
                        file,
                        text,
                        at,
                        format!(
                            "change {number}: {field} of {name} is changed at line {first} already"
                        ),
                    );
                    continue;
                }
                seen.insert((item, field), line);
                let source = if Guid::try_parse(name).is_some() {
                    format!("{file}:{line}")
                } else {
                    format!("{file}:{line} ({name})")
                };
                out.push((Change::new(item, field, value), source));
            }
        }
    }
    out
}
