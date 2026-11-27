//! The binary interface between the loader and a mod DLL.
//!
//! Everything here is `repr(C)` and crosses DLL boundaries, so the layout is
//! the contract: structs start with a [`Header`], enums are integers, strings
//! are borrowed for the call, and no function unwinds. A loader loads the
//! mods built for its own [`ABI_MAJOR`] and its [`ABI_MINOR`] or an older
//! one ([`check_abi`]). Mods normally use the safe layer of `enshwick`
//! instead of these types.
//!
//! Every status, kind, cause and flag set grows only by appending. A mod
//! treats a [`Status`] it does not know as a failure and a kind or cause it
//! does not know as "other"; it ignores flag bits it does not know, and the
//! loader refuses an unknown bit a mod sends with [`Status::BAD_ARGS`].
//!
//! A filter's answer of 0 lets the game decide, or the filters after it
//! ([`FILTER_PASS`]); 1 is the filter's own yes where it has one, such as
//! [`SkillCheck::UNLOCKED`], and 2 its no ([`FILTER_REFUSE`]). The loader
//! reads an answer it does not know as 0, so a zeroed answer is always safe.
//!
//! A yes or no is a `u8`, 1 or 0, in every field, parameter and answer; the
//! loader refuses any other byte a mod sends with [`Status::BAD_ARGS`]. Every
//! GUID is a [`Guid`], and every length, count or capacity a `u32`, but a
//! [`Str`]'s length, a `usize` as a Rust `&str` has.

/// Whether `id` is a valid mod id: `<author>.<name>`, each part of
/// `[a-z0-9-]`, at most 64 characters in all.
///
/// The author part keeps two authors' mods of one name apart; the id names
/// the mod's settings file and data folder, so it stays portable.
///
/// # Examples
///
/// ```
/// use enshwick::abi::is_valid_id;
///
/// assert!(is_valid_id("you.trading-post"));
/// assert!(!is_valid_id("trading-post"));
/// assert!(!is_valid_id("you.trading.post"));
/// assert!(!is_valid_id("You.Trading Post"));
/// ```
#[must_use]
pub const fn is_valid_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    if bytes.len() > 64 {
        return false;
    }
    let mut dot = None;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'.' {
            if dot.is_some() {
                return false;
            }
            dot = Some(i);
        } else if !(b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-') {
            return false;
        }
        i += 1;
    }
    match dot {
        Some(at) => at > 0 && at + 1 < bytes.len(),
        None => false,
    }
}

/// The start of every struct that crosses the boundary on its own: its size
/// and the ABI version of its writer.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{ABI_MAJOR, ABI_MINOR, Header, Manifest};
///
/// let header = Header::of::<Manifest>();
/// assert_eq!((header.abi_major, header.abi_minor), (ABI_MAJOR, ABI_MINOR));
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable, bytemuck::Pod)]
#[expect(
    clippy::exhaustive_structs,
    reason = "the header itself: its fields are the ABI's version check"
)]
pub struct Header {
    /// The size of the whole struct as its writer knew it.
    pub size: u32,
    /// [`ABI_MAJOR`] of the writer.
    pub abi_major: u16,
    /// [`ABI_MINOR`] of the writer.
    pub abi_minor: u16,
}

impl Header {
    /// The header for a struct of type `T` written by this build.
    ///
    /// # Panics
    ///
    /// Never at run time: a struct over 4 GiB fails to compile.
    ///
    /// # Examples
    ///
    /// See [`Header`].
    #[must_use]
    pub const fn of<T>() -> Self {
        let size = size_of::<T>();
        assert!(size <= u32::MAX as usize, "an ABI struct fits in u32");
        #[expect(
            clippy::cast_possible_truncation,
            reason = "checked by the assert above"
        )]
        Self {
            size: size as u32,
            abi_major: ABI_MAJOR,
            abi_minor: ABI_MINOR,
        }
    }

    /// Whether the struct this header starts carries a field that ends at
    /// byte `end`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{Header, Manifest};
    ///
    /// let header = Header::of::<Manifest>();
    /// assert!(header.covers(size_of::<Manifest>()));
    /// assert!(!header.covers(size_of::<Manifest>() + 1));
    /// ```
    #[must_use]
    pub const fn covers(self, end: usize) -> bool {
        end <= self.size as usize
    }
}

