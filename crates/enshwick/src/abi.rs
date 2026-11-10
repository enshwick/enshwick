//! The binary interface between the loader and a mod DLL.
//!
//! Everything here is `repr(C)` and crosses DLL boundaries, so the layout is
//! the contract: structs start with a [`Header`], enums are integers, strings
//! are borrowed for the call, and no function unwinds. A loader loads the
//! mods built for its own [`ABI_MAJOR`] and its [`ABI_MINOR`] or an older
//! one ([`check_abi`]). Mods normally use the safe layer of this crate
//! instead of these types.

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

/// Whether `id` is a mod id of either form `replaces` takes: a valid id, or
/// 1 to 64 characters of `[a-z0-9-]`, the old form without an author.
///
/// # Examples
///
/// ```
/// use enshwick::abi::is_valid_old_form_id;
///
/// assert!(is_valid_old_form_id("trading-post"));
/// assert!(is_valid_old_form_id("you.trading-post"));
/// assert!(!is_valid_old_form_id("Trading Post"));
/// ```
#[must_use]
pub const fn is_valid_old_form_id(id: &str) -> bool {
    if is_valid_id(id) {
        return true;
    }
    let bytes = id.as_bytes();
    if bytes.is_empty() || bytes.len() > 64 {
        return false;
    }
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if !(b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-') {
            return false;
        }
        i += 1;
    }
    true
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

/// A struct that starts with a [`Header`] and crosses the boundary only as
/// far as both sides know it, through [`read_headed`] and [`write_headed`].
///
/// [`Zeroable`](bytemuck::Zeroable) makes the fields the writer does not know
/// read as zero.
///
/// # Safety
///
/// - The type is `repr(C)` and its first field is a [`Header`] at offset 0.
/// - Any bytes the other side writes for the fields it knows form a valid
///   value of those fields, which holds for the integer, pointer and
///   function pointer fields the ABI uses.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Headed, Header, TickData};
///
/// fn size_of_headed<T: Headed>() -> usize {
///     size_of::<T>()
/// }
/// assert_eq!(size_of_headed::<TickData>(), Header::of::<TickData>().size as usize);
/// ```
pub unsafe trait Headed: bytemuck::Zeroable + Copy {
    /// Where the fields every writer of this [`ABI_MAJOR`] knows end: the
    /// struct's size at minor 0 of the major. A reader of a struct the other
    /// side wrote requires this much, and reads a later field only where
    /// [`Header::covers`] it; the layout tests hold it to the golden file.
    const BORN: usize;
}

/// Implements [`Headed`] for an ABI struct beside its definition and checks at
/// compile time that its [`Header`] sits at offset 0. `born` is the struct's
/// size at minor 0 of this major, once a minor appended a field to it.
macro_rules! impl_headed {
    ($ty:ty) => {
        impl_headed!($ty, born = size_of::<$ty>());
    };
    ($ty:ty, born = $born:expr) => {
        // SAFETY: the struct is `repr(C)`, its first field is the `Header`
        // checked at offset 0 below, and its other fields are integers,
        // pointers and `Option`s of function pointers, valid for any bytes
        // the other side writes.
        unsafe impl $crate::abi::Headed for $ty {
            const BORN: usize = $born;
        }
        const _: () = assert!(core::mem::offset_of!($ty, header) == 0);
        const _: () =
            assert!($born >= size_of::<$crate::abi::Header>() && $born <= size_of::<$ty>());
    };
}

/// Copies a struct that starts with a [`Header`] from the other side, which
/// may know an older or a newer version of it.
///
/// Reads only the bytes the writer's header covers; the fields it does not
/// know stay zero. `None` when `raw` is null or the writer's struct ends
/// before byte `required`, the end of the fields every version has. The copy
/// keeps the writer's header, so check [`Header::covers`] before trusting a
/// field newer than `required` that zero does not default well.
///
/// # Safety
///
/// - `raw` is null, or points at a `T` the other side wrote, readable for the
///   size its header states; no alignment is needed.
/// - No other thread writes that struct during the copy.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{TickData, read_headed};
///
/// let mut old = TickData::new(7);
/// old.header.size = 8;
/// // SAFETY: `old` is a tick readable for the eight bytes its header states.
/// let tick = unsafe { read_headed(&raw const old, 8) }.ok_or("too short")?;
/// assert_eq!(tick.step, 0);
/// # Ok::<(), &str>(())
/// ```
#[must_use]
pub unsafe fn read_headed<T: Headed>(raw: *const T, required: usize) -> Option<T> {
    if raw.is_null() {
        return None;
    }
    // SAFETY: `raw` is non-null and the caller guarantees it is readable for
    // its header, the first eight bytes of every `Headed` type.
    // The other side's headed struct, not game memory.
    // ast-grep-ignore: game-memory-through-raw
    let header = unsafe { raw.cast::<Header>().read_unaligned() };
    if !header.covers(required.max(size_of::<Header>())) {
        return None;
    }
    if header.covers(size_of::<T>()) {
        // SAFETY: the caller guarantees `raw` readable for `header.size`
        // bytes, which the check above found at least a whole `T`, and
        // `Headed` makes any bytes the writer wrote a valid `T`.
        // The other side's headed struct, not game memory.
        // ast-grep-ignore: game-memory-through-raw
        return Some(unsafe { raw.read_unaligned() });
    }
    let known = (header.size as usize).min(size_of::<T>());
    let mut out = T::zeroed();
    // SAFETY: the caller guarantees `raw` readable for `header.size` bytes and
    // `out` is a local `T`; `known` is at most both sizes, and the two do not
    // overlap.
    unsafe {
        core::ptr::copy_nonoverlapping(raw.cast::<u8>(), (&raw mut out).cast::<u8>(), known);
    };
    Some(out)
}

