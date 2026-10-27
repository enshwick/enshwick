//! A build table: reading its TOML, resolving it for a build and carrying
//! it over to a build without one.

use std::collections::BTreeMap;

use enshwick::code::Pattern;

use crate::entries::read;
use crate::reflect::Fingerprint;
use crate::{Constant, GameVersion, Kind, Locate, Operand, Resolved, Side, SteamBuild, Symbol};

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

/// One build's table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    pub(crate) name: String,
    pub(crate) side: Side,
    pub(crate) fingerprint: Fingerprint,
    pub(crate) steam_build: Option<SteamBuild>,
    pub(crate) game_version: Option<GameVersion>,
    pub(crate) revision: u32,
    pub(crate) checks: Vec<(String, u64)>,
    pub(crate) symbols: Vec<Symbol>,
    pub(crate) constants: Vec<Constant>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TableFile {
    name: String,
    side: Side,
    guid: String,
    age: u32,
    #[serde(default)]
    steam_build: Option<SteamBuild>,
    #[serde(default)]
    game_version: Option<GameVersion>,
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
    rel: Option<u64>,
    end: Option<u64>,
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
    at: Option<u64>,
    size: Option<u8>,
    #[serde(default)]
    neg: bool,
    #[serde(default)]
    shl: bool,
    #[serde(default)]
    plus: i64,
    via: Option<String>,
    within: Option<u32>,
    attribute: Option<String>,
    cell: Option<String>,
    #[serde(default = "carried")]
    carried: bool,
}

const fn carried() -> bool {
    true
}

impl ConstantFull {
    /// The operand `at`, `size`, `neg` and `plus` name.
    fn operand(&self, id: &str) -> Result<Option<Operand>, TableError> {
        let bad = |why: &str| Err(TableError::Toml(format!("{id}: {why}")));
        let operand = match (self.at, self.size) {
            (None, None) if !self.neg && !self.shl && self.plus == 0 => return Ok(None),
            (Some(at), Some(size)) => Operand::new(at, size),
            _ => return bad("`at` and `size` go together, and `neg`, `shl` and `plus` need them"),
        };
        let Some(mut operand) = operand else {
            return bad("an operand is 1, 2, 4 or 8 bytes");
        };
        if self.pattern.is_none() {
            return bad("an operand needs a pattern");
        }
        match (self.neg, self.shl) {
            (true, true) => return bad("`neg` and `shl` exclude each other"),
            (true, false) => operand = operand.negative(),
            (false, true) => operand = operand.shift(),
            (false, false) => {}
        }
        Ok(Some(operand.plus(self.plus)))
    }

    /// Refuses a row whose keys contradict each other, or whose `cell`
    /// names none of `symbols`.
    fn check(&self, id: &str, symbols: &[Symbol]) -> Result<(), TableError> {
        let bad = |why: &str| Err(TableError::Toml(format!("{id}: {why}")));
        if self.via.is_some() && self.pattern.is_none() {
            return bad("`via` needs a pattern");
        }
        if self.within.is_some() && self.via.is_none() {
            return bad("`within` needs `via`");
        }
        if self.attribute.is_some() && (self.pattern.is_some() || self.via.is_some()) {
            return bad("`attribute` takes no pattern and no `via`");
        }
        if !self.carried
            && (self.pattern.is_some() || self.via.is_some() || self.attribute.is_some())
        {
            return bad("`carried = false` takes no pattern, `via` or `attribute`");
        }
        if let Some(cell) = &self.cell {
            if self.pattern.is_some()
                || self.via.is_some()
                || self.attribute.is_some()
                || !self.carried
            {
                return bad("`cell` takes no pattern, `via`, `attribute` or `carried = false`");
            }
            if !symbols.iter().any(|s| &s.id == cell) {
                return bad("`cell` names no symbol of the table");
            }
        }
        Ok(())
    }
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
    /// use enshwick_gamedata::{Side, Table};
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
    /// # Ok::<(), enshwick_gamedata::TableError>(())
    /// ```
    pub fn parse(text: &str) -> Result<Self, TableError> {
        let file: TableFile = toml::from_str(text).map_err(|e| TableError::Toml(e.to_string()))?;
        let guid = parse_guid(&file.guid).ok_or_else(|| TableError::Guid(file.guid.clone()))?;
        let symbols: Vec<Symbol> = file
            .symbols
            .into_iter()
            .map(|(id, value)| {
                if id.strip_suffix(".bytes").is_some() {
                    return Err(TableError::Toml(format!(
                        "{id}: pinned bytes are a constant, not a symbol"
                    )));
                }
                let (rva, text, locate) = match value {
                    SymbolValue::Plain(rva) => (rva, None, Locate::At(0)),
                    SymbolValue::Full(full) => {
                        let locate = match (full.rel, full.end) {
                            (Some(at), end) => Locate::Rel {
                                at,
                                end: end.unwrap_or(at + 4),
                            },
                            (None, None) => Locate::At(full.at),
                            (None, Some(_)) => {
                                return Err(TableError::Toml(format!("{id}: `end` needs `rel`")));
                            }
                        };
                        (full.rva, full.pattern, locate)
                    }
                };
                Ok(Symbol {
                    pattern: pattern(&id, text)?,
                    id,
                    rva,
                    locate,
                })
            })
            .collect::<Result<_, TableError>>()?;
        let constants = file
            .constants
            .into_iter()
            .map(|(id, value)| {
                let (value, text, operand, via, within, attribute, cell, carried) = match value {
                    ConstantValue::Plain(value) => {
                        (value.value(&id)?, None, None, None, None, None, None, true)
                    }
                    ConstantValue::Full(full) => {
                        let operand = full.operand(&id)?;
                        full.check(&id, &symbols)?;
                        (
                            full.value.value(&id)?,
                            full.pattern,
                            operand,
                            full.via,
                            full.within,
                            full.attribute,
                            full.cell,
                            full.carried,
                        )
                    }
                };
                Ok(Constant {
                    pattern: pattern(&id, text)?,
                    id,
                    value,
                    operand,
                    via,
                    within,
                    attribute,
                    cell,
                    carried,
                })
            })
            .collect::<Result<_, TableError>>()?;
        Ok(Self {
            name: file.name,
            side: file.side,
            fingerprint: Fingerprint::new(guid, file.age),
            steam_build: file.steam_build,
            game_version: file.game_version,
            revision: file.revision,
            checks: file.checks.into_iter().collect(),
            symbols,
            constants,
        })
    }