#[macro_use]
mod headed;

mod actions;
mod altars;
mod buffs;
mod chat;
mod checks;
mod client;
mod content;
mod controls;
mod events;
mod hooks;
mod host_api;
mod ids;
mod inventory;
mod items;
mod manifest;
mod mods;
mod net;
mod pacing;
mod points;
mod quests;
mod rates;
mod script;
mod services;
mod sound;
mod stats;
mod survivors;
mod terrain;
mod transactions;
mod ui;
mod version;
mod world;
mod writes;

/// The GUID every ABI field holds.
#[doc(no_inline)]
pub use crate::content::Guid;

/// A place in the world in metres, y up: the one form of every position the
/// ABI passes; the loader converts the engine's own forms.
pub type Metres = [f64; 3];

/// The metres a position's axis stays below either way: 2^31, past which the
/// engine's 32.32 fixed point saturates.
pub(crate) const MAX_POSITION_METRES: f64 = 2_147_483_648.0;

/// Whether the engine can hold `position`: every axis finite and below 2^31
/// (2,147,483,648) metres either way, its 32.32 fixed point's range. The
/// loader refuses a position past it with [`Status::BAD_ARGS`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::in_fixed_range;
///
/// assert!(in_fixed_range([2_147_483_647.0, -2_147_483_647.0, 0.0]));
/// assert!(!in_fixed_range([2_147_483_648.0, 0.0, 0.0]));
/// assert!(!in_fixed_range([f64::NAN, 0.0, 0.0]));
/// ```
#[must_use]
pub fn in_fixed_range(position: Metres) -> bool {
    position.iter().all(|m| m.abs() < MAX_POSITION_METRES)
}

/// A whole cell of the engine's 0.5 m grid, y up, as survivors and altar
/// zones count them.
pub type Cell = [i64; 3];
pub use crate::headed;
pub use actions::*;
pub use altars::*;
pub use buffs::*;
pub use chat::*;
pub use checks::*;
pub use client::*;
pub use content::*;
pub use controls::*;
pub use events::*;
pub use headed::{Headed, read_headed, write_headed};
pub use hooks::*;
pub use host_api::*;
pub use ids::*;
pub use inventory::*;
pub use items::*;
pub use manifest::*;
pub use mods::*;
pub use net::*;
pub use pacing::*;
pub use points::*;
pub use quests::*;
pub use rates::*;
pub use services::*;
pub use sound::*;
pub use stats::*;
pub use survivors::*;
pub use terrain::*;
pub use transactions::*;
pub use ui::*;
pub use version::*;
pub use world::*;
pub use writes::*;

/// A UTF-8 string borrowed for the duration of a call, or for as long as its
/// owner documents.
///
/// # Examples
///
/// ```
/// use enshwick::abi::Str;
///
/// let id = Str::new("you.lockers");
/// // SAFETY: `id` borrows a `'static` string.
/// assert_eq!(unsafe { id.as_str() }, Some("you.lockers"));
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct Str {
    ptr: *const u8,
    len: usize,
}

// SAFETY: a `Str` is a borrowed view of immutable bytes; sharing it across
// threads shares only the view.
unsafe impl Send for Str {}
// SAFETY: a `Str` has no method that writes through `ptr`, so threads
// sharing a `&Str` only read the immutable bytes it views.
unsafe impl Sync for Str {}

impl Str {
    /// The empty string.
    pub const EMPTY: Self = Self::new("");

    /// A view of `text`.
    ///
    /// # Examples
    ///
    /// See [`Str`].
    #[must_use]
    pub const fn new(text: &str) -> Self {
        Self {
            ptr: text.as_ptr(),
            len: text.len(),
        }
    }

