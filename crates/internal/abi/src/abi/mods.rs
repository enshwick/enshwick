//! The loaded mods as the loader reports them, and a mod's handle.

use crate::abi::{Header, ManifestFlags, Side, Str};

/// A mod the loader loaded, from [`HostApi::mods`](super::HostApi::mods); the loader itself comes
/// first, with the id `enshwick`.
///
/// Every string stays valid for the life of the process.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, ManifestFlags, ModInfo, Side, Str};
///
/// let post = enshwick::abi::headed!(ModInfo {
///     id: Str::new("you.trading-post"),
///     version: Str::new("0.1.0"),
///     side: Side::BOTH,
///     flags: ManifestFlags::NONE,
///     off: 0,
///     file: Str::new("mods/you.trading-post/trading_post.dll"),
///     name: Str::new("Trading Post"),
///     description: Str::EMPTY,
///     authors: core::ptr::null(),
///     author_count: 0,
///     license: Str::new("MIT"),
///     homepage: Str::EMPTY,
/// });
/// assert_eq!(post.off, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct ModInfo {
    /// Size and version of this struct.
    pub header: Header,
    /// The mod id.
    pub id: Str,
    /// The mod's SemVer version.
    pub version: Str,
    /// Where the mod runs, [`Side::SERVER`] or [`Side::BOTH`]; a
    /// [`Side::BOTH`] mod counts in the join check.
    pub side: Side,
    /// The manifest's flags.
    pub flags: ManifestFlags,
    /// 1 when the mod is off: refused at init or faulted since.
    pub off: u8,
    /// The DLL's path relative to the game's folder, with `/` separators,
    /// such as `mods/you.trading-post/trading_post.dll`.
    pub file: Str,
    /// The manifest's human metadata, as in [`Manifest`](super::Manifest);
    /// empty in [`HostApi::join_missing`](super::HostApi::join_missing),
    /// which never passes on text a server chose.
    pub name: Str,
    /// One sentence on what the mod does.
    pub description: Str,
    /// The authors' names.
    pub authors: *const Str,
    /// How many names `authors` points at.
    pub author_count: u32,
    /// An SPDX licence expression.
    pub license: Str,
    /// Where players read about the mod.
    pub homepage: Str,
}

impl_headed!(ModInfo);

impl ModInfo {
    /// A listing with every field empty, to fill with `..ModInfo::EMPTY`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{ModInfo, Str};
    ///
    /// let post = enshwick::abi::headed!(ModInfo { id: Str::new("you.trading-post"), ..ModInfo::EMPTY });
    /// assert!(post.version.is_empty());
    /// ```
    pub const EMPTY: Self = Self {
        header: Header::of::<Self>(),
        id: Str::EMPTY,
        version: Str::EMPTY,
        side: Side(0),
        flags: ManifestFlags::NONE,
        off: 0,
        file: Str::EMPTY,
        name: Str::EMPTY,
        description: Str::EMPTY,
        authors: core::ptr::null(),
        author_count: 0,
        license: Str::EMPTY,
        homepage: Str::EMPTY,
    };
}

// SAFETY: a listing's strings and its author list stay valid and unchanged
// for the life of the process, as `Str` itself is shared.
unsafe impl Send for ModInfo {}
// SAFETY: a listing's strings and its author list stay valid and unchanged
// for the life of the process and nothing writes through them, so threads
// sharing a `&ModInfo` only read immutable data.
unsafe impl Sync for ModInfo {}

/// A part or feature of the loader that is off, which
/// [`HostApi::loader_off`](super::HostApi::loader_off) lists.
///
/// The loader turns off only what a build lacks or what faulted, and keeps
/// running the rest; every part or feature it does not list runs. The
/// names are the ones the log and the Mods screen show, and a new part of
/// a later loader appears with no new field.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, LoaderOff};
///
/// let off = enshwick::abi::headed!(LoaderOff { ..LoaderOff::EMPTY });
/// assert_eq!(off.at, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct LoaderOff {
    /// Size and version of this struct.
    pub header: Header,
    /// The part or feature, as its log line names it.
    pub name: Str,
    /// Why it is off, as the log says: the build entry it lacks and why,
    /// or the fault that turned it off.
    pub reason: Str,
    /// When it turned off, in seconds since the Unix epoch.
    pub at: u64,
}

impl_headed!(LoaderOff);

impl LoaderOff {
    /// An entry with every field empty, to fill with `..LoaderOff::EMPTY`.
    pub const EMPTY: Self = Self {
        header: Header::of::<Self>(),
        name: Str::EMPTY,
        reason: Str::EMPTY,
        at: 0,
    };
}

// SAFETY: the loader keeps each listed name and reason alive and unchanged
// for the life of the process, as `Str` itself is shared.
unsafe impl Send for LoaderOff {}
// SAFETY: the same text, which nothing writes through, so threads sharing a
// `&LoaderOff` only read immutable data.
unsafe impl Sync for LoaderOff {}

/// An opaque handle the loader gives each mod; every call names its caller
/// with it.
///
/// # Examples
///
/// ```
/// use enshwick::abi::ModHandle;
///
/// assert_eq!(ModHandle::NONE, ModHandle(0));
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, bytemuck::Zeroable)]
#[expect(
    clippy::exhaustive_structs,
    reason = "a newtype over one value, which never gains a field"
)]
pub struct ModHandle(pub usize);

impl ModHandle {
    /// No mod.
    pub const NONE: Self = Self(0);
}
