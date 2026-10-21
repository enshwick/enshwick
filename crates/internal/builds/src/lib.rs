//! The loader's build tables: per game build, the engine functions and
//! constants that reflection does not describe, by symbolic id.
//!
//! A table is a TOML file named after the build's fingerprint (see
//! [`file_name`]). The loader embeds the tables of `tables/` ([`embedded`])
//! and reads the ones the updater fetched, each with a minisign signature by
//! the project key ([`verify`]). For the running build it takes the table
//! made for it ([`exact`], [`Table::resolve`]); on a build without one, the
//! newest table of the same side ([`newest`]) finds each entry by its pattern
//! ([`Table::fall_back`]), and what no pattern finds stays missing.

use std::collections::BTreeMap;

use enshwick_loader_api::code::Pattern;
pub use enshwick_loader_reflect::Fingerprint;
use minisign_verify::{PublicKey, Signature};

/// Enshwick's release key, which signs every table the release workflow
/// publishes.
pub const PROJECT_KEY: &str = include_str!("../../../../mods/enshwick-updater/keys/enshwick.pub");

/// The first word of a table signature's trusted comment; the second is the
/// table's file name.
pub const SIGNATURE_TAG: &str = "build-table";

/// The suffix of a table's signature file, after the table's own name.
pub const SIGNATURE_SUFFIX: &str = ".minisig";

/// The binary a table is for.
///
/// # Examples
///
/// ```
/// use enshwick_loader_builds::Side;
///
/// assert_eq!(Side::Server.as_str(), "server");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    /// The dedicated server.
    Server,
    /// The game client.
    Client,
}

impl Side {
    /// The side as table files spell it.
    ///
    /// # Examples
    ///
    /// See [`Side`].
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Server => "server",
            Self::Client => "client",
        }
    }
}

/// Why a table does not read.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TableError {
    /// The text is no table.
    #[error("{0}")]
    Toml(String),
    /// The fingerprint GUID is not 32 lowercase hex digits.
    #[error("guid {0:?} is not 32 lowercase hex digits")]
    Guid(String),
    /// A pattern does not parse.
    #[error("{id}: {reason}")]
    Pattern {
        /// The entry.
        id: String,
        /// Why.
        reason: String,
    },
}

/// Why [`verify`] refuses a table.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum VerifyError {
    /// The signature file does not read.
    #[error("unreadable signature: {0}")]
    Unreadable(String),
    /// The project key did not sign it.
    #[error("the project key did not sign it")]
    Unsigned,
    /// It was signed for another file.
    #[error("it was signed for \"{0}\"")]
    OtherFile(String),
}

/// A pinned engine function or address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Symbol {
    id: String,
    rva: u64,
    pattern: Option<Pattern>,
    at: u64,
}

impl Symbol {
    /// The symbolic id, such as `npc.lookup`.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The address relative to the image base.
    #[must_use]
    pub const fn rva(&self) -> u64 {
        self.rva
    }

    /// Code that finds the symbol on another build, and how far past the
    /// match's start the symbol lies.
    #[must_use]
    pub const fn pattern(&self) -> Option<(&Pattern, u64)> {
        match &self.pattern {
            Some(pattern) => Some((pattern, self.at)),
            None => None,
        }
    }
}

/// A pinned engine constant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Constant {
    id: String,
    value: u64,
    pattern: Option<Pattern>,
}

impl Constant {
    /// The symbolic id, such as `ecs.context_row`.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The value.
    #[must_use]
    pub const fn value(&self) -> u64 {
        self.value
    }

    /// Code that uses the value, with the value's bytes fixed.
    #[must_use]
    pub const fn pattern(&self) -> Option<&Pattern> {
        self.pattern.as_ref()
    }
}

