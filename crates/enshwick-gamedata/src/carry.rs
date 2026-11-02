//! A carried table saved for the next start on the same code.

use core::ops::Range;

use twox_hash::XxHash3_128;

use crate::{Kind, Resolved, Table};

/// The first line of a saved carried table; a file without it is not read.
const HEADER: &str = "enshwick carried table 2";
/// The first word of a line for an entry whose pattern matched once, before
/// its id and where, in hex.
const START: &str = "start";
/// The first word of a line for an entry whose pattern did not match once,
/// before its id.
const NONE: &str = "none";

/// Which code and which table a carried table was found for: the XXH3-128
/// of the table's text, the code's RVA and every byte of the code.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::Table;
///
/// let table = Table::parse(
///     r#"
///     name = "1.0 server (1)"
///     side = "server"
///     guid = "00000000000000000000000000000001"
///     age = 2
///     "#,
/// )?;
/// assert_eq!(table.carry_key(&[0xc3], 0x1000, 0..0), table.carry_key(&[0xc3], 0x1000, 0..0));
/// assert_ne!(table.carry_key(&[0xc3], 0x1000, 0..0), table.carry_key(&[0xcc], 0x1000, 0..0));
/// assert_eq!(table.carry_key(&[0xc3, 1], 0x1000, 1..2), table.carry_key(&[0xc3, 2], 0x1000, 1..2));
/// # Ok::<(), enshwick_gamedata::TableError>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CarryKey(u128);

impl core::fmt::Display for CarryKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:032x}", self.0)
    }
}

impl Table {
    /// The key a table carried to `code`, the code section that starts at
    /// `code_rva`, is saved and found under; the bytes of `code` in `skip`,
    /// which change from start to start, are left out.
    #[must_use]
    pub fn carry_key(&self, code: &[u8], code_rva: u64, skip: Range<usize>) -> CarryKey {
        let end = skip.end.min(code.len());
        let start = skip.start.min(end);
        let mut hasher = XxHash3_128::new();
        hasher.write(&self.digest.to_le_bytes());
        hasher.write(&code_rva.to_le_bytes());
        hasher.write(&code[..start]);
        hasher.write(&code[end..]);
        CarryKey(hasher.finish_128())
    }

    /// The table carried to `code`, the code section that starts at
    /// `code_rva`, from the text [`Resolved::to_saved`] wrote for `key`:
    /// [`Table::fall_back`] without its search, since the text says where
    /// each pattern matched. Every entry is checked against `code` as the
    /// search would check it, and `cell` reads the cells again. `None` for
    /// another key, another table, a damaged file or a pattern that does not
    /// match where the text says, which [`Table::fall_back`] then replaces.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_gamedata::Table;
    ///
    /// let table = Table::parse(
    ///     r#"
    ///     name = "1.0 server (1)"
    ///     side = "server"
    ///     guid = "00000000000000000000000000000001"
    ///     age = 2
    ///     [symbols]
    ///     "f.ret" = { rva = 0x1000, pattern = "CC C3" }
    ///     "#,
    /// )?;
    /// let code = [0x90, 0xcc, 0xc3];
    /// let key = table.carry_key(&code, 0x2000, 0..0);
    /// let carried = table.fall_back(&code, 0x2000, |_| None);
    /// let saved = carried.to_saved(key).ok_or("not carried")?;
    /// assert_eq!(table.restore(&saved, key, &code, 0x2000, |_| None), Some(carried));
    /// let other = table.carry_key(&[0xc3], 0x2000, 0..0);
    /// assert_eq!(table.restore(&saved, other, &code, 0x2000, |_| None), None);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub fn restore(
        &self,
        saved: &str,
        key: CarryKey,
        code: &[u8],
        code_rva: u64,
        cell: impl Fn(u64) -> Option<u32> + Sync,
    ) -> Option<Resolved> {
        let mut lines = saved.lines();
        if lines.next()? != HEADER || lines.next()? != format!("key {key}") {
            return None;
        }
        let lines: Vec<&str> = lines.collect();
        if lines.len() != self.carry_patterns().count() {
            return None;
        }
        // shortcut: a start is checked to match, not to match once. The key
        // pins every code byte but the entry hold, where the search found it
        // once, so only a deliberate edit, or a second match inside the few
        // bytes of the hold, could name a second match; proving it once costs
        // the search the save is there to spare.
        let starts = lines
            .iter()
            .zip(self.carry_patterns())
            .map(|(line, (id, pattern))| {
                let (word, rest) = line.split_once(' ')?;
                let (named, at) = rest.split_once(' ').unwrap_or((rest, ""));
                if named != id {
                    return None;
                }
                match word {
                    START if !at.is_empty() && at.bytes().all(|b| b.is_ascii_hexdigit()) => {
                        let at = usize::from_str_radix(at, 16).ok()?;
                        pattern.matches(code.get(at..)?).then_some(Some(at))
                    }
                    NONE if at.is_empty() => Some(None),
                    _ => None,
                }
            })
            .collect::<Option<Vec<_>>>()?;
        Some(self.carried_at(code, code_rva, starts, cell))
    }
}