    /// The length in bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::Str;
    ///
    /// assert_eq!(Str::new("abc").len(), 3);
    /// ```
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether the string is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::Str;
    ///
    /// assert!(Str::EMPTY.is_empty());
    /// ```
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The text, or `None` when the bytes are not UTF-8 or the pointer is
    /// null or the length impossible.
    ///
    /// # Safety
    ///
    /// - Unless the pointer is null or the length is zero, `ptr` is readable
    ///   for `len` bytes.
    /// - Those bytes stay alive and unchanged for the lifetime `'a` the
    ///   caller picks.
    ///
    /// # Examples
    ///
    /// See [`Str`].
    #[must_use]
    pub unsafe fn as_str<'a>(&self) -> Option<&'a str> {
        if self.len == 0 {
            return Some("");
        }
        if self.ptr.is_null() || isize::try_from(self.len).is_err() {
            return None;
        }
        // SAFETY: `ptr` is non-null and `len` fits `isize`, both checked just
        // before; the caller guarantees `len` readable bytes at `ptr`, alive
        // and unchanged for `'a`.
        let bytes = unsafe { core::slice::from_raw_parts(self.ptr, self.len) };
        core::str::from_utf8(bytes).ok()
    }
}

/// The outcome of a call across the interface.
///
/// Each status has one meaning. [`Status::MISSING`] names only a thing that
/// is absent, [`Status::UNAVAILABLE`] a call that cannot run in this process
/// or on this build, [`Status::REFUSED`] a refusal by the loader's rules or
/// another mod, [`Status::BUSY`] a full queue or rate that a later call may
/// pass, [`Status::TOO_SMALL`] a buffer too short for an answer whose length
/// the call wrote, [`Status::BAD_ARGS`] an argument that is malformed, and
/// [`Status::LOADER_FAULT`] a fault inside the loader itself.
///
/// Values only append in later minors, and a mod treats a status it does
/// not know as a failure.
///
/// # Examples
///
/// ```
/// use enshwick::abi::Status;
///
/// assert!(Status::OK.is_ok());
/// assert!(!Status::MISSING.is_ok());
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, bytemuck::Zeroable)]
#[expect(
    clippy::exhaustive_structs,
    reason = "a newtype over one value, which never gains a field"
)]
pub struct Status(pub u32);

impl Status {
    /// Success.
    pub const OK: Self = Self(0);
    /// What the call names is not there: an id, entity, key, file or symbol
    /// that does not exist.
    pub const MISSING: Self = Self(1);
    /// More than one candidate answers a lookup.
    pub const AMBIGUOUS: Self = Self(2);
    /// The loader's rules or another mod refuse the request, and asking
    /// again unchanged gets the same answer.
    pub const REFUSED: Self = Self(3);
    /// An argument is malformed: a null pointer the call needs, a value out
    /// of range, text that is not UTF-8.
    pub const BAD_ARGS: Self = Self(4);
    /// The call came in a phase where it is not allowed, such as a
    /// registration after `enshwick_mod_init`.
    pub const TOO_LATE: Self = Self(5);
    /// The mod has faulted and is off.
    pub const FAULTED: Self = Self(6);
    /// Called from a thread the function does not serve, such as a game
    /// read off the server's game thread, or a world change inside a step.
    pub const WRONG_THREAD: Self = Self(7);
    /// The part of the world asked about is not loaded yet; asking starts
    /// loading it, so the same call works again in a second or so.
    pub const NOT_LOADED: Self = Self(8);
    /// The box asked about leaves the world: past its edge across, or above
    /// the height the loader takes.
    pub const OUTSIDE_WORLD: Self = Self(9);
    /// The component type is the static half of its ECS type: the set-up
    /// values the entity's template holds and every entity of that template
    /// shares, which a write would change for all of them.
    pub const STATIC_HALF: Self = Self(10);
    /// The entity named is no prop: not placed from an item, or a creature
    /// such as an animal, a pet or a survivor, which only the general
    /// entity calls reach.
    pub const NOT_A_PROP: Self = Self(11);
    /// The entity has nothing the world's next load finds it again by: a
    /// player's character, or an entity with no place in the world.
    pub const NOT_KEPT: Self = Self(12);
    /// The call cannot run here: no server runs in this process, as on a
    /// client in another's game, no world runs, the side is the other one,
    /// or this game build lacks the pieces the call needs.
    pub const UNAVAILABLE: Self = Self(13);
    /// The resource asked about is being built on this thread, by a patch
    /// further up the call, so reading it would build it again without
    /// end.
    pub const BUILDING: Self = Self(14);
    /// The loader failed inside its own code; the entry turns off and
    /// answers this until the process ends, and the mod stays on.
    pub const LOADER_FAULT: Self = Self(15);
    /// A queue is full or a rate is spent: the same call may pass later.
    pub const BUSY: Self = Self(16);
    /// The caller's buffer is too short; the call wrote the length the
    /// answer needs and nothing else.
    pub const TOO_SMALL: Self = Self(17);