/// One build's table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    name: String,
    side: Side,
    fingerprint: Fingerprint,
    build: Option<u64>,
    revision: u32,
    checks: Vec<(String, u64)>,
    symbols: Vec<Symbol>,
    constants: Vec<Constant>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TableFile {
    name: String,
    side: Side,
    guid: String,
    age: u32,
    #[serde(default)]
    build: Option<u64>,
    #[serde(default = "first_revision")]
    revision: u32,
    #[serde(default)]
    checks: BTreeMap<String, u64>,
    #[serde(default)]
    symbols: BTreeMap<String, SymbolValue>,
    #[serde(default)]
    constants: BTreeMap<String, ConstantValue>,
}

const fn first_revision() -> u32 {
    1
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum SymbolValue {
    Plain(u64),
    Full(SymbolFull),
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SymbolFull {
    rva: u64,
    pattern: Option<String>,
    #[serde(default)]
    at: u64,
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum ConstantValue {
    Plain(Number),
    Full(ConstantFull),
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ConstantFull {
    value: Number,
    pattern: Option<String>,
}

/// A TOML integer, or a hex string for a value past `i64::MAX`, since TOML
/// integers are signed.
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum Number {
    Integer(u64),
    Text(String),
}

impl Number {
    fn value(self, id: &str) -> Result<u64, TableError> {
        match self {
            Self::Integer(value) => Ok(value),
            Self::Text(text) => text
                .strip_prefix("0x")
                .and_then(|hex| u64::from_str_radix(&hex.replace('_', ""), 16).ok())
                .ok_or_else(|| TableError::Toml(format!("{id}: {text:?} is no hex number"))),
        }
    }
}

fn pattern(id: &str, text: Option<String>) -> Result<Option<Pattern>, TableError> {
    text.map(|text| {
        Pattern::parse(&text).map_err(|e| TableError::Pattern {
            id: id.to_owned(),
            reason: e.to_string(),
        })
    })
    .transpose()
}

impl Table {
    /// Reads a table file.
    ///
    /// # Errors
    ///
    /// [`TableError`] when the text is no table.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_builds::{Side, Table};
    ///
    /// let table = Table::parse(
    ///     r#"
    ///     name = "1.0 server (1)"
    ///     side = "server"
    ///     guid = "00000000000000000000000000000001"
    ///     age = 2
    ///     [symbols]
    ///     "npc.lookup" = 0x20_2e80
    ///     "#,
    /// )?;
    /// assert_eq!(table.side(), Side::Server);
    /// assert_eq!(table.symbols()[0].rva(), 0x20_2e80);
    /// # Ok::<(), enshwick_loader_builds::TableError>(())
    /// ```
    pub fn parse(text: &str) -> Result<Self, TableError> {
        let file: TableFile = toml::from_str(text).map_err(|e| TableError::Toml(e.to_string()))?;
        let guid = parse_guid(&file.guid).ok_or_else(|| TableError::Guid(file.guid.clone()))?;
        let symbols = file
            .symbols
            .into_iter()
            .map(|(id, value)| {
                let (rva, text, at) = match value {
                    SymbolValue::Plain(rva) => (rva, None, 0),
                    SymbolValue::Full(full) => (full.rva, full.pattern, full.at),
                };
                Ok(Symbol {
                    pattern: pattern(&id, text)?,
                    id,
                    rva,
                    at,
                })
            })
            .collect::<Result<_, TableError>>()?;
        let constants = file
            .constants
            .into_iter()
            .map(|(id, value)| {
                let (value, text) = match value {
                    ConstantValue::Plain(value) => (value.value(&id)?, None),
                    ConstantValue::Full(full) => (full.value.value(&id)?, full.pattern),
                };
                Ok(Constant {
                    pattern: pattern(&id, text)?,
                    id,
                    value,
                })
            })
            .collect::<Result<_, TableError>>()?;
        Ok(Self {
            name: file.name,
            side: file.side,
            fingerprint: Fingerprint::new(guid, file.age),
            build: file.build,
            revision: file.revision,
            checks: file.checks.into_iter().collect(),
            symbols,
            constants,
        })
    }

    /// The name the log shows, such as `1.0 server (1269579)`.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The binary the table is for.
    #[must_use]
    pub const fn side(&self) -> Side {
        self.side
    }

    /// The build the table is for.
    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }

    /// Steam's build id, when the table names it.
    #[must_use]
    pub const fn build(&self) -> Option<u64> {
        self.build
    }

    /// Raised when a table is republished for the same build; the highest
    /// wins.
    #[must_use]
    pub const fn revision(&self) -> u32 {
        self.revision
    }

    /// Systems whose reflected entry must equal the pinned address, sorted
    /// by name.
    #[must_use]
    pub fn checks(&self) -> &[(String, u64)] {
        &self.checks
    }

    /// The symbols, sorted by id.
    #[must_use]
    pub fn symbols(&self) -> &[Symbol] {
        &self.symbols
    }

    /// The constants, sorted by id.
    #[must_use]
    pub fn constants(&self) -> &[Constant] {
        &self.constants
    }

    /// The file this table is published as.
    #[must_use]
    pub fn file_name(&self) -> String {
        file_name(self.side, self.fingerprint)
    }

    /// The table as the build it was made for uses it: every entry.
    #[must_use]
    pub fn resolve(&self) -> Resolved {
        Resolved {
            name: self.name.clone(),
            kind: Kind::Exact,
            checks: self.checks.clone(),
            symbols: self.symbols.iter().map(|s| (s.id.clone(), s.rva)).collect(),
            constants: self
                .constants
                .iter()
                .map(|c| (c.id.clone(), c.value))
                .collect(),
            missing: Vec::new(),
        }
    }

    /// The table carried to a build without its own: each entry found by
    /// its pattern in `code`, the code section that starts at `code_rva`.
    /// A symbol counts when its pattern matches exactly once and the code
    /// there still starts with the bytes its `<id>.bytes` constant pins, if
    /// any; a constant counts when the code that used it is still found
    /// once. Everything else is [`Resolved::missing`]. The checks do not
    /// apply.
    #[must_use]
    pub fn fall_back(&self, code: &[u8], code_rva: u64) -> Resolved {
        let found_symbols = search(&self.symbols, |s| {
            let (pattern, at) = s.pattern()?;
            let start = pattern.find_unique(code).ok()?;
            let offset = start as u64 + at;
            let bytes_ok = self
                .constant_value(&format!("{}.bytes", s.id))
                .is_none_or(|want| expected_bytes(code, offset, want, pattern, at));
            bytes_ok.then_some(code_rva + offset)
        });
        let mut symbols = Vec::new();
        let mut missing = Vec::new();
        for (symbol, found) in self.symbols.iter().zip(found_symbols) {
            match found {
                Some(rva) => symbols.push((symbol.id.clone(), rva)),
                None => missing.push(symbol.id.clone()),
            }
        }
        let found_constants = search(&self.constants, |c| {
            if let Some(symbol) = c.id.strip_suffix(".bytes") {
                let (_, rva) = symbols.iter().find(|(id, _)| id == symbol)?;
                let at = usize::try_from(rva - code_rva).ok()?;
                let bytes = code.get(at..at + 8)?;
                let value = u64::from_le_bytes(bytes.try_into().ok()?);
                return (value == c.value).then_some(c.value);
            }
            c.pattern()?.find_unique(code).ok().map(|_| c.value)
        });
        let mut constants = Vec::new();
        for (constant, found) in self.constants.iter().zip(found_constants) {
            match found {
                Some(value) => constants.push((constant.id.clone(), value)),
                None => missing.push(constant.id.clone()),
            }
        }
        missing.sort();
        Resolved {
            name: format!("{} (by patterns, for a build without a table)", self.name),
            kind: Kind::Fallback,
            checks: Vec::new(),
            symbols,
            constants,
            missing,
        }
    }

    fn constant_value(&self, id: &str) -> Option<u64> {
        self.constants
            .binary_search_by(|c| c.id.as_str().cmp(id))
            .ok()
            .map(|i| self.constants[i].value)
    }
}

/// Whether the 8 bytes at `offset` in `code` equal `want` wherever the
/// pattern, whose match starts `at` bytes before `offset`, fixes a byte;
/// bytes past the pattern must equal outright.
fn expected_bytes(code: &[u8], offset: u64, want: u64, pattern: &Pattern, at: u64) -> bool {
    let Ok(offset) = usize::try_from(offset) else {
        return false;
    };
    let Some(have) = code.get(offset..offset + 8) else {
        return false;
    };
    let want = want.to_le_bytes();
    let at = usize::try_from(at).unwrap_or(usize::MAX);
    (0..8).all(|i| {
        at.checked_add(i).and_then(|p| pattern.is_fixed(p)) == Some(false) || have[i] == want[i]
    })
}

/// Runs `find` over `entries` on all cores, in order.
fn search<T: Sync, R: Send>(
    entries: &[T],
    find: impl Fn(&T) -> Option<R> + Sync,
) -> Vec<Option<R>> {
    let threads = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .clamp(1, 16);
    let chunk = entries.len().div_ceil(threads).max(1);
    std::thread::scope(|scope| {
        let workers: Vec<_> = entries
            .chunks(chunk)
            .map(|part| scope.spawn(|| part.iter().map(&find).collect::<Vec<_>>()))
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().unwrap_or_else(|_| Vec::new()))
            .collect()
    })
}

/// How a [`Resolved`] table came about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The table was made for the running build.
    Exact,
    /// Another build's table, its entries found by their patterns.
    Fallback,
}

/// A table as the running build uses it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    name: String,
    kind: Kind,
    checks: Vec<(String, u64)>,
    symbols: Vec<(String, u64)>,
    constants: Vec<(String, u64)>,
    missing: Vec<String>,
}

