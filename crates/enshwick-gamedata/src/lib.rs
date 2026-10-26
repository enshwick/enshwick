//! The loader's build tables: per game build, the engine functions and
//! constants that reflection does not describe, by symbolic id.
//!
//! A table is a TOML file named after the build's fingerprint (see
//! [`file_name`]). The loader embeds the tables of `tables/` ([`embedded`]).
//! For the running build it takes the table
//! made for it ([`exact`], [`Table::resolve`]); on a build without one, the
//! newest table of the same side ([`newest`]) finds each entry by its pattern
//! ([`Table::fall_back`]), and what no pattern finds stays missing. The
//! game's own archive reads with [`archive::Archive`], and the engine's
//! reflection data with [`reflect::Reflection`].

use std::collections::BTreeMap;

use enshwick::code::Pattern;

pub mod archive;
pub mod reflect;

pub use reflect::Fingerprint;

/// The binary a table is for.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::Side;
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

/// Steam's id for one published build of the game, the `buildid` that
/// steamcmd and `steamapps/appmanifest_*.acf` show; it grows with each build.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::SteamBuild;
///
/// assert!(SteamBuild::new(26_083_076) > SteamBuild::new(26_082_781));
/// assert_eq!(SteamBuild::new(26_083_076).to_string(), "26083076");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Deserialize)]
#[serde(transparent)]
pub struct SteamBuild(u64);

impl SteamBuild {
    /// Wraps a build id read from Steam.
    ///
    /// # Examples
    ///
    /// See [`SteamBuild`].
    #[must_use]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// The id as Steam spells it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_gamedata::SteamBuild;
    ///
    /// assert_eq!(SteamBuild::new(7).get(), 7);
    /// ```
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for SteamBuild {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// The game's own version number, which a dedicated server puts first after
/// the protocol in its Steam keywords; several Steam builds can share one.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::GameVersion;
///
/// assert_eq!(GameVersion::new(1_269_579).to_string(), "1269579");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Deserialize)]
#[serde(transparent)]
pub struct GameVersion(u32);

impl GameVersion {
    /// Wraps a version number read from the game.
    ///
    /// # Examples
    ///
    /// See [`GameVersion`].
    #[must_use]
    pub const fn new(number: u32) -> Self {
        Self(number)
    }

    /// The number as the game spells it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_gamedata::GameVersion;
    ///
    /// assert_eq!(GameVersion::new(7).get(), 7);
    /// ```
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl std::fmt::Display for GameVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
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

/// Where a symbol lies relative to its pattern's match.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::Locate;
///
/// // A call at offset 4 of the match: its rel32 at 5, the next instruction at 9.
/// let call = Locate::Rel { at: 5, end: 9 };
/// assert_ne!(call, Locate::At(0));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Locate {
    /// The symbol is the code this many bytes past the match's start.
    At(u64),
    /// The symbol is what a rel32 displacement in the match points at: a
    /// call or jump target, or a RIP-relative operand, as for data or for a
    /// function the game holds several identical copies of.
    Rel {
        /// The displacement's offset in the match.
        at: u64,
        /// The offset in the match where its instruction ends, which the
        /// displacement counts from.
        end: u64,
    },
}

/// A pinned engine function or address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Symbol {
    id: String,
    rva: u64,
    pattern: Option<Pattern>,
    locate: Locate,
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

    /// Code that finds the symbol on another build, and where the symbol
    /// lies from its match.
    #[must_use]
    pub const fn pattern(&self) -> Option<(&Pattern, Locate)> {
        match &self.pattern {
            Some(pattern) => Some((pattern, self.locate)),
            None => None,
        }
    }

    /// The symbol's address relative to the image base as its pattern finds
    /// it in `code`, the code section that starts at `code_rva`; `None`
    /// without a pattern or a unique match.
    #[must_use]
    pub fn find(&self, code: &[u8], code_rva: u64) -> Option<u64> {
        let pattern = self.pattern.as_ref()?;
        let start = u64::try_from(pattern.find_unique(code).ok()?).ok()?;
        let base = code_rva.checked_add(start)?;
        match self.locate {
            Locate::At(at) => base.checked_add(at),
            Locate::Rel { at, end } => {
                let disp = u32::try_from(read(code, start.checked_add(at)?, 4)?).ok()?;
                base.checked_add(end)?
                    .checked_add_signed(i64::from(i32::from_le_bytes(disp.to_le_bytes())))
            }
        }
    }
}

/// The `size` little-endian bytes of `code` at `at`, zero-extended.
fn read(code: &[u8], at: u64, size: u8) -> Option<u64> {
    let at = usize::try_from(at).ok()?;
    let bytes = code.get(at..at.checked_add(usize::from(size))?)?;
    let mut word = [0u8; 8];
    word.get_mut(..bytes.len())?.copy_from_slice(bytes);
    Some(u64::from_le_bytes(word))
}

/// Where a constant's pattern reads its value: the displacement or
/// immediate of the instruction that uses it.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::Operand;
///
/// let disp8 = Operand::new(3, 1).ok_or("a bad size")?;
/// assert_eq!((disp8.at(), disp8.size()), (3, 1));
/// assert_eq!(disp8.read(&[0x48, 0x8b, 0x41, 0x46], 0), Some(0x46));
/// let below_rbp = disp8.negative();
/// assert_eq!(below_rbp.read(&[0x48, 0x8d, 0x55, 0xa0], 0), Some(0x60));
/// assert_eq!(disp8.plus(8).read(&[0x48, 0x8b, 0x41, 0x46], 0), Some(0x4e));
/// // shl r9, 6: a size of 0x40 that only appears as a shift.
/// assert_eq!(disp8.shift().read(&[0x49, 0xc1, 0xe1, 0x06], 0), Some(0x40));
/// assert!(Operand::new(3, 3).is_none());
/// # Ok::<(), &str>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Operand {
    at: u64,
    size: u8,
    negative: bool,
    shift: bool,
    plus: i64,
}