    /// Whether this is [`Status::OK`].
    ///
    /// # Examples
    ///
    /// See [`Status`].
    #[must_use]
    pub const fn is_ok(self) -> bool {
        self.0 == Self::OK.0
    }
}

/// Which binary of the game runs, or where a mod runs.
///
/// A mod is [`Side::SERVER`] or [`Side::BOTH`]; [`Side::CLIENT`] names only
/// the game client as a binary.
///
/// # Examples
///
/// ```
/// use enshwick::abi::Side;
///
/// assert!(Side::BOTH.includes(Side::CLIENT));
/// assert!(!Side::SERVER.includes(Side::CLIENT));
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, bytemuck::Zeroable)]
#[expect(
    clippy::exhaustive_structs,
    reason = "a newtype over one value, which never gains a field"
)]
pub struct Side(pub u8);

impl Side {
    /// The dedicated server; as a mod's side, a mod that runs only where the
    /// world runs: the dedicated server, or a client that hosts or plays
    /// alone. Players need not install it.
    pub const SERVER: Self = Self(1);
    /// The game client, as a binary; never a mod's side.
    pub const CLIENT: Self = Self(2);
    /// As a mod's side, a mod that runs where the world runs and on every
    /// client playing in it, which must have it in the world's version.
    pub const BOTH: Self = Self(3);

    /// Whether `self` covers every side in `other`.
    ///
    /// # Examples
    ///
    /// See [`Side`].
    #[must_use]
    pub const fn includes(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// The severity of a log line.
///
/// # Examples
///
/// ```
/// use enshwick::abi::Level;
///
/// assert!(Level::ERROR.0 < Level::DEBUG.0);
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, bytemuck::Zeroable)]
#[expect(
    clippy::exhaustive_structs,
    reason = "a newtype over one value, which never gains a field"
)]
pub struct Level(pub u8);

impl Level {
    /// Something failed and a feature is off.
    pub const ERROR: Self = Self(0);
    /// Something unexpected that the mod handled.
    pub const WARN: Self = Self(1);
    /// A milestone worth one line.
    pub const INFO: Self = Self(2);
    /// Detail for development, off in releases.
    pub const DEBUG: Self = Self(3);

    /// The level's name as settings and log lines spell it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::Level;
    ///
    /// assert_eq!(Level::WARN.name(), "warn");
    /// ```
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::ERROR => "error",
            Self::WARN => "warn",
            Self::INFO => "info",
            _ => "debug",
        }
    }
}

impl Default for Level {
    fn default() -> Self {
        Self::INFO
    }
}

impl core::str::FromStr for Level {
    type Err = ParseLevelError;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "error" => Ok(Self::ERROR),
            "warn" => Ok(Self::WARN),
            "info" => Ok(Self::INFO),
            "debug" => Ok(Self::DEBUG),
            _ => Err(ParseLevelError),
        }
    }
}

impl serde::Serialize for Level {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.name())
    }
}

impl<'de> serde::Deserialize<'de> for Level {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        name.parse().map_err(serde::de::Error::custom)
    }
}

/// A level name other than `error`, `warn`, `info` or `debug`.
///
/// # Examples
///
/// ```
/// use enshwick::abi::Level;
///
/// assert!("loud".parse::<Level>().is_err());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error, crate::script::ErrorKind)]
#[error("a log level is one of \"error\", \"warn\", \"info\" or \"debug\"")]
#[expect(
    clippy::exhaustive_structs,
    reason = "a unit marker, which never gains a field"
)]
pub struct ParseLevelError;