impl Resolved {
    /// The name the log shows.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Whether the table was made for this build.
    #[must_use]
    pub const fn kind(&self) -> Kind {
        self.kind
    }

    /// Systems whose reflected entry must equal the pinned address; none on
    /// a fallback.
    #[must_use]
    pub fn checks(&self) -> &[(String, u64)] {
        &self.checks
    }

    /// The symbol `id`, relative to the image base.
    #[must_use]
    pub fn symbol(&self, id: &str) -> Option<u64> {
        find(&self.symbols, id)
    }

    /// The constant `id`.
    #[must_use]
    pub fn constant(&self, id: &str) -> Option<u64> {
        find(&self.constants, id)
    }

    /// Every symbol, sorted by id.
    #[must_use]
    pub fn symbols(&self) -> &[(String, u64)] {
        &self.symbols
    }

    /// Every constant, sorted by id.
    #[must_use]
    pub fn constants(&self) -> &[(String, u64)] {
        &self.constants
    }

    /// The entries of the source table this build lacks, sorted.
    #[must_use]
    pub fn missing(&self) -> &[String] {
        &self.missing
    }
}

fn find(entries: &[(String, u64)], id: &str) -> Option<u64> {
    entries
        .binary_search_by(|(have, _)| have.as_str().cmp(id))
        .ok()
        .map(|i| entries[i].1)
}