impl Resolved {
    /// The text [`Table::restore`] reads back for `key`; `None` for a
    /// table made for its build, which needs no saving.
    #[must_use]
    pub fn to_saved(&self, key: CarryKey) -> Option<String> {
        if self.kind != Kind::Fallback {
            return None;
        }
        let starts = self.starts.iter().map(|(id, start)| match start {
            Some(at) => format!("{START} {id} {at:x}"),
            None => format!("{NONE} {id}"),
        });
        let lines: Vec<String> = [HEADER.to_owned(), format!("key {key}")]
            .into_iter()
            .chain(starts)
            .collect();
        let mut text = lines.join("\n");
        text.push('\n');
        Some(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &str = r#"
        name = "test (2)"
        side = "server"
        guid = "000102030405060708090a0b0c0d0e0f"
        age = 7
        [symbols]
        "f.found" = { rva = 0x1004, pattern = "11 22 ?? 44", at = 1 }
        "f.twice" = { rva = 0x1010, pattern = "AA" }
        [constants]
        "c.read" = { value = 0x48, pattern = "8B 48 ?? 48", at = 2, size = 1 }
        "c.gone" = { value = 0x50, pattern = "8B 50", fixed = true }
        "c.only" = { value = 3, carried = false }
    "#;

    fn code() -> Vec<u8> {
        let mut code = vec![0u8; 64];
        code[8..12].copy_from_slice(&[0x11, 0x22, 0x99, 0x44]);
        code[20] = 0xaa;
        code[30] = 0xaa;
        code[40..44].copy_from_slice(&[0x8b, 0x48, 0x01, 0x48]);
        code
    }

    /// `table` restored from `saved` for `key` on [`code`] at `0x5000`.
    fn restore(table: &Table, saved: &str, key: CarryKey) -> Option<Resolved> {
        table.restore(saved, key, &code(), 0x5000, |_| None)
    }

    #[test]
    fn a_saved_carried_table_restores_equal_for_the_same_code() {
        let table = Table::parse(TABLE).unwrap();
        let code = code();
        let key = table.carry_key(&code, 0x5000, 0..0);
        let carried = table.fall_back(&code, 0x5000, |_| None);
        let saved = carried.to_saved(key).unwrap();
        assert_eq!(
            saved,
            format!(
                "{HEADER}\nkey {key}\nstart f.found 8\nnone f.twice\nnone c.gone\nstart c.read 28\n"
            )
        );
        assert_eq!(restore(&table, &saved, key), Some(carried));
    }

    #[test]
    fn other_code_another_rva_or_another_table_misses_the_saved_one() {
        let table = Table::parse(TABLE).unwrap();
        let mut code = code();
        let key = table.carry_key(&code, 0x5000, 0..0);
        let saved = table
            .fall_back(&code, 0x5000, |_| None)
            .to_saved(key)
            .unwrap();
        assert_ne!(table.carry_key(&code, 0x6000, 0..0), key);
        let edited = Table::parse(&TABLE.replace("0x1010", "0x1011")).unwrap();
        assert_ne!(edited.carry_key(&code, 0x5000, 0..0), key);
        assert_eq!(
            restore(&edited, &saved, edited.carry_key(&code, 0x5000, 0..0)),
            None
        );
        code[63] = 1;
        assert_ne!(table.carry_key(&code, 0x5000, 0..0), key);
    }

    #[test]
    fn a_damaged_or_incomplete_save_is_not_read() {
        let table = Table::parse(TABLE).unwrap();
        let code = code();
        let key = table.carry_key(&code, 0x5000, 0..0);
        let saved = table
            .fall_back(&code, 0x5000, |_| None)
            .to_saved(key)
            .unwrap();
        let edited = |from: &str, to: &str| restore(&table, &saved.replace(from, to), key);
        assert!(restore(&table, &saved, key).is_some());
        assert_eq!(edited("none f.twice\n", ""), None);
        assert_eq!(
            restore(&table, &format!("{saved}none f.twice\n"), key),
            None
        );
        assert_eq!(edited("start f.found 8", "start f.found 8 9"), None);
        assert_eq!(edited("start f.found 8", "start f.found z8"), None);
        assert_eq!(edited("start f.found 8", "start f.found +8"), None);
        assert_eq!(edited("start f.found 8", "start f.found"), None);
        assert_eq!(edited("start f.found 8", "start f.found ffffff"), None);
        assert_eq!(edited("none f.twice", "none f.twice 4"), None);
        assert_eq!(edited("none f.twice", "None f.twice"), None);
        assert_eq!(edited(HEADER, "enshwick carried table 1"), None);
        assert_eq!(restore(&table, "", key), None);
    }

    #[test]
    fn a_save_whose_lines_name_other_entries_is_not_read() {
        let table = Table::parse(TABLE).unwrap();
        let code = code();
        let key = table.carry_key(&code, 0x5000, 0..0);
        let saved = table
            .fall_back(&code, 0x5000, |_| None)
            .to_saved(key)
            .unwrap();
        let swapped = saved
            .replace("none f.twice", "none X")
            .replace("none c.gone", "none f.twice")
            .replace("none X", "none c.gone");
        assert_eq!(restore(&table, &swapped, key), None);
    }

    #[test]
    fn a_save_whose_symbol_or_constant_the_code_does_not_hold_is_not_read() {
        let table = Table::parse(TABLE).unwrap();
        let code = code();
        let key = table.carry_key(&code, 0x5000, 0..0);
        let saved = table
            .fall_back(&code, 0x5000, |_| None)
            .to_saved(key)
            .unwrap();
        let edited = |from: &str, to: &str| restore(&table, &saved.replace(from, to), key);
        assert_eq!(edited("start f.found 8", "start f.found 9"), None);
        assert_eq!(edited("start c.read 28", "start c.read 29"), None);
    }

    #[test]
    fn a_table_made_for_its_build_is_not_saved() {
        let table = Table::parse(TABLE).unwrap();
        assert_eq!(
            table.resolve().to_saved(table.carry_key(&[], 0, 0..0)),
            None
        );
    }
}