impl Operand {
    /// The operand of `size` bytes, 1, 2, 4 or 8, `at` bytes past the
    /// match's start, read as it is.
    #[must_use]
    pub const fn new(at: u64, size: u8) -> Option<Self> {
        match size {
            1 | 2 | 4 | 8 => Some(Self {
                at,
                size,
                negative: false,
                shift: false,
                plus: 0,
            }),
            _ => None,
        }
    }

    /// The operand read as the value negated, for a frame slot the code
    /// reaches as `[rbp - value]`.
    #[must_use]
    pub const fn negative(self) -> Self {
        Self {
            negative: true,
            ..self
        }
    }

    /// The operand read as a shift count, the value being `1 << count`, for
    /// a size the code only multiplies by as `shl reg, count`.
    #[must_use]
    pub const fn shift(self) -> Self {
        Self {
            shift: true,
            ..self
        }
    }

    /// The operand read with `plus` added, for a value the code only reaches
    /// next to it, such as a size from the offset of the last field.
    #[must_use]
    pub const fn plus(self, plus: i64) -> Self {
        Self { plus, ..self }
    }

    /// Its offset in the match.
    #[must_use]
    pub const fn at(self) -> u64 {
        self.at
    }

    /// Its size in bytes.
    #[must_use]
    pub const fn size(self) -> u8 {
        self.size
    }

    /// Whether it is read negated.
    #[must_use]
    pub const fn is_negative(self) -> bool {
        self.negative
    }

    /// Whether it is read as a shift count.
    #[must_use]
    pub const fn is_shift(self) -> bool {
        self.shift
    }

    /// What is added to it.
    #[must_use]
    pub const fn addend(self) -> i64 {
        self.plus
    }

    /// The value it gives in the match at `start` in `code`: its bytes
    /// zero-extended, sign-extended and negated, or as a shift count, plus
    /// the addend.
    #[must_use]
    pub fn read(self, code: &[u8], start: u64) -> Option<u64> {
        let raw = read(code, start.checked_add(self.at)?, self.size)?;
        let value = if self.negative {
            let unused = 64 - u32::from(self.size) * 8;
            let signed = (raw << unused).cast_signed() >> unused;
            u64::try_from(signed.checked_neg()?).ok()?
        } else if self.shift {
            1u64.checked_shl(u32::try_from(raw).ok()?)?
        } else {
            raw
        };
        value.checked_add_signed(self.plus)
    }
}