/// The file name of the table for a build: `<side>-<guid>-<age>.toml`.
///
/// # Examples
///
/// ```
/// use enshwick_loader_builds::{Fingerprint, Side, file_name};
///
/// let name = file_name(Side::Client, Fingerprint::new([0xab; 16], 410));
/// assert_eq!(name, format!("client-{}-410.toml", "ab".repeat(16)));
/// ```
#[must_use]
pub fn file_name(side: Side, fingerprint: Fingerprint) -> String {
    let mut guid = String::with_capacity(32);
    for byte in fingerprint.guid() {
        guid.push(char::from(HEX[usize::from(byte >> 4)]));
        guid.push(char::from(HEX[usize::from(byte & 0xf)]));
    }
    format!("{}-{guid}-{}.toml", side.as_str(), fingerprint.age())
}

const HEX: &[u8; 16] = b"0123456789abcdef";

fn parse_guid(text: &str) -> Option<[u8; 16]> {
    let digits = text.as_bytes();
    if digits.len() != 32 {
        return None;
    }
    let digit = |c: u8| {
        HEX.iter()
            .position(|&h| h == c)
            .and_then(|d| u8::try_from(d).ok())
    };
    let mut out = [0u8; 16];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = (digit(digits[2 * i])? << 4) | digit(digits[2 * i + 1])?;
    }
    Some(out)
}

