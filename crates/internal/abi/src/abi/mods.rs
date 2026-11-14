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
/// let post = ModInfo {
///     header: Header::of::<ModInfo>(),
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
/// };
/// assert_eq!(post.off, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
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
    pub author_count: usize,
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
    /// let post = ModInfo { id: Str::new("you.trading-post"), ..ModInfo::EMPTY };
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
pub struct ModHandle(pub usize);

impl ModHandle {
    /// No mod.
    pub const NONE: Self = Self(0);
}