    /// The name the log shows, such as `1.0 server (26083076)`.
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

    /// Steam's id of the build, when the table names it.
    #[must_use]
    pub const fn steam_build(&self) -> Option<SteamBuild> {
        self.steam_build
    }

    /// The game's own version number on the build, when the table names it.
    #[must_use]
    pub const fn game_version(&self) -> Option<GameVersion> {
        self.game_version
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
            uncarried: Vec::new(),
            attributes: self.attributes(),
            heads: self.heads(),
        }
    }

    /// Each symbol whose pattern matches where its code starts, or a fixed
    /// distance before it, with that pattern and distance.
    fn heads(&self) -> Vec<(String, Pattern, u64)> {
        self.symbols
            .iter()
            .filter_map(|s| match (&s.pattern, s.locate) {
                (Some(pattern), Locate::At(at)) => Some((s.id.clone(), pattern.clone(), at)),
                _ => None,
            })
            .collect()
    }

    fn attributes(&self) -> Vec<(String, String)> {
        self.constants
            .iter()
            .filter_map(|c| Some((c.id.clone(), c.attribute.clone()?)))
            .collect()
    }

    /// The table carried to a build without its own: each entry found by
    /// its pattern in `code`, the code section that starts at `code_rva`.
    /// A symbol counts when its pattern matches exactly once and the code
    /// there still starts with the bytes its `<id>.bytes` constant pins, if
    /// any; a constant counts when the code that uses it is still found
    /// once, and takes the value read from that code where its operand
    /// say; a constant read [`Constant::via`] a symbol counts when that
    /// symbol counts and its code there matches; a constant in a
    /// [`Constant::cell`] counts when the cell's symbol counts and `cell`
    /// reads the `u32` at its RVA, and takes that value. Everything else is
    /// [`Resolved::missing`], the slots of [`Constant::attribute`]s among
    /// them until the loader supplies them, except the constants that are
    /// not [`Constant::carried`], which are [`Resolved::uncarried`]. The
    /// checks do not apply.
    #[must_use]
    pub fn fall_back(
        &self,
        code: &[u8],
        code_rva: u64,
        cell: impl Fn(u64) -> Option<u32> + Sync,
    ) -> Resolved {
        let found_symbols = search(&self.symbols, |s| {
            let rva = s.find(code, code_rva)?;
            let bytes_ok = self
                .constant_value(&format!("{}.bytes", s.id))
                .is_none_or(|want| expected_bytes(code, code_rva, rva, want, s));
            bytes_ok.then_some(rva)
        });
        let mut symbols = Vec::new();
        let mut missing = Vec::new();
        for (symbol, found) in self.symbols.iter().zip(found_symbols) {
            match found {
                Some(rva) => symbols.push((symbol.id.clone(), rva)),
                None => missing.push(symbol.id.clone()),
            }
        }
        let (carried, uncarried): (Vec<&Constant>, Vec<&Constant>) =
            self.constants.iter().partition(|c| c.carried);
        let found_constants = search(&carried, |c| {
            if let Some(symbol) = c.id.strip_suffix(".bytes") {
                let (_, rva) = symbols.iter().find(|(id, _)| id == symbol)?;
                let value = read(code, rva.checked_sub(code_rva)?, 8)?;
                return (value == c.value).then_some(c.value);
            }
            if let Some(via) = &c.via {
                let (_, rva) = symbols.iter().find(|(id, _)| id == via)?;
                return c.find_at(code, rva.checked_sub(code_rva)?);
            }
            if let Some(at) = &c.cell {
                let (_, rva) = symbols.iter().find(|(id, _)| id == at)?;
                return cell(*rva).map(u64::from);
            }
            c.find(code)
        });
        let mut constants = Vec::new();
        for (constant, found) in carried.iter().zip(found_constants) {
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
            uncarried: uncarried.iter().map(|c| c.id.clone()).collect(),
            attributes: self.attributes(),
            heads: self.heads(),
        }
    }

    /// The value of `constant` as its pattern finds it in `code`, the code
    /// section that starts at `code_rva`, on the build the table is for: a
    /// value read [`Constant::via`] a symbol in the code where the table
    /// pins that symbol, any other in its unique match.
    #[must_use]
    pub fn find_pinned(&self, constant: &Constant, code: &[u8], code_rva: u64) -> Option<u64> {
        match constant.via() {
            Some(via) => {
                let symbol = self.symbols.iter().find(|s| s.id == via)?;
                constant.find_at(code, symbol.rva.checked_sub(code_rva)?)
            }
            None => constant.find(code),
        }
    }

    fn constant_value(&self, id: &str) -> Option<u64> {
        self.constants
            .binary_search_by(|c| c.id.as_str().cmp(id))
            .ok()
            .map(|i| self.constants[i].value)
    }
}