/// Every table this crate embeds, as `(file name, text)`.
///
/// # Examples
///
/// ```
/// let tables = enshwick_loader_builds::embedded();
/// assert!(tables.iter().all(|(file, _)| file.ends_with(".toml")));
/// ```
#[must_use]
pub fn embedded() -> &'static [(&'static str, &'static str)] {
    include!(concat!(env!("OUT_DIR"), "/embedded.rs"))
}

/// The table for a build among `tables`: the highest revision of those made
/// for it.
#[must_use]
pub fn exact(tables: &[Table], side: Side, fingerprint: Fingerprint) -> Option<&Table> {
    tables
        .iter()
        .filter(|t| t.side == side && t.fingerprint == fingerprint)
        .max_by_key(|t| t.revision)
}

/// The table of the newest build of `side` among `tables`, by Steam build
/// id, then revision: the base for a build without a table.
#[must_use]
pub fn newest(tables: &[Table], side: Side) -> Option<&Table> {
    tables
        .iter()
        .filter(|t| t.side == side)
        .max_by_key(|t| (t.build.unwrap_or(0), t.revision))
}

/// Checks that the project key signed the table `text` published as `file`:
/// `signature` is the text of its `.minisig`, whose trusted comment must be
/// [`SIGNATURE_TAG`] and `file`, so a table cannot pass for another build's.
///
/// # Errors
///
/// [`VerifyError`] when the table is refused.
pub fn verify(text: &[u8], signature: &str, file: &str) -> Result<(), VerifyError> {
    verify_with(PROJECT_KEY, text, signature, file)
}