/// A pinned engine constant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Constant {
    id: String,
    value: u64,
    pattern: Option<Pattern>,
    operand: Option<Operand>,
    via: Option<String>,
    within: Option<u32>,
    attribute: Option<String>,
    carried: bool,
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

    /// Code that uses the value.
    #[must_use]
    pub const fn pattern(&self) -> Option<&Pattern> {
        self.pattern.as_ref()
    }

    /// Where the value lies in the pattern's match; `None` when the pattern
    /// fixes the value's bytes, so that a match only confirms it.
    #[must_use]
    pub const fn operand(&self) -> Option<Operand> {
        self.operand
    }

    /// The symbol in whose code the value is read, for a value only a
    /// function too short to find on its own holds: its pattern must match
    /// where the symbol starts, and the operand is read there.
    #[must_use]
    pub fn via(&self) -> Option<&str> {
        self.via.as_deref()
    }

    /// How many bytes from the [`Self::via`] symbol's start its pattern is
    /// searched in, for a value in the body of a function the game holds in
    /// identical copies: the pattern must match exactly once there. `None`
    /// when it must match where the symbol starts.
    #[must_use]
    pub const fn within(&self) -> Option<u32> {
        self.within
    }

    /// The name of the attribute whose slot in its set the value is; the
    /// game looks such slots up in its data at run time, so no code holds
    /// them and the loader reads them from the game's archive.
    #[must_use]
    pub fn attribute(&self) -> Option<&str> {
        self.attribute.as_deref()
    }

    /// Whether a build without its own table takes the value over; `false`
    /// for a value only this build holds, such as a function table slot,
    /// which such a build neither finds nor misses.
    #[must_use]
    pub const fn carried(&self) -> bool {
        self.carried
    }

    /// The value as its pattern finds it in `code`: read from the operand,
    /// or the pinned value when the pattern only confirms it; `None` without
    /// a pattern or a unique match, and for a value read [`Self::via`] a
    /// symbol.
    #[must_use]
    pub fn find(&self, code: &[u8]) -> Option<u64> {
        if self.via.is_some() {
            return None;
        }
        let start = u64::try_from(self.pattern.as_ref()?.find_unique(code).ok()?).ok()?;
        self.read(code, start)
    }

    /// The value in the code of the symbol at `start`: where the pattern
    /// matches at `start`, or exactly once in the [`Self::within`] bytes
    /// from it; read from the operand, or the pinned value when the pattern
    /// only confirms it. `None` without a pattern or such a match.
    #[must_use]
    pub fn find_at(&self, code: &[u8], start: u64) -> Option<u64> {
        let at = usize::try_from(start).ok()?;
        let pattern = self.pattern.as_ref()?;
        let found = match self.within {
            Some(within) => {
                let end = at
                    .checked_add(usize::try_from(within).ok()?)?
                    .min(code.len());
                let offset = pattern.find_unique(code.get(at..end)?).ok()?;
                start.checked_add(u64::try_from(offset).ok()?)?
            }
            None if pattern.matches(code.get(at..)?) => start,
            None => return None,
        };
        self.read(code, found)
    }

    fn read(&self, code: &[u8], start: u64) -> Option<u64> {
        match self.operand {
            Some(operand) => operand.read(code, start),
            None => Some(self.value),
        }
    }
}

