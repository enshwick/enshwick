//! A mod's manifest: its id, version, dependencies and build rows.

use crate::abi::{Header, Side, Str};

/// A game build's identity: the CodeView GUID, in file order, and age.
///
/// # Examples
///
/// ```
/// use enshwick::abi::Fingerprint;
///
/// let build = Fingerprint { guid: [0x1c; 16], age: 387 };
/// assert_eq!(build.age, 387);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, bytemuck::Zeroable)]
pub struct Fingerprint {
    /// The GUID in file order.
    pub guid: [u8; 16],
    /// The linker's age counter.
    pub age: u32,
}

/// The running game, as the loader identified it.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{BuildInfo, BuildTable, Fingerprint, Header, Side};
///
/// let build = BuildInfo {
///     header: Header::of::<BuildInfo>(),
///     side: Side::SERVER,
///     base: 0x1_4000_0000,
///     fingerprint: Fingerprint { guid: [0; 16], age: 0 },
///     table: BuildTable::EXACT,
/// };
/// assert_eq!(build.side, Side::SERVER);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable)]
pub struct BuildInfo {
    /// Size and version of this struct.
    pub header: Header,
    /// [`Side::SERVER`] or [`Side::CLIENT`].
    pub side: Side,
    /// Where the game's image is loaded.
    pub base: u64,
    /// The build fingerprint.
    pub fingerprint: Fingerprint,
    /// Whether the loader's build table was made for this build.
    pub table: BuildTable,
}

impl_headed!(BuildInfo);

/// Where the loader's build table for the running build comes from.
///
/// # Examples
///
/// ```
/// use enshwick::abi::BuildTable;
///
/// assert_ne!(BuildTable::EXACT, BuildTable::PATTERNS);
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, bytemuck::Zeroable)]
pub struct BuildTable(pub u8);

impl BuildTable {
    /// The table was made for this build: every entry it pins is there.
    pub const EXACT: Self = Self(0);
    /// The build has no table yet: the newest table of its side found what
    /// it could by code patterns, and the rest is missing until the table
    /// for this build arrives.
    pub const PATTERNS: Self = Self(1);
}

/// One symbol pinned for one game build: a name and its address relative to
/// the image base.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Str, Symbol};
///
/// let symbol = Symbol { name: Str::new("npc.lookup"), rva: 0x20_2e80 };
/// assert_eq!(symbol.rva, 0x20_2e80);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Symbol {
    /// The symbolic id, such as `npc.lookup`.
    pub name: Str,
    /// The address relative to the image base.
    pub rva: u64,
}

/// A build fingerprint's GUID from its 32 lowercase hex digits, for build rows
/// written in source.
///
/// # Panics
///
/// When `hex` is not 32 lowercase hex digits; in a `const` or `static` that
/// fails the build.
///
/// # Examples
///
/// ```
/// use enshwick::abi::guid;
///
/// assert_eq!(guid("1c129660cd6efc4f9f0b21652d5ff2bf")[0], 0x1c);
/// ```
#[must_use]
pub const fn guid(hex: &str) -> [u8; 16] {
    let bytes = hex.as_bytes();
    assert!(bytes.len() == 32, "a GUID is 32 hex digits");
    let mut out = [0u8; 16];
    let mut i = 0;
    while i < 16 {
        out[i] = (digit(bytes[2 * i]) << 4) | digit(bytes[2 * i + 1]);
        i += 1;
    }
    out
}

#[expect(
    clippy::panic,
    reason = "used in const evaluation, where a bad literal fails the build"
)]
const fn digit(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        _ => panic!("a GUID is lowercase hex"),
    }
}

/// The symbols a mod pins itself for one game build, beside the loader's own
/// table.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{BuildRow, Fingerprint, Side, Str, Symbol};
///
/// static SYMBOLS: [Symbol; 1] = [Symbol { name: Str::new("npc.update"), rva: 0x20_55f0 }];
/// let row = BuildRow {
///     side: Side::SERVER,
///     fingerprint: Fingerprint { guid: [0; 16], age: 0 },
///     symbols: SYMBOLS.as_ptr(),
///     symbol_count: SYMBOLS.len(),
///     constants: core::ptr::null(),
///     constant_count: 0,
/// };
/// assert_eq!(row.symbol_count, 1);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BuildRow {
    /// The binary the row is for.
    pub side: Side,
    /// The build the row is for.
    pub fingerprint: Fingerprint,
    /// The pinned symbols.
    pub symbols: *const Symbol,
    /// How many symbols `symbols` points at.
    pub symbol_count: usize,
    /// Engine constants the mod pins for the build, such as offsets in the
    /// caller frames it reads; the `rva` field holds the value.
    pub constants: *const Symbol,
    /// How many constants `constants` points at.
    pub constant_count: usize,
}

// SAFETY: a build row is immutable static data pointing at static symbols.
unsafe impl Sync for BuildRow {}

/// How a mod relates to another mod it names.
///
/// # Examples
///
/// ```
/// use enshwick::abi::Relation;
///
/// assert_ne!(Relation::REQUIRES, Relation::BREAKS);
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Relation(pub u8);