/// Writes `value`, a struct that starts with a [`Header`], to the other side,
/// which may know an older or a newer version of it.
///
/// Writes only the bytes both sides know, and in the written header the
/// size of what it wrote. `false`, writing nothing, when `out` is null or its
/// header is shorter than a [`Header`].
///
/// # Safety
///
/// - `out` is null, or points at a `T` whose header states its size, writable
///   for that many bytes; no alignment is needed.
/// - Nothing else reads or writes that struct during the write.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{TickData, write_headed};
///
/// let mut old = TickData::new(0);
/// old.header.size = 8;
/// let new = TickData::new(7);
/// // SAFETY: `old` is a tick writable for the eight bytes its header states.
/// assert!(unsafe { write_headed(&raw mut old, new) });
/// assert_eq!((old.header.size, old.step), (8, 0));
/// ```
#[must_use]
pub unsafe fn write_headed<T: Headed>(out: *mut T, value: T) -> bool {
    if out.is_null() {
        return false;
    }
    // SAFETY: `out` is non-null and the caller guarantees it is readable for
    // its header, the first eight bytes of every `Headed` type.
    // The other side's headed out struct, not game memory.
    // ast-grep-ignore: game-memory-through-raw
    let room = unsafe { out.cast::<Header>().read_unaligned() }.size as usize;
    if room < size_of::<Header>() {
        return false;
    }
    let known = room.min(size_of::<T>());
    // SAFETY: the caller guarantees `out` writable for `room` bytes and
    // `value` is a local `T`; `known` is at most both sizes, and the two do
    // not overlap.
    unsafe {
        core::ptr::copy_nonoverlapping((&raw const value).cast::<u8>(), out.cast(), known);
    };
    // `known` is at most `room`, a `u32`.
    let size = u32::try_from(known).unwrap_or(u32::MAX);
    // SAFETY: `size` is the header's first field, inside the `room` bytes the
    // caller guarantees writable.
    unsafe { out.cast::<u32>().write_unaligned(size) };
    true
}

mod actions;
mod altars;
mod buffs;
mod chat;
mod checks;
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
mod rates;
mod services;
mod stats;
mod survivors;
mod transactions;
mod ui;
mod version;
mod world;
mod writes;

pub use actions::*;
pub use altars::*;
pub use buffs::*;
pub use chat::*;
pub use checks::*;
pub use content::*;
pub use controls::*;
pub use events::*;
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
pub use rates::*;
pub use services::*;
pub use stats::*;
pub use survivors::*;
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
pub struct Status(pub u32);

impl Status {
    /// Success.
    pub const OK: Self = Self(0);
    /// What was asked for does not exist on this game build.
    pub const MISSING: Self = Self(1);
    /// More than one candidate answers a lookup.
    pub const AMBIGUOUS: Self = Self(2);
    /// The request conflicts with another mod or with the loader's rules.
    pub const REFUSED: Self = Self(3);
    /// An argument is malformed.
    pub const BAD_ARGS: Self = Self(4);
    /// The call came in a phase where it is not allowed, such as a hook after
    /// arming.
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
    /// The call cannot run in this process: no server runs here, as on a
    /// client in another's game, or this build lacks the pieces the call
    /// needs; [`Status::MISSING`] then means only that what the call names
    /// is not there. A mod built for an older minor reads it as any status
    /// it does not know.
    pub const UNAVAILABLE: Self = Self(13);
    /// The resource asked about is being built on this thread, by a patch
    /// further up the call, so reading it would build it again without
    /// end. A mod built for an older minor reads it as any status it does
    /// not know.
    pub const BUILDING: Self = Self(14);

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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("a log level is one of \"error\", \"warn\", \"info\" or \"debug\"")]
pub struct ParseLevelError;