/// [`verify`] with another public key, the text of a minisign `.pub` file.
///
/// # Errors
///
/// [`VerifyError`] when the table is refused or the key does not read.
pub fn verify_with(key: &str, text: &[u8], signature: &str, file: &str) -> Result<(), VerifyError> {
    let key = PublicKey::decode(key.trim()).map_err(|e| VerifyError::Unreadable(e.to_string()))?;
    let signature =
        Signature::decode(signature).map_err(|e| VerifyError::Unreadable(e.to_string()))?;
    key.verify(text, &signature, false)
        .map_err(|_| VerifyError::Unsigned)?;
    let comment = signature.trusted_comment();
    let mut words = comment.split_whitespace();
    if words.next() == Some(SIGNATURE_TAG) && words.next() == Some(file) && words.next().is_none() {
        Ok(())
    } else {
        Err(VerifyError::OtherFile(comment.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tables() -> Vec<Table> {
        embedded()
            .iter()
            .map(|(file, text)| {
                let table = Table::parse(text).unwrap_or_else(|e| panic!("{file}: {e}"));
                assert_eq!(table.file_name(), *file);
                table
            })
            .collect()
    }

    #[test]
    fn every_embedded_table_reads_and_is_named_after_its_build() {
        let tables = tables();
        assert!(tables.iter().any(|t| t.side() == Side::Server));
        assert!(tables.iter().any(|t| t.side() == Side::Client));
        for table in &tables {
            assert!(table.symbols().iter().all(|s| s.rva() != 0));
            assert!(!table.checks().is_empty(), "{}", table.name());
        }
    }

    #[test]
    fn the_client_table_carries_every_server_constant_and_symbol() {
        let tables = tables();
        let server = newest(&tables, Side::Server).unwrap().resolve();
        let client = newest(&tables, Side::Client).unwrap().resolve();
        for (id, value) in server.constants() {
            assert_eq!(client.constant(id), Some(*value), "{id}");
        }
        for (id, _) in server.symbols() {
            assert!(client.symbol(id).is_some(), "{id}");
        }
        assert_eq!(server.symbol("npc.lookup"), Some(0x20_2e80));
        assert_eq!(client.symbol("npc.lookup"), Some(0x46_98b0));
    }

    const TABLE: &str = r#"
        name = "test (2)"
        side = "server"
        guid = "000102030405060708090a0b0c0d0e0f"
        age = 7
        build = 2
        [checks]
        "a_system" = 0x10
        [symbols]
        "f.plain" = 0x1000
        "f.found" = { rva = 0x1004, pattern = "11 22 ?? 44", at = 1 }
        "f.twice" = { rva = 0x1010, pattern = "AA" }
        "f.changed" = { rva = 0x1020, pattern = "11 22 ?? 44", at = 1 }
        [constants]
        "c.plain" = 8
        "c.used" = { value = 0x48, pattern = "8B 48 ?? 48" }
        "c.gone" = { value = 0x50, pattern = "8B 50" }
        "f.found.bytes" = 0x0807_0605_3344_7722
        "f.changed.bytes" = "0x9111_1111_1111_1111"
    "#;

    #[test]
    fn a_table_parses_with_plain_and_patterned_entries() {
        let table = Table::parse(TABLE).unwrap();
        assert_eq!(table.fingerprint().age(), 7);
        assert_eq!(table.fingerprint().guid()[15], 0x0f);
        assert_eq!(table.build(), Some(2));
        assert_eq!(table.revision(), 1);
        let resolved = table.resolve();
        assert_eq!(resolved.kind(), Kind::Exact);
        assert_eq!(resolved.symbol("f.plain"), Some(0x1000));
        assert_eq!(resolved.constant("c.used"), Some(0x48));
        assert_eq!(resolved.checks(), [("a_system".to_owned(), 0x10)]);
        assert!(resolved.missing().is_empty());
    }

    #[test]
    fn a_fallback_keeps_only_what_its_patterns_find() {
        let table = Table::parse(TABLE).unwrap();
        let mut code = vec![0u8; 64];
        // f.found: 11 22 ?? 44 at 8, then the bytes it pins from at = 1 on.
        code[8..17].copy_from_slice(&[0x11, 0x22, 0x99, 0x44, 0x33, 0x05, 0x06, 0x07, 0x08]);
        code[20] = 0xaa;
        code[30] = 0xaa;
        code[40..44].copy_from_slice(&[0x8b, 0x48, 0x01, 0x48]);
        let resolved = table.fall_back(&code, 0x5000);
        assert_eq!(resolved.kind(), Kind::Fallback);
        assert!(resolved.checks().is_empty());
        assert_eq!(resolved.symbol("f.found"), Some(0x5000 + 9));
        assert_eq!(resolved.symbol("f.plain"), None);
        assert_eq!(resolved.symbol("f.twice"), None);
        assert_eq!(resolved.constant("c.used"), Some(0x48));
        assert_eq!(resolved.constant("c.plain"), None);
        assert_eq!(resolved.constant("c.gone"), None);
        assert_eq!(
            resolved.missing(),
            [
                "c.gone",
                "c.plain",
                "f.changed",
                "f.changed.bytes",
                "f.found.bytes",
                "f.plain",
                "f.twice"
            ]
        );
    }

    #[test]
    fn expected_bytes_count_only_where_the_pattern_fixes_them() {
        let pattern = Pattern::parse("11 ?? 33").unwrap();
        let code = [0x11, 0x99, 0x33, 4, 5, 6, 7, 8, 9];
        let want = u64::from_le_bytes([0x11, 0x22, 0x33, 4, 5, 6, 7, 8]);
        assert!(expected_bytes(&code, 0, want, &pattern, 0));
        let wrong = u64::from_le_bytes([0x11, 0x22, 0x33, 4, 5, 6, 7, 0]);
        assert!(!expected_bytes(&code, 0, wrong, &pattern, 0));
    }

    #[test]
    fn the_newest_table_of_a_side_is_the_highest_build() {
        let old = Table::parse(TABLE).unwrap();
        let mut new = old.clone();
        new.build = Some(3);
        new.fingerprint = Fingerprint::new([1; 16], 1);
        let tables = [old.clone(), new.clone()];
        assert_eq!(newest(&tables, Side::Server), Some(&new));
        assert_eq!(newest(&tables, Side::Client), None);
        assert_eq!(exact(&tables, Side::Server, old.fingerprint()), Some(&old));
        let mut revised = old.clone();
        revised.revision = 2;
        let tables = [old.clone(), revised.clone()];
        assert_eq!(
            exact(&tables, Side::Server, old.fingerprint()),
            Some(&revised)
        );
    }

    #[test]
    fn malformed_tables_are_refused() {
        assert!(matches!(Table::parse("name = 1"), Err(TableError::Toml(_))));
        let bad_guid = TABLE.replace("000102030405060708090a0b0c0d0e0f", "XYZ");
        assert!(matches!(Table::parse(&bad_guid), Err(TableError::Guid(_))));
        let bad_pattern = TABLE.replace("\"AA\"", "\"A\"");
        assert!(matches!(
            Table::parse(&bad_pattern),
            Err(TableError::Pattern { .. })
        ));
        let unknown = format!("extra = 1\n{TABLE}");
        assert!(Table::parse(&unknown).is_err());
    }

    /// A throwaway key and `b"table"` signed with the trusted comment
    /// `build-table server-x.toml`.
    const TEST_KEY: &str = include_str!("../testdata/test.pub");
    const TEST_SIGNATURE: &str = include_str!("../testdata/table.minisig");

    #[test]
    fn a_table_signed_for_its_file_passes_and_nothing_else() {
        let file = "server-x.toml";
        assert_eq!(
            verify_with(TEST_KEY, b"table", TEST_SIGNATURE, file),
            Ok(())
        );
        assert_eq!(
            verify_with(TEST_KEY, b"Table", TEST_SIGNATURE, file),
            Err(VerifyError::Unsigned)
        );
        assert!(matches!(
            verify_with(TEST_KEY, b"table", TEST_SIGNATURE, "client-x.toml"),
            Err(VerifyError::OtherFile(_))
        ));
        assert_eq!(
            verify(b"table", TEST_SIGNATURE, file),
            Err(VerifyError::Unsigned)
        );
        assert!(matches!(
            verify(b"table", "garbage", file),
            Err(VerifyError::Unreadable(_))
        ));
    }

    /// On the executables `ENSH_SERVER_EXE` and `ENSH_CLIENT_EXE` name (each
    /// skipped without it), the patterns of their own table find every
    /// patterned entry where the table pins it.
    #[test]
    fn patterns_find_their_entries_on_their_own_build() {
        for var in ["ENSH_SERVER_EXE", "ENSH_CLIENT_EXE"] {
            let Ok(path) = std::env::var(var) else {
                continue;
            };
            let bytes = std::fs::read(path).unwrap();
            let image = enshwick_loader_reflect::Image::parse(
                &bytes,
                enshwick_loader_reflect::Layout::File,
                None,
            )
            .unwrap();
            let fingerprint = image.fingerprint().unwrap();
            let tables = tables();
            let side = if var == "ENSH_SERVER_EXE" {
                Side::Server
            } else {
                Side::Client
            };
            let table = exact(&tables, side, fingerprint).unwrap();
            let text = image.section(".text").unwrap();
            let range = image.range(text);
            let code = image.read(range.start, range.end - range.start).unwrap();
            let resolved = table.fall_back(code, range.start - image.base());
            let mut found = 0;
            for symbol in table.symbols() {
                if symbol.pattern().is_some() {
                    assert_eq!(
                        resolved.symbol(symbol.id()),
                        Some(symbol.rva()),
                        "{}",
                        symbol.id()
                    );
                    found += 1;
                }
            }
            for constant in table.constants() {
                if constant.pattern().is_some() {
                    assert_eq!(
                        resolved.constant(constant.id()),
                        Some(constant.value()),
                        "{}",
                        constant.id()
                    );
                }
            }
            assert!(found * 4 > table.symbols().len() * 3, "{var}: {found}");
        }
    }

    #[test]
    fn the_project_key_reads() {
        assert!(PublicKey::decode(PROJECT_KEY.trim()).is_ok());
    }
}