/// The exports a mod DLL provides, by name.
///
/// # Examples
///
/// ```
/// assert_eq!(enshwick::abi::exports::INIT, "enshwick_mod_init");
/// ```
pub mod exports {
    /// `extern "system" fn() -> u16`: the [`ABI_MAJOR`](super::ABI_MAJOR)
    /// the mod was built for; its manifest's header holds the minor.
    pub const ABI: &str = "enshwick_mod_abi";
    /// `extern "system" fn() -> *const Manifest`: the manifest, or null.
    ///
    /// Any thread may call it any number of times; the first call builds the
    /// manifest and allocates, and a panic there returns null. The manifest
    /// stays valid for the process.
    pub const QUERY: &str = "enshwick_mod_query";
    /// `extern "system" fn(*const HostApi, ModHandle) -> Status`: registers
    /// hooks, services and settings.
    pub const INIT: &str = "enshwick_mod_init";
    /// `extern "system" fn(*const HostApi, ModHandle) -> Status`: after every
    /// chain is armed; any status but [`Status::OK`](super::Status::OK) turns
    /// the mod off, as a failed init does.
    pub const READY: &str = "enshwick_mod_ready";
}

const _: () = {
    assert!(size_of::<Header>() == 8);
    assert!(size_of::<Str>() == 16);
    assert!(size_of::<Status>() == 4);
    assert!(size_of::<Fingerprint>() == 20);
    // Frozen: a symbol is a name and an address, a dependency an id, a range and a relation,
    // a build row a build and its lists, which grow by their counts.
    assert!(size_of::<Symbol>() == 24);
    assert!(size_of::<BuildRow>() == 56);
    assert!(size_of::<Dependency>() == 40);
    assert!(core::mem::offset_of!(Manifest, header) == 0);
    assert!(core::mem::offset_of!(Manifest, id) == 8);
    assert!(core::mem::offset_of!(Manifest, version) == 24);
    assert!(core::mem::offset_of!(Manifest, side) == 40);
    assert!(core::mem::offset_of!(Manifest, flags) == 44);
    assert!(core::mem::offset_of!(Manifest, dependencies) == 48);
    assert!(core::mem::offset_of!(HookSpec, target) == 8);
    assert!(core::mem::offset_of!(HookSpec, next) == 24);
    assert!(core::mem::offset_of!(HookSpec, flags) == 36);
    assert!(size_of::<AffixSpec>() == 8 + 16 + 8 + 16);
    // Frozen: a modifier is the game's stat, op and range; an item line adds only its affix.
    assert!(size_of::<AffixModifierSpec>() == 20);
    assert!(size_of::<ItemLine>() == 16);
    assert!(size_of::<AffixRoll>() == 8 + 12 + 4 + 8);
    assert!(size_of::<StatSpec>() == 8 + 16 + 4);
    // Frozen: a stat reference, a modifier and its source are plain values (HostApi list rule).
    assert!(size_of::<StatRef>() == 8);
    assert!(size_of::<StatModifier>() == 12);
    assert!(size_of::<StatSource>() == 20);
    assert!(size_of::<StatModify>() == 8 + 8 + 12 + 4 + 8);
    assert!(size_of::<StatChange>() == 8 + 24);
    assert!(size_of::<SaveInfo>() == 8 + 16 + 8 + 8 + 4 + 4);
    assert!(size_of::<ChatLine>() == 8 + 8 + 4 + 4 + 8 + 16);
    assert!(size_of::<ChatRewrite>() == 8 + 8 + 4 + 4);
    assert!(size_of::<UiExtension>() == 8 + 16 + 8 + 8 + 8 + 16);
    assert!(size_of::<ItemAction>() == 8 + 24);
    assert!(size_of::<ControlSpec>() == 8 + 2 * 16 + 8 + 16 + 8 + 16);
    assert!(core::mem::offset_of!(ControlEvent, player) == 8 + 16 + 8);
    assert!(size_of::<TypeEntry>() == 8 + 4 * 16 + 8 + 8 + 16 + 3 * 8);
    // Frozen: the reflection's own field, constant and attribute records; more is read by type name.
    assert!(size_of::<TypeField>() == 16 + 16 + 8 + 8);
    assert!(size_of::<TypeConstant>() == 16 + 8);
    assert!(size_of::<TypeAttribute>() == 3 * 16);
    assert!(size_of::<MapMarkerTypeInfo>() == 8 + 16 + 4 + 1 + 4 + 3);
    assert!(size_of::<MarkerTypeSorting>() == 1);
    assert!(size_of::<TickData>() == 8 + 6 * 8);
    assert!(size_of::<TickHandle>() == 4);
    assert!(size_of::<MapFrameInfo>() == 8 + 16 + 16 + 8);
    assert!(size_of::<MapMarkerInfo>() == 8 + 24 + 16 + 16);
    assert!(size_of::<FogInfo>() == 8 + 16 + 16 + 16 + 16);
    assert!(size_of::<CallSpec>() == 8 + 16 + 8 + 16 + 16);
    assert!(size_of::<AccountActionType>() == 1);
    assert!(size_of::<PropPlace>() == 8 + 8 + 24 + 16);
    assert!(size_of::<PropRemove>() == 8 + 8);
    assert!(size_of::<VoxelWrite>() == 8 + 24 + 16);
    assert!(size_of::<VoxelRegion>() == 8 + 24);
    assert!(size_of::<BlockPlace>() == 8 + 8 + 24 + 16);
    assert!(size_of::<BuildRights>() == 8 + 8 + 48);
    // Frozen: a kind and a pointer to its headed record, which grows instead.
    assert!(size_of::<TransactionOp>() == 16);
    assert!(size_of::<TransactionInfo>() == 8 + 16 + 8 + 8 + 8);
    // Frozen: the game's packed four-byte voxel.
    assert!(size_of::<Voxel>() == 4);
    // Frozen: a cell with its voxel before and after, all a change is.
    assert!(size_of::<VoxelChange>() == 12 + 4 + 4);
    assert!(size_of::<VoxelEditRequest>() == 8 + 8 + 24 + 8 + 8);
    assert!(size_of::<AltarLimit>() == 8 + 16);
    assert!(size_of::<BaseLimitInfo>() == 8 + 28);
    assert!(size_of::<AltarZone>() == 8 + 8 + 16);
    assert!(size_of::<LoopWaitData>() == 8 + 8 + 8);
    assert!(size_of::<DataChange>() == 8 + 16 + 4 + 4 + 8 + 16 + 16 + 4 + 4);
    assert!(size_of::<RecipeKind>() == 4);
    assert!(size_of::<KnowledgeInput>() == 8 + 24);
    // Frozen: a rename is two content GUIDs.
    assert!(size_of::<ContentRename>() == 16 + 16);
    assert!(size_of::<PointSpec>() == 8 + 16 + 4 + 4 + 16 + 16);
    assert!(size_of::<PointLink>() == 8 + 3 * 8 + 3 * 4 + 4);
    assert!(size_of::<PointHandle>() == 8 + 8 + 8);
    // Frozen: a field is its name and the hash of its type, all a layout compares.
    assert!(size_of::<LayoutField>() == 24);
    assert!(size_of::<Layout>() == 16);
    assert!(size_of::<ServiceSpec>() == 8 + 16 + 8 + 16 + 8);
    assert!(size_of::<ServiceFound>() == 8 + 8 + 8 + 3 * 16);
    assert!(size_of::<EventSpec>() == 8 + 16 + 16);
    assert!(size_of::<DaytimeInfo>() == 8 + 5 * 8 + 8);
    assert!(size_of::<DamageCheck>() == 8 + 16 + 4 + 4 + 8);
    // Headed: written at the entry size the caller states.
    assert!(size_of::<BuffTypeInfo>() == 8 + 12 + 4 + 8);
    assert!(size_of::<ActiveBuff>() == 8 + 16 + 16);
    assert!(size_of::<BuffRequest>() == 8 + 16 + 8);
    assert!(size_of::<BuffTypeAdd>() == 8 + 4 * 16 + 8 + 8);
    assert!(size_of::<KnowledgeInfo>() == 8 + 16);
    assert!(size_of::<KnowledgeData>() == 8 + 12);
    assert!(size_of::<PlayerDeathData>() == 8 + 4 + 4 + 8 + 24 + 16 + 4 + 4);
    assert!(size_of::<WaypointData>() == 8 + 8 + 8 + 24 + 4 + 4);
    assert!(size_of::<SoundPlay>() == 8 + 16 + 4 + 4 + 24 + 4 + 4);
    assert!(size_of::<SoundSteer>() == 8 + 4 + 4 + 4 + 4 + 24);
    assert!(size_of::<SoundInfo>() == 8 + 4 + 4);
    assert!(size_of::<SoundAudience>() == 8 + 4 + 4 + 8 + 24);
    assert!(size_of::<QuestData>() == 8 + 8);
    assert!(size_of::<QuestStepData>() == 8 + 12);
    assert!(size_of::<QuestProgressData>() == 8 + 24);
    assert!(size_of::<QuestPayment>() == 8 + 16 + 4 + 4 * QuestPayment::MAX);
    // Headed: written at the entry size the caller states.
    assert!(size_of::<WeatherFront>() == 8 + 8 + 24 + 8 + 8 + 16);
    assert!(size_of::<WeatherSample>() == 8 + 16);
    assert!(size_of::<ChunkData>() == 8 + 12);
    assert!(size_of::<TerrainPageData>() == 8 + 12);
    assert!(size_of::<ProgressWriteData>() == 8 + 28);
    // Headed: written at the entry size the caller states.
    assert!(size_of::<TerrainMaterialInfo>() == 8 + 12);
    assert!(size_of::<TerrainEdit>() == 8 + 16 + 24 + 8 + 16);
    // Frozen: each kind fits these fields; a kind that needs more is a new entry.
    assert!(size_of::<InventoryOp>() == 32);
    assert!(size_of::<InventoryRequest>() == 32);
    assert!(size_of::<InventoryOpResult>() == 8);
    assert!(size_of::<InventoryResult>() == 24);
    assert!(size_of::<ItemTypeInfo>() == 20 + 16);
    assert!(size_of::<PlayerInfo>() == 8 + 4 + 4 + 8 + 4 + 4 + 128);
    // Frozen: the whole x64 general-purpose set; vector registers get an entry of their own.
    assert!(size_of::<Registers>() == 17 * 8);
    assert!(core::mem::offset_of!(Registers, rsp) == 4 * 8);
    assert!(core::mem::offset_of!(Registers, rflags) == 16 * 8);
    assert!(size_of::<MidHookSpec>() == 8 + 32);
    assert!(size_of::<EventTypeInfo>() == 8 + 16 + 4 + 4);
    assert!(size_of::<ActionCall>() == 8 + 8 + 8 + 16 + 16 + 8 + 8 + 16 + 4 + 4);
    assert!(size_of::<TeleportRequest>() == 8 + 8 + 24 + 16);
    assert!(size_of::<RespawnPointInfo>() == 8 + 8 + 24 + 16);
    assert!(size_of::<SpawnRequest>() == 8 + 16 + 24 + 16 + 8 + 8);
    assert!(size_of::<CraftCheck>() == 8 + 16);
    assert!(size_of::<BuildCheck>() == 8 + 8 + 24 + 8 + 8);
    assert!(size_of::<NetMessage>() == 8 + 8 + 8 + 8 + 8);
    assert!(size_of::<UiFrame>() == 64 + 16 + 24 + 24);
    assert!(size_of::<UiTooltipItem>() == 8 + 8 + 8);
    assert!(size_of::<UiWidget>() == 8 + 16 + 32 + 8 + 32 + 16 + 8);
    assert!(size_of::<UiMarker>() == 8 + 24 + 16 + 8);
    assert!(size_of::<UiDialog>() == 8 + 48 + 8);
    assert!(size_of::<GameEventData>() == 24);
    // Frozen: an entity and its place, a plain value (HostApi list rule).
    assert!(size_of::<EntityPlace>() == 32);
    assert!(size_of::<SkillCheck>() == 20);
    assert!(size_of::<Comfort>() == 40);
    // Frozen: one character's row; what concerns the pass grows in the headed ExperiencePass.
    assert!(size_of::<ExperienceRow>() == 20);
    assert!(size_of::<ExperiencePass>() == 40);
    assert!(size_of::<Interaction>() == 8 + 16 + 8 + 24 + 24 + 16);
    assert!(size_of::<ContainerPut>() == 8 + 16 + 8);
    assert!(size_of::<SurvivorLookup>() == 8 + 16 + 24 + 16);
    assert!(size_of::<BedCheck>() == 8 + 12 + 4 + 48);
    assert!(size_of::<Summon>() == 8 + 8 + 16 + 24 + 8 + 4 + 4 + 4 + 4);
    assert!(size_of::<Bounds>() == 48);
    assert!(size_of::<AltarInfo>() == 16 + 17 * 48 + 24);
    assert!(size_of::<BuildingZoneCheck>() == 8 + 16 + 48 + 8);
    assert!(core::mem::offset_of!(Manifest, name) == 80);
    assert!(core::mem::offset_of!(Manifest, points) == 80 + 16 * 4 + 16);
    assert!(core::mem::offset_of!(Manifest, exports) == 80 + 16 * 4 + 16 + 16);
    assert!(size_of::<Manifest>() == 80 + 16 * 4 + 16 + 16 + 16);
    assert!(size_of::<ResourceKey>() == 24);
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_headed_struct_from_a_zeroed_base_takes_this_side_s_header() {
        let base: TickData = bytemuck::Zeroable::zeroed();
        let tick = crate::headed!(TickData { step: 3, ..base });
        assert_eq!(tick.header, Header::of::<TickData>());
        assert_eq!(tick.step, 3);
    }

    #[test]
    fn a_null_or_impossible_text_reads_as_none() {
        let null = Str {
            ptr: core::ptr::null(),
            len: 5,
        };
        let huge = Str {
            ptr: c"x".as_ptr().cast(),
            len: usize::MAX,
        };
        // SAFETY: `as_str` returns `None` for a null pointer and for a length
        // over `isize::MAX` before it reads, so neither view is dereferenced.
        let (null, huge) = (unsafe { null.as_str() }, unsafe { huge.as_str() });
        assert_eq!((null, huge), (None, None));
    }

    #[test]
    fn a_position_in_range_is_finite_and_below_two_to_the_31_metres_on_each_axis() {
        let below = MAX_POSITION_METRES - 1.0;
        assert!(in_fixed_range([below, -below, 0.0]));
        for out in [
            [MAX_POSITION_METRES, 0.0, 0.0],
            [0.0, -MAX_POSITION_METRES, 0.0],
            [0.0, 0.0, 1e300],
            [f64::NEG_INFINITY, 0.0, 0.0],
            [0.0, f64::NAN, 0.0],
        ] {
            assert!(!in_fixed_range(out), "{out:?}");
        }
    }

    #[test]
    fn a_box_whose_voxel_count_overflows_is_refused() {
        assert_eq!(Voxel::read_count([0; 3], [1 << 30, 1 << 30, 16]), None);
        assert_eq!(Voxel::read_count([0; 3], [65, 64, 64]), None);
        assert_eq!(
            Voxel::read_count([0; 3], [64, 64, 64]),
            Some(Voxel::READ_MAX)
        );
    }

    #[test]
    fn an_empty_box_or_one_past_the_end_reads_no_voxels() {
        assert_eq!(Voxel::read_count([0; 3], [0, 3, 4]), None);
        assert_eq!(Voxel::read_count([Voxel::END, 0, 0], [1; 3]), None);
        assert_eq!(Voxel::read_count([Voxel::END - 1, 0, 0], [1; 3]), Some(1));
    }
}

#[cfg(test)]
mod layout_tests;
#[cfg(test)]
mod proofs;