/// One build's table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    name: String,
    side: Side,
    fingerprint: Fingerprint,
    steam_build: Option<SteamBuild>,
    game_version: Option<GameVersion>,
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
        let symbols = file
            .symbols
            .into_iter()
            .map(|(id, value)| {
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
                let (value, text, operand, via, within, attribute, carried) = match value {
                    ConstantValue::Plain(value) => {
                        (value.value(&id)?, None, None, None, None, None, true)
                    }
                    ConstantValue::Full(full) => {
                        let operand = full.operand(&id)?;
                        let bad = |why: &str| Err(TableError::Toml(format!("{id}: {why}")));
                        if full.via.is_some() && full.pattern.is_none() {
                            return bad("`via` needs a pattern");
                        }
                        if full.within.is_some() && full.via.is_none() {
                            return bad("`within` needs `via`");
                        }
                        if full.attribute.is_some()
                            && (full.pattern.is_some() || full.via.is_some())
                        {
                            return bad("`attribute` takes no pattern and no `via`");
                        }
                        if !full.carried
                            && (full.pattern.is_some()
                                || full.via.is_some()
                                || full.attribute.is_some())
                        {
                            return bad("`carried = false` takes no pattern, `via` or `attribute`");
                        }
                        (
                            full.value.value(&id)?,
                            full.pattern,
                            operand,
                            full.via,
                            full.within,
                            full.attribute,
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
        }
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
    /// symbol counts and its code there matches. Everything else is
    /// [`Resolved::missing`], the slots of [`Constant::attribute`]s among
    /// them until the loader supplies them, except the constants that are
    /// not [`Constant::carried`], which are [`Resolved::uncarried`]. The
    /// checks do not apply.
    #[must_use]
    pub fn fall_back(&self, code: &[u8], code_rva: u64) -> Resolved {
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
fn expected_bytes(code: &[u8], code_rva: u64, rva: u64, want: u64, symbol: &Symbol) -> bool {
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
            .map(|part| {
                (
                    part.len(),
                    scope.spawn(|| part.iter().map(&find).collect::<Vec<_>>()),
                )
            })
            .collect();
        // A chunk that panicked counts as not found, entry for entry, so the
        // entries after it keep their places.
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
    uncarried: Vec<String>,
    attributes: Vec<(String, String)>,
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

    /// The constants of the source table only its own build holds, left
    /// out on a build it is carried to and not [`Self::missing`] there
    /// ([`Constant::carried`]).
    #[must_use]
    pub fn uncarried(&self) -> &[String] {
        &self.uncarried
    }

    /// The constants that are slots of attributes, by id, with the name of
    /// their attribute ([`Constant::attribute`]).
    #[must_use]
    pub fn attributes(&self) -> &[(String, String)] {
        &self.attributes
    }

    /// Sets the constant `id` to `value`, read from the game's data, and
    /// takes it off [`Self::missing`]; the value it had, if any.
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
    ///     [constants]
    ///     "attribute.health_max_slot" = { value = 7, attribute = "Health_Max" }
    ///     "#,
    /// )?;
    /// let mut resolved = table.fall_back(&[], 0);
    /// assert_eq!(resolved.missing(), ["attribute.health_max_slot"]);
    /// assert_eq!(resolved.supply("attribute.health_max_slot", 7), None);
    /// assert_eq!(resolved.constant("attribute.health_max_slot"), Some(7));
    /// assert!(resolved.missing().is_empty());
    /// # Ok::<(), enshwick_gamedata::TableError>(())
    /// ```
    pub fn supply(&mut self, id: &str, value: u64) -> Option<u64> {
        self.missing.retain(|have| have != id);
        match self
            .constants
            .binary_search_by(|(have, _)| have.as_str().cmp(id))
        {
            Ok(i) => Some(core::mem::replace(&mut self.constants[i].1, value)),
            Err(i) => {
                self.constants.insert(i, (id.to_owned(), value));
                None
            }
        }
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

/// Every table this crate embeds, as `(file name, text)`.
///
/// # Examples
///
/// ```
/// let tables = enshwick_gamedata::embedded();
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
/// id, then the game's version, then revision: the base for a build without
/// a table. A table without a Steam build id ranks below every table with
/// one.
#[must_use]
pub fn newest(tables: &[Table], side: Side) -> Option<&Table> {
    tables
        .iter()
        .filter(|t| t.side == side)
        .max_by_key(|t| (t.steam_build, t.game_version, t.revision))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panicking_search_keeps_the_other_entries_in_place() {
        let entries: Vec<u32> = (0..100).collect();
        let found = search(&entries, |&n| {
            assert!(n != 3, "a broken pattern");
            Some(n)
        });
        assert_eq!(found.len(), entries.len());
        assert_eq!(found[3], None);
        assert_eq!(found[99], Some(99));
    }

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
    #[cfg_attr(miri, ignore = "parsing every embedded table takes minutes under Miri")]
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
    #[cfg_attr(miri, ignore = "parsing every embedded table takes minutes under Miri")]
    fn the_client_table_carries_every_server_constant_and_symbol() {
        let tables = tables();
        let server = newest(&tables, Side::Server).unwrap().resolve();
        let client = newest(&tables, Side::Client).unwrap().resolve();
        for (id, value) in server.constants() {
            let expected = match id.as_str() {
                // The client's weather instances carry 8 bytes more.
                "weather.entry_size" => 0x50,
                // The client's session holds more before its host machine.
                "session.host_machine" => 0x1_1a98,
                _ => *value,
            };
            assert_eq!(client.constant(id), Some(expected), "{id}");
        }
        for (id, _) in server.symbols() {
            assert!(client.symbol(id).is_some(), "{id}");
        }
        assert_eq!(server.symbol("npc.lookup"), Some(0x20_3d30));
        assert_eq!(client.symbol("npc.lookup"), Some(0x46_9c30));
    }

    const TABLE: &str = r#"
        name = "test (2)"
        side = "server"
        guid = "000102030405060708090a0b0c0d0e0f"
        age = 7
        steam_build = 2
        game_version = 9
        [checks]
        "a_system" = 0x10
        [symbols]
        "f.plain" = 0x1000
        "f.found" = { rva = 0x1004, pattern = "11 22 ?? 44", at = 1 }
        "f.twice" = { rva = 0x1010, pattern = "AA" }
        "f.changed" = { rva = 0x1020, pattern = "11 22 ?? 44", at = 1 }
        "f.called" = { rva = 0x1030, pattern = "E8 ?? ?? ?? ?? C3", rel = 1 }
        [constants]
        "c.plain" = 8
        "c.used" = { value = 0x48, pattern = "8B 48 ?? 48" }
        "c.read" = { value = 0x48, pattern = "8B 48 ?? 48", at = 2, size = 1 }
        "c.wide" = { value = 0x1234, pattern = "B9 ?? ?? ?? ?? 8B 41 07", at = 1, size = 4 }
        "c.gone" = { value = 0x50, pattern = "8B 50" }
        "c.below" = { value = 0x60, pattern = "48 8D 55 ??", at = 3, size = 1, neg = true }
        "c.after" = { value = 0x50, pattern = "8B 48 ?? 48", at = 2, size = 1, plus = 8 }
        "f.found.bytes" = 0x0807_0605_3344_7722
        "f.changed.bytes" = "0x9111_1111_1111_1111"
    "#;

    #[test]
    fn a_table_parses_with_plain_and_patterned_entries() {
        let table = Table::parse(TABLE).unwrap();
        assert_eq!(table.fingerprint().age(), 7);
        assert_eq!(table.fingerprint().guid()[15], 0x0f);
        assert_eq!(table.steam_build(), Some(SteamBuild::new(2)));
        assert_eq!(table.game_version(), Some(GameVersion::new(9)));
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
        assert_eq!(resolved.constant("c.read"), Some(0x01), "read, not pinned");
        assert_eq!(resolved.constant("c.after"), Some(0x09));
        assert_eq!(resolved.constant("c.plain"), None);
        assert_eq!(resolved.constant("c.gone"), None);
        assert_eq!(
            resolved.missing(),
            [
                "c.below",
                "c.gone",
                "c.plain",
                "c.wide",
                "f.called",
                "f.changed",
                "f.changed.bytes",
                "f.found.bytes",
                "f.plain",
                "f.twice"
            ]
        );
    }

    /// A constant read `within` a symbol's code counts where its pattern
    /// matches once in those bytes, wherever the code before it moved, and
    /// not where it matches twice or only past them.
    #[test]
    fn constants_within_a_symbol_are_read_where_they_match_once() {
        let table = Table::parse(
            r#"
            name = "1.0 server (1)"
            side = "server"
            guid = "00000000000000000000000000000001"
            age = 2
            [symbols]
            "g.body" = { rva = 0x5010, pattern = "55 48 89 E5" }
            [constants]
            "g.field" = { value = 0x48, via = "g.body", within = 0x20, pattern = "8B 41 ?? 90", at = 2, size = 1 }
            "g.twice" = { value = 0x10, via = "g.body", within = 0x20, pattern = "0F B6 ?? 0C", at = 2, size = 1 }
            "g.far" = { value = 0x10, via = "g.body", within = 0x8, pattern = "8B 41 ?? 90", at = 2, size = 1 }
            "#,
        )
        .unwrap();
        let mut code = vec![0xccu8; 64];
        code[0x10..0x14].copy_from_slice(&[0x55, 0x48, 0x89, 0xe5]);
        code[0x1a..0x1e].copy_from_slice(&[0x8b, 0x41, 0x58, 0x90]);
        code[0x20..0x24].copy_from_slice(&[0x0f, 0xb6, 0x01, 0x0c]);
        code[0x28..0x2c].copy_from_slice(&[0x0f, 0xb6, 0x02, 0x0c]);
        // Past the 0x20 bytes: a copy of the field's code in the next function.
        code[0x32..0x36].copy_from_slice(&[0x8b, 0x41, 0x60, 0x90]);
        let resolved = table.fall_back(&code, 0x5000);
        assert_eq!(resolved.constant("g.field"), Some(0x58));
        assert_eq!(resolved.missing(), ["g.far", "g.twice"]);
        assert!(
            Table::parse(
                r#"
                name = "x"
                side = "server"
                guid = "00000000000000000000000000000001"
                age = 2
                [constants]
                "x" = { value = 1, within = 8, pattern = "90" }
                "#
            )
            .is_err(),
            "`within` needs `via`"
        );
    }

    /// A constant read via a symbol counts where the symbol is found and
    /// its code matches; a slot of an attribute waits for the loader.
    #[test]
    fn constants_via_a_symbol_or_from_data_fall_back_as_they_say() {
        let table = Table::parse(
            r#"
            name = "1.0 server (1)"
            side = "server"
            guid = "00000000000000000000000000000001"
            age = 2
            [symbols]
            "g.get" = { rva = 0x5020, pattern = "E8 ?? ?? ?? ?? 0F 0B", rel = 1 }
            "g.lost" = { rva = 0x5030, pattern = "77 66 55" }
            [constants]
            "g.blob" = { value = 0x48, via = "g.get", pattern = "48 8B 41 ?? C3", at = 3, size = 1 }
            "g.other" = { value = 0x48, via = "g.lost", pattern = "48 8B 41 ?? C3", at = 3, size = 1 }
            "g.slot" = { value = 7, attribute = "Health_Max" }
            "#,
        )
        .unwrap();
        let mut code = vec![0xccu8; 64];
        // A call at 0 to 0x20, where the getter reads +0x50 now.
        code[..7].copy_from_slice(&[0xe8, 0x1b, 0, 0, 0, 0x0f, 0x0b]);
        code[0x20..0x25].copy_from_slice(&[0x48, 0x8b, 0x41, 0x50, 0xc3]);
        let mut resolved = table.fall_back(&code, 0x5000);
        assert_eq!(resolved.symbol("g.get"), Some(0x5020));
        assert_eq!(resolved.constant("g.blob"), Some(0x50));
        assert_eq!(resolved.missing(), ["g.lost", "g.other", "g.slot"]);
        assert_eq!(
            resolved.attributes(),
            [("g.slot".to_owned(), "Health_Max".to_owned())]
        );
        assert_eq!(resolved.supply("g.slot", 7), None);
        assert_eq!(resolved.supply("g.blob", 0x50), Some(0x50));
        assert_eq!(resolved.missing(), ["g.lost", "g.other"]);
        code[0x23] = 0x48;
        code[0x24] = 0x90;
        assert_eq!(table.fall_back(&code, 0x5000).constant("g.blob"), None);
        assert_eq!(table.resolve().constant("g.slot"), Some(7));
        for bad in [
            r#""x" = { value = 1, via = "g.get" }"#,
            r#""x" = { value = 1, attribute = "A", pattern = "11" }"#,
        ] {
            let text = format!(
                "name = \"n\"
side = \"server\"
guid = \"{}\"
age = 1
[constants]
{bad}
",
                "0".repeat(32)
            );
            assert!(Table::parse(&text).is_err(), "{bad}");
        }
    }

    /// A constant only its own build holds counts there, and a build the
    /// table is carried to neither finds nor misses it.
    #[test]
    fn a_constant_kept_from_carrying_is_neither_found_nor_missing() {
        let text = r#"
            name = "1.0 client (1)"
            side = "client"
            guid = "00000000000000000000000000000001"
            age = 2
            [constants]
            "g.slot" = { value = 16, carried = false }
            "g.plain" = 8
            "#;
        let table = Table::parse(text).unwrap();
        let carried = table.fall_back(&[0xcc; 16], 0x1000);
        assert_eq!(carried.constant("g.slot"), None);
        assert_eq!(carried.missing(), ["g.plain"]);
        assert_eq!(carried.uncarried(), ["g.slot"]);
        let exact = table.resolve();
        assert_eq!(exact.constant("g.slot"), Some(16));
        assert!(exact.uncarried().is_empty());
        for bad in [
            r#"{ value = 1, carried = false, pattern = "11" }"#,
            r#"{ value = 1, carried = false, attribute = "A" }"#,
        ] {
            let text = text.replace("{ value = 16, carried = false }", bad);
            assert!(Table::parse(&text).is_err(), "{bad}");
        }
    }

    /// The join's slot in the online provider's table is per build: a
    /// client table carried to any code leaves it out without missing it.
    #[test]
    #[cfg_attr(miri, ignore = "parsing every embedded table takes minutes under Miri")]
    fn a_carried_client_table_never_misses_the_join_slot() {
        let tables = tables();
        let clients: Vec<&Table> = tables.iter().filter(|t| t.side() == Side::Client).collect();
        assert!(!clients.is_empty());
        for table in clients {
            let carried = table.fall_back(&[0xcc; 64], 0x1000);
            let slot = "net.provider_join_slot";
            assert!(
                !carried.missing().iter().any(|id| id == slot),
                "{}",
                table.name()
            );
            assert!(
                carried.uncarried().iter().any(|id| id == slot),
                "{}",
                table.name()
            );
            assert_eq!(table.resolve().constant(slot), Some(16), "{}", table.name());
        }
    }

    #[test]
    fn expected_bytes_count_only_where_the_pattern_fixes_them() {
        let mut symbol = Symbol {
            id: "s".into(),
            rva: 0x10,
            pattern: Some(Pattern::parse("11 ?? 33").unwrap()),
            locate: Locate::At(0),
        };
        let code = [0x11, 0x99, 0x33, 4, 5, 6, 7, 8, 9];
        let want = u64::from_le_bytes([0x11, 0x22, 0x33, 4, 5, 6, 7, 8]);
        assert!(expected_bytes(&code, 0x10, 0x10, want, &symbol));
        let wrong = u64::from_le_bytes([0x11, 0x22, 0x33, 4, 5, 6, 7, 0]);
        assert!(!expected_bytes(&code, 0x10, 0x10, wrong, &symbol));
        symbol.locate = Locate::Rel { at: 0, end: 4 };
        assert!(
            !expected_bytes(&code, 0x10, 0x10, want, &symbol),
            "a target is compared outright"
        );
    }

    #[test]
    fn operands_read_the_value_from_the_code() {
        let table = Table::parse(TABLE).unwrap();
        // c.read: 8B 48 ?? 48, the disp8 read; c.wide: the imm32.
        let mut code = vec![0u8; 80];
        code[40..44].copy_from_slice(&[0x8b, 0x48, 0x52, 0x48]);
        code[48..56].copy_from_slice(&[0xb9, 0x34, 0x12, 0, 0, 0x8b, 0x41, 0x07]);
        // f.called: E8 rel32 at 56, calling 0x5000 + 61 - 0x10.
        code[56..61].copy_from_slice(&[0xe8, 0xf0, 0xff, 0xff, 0xff]);
        code[61] = 0xc3;
        let resolved = table.fall_back(&code, 0x5000);
        assert_eq!(resolved.constant("c.read"), Some(0x52));
        assert_eq!(resolved.constant("c.after"), Some(0x5a));
        assert_eq!(resolved.constant("c.wide"), Some(0x1234));
        assert_eq!(resolved.symbol("f.called"), Some(0x5000 + 61 - 0x10));
        code[62..66].copy_from_slice(&[0x48, 0x8d, 0x55, 0xa0]);
        let resolved = table.fall_back(&code, 0x5000);
        assert_eq!(
            resolved.constant("c.below"),
            Some(0x60),
            "lea rdx, [rbp - 0x60]"
        );
        let read = table
            .constants()
            .iter()
            .find(|c| c.id() == "c.read")
            .unwrap();
        assert_eq!(read.operand(), Operand::new(2, 1));
    }

    #[test]
    fn malformed_operands_are_refused() {
        for bad in [
            "{ value = 1, pattern = \"8B\", at = 1 }",
            "{ value = 1, pattern = \"8B\", at = 1, size = 3 }",
            "{ value = 1, pattern = \"8B\", neg = true }",
            "{ value = 1, at = 1, size = 1 }",
            "{ value = 1, pattern = \"8B\", plus = 1 }",
        ] {
            let text = TABLE.replace("\"c.plain\" = 8", &format!("\"c.plain\" = {bad}"));
            assert!(Table::parse(&text).is_err(), "{bad}");
        }
        let text = TABLE.replace("pattern = \"AA\" }", "pattern = \"AA\", end = 4 }");
        assert!(Table::parse(&text).is_err());
    }

    #[test]
    fn a_table_with_a_steam_build_is_newer_than_one_without() {
        let mut known = Table::parse(TABLE).unwrap();
        known.game_version = None;
        let mut unknown = known.clone();
        unknown.steam_build = None;
        unknown.game_version = Some(GameVersion::new(u32::MAX));
        unknown.fingerprint = Fingerprint::new([1; 16], 1);
        let tables = [unknown.clone(), known.clone()];
        assert_eq!(newest(&tables, Side::Server), Some(&known));
    }

    #[test]
    fn the_single_build_key_of_old_tables_is_refused() {
        let text = TABLE.replace("steam_build = 2", "build = 2");
        assert!(Table::parse(&text).is_err());
    }

    #[test]
    #[cfg_attr(miri, ignore = "parsing every embedded table takes minutes under Miri")]
    fn the_newest_embedded_tables_name_their_steam_builds() {
        let tables = tables();
        let steam = |side| newest(&tables, side).and_then(Table::steam_build);
        assert_eq!(steam(Side::Client), Some(SteamBuild::new(26_082_781)));
        assert_eq!(steam(Side::Server), Some(SteamBuild::new(26_083_076)));
    }

    #[test]
    fn the_newest_table_of_a_side_is_the_highest_build() {
        let old = Table::parse(TABLE).unwrap();
        let mut new = old.clone();
        new.steam_build = Some(SteamBuild::new(3));
        new.game_version = Some(GameVersion::new(8));
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

    /// On the executables `ENSH_SERVER_EXE` and `ENSH_CLIENT_EXE` name (each
    /// skipped without it), the patterns of their own table find every
    /// patterned entry where the table pins it.
    #[test]
    #[cfg_attr(miri, ignore = "parsing every embedded table takes minutes under Miri")]
    fn patterns_find_their_entries_on_their_own_build() {
        for var in ["ENSH_SERVER_EXE", "ENSH_CLIENT_EXE"] {
            let Ok(path) = std::env::var(var) else {
                continue;
            };
            let bytes = std::fs::read(path).unwrap();
            let image = reflect::Image::parse(&bytes, reflect::Layout::File, None).unwrap();
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
            let code_rva = range.start - image.base();
            let mut wrong = Vec::new();
            let mut unique = |id: &str, pattern: &Pattern| {
                let count = pattern.find_all(code).take(2).count();
                if count != 1 {
                    wrong.push(format!("{id}: {count} matches"));
                }
            };
            for symbol in table.symbols() {
                if let Some((pattern, _)) = symbol.pattern() {
                    unique(symbol.id(), pattern);
                }
            }
            for constant in table.constants() {
                if let Some(pattern) = constant.pattern()
                    && constant.via().is_none()
                {
                    unique(constant.id(), pattern);
                }
            }
            for symbol in table.symbols() {
                let found = symbol.find(code, code_rva);
                if symbol.pattern().is_some() && found != Some(symbol.rva()) {
                    wrong.push(format!("{}: found at {found:x?}", symbol.id()));
                }
            }
            for constant in table.constants() {
                let found = table.find_pinned(constant, code, code_rva);
                if constant.pattern().is_some() && found != Some(constant.value()) {
                    wrong.push(format!("{}: reads {found:x?}", constant.id()));
                }
            }
            let resolved = table.fall_back(code, code_rva);
            assert!(wrong.is_empty(), "{var}:\n{}", wrong.join("\n"));
            assert!(
                !resolved
                    .missing()
                    .iter()
                    .any(|id| id == "net.provider_join_slot"),
                "{var}"
            );
            let (symbols, constants) = (resolved.symbols().len(), resolved.constants().len());
            let floor = match side {
                Side::Server => COVERED_SERVER,
                Side::Client => COVERED_CLIENT,
            };
            assert!(
                symbols >= floor.0 && constants >= floor.1,
                "{var}: patterns cover {symbols} symbols and {constants} constants, fewer than {floor:?}; missing {:?}",
                resolved.missing()
            );
        }
    }

    /// How many symbols and constants the patterns of each side's table find
    /// on its own build, at least; raised as patterns are added.
    const COVERED_SERVER: (usize, usize) = (121, 271);
    const COVERED_CLIENT: (usize, usize) = (183, 427);
}
