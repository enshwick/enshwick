//! Changes to a recipe file, and the split of disabled ids into messages.

use super::{EditRulesError, Kind};

/// `text` with `name` added to the `[disable]` (`disable`) or `[enable]`
/// list of `kind`, and taken out of the other; comments kept.
///
/// # Errors
///
/// When `text` is no TOML or its lists are no lists.
pub fn edit_file(
    text: &str,
    kind: Kind,
    name: &str,
    disable: bool,
) -> Result<String, EditRulesError> {
    let mut doc: toml_edit::DocumentMut = text.parse()?;
    let (to, from) = if disable {
        ("disable", "enable")
    } else {
        ("enable", "disable")
    };
    if let Some(list) = doc
        .get_mut(from)
        .and_then(|t| t.get_mut(kind.key()))
        .and_then(toml_edit::Item::as_array_mut)
    {
        list.retain(|v| v.as_str() != Some(name));
    }
    let table = doc
        .entry(to)
        .or_insert_with(toml_edit::table)
        .as_table_like_mut()
        .ok_or(EditRulesError::NotTable { table: to })?;
    let list = table
        .entry(kind.key())
        .or_insert(toml_edit::value(toml_edit::Array::new()))
        .as_array_mut()
        .ok_or(EditRulesError::NotList {
            table: to,
            key: kind.key(),
        })?;
    if !list.iter().any(|v| v.as_str() == Some(name)) {
        list.push(name);
    }
    Ok(doc.to_string())
}

/// Splits ids into the messages a client takes: the first and last flags
/// and up to `per` ids each, at least one message.
#[must_use]
pub fn parts(ids: &[u32], per: usize) -> Vec<(bool, bool, Vec<u32>)> {
    let chunks: Vec<&[u32]> = if ids.is_empty() {
        vec![&[]]
    } else {
        ids.chunks(per.max(1)).collect()
    };
    let last = chunks.len() - 1;
    chunks
        .into_iter()
        .enumerate()
        .map(|(i, chunk)| (i == 0, i == last, chunk.to_vec()))
        .collect()
}