/// Whether the 8 bytes at `rva` in `code`, which starts at `code_rva`,
/// equal `want` wherever the symbol's pattern does not leave the byte open;
/// bytes past its pattern, or of a symbol found through a displacement,
/// must equal outright.
pub(crate) fn expected_bytes(
    code: &[u8],
    code_rva: u64,
    rva: u64,
    want: u64,
    symbol: &Symbol,
) -> bool {
    let Some(have) = rva.checked_sub(code_rva).and_then(|at| read(code, at, 8)) else {
        return false;
    };
    let (have, want) = (have.to_le_bytes(), want.to_le_bytes());
    let open = |i: usize| match symbol.pattern() {
        Some((pattern, Locate::At(at))) => {
            usize::try_from(at)
                .ok()
                .and_then(|at| at.checked_add(i))
                .and_then(|p| pattern.is_fixed(p))
                == Some(false)
        }
        _ => false,
    };
    (0..8).all(|i| open(i) || have[i] == want[i])
}

/// Runs `find` over `entries` on all cores, in order.
pub(crate) fn search<T: Sync, R: Send>(
    entries: &[T],
    find: impl Fn(&T) -> Option<R> + Sync,
) -> Vec<Option<R>> {
    let threads = std::thread::available_parallelism().map_or(1, usize::from);
    search_on(threads, entries, find)
}

/// [`search`] on at most `threads` threads, at most 16.
pub(crate) fn search_on<T: Sync, R: Send>(
    threads: usize,
    entries: &[T],
    find: impl Fn(&T) -> Option<R> + Sync,
) -> Vec<Option<R>> {
    let chunk = entries.len().div_ceil(threads.clamp(1, 16)).max(1);
    std::thread::scope(|scope| {
        let workers: Vec<_> = entries
            .chunks(chunk)
            .map(|part| {
                (
                    part.len(),
                    scope.spawn(|| {
                        part.iter()
                            .map(|entry| {
                                // A panicking entry counts as not found, and
                                // the rest of its chunk is still searched.
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    find(entry)
                                }))
                                .ok()
                                .flatten()
                            })
                            .collect::<Vec<_>>()
                    }),
                )
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|(len, worker)| {
                worker
                    .join()
                    .unwrap_or_else(|_| core::iter::repeat_with(|| None).take(len).collect())
            })
            .collect()
    })
}

/// The file name of the table for a build: `<side>-<guid>-<age>.toml`.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::{Fingerprint, Side, file_name};
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