impl Relation {
    /// The other mod must be present in the version range; it loads first.
    pub const REQUIRES: Self = Self(0);
    /// When the other mod is present, it loads first; it may be absent.
    pub const AFTER: Self = Self(1);
    /// This mod does not work beside the other mod in the version range (any
    /// version when the range is empty): when that one loads, this one is
    /// skipped.
    pub const BREAKS: Self = Self(2);
    /// This mod works better beside the other mod in the version range: it
    /// loads first when present, and the loader notes when it is missing or
    /// in another version.
    pub const RECOMMENDS: Self = Self(3);
    /// This mod takes the place of the other mod, such as one it folds in
    /// (any version; the range stays empty): while this one is present, the
    /// loader never starts the other and names its DLL to delete.
    pub const REPLACES: Self = Self(4);
}

/// A relation to another mod, by id and SemVer range.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Dependency, Relation, Str};
///
/// let zones = Dependency {
///     id: Str::new("you.lockers"),
///     range: Str::new(">=0.1, <1.0"),
///     relation: Relation::REQUIRES,
/// };
/// assert_eq!(zones.relation, Relation::REQUIRES);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Dependency {
    /// The other mod's id.
    pub id: Str,
    /// A Cargo-style version requirement, such as `>=0.1, <1.0`; empty for any.
    pub range: Str,
    /// How the two relate.
    pub relation: Relation,
}

/// Flags of a [`Manifest`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::ManifestFlags;
///
/// assert!(ManifestFlags::JOIN.contains(ManifestFlags::JOIN));
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, bytemuck::Zeroable)]
pub struct ManifestFlags(pub u32);

impl ManifestFlags {
    /// No flag.
    pub const NONE: Self = Self(0);
    /// Server and clients must run the same version of this mod to play
    /// together; it counts in the join check.
    pub const JOIN: Self = Self(1);
    /// The mod may be reloaded while the game runs, for development: it
    /// keeps no state it cannot hand over, stops its own threads in
    /// `shutdown` and uses only what a reload can unlink (hooks, events,
    /// lookups, settings, logging); see the modding book.
    pub const RELOAD: Self = Self(2);
    /// The mod is a developer tool, never for real play: while it runs,
    /// every player reads a warning that developer tools are on.
    pub const DEVELOPER: Self = Self(4);

    /// Whether every flag of `other` is set.
    ///
    /// # Examples
    ///
    /// See [`ManifestFlags`].
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// What a mod is, read by the loader before it runs any of the mod's code.
///
/// It is static data returned by `enshwick_mod_query`.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, Manifest, ManifestFlags, Side, Str};
///
/// let manifest = Manifest {
///     header: Header::of::<Manifest>(),
///     id: Str::new("you.lanterns"),
///     version: Str::new("0.1.0"),
///     side: Side::BOTH,
///     flags: ManifestFlags::JOIN,
///     dependencies: core::ptr::null(),
///     dependency_count: 0,
///     builds: core::ptr::null(),
///     build_count: 0,
///     name: Str::new("Flame Altar Count"),
///     description: Str::EMPTY,
///     authors: core::ptr::null(),
///     author_count: 0,
///     license: Str::new("MIT"),
///     homepage: Str::EMPTY,
///     points: Str::EMPTY,
///     exports: core::ptr::null(),
///     export_count: 0,
/// };
/// assert!(manifest.flags.contains(ManifestFlags::JOIN));
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct Manifest {
    /// Size and version of this struct.
    pub header: Header,
    /// `<author>.<name>`, see [`is_valid_id`](super::is_valid_id); names the settings file.
    pub id: Str,
    /// The mod's SemVer version.
    pub version: Str,
    /// The sides the mod runs on.
    pub side: Side,
    /// Manifest flags.
    pub flags: ManifestFlags,
    /// Relations to other mods.
    pub dependencies: *const Dependency,
    /// How many entries `dependencies` points at.
    pub dependency_count: usize,
    /// Symbols and constants the mod pins itself, per build.
    pub builds: *const BuildRow,
    /// How many rows `builds` points at.
    pub build_count: usize,
    /// The name players read, such as `Survivors per Altar`; empty for the
    /// id.
    pub name: Str,
    /// One sentence on what the mod does.
    pub description: Str,
    /// The authors' names.
    pub authors: *const Str,
    /// How many names `authors` points at.
    pub author_count: usize,
    /// An SPDX licence expression, such as `MIT OR Apache-2.0`.
    pub license: Str,
    /// Where players read about the mod, an `https://` address.
    pub homepage: Str,
    /// The schemas of the extension points the mod defines, as TOML that
    /// `cargo enshwick deps` reads; empty for none. The same text sits in
    /// the DLL's `.enshpt` section, so a tool reads it without running the
    /// mod.
    pub points: Str,
    /// The local names of what other mods may use: calls, points, services
    /// and events named `<id>:<name>`. The loader refuses another mod a
    /// reference to a name of this mod outside the list.
    pub exports: *const Str,
    /// How many names `exports` points at.
    pub export_count: usize,
}

impl_headed!(Manifest);

// SAFETY: a manifest is immutable static data; its pointers refer to statics.
unsafe impl Sync for Manifest {}