/// The exports a mod DLL provides, by name.
///
/// # Examples
///
/// ```
/// assert_eq!(enshwick::abi::exports::INIT, "enshwick_mod_init");
/// ```
pub mod exports {
    /// `extern "system" fn() -> u32`: the [`ABI_MAJOR`](super::ABI_MAJOR)
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
    /// `extern "system" fn(*const HostApi, ModHandle)`: after every chain is
    /// armed.
    pub const READY: &str = "enshwick_mod_ready";
}

const _: () = {
    assert!(size_of::<Header>() == 8);
    assert!(size_of::<Str>() == 16);
    assert!(size_of::<Status>() == 4);
    assert!(size_of::<Fingerprint>() == 20);
    assert!(size_of::<Symbol>() == 24);
    assert!(core::mem::offset_of!(Manifest, header) == 0);
    assert!(core::mem::offset_of!(Manifest, id) == 8);
    assert!(core::mem::offset_of!(Manifest, version) == 24);
    assert!(core::mem::offset_of!(Manifest, side) == 40);
    assert!(core::mem::offset_of!(Manifest, flags) == 44);
    assert!(core::mem::offset_of!(Manifest, dependencies) == 48);
    assert!(core::mem::offset_of!(HookSpec, target) == 0);
    assert!(core::mem::offset_of!(HookSpec, next) == 16);
    assert!(core::mem::offset_of!(HookSpec, flags) == 28);
    assert!(size_of::<AffixDefData>() == 8 + 32 + 8 + 16);
    // Frozen: read and written as arrays.
    assert!(size_of::<AffixModifierData>() == 20);
    assert!(size_of::<ItemLine>() == 16);
    assert!(size_of::<AffixRoll>() == 8 + 12 + 4 + 8);
    assert!(size_of::<StatDefData>() == 8 + 16 + 16);
    // Frozen: written as arrays.
    assert!(size_of::<StatRef>() == 8);
    assert!(size_of::<StatModifierData>() == 12);
    assert!(size_of::<StatSource>() == 20);
    assert!(size_of::<StatModify>() == 8 + 8 + 12 + 4 + 8);
    assert!(size_of::<StatChange>() == 8 + 24);
    assert!(size_of::<SaveInfo>() == 8 + 16 + 8 + 8 + 4 + 4);
    assert!(size_of::<ChatLine>() == 8 + 8 + 4 + 4 + 8 + 16);
    assert!(size_of::<ChatRewrite>() == 8 + 8 + 4 + 4);
    assert!(size_of::<UiExtension>() == 8 + 16 + 8 + 8 + 8);
    assert!(size_of::<ItemAction>() == 8 + 24);
    assert!(size_of::<ControlSpec>() == 8 + 2 * 16 + 8 + 16 + 8 + 16);
    assert!(core::mem::offset_of!(ControlEvent, player) == 8 + 16 + 8);
    assert!(size_of::<TypeEntry>() == 8 + 4 * 16 + 8 + 8 + 16 + 3 * 8);
    // Frozen: read as arrays.
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
    // Frozen: read as an array.
    assert!(size_of::<TransactionOp>() == 16);
    assert!(size_of::<TransactionInfo>() == 8 + 16 + 8 + 8 + 8);
    // Frozen: written as an array.
    assert!(size_of::<Voxel>() == 4);
    // Frozen: read as an array.
    assert!(size_of::<VoxelChange>() == 12 + 4 + 4);
    assert!(size_of::<VoxelEditData>() == 8 + 8 + 24 + 8 + 8);
    assert!(size_of::<AltarLimit>() == 8 + 8);
    assert!(size_of::<BaseLimitInfo>() == 8 + 28);
    assert!(size_of::<AltarZone>() == 8 + 8 + 16);
    assert!(size_of::<LoopWaitData>() == 8 + 8 + 8);
    assert!(size_of::<DataChange>() == 8 + 16 + 4 + 4 + 8 + 16 + 16 + 4 + 4);
    assert!(size_of::<RecipeKind>() == 4);
    assert!(size_of::<KnowledgeInput>() == 8 + 24);
    // Frozen: written as an array.
    assert!(size_of::<ContentRename>() == 16 + 16);
    assert!(size_of::<PointSpec>() == 8 + 16 + 4 + 4 + 16 + 16);
    assert!(size_of::<PointLink>() == 8 + 3 * 8 + 3 * 4 + 4);
    assert!(size_of::<PointHandle>() == 8 + 8 + 8);
    // Frozen: written as an array.
    assert!(size_of::<LayoutField>() == 24);
    assert!(size_of::<Layout>() == 16);
    assert!(size_of::<ServiceSpec>() == 8 + 16 + 8 + 16 + 8);
    assert!(size_of::<ServiceFound>() == 8 + 8 + 8 + 3 * 16);
    assert!(size_of::<EventSpec>() == 8 + 16 + 16);
    assert!(size_of::<DaytimeInfo>() == 8 + 5 * 8 + 8);
    assert!(size_of::<DamageCheck>() == 8 + 16 + 13 * 4 + 4);
    // Frozen: written as arrays.
    assert!(size_of::<BuffTypeInfo>() == 8 + 12 + 4 + 8);
    assert!(size_of::<ActiveBuff>() == 8 + 16 + 16);
    assert!(size_of::<BuffRequest>() == 8 + 16 + 8);
    assert!(size_of::<KnowledgeInfo>() == 8 + 16);
    assert!(size_of::<KnowledgeData>() == 8 + 12);
    assert!(size_of::<PlayerDeathData>() == 8 + 8 + 8 + 12 + 16 + 4 + 8);
    assert!(size_of::<QuestData>() == 8 + 8);
    // Written as an array at the stride of the mod's own header.
    assert!(size_of::<WeatherFront>() == 8 + 8 + 24 + 8 + 8 + 16);
    assert!(size_of::<ChunkData>() == 8 + 16);
    assert!(size_of::<TerrainPageData>() == 8 + 12);
    // Frozen: written as an array.
    assert!(size_of::<TerrainMaterialInfo>() == 8 + 12);
    assert!(size_of::<TerrainEdit>() == 8 + 16 + 12 + 8 + 16);
    assert!(size_of::<InventoryOp>() == 32);
    assert!(size_of::<InventoryRequest>() == 32);
    assert!(size_of::<InventoryOpResult>() == 8);
    assert!(size_of::<InventoryResult>() == 24);
    assert!(size_of::<ItemData>() == 20 + 16);
    // Frozen: embedded in calls and written as an array.
    assert!(size_of::<PlayerInfo>() == 96);
    assert!(core::mem::offset_of!(PlayerInfo, name) == 26);
    assert!(size_of::<Registers>() == 17 * 8);
    assert!(core::mem::offset_of!(Registers, rsp) == 4 * 8);
    assert!(core::mem::offset_of!(Registers, rflags) == 16 * 8);
    assert!(size_of::<MidHookSpec>() == 32);
    assert!(size_of::<EventTypeInfo>() == 8 + 16 + 4 + 4);
    assert!(size_of::<ActionCall>() == 8 + 8 + 96 + 16 + 16 + 40 + 8 + 16);
    assert!(size_of::<TeleportRequest>() == 8 + 8 + 24 + 16);
    assert!(size_of::<SpawnRequest>() == 8 + 16 + 24 + 16 + 8 + 8 + 8);
    assert!(size_of::<CraftCheck>() == 8 + 16);
    assert!(size_of::<BuildCheck>() == 8 + 8 + 24 + 8);
    assert!(size_of::<NetMessage>() == 8 + 8 + 8 + 8 + 8);
    assert!(size_of::<UiFrame>() == 64 + 16 + 24);
    assert!(size_of::<UiWidget>() == 8 + 16 + 32 + 8 + 32 + 16);
    assert!(size_of::<UiMarker>() == 8 + 24 + 16 + 8);
    assert!(size_of::<UiDialog>() == 8 + 48 + 8);
    assert!(size_of::<GameEventData>() == 24);
    // Frozen: written as an array.
    assert!(size_of::<EntityPlace>() == 32);
    assert!(size_of::<SkillCheck>() == 20);
    assert!(size_of::<Comfort>() == 28);
    // Frozen: passed as an array.
    assert!(size_of::<ExperienceRow>() == 20);
    assert!(size_of::<ExperiencePass>() == 48);
    assert!(size_of::<Interaction>() == 8 + 16 + 8 + 24);
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
    fn a_box_whose_voxel_count_overflows_is_refused() {
        assert_eq!(Voxel::read_count([0; 3], [1 << 30, 1 << 30, 16]), None);
        assert_eq!(Voxel::read_count([0; 3], [65, 64, 64]), None);
        assert_eq!(
            Voxel::read_count([0; 3], [64, 64, 64]),
            Some(Voxel::READ_MAX)
        );
    }
}

#[cfg(test)]
mod layout_tests;
#[cfg(test)]
mod proofs;
