//! A carried table saved for the next start on the same code.

use rustc_hash::FxHashMap;
use twox_hash::XxHash3_128;

use crate::{Kind, Resolved, Table};

/// The first line of a saved carried table; a file without it is not read.
const HEADER: &str = "enshwick carried table 1";

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
/// assert_eq!(table.carry_key(&[0xc3], 0x1000), table.carry_key(&[0xc3], 0x1000));
/// assert_ne!(table.carry_key(&[0xc3], 0x1000), table.carry_key(&[0xcc], 0x1000));
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
    /// `code_rva`, is saved and found under.
    #[must_use]
    pub fn carry_key(&self, code: &[u8], code_rva: u64) -> CarryKey {
        let mut hasher = XxHash3_128::new();
        hasher.write(&self.digest.to_le_bytes());
        hasher.write(&code_rva.to_le_bytes());
        hasher.write(code);
        CarryKey(hasher.finish_128())
    }

    /// The table carried to the code `key` was made for, from the text
    /// [`Resolved::to_saved`] wrote; `None` for another key, another table
    /// or a damaged file, which [`Table::fall_back`] then replaces.
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
    /// let key = table.carry_key(&code, 0x2000);
    /// let carried = table.fall_back(&code, 0x2000, |_| None);
    /// let saved = carried.to_saved(key).ok_or("not carried")?;
    /// assert_eq!(table.restore(&saved, key), Some(carried));
    /// assert_eq!(table.restore(&saved, table.carry_key(&[0xc3], 0x2000)), None);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub fn restore(&self, saved: &str, key: CarryKey) -> Option<Resolved> {
        let mut lines = saved.lines();
        if lines.next()? != HEADER || lines.next()? != format!("key {key}") {
            return None;
        }
        let mut found: FxHashMap<&str, (Entry, Option<u64>)> = FxHashMap::default();
        for line in lines {
            let mut words = line.split(' ');
            let (entry, id) = (words.next()?, words.next()?);
            let (entry, value) = match entry {
                "symbol" => (Entry::Symbol, Some(hex(words.next()?)?)),
                "constant" => (Entry::Constant, Some(hex(words.next()?)?)),
                "missing" => (Entry::Missing, None),
                _ => return None,
            };
            if words.next().is_some() || found.insert(id, (entry, value)).is_some() {
                return None;
            }
        }
        let carried: Vec<_> = self.constants.iter().filter(|c| c.carried).collect();
        if found.len() != self.symbols.len() + carried.len() {
            return None;
        }
        let mut symbols = Vec::new();
        let mut constants = Vec::new();
        let mut missing = Vec::new();
        for symbol in &self.symbols {
            match found.get(symbol.id.as_str())? {
                (Entry::Symbol, Some(rva)) => symbols.push((symbol.id.clone(), *rva)),
                (Entry::Missing, _) => missing.push(symbol.id.clone()),
                _ => return None,
            }
        }
        for constant in carried {
            match found.get(constant.id.as_str())? {
                (Entry::Constant, Some(value)) => constants.push((constant.id.clone(), *value)),
                (Entry::Missing, _) => missing.push(constant.id.clone()),
                _ => return None,
            }
        }
        Some(self.carried(symbols, constants, missing))
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
        let symbols = self
            .symbols
            .iter()
            .map(|(id, rva)| format!("symbol {id} {rva:x}"));
        let constants = self
            .constants
            .iter()
            .map(|(id, value)| format!("constant {id} {value:x}"));
        let missing = self.missing.iter().map(|id| format!("missing {id}"));
        let lines: Vec<String> = [HEADER.to_owned(), format!("key {key}")]
            .into_iter()
            .chain(symbols)
            .chain(constants)
            .chain(missing)
            .collect();
        let mut text = lines.join("\n");
        text.push('\n');
        Some(text)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Entry {
    Symbol,
    Constant,
    Missing,
}

fn hex(word: &str) -> Option<u64> {
    u64::from_str_radix(word, 16).ok()
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

    #[test]
    fn a_saved_carried_table_restores_equal_for_the_same_code() {
        let table = Table::parse(TABLE).unwrap();
        let code = code();
        let key = table.carry_key(&code, 0x5000);
        let carried = table.fall_back(&code, 0x5000, |_| None);
        let saved = carried.to_saved(key).unwrap();
        assert_eq!(table.restore(&saved, key), Some(carried));
    }

    #[test]
    fn other_code_another_rva_or_another_table_misses_the_saved_one() {
        let table = Table::parse(TABLE).unwrap();
        let mut code = code();
        let key = table.carry_key(&code, 0x5000);
        let saved = table
            .fall_back(&code, 0x5000, |_| None)
            .to_saved(key)
            .unwrap();
        assert_ne!(table.carry_key(&code, 0x6000), key);
        let edited = Table::parse(&TABLE.replace("0x1010", "0x1011")).unwrap();
        assert_ne!(edited.carry_key(&code, 0x5000), key);
        assert_eq!(
            edited.restore(&saved, edited.carry_key(&code, 0x5000)),
            None
        );
        code[63] = 1;
        assert_ne!(table.carry_key(&code, 0x5000), key);
    }

    #[test]
    fn a_damaged_or_incomplete_save_is_not_read() {
        let table = Table::parse(TABLE).unwrap();
        let code = code();
        let key = table.carry_key(&code, 0x5000);
        let saved = table
            .fall_back(&code, 0x5000, |_| None)
            .to_saved(key)
            .unwrap();
        let without = |line: &str| saved.replace(&format!("{line}\n"), "");
        assert!(table.restore(&saved, key).is_some());
        assert_eq!(table.restore(&without("missing f.twice"), key), None);
        assert_eq!(
            table.restore(&format!("{saved}missing f.twice\n"), key),
            None
        );
        assert_eq!(
            table.restore(&format!("{saved}missing c.only\n"), key),
            None
        );
        assert_eq!(
            table.restore(&saved.replace("symbol f.found", "constant f.found"), key),
            None
        );
        assert_eq!(
            table.restore(&saved.replace(HEADER, "enshwick carried table 0"), key),
            None
        );
        assert_eq!(table.restore(&saved.replace(" 5009", " 50z9"), key), None);
        assert_eq!(table.restore("", key), None);
    }

    #[test]
    fn a_table_made_for_its_build_is_not_saved() {
        let table = Table::parse(TABLE).unwrap();
        assert_eq!(table.resolve().to_saved(table.carry_key(&[], 0)), None);
    }
}
