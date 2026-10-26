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
///     flags: ManifestFlags::JOIN,
///     off: 0,
///     file: Str::new("mods/trading_post.dll"),
///     name: Str::new("Trading Post"),
///     description: Str::EMPTY,
///     authors: core::ptr::null(),
///     author_count: 0,
///     license: Str::new("MIT"),
///     homepage: Str::EMPTY,
///     replaced_by: Str::EMPTY,
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
    /// The sides the mod runs on.
    pub side: Side,
    /// The manifest's flags; [`ManifestFlags::JOIN`] when it counts in the
    /// join check.
    pub flags: ManifestFlags,
    /// 1 when the mod is off: refused at init or faulted since.
    pub off: u8,
    /// The DLL's path relative to the game's folder, with `/` separators,
    /// such as `mods/trading_post.dll`.
    pub file: Str,
    /// The manifest's human metadata, as in [`Manifest`](super::Manifest); in
    /// [`HostApi::join_missing`](super::HostApi::join_missing), the name and homepage the server advertises
    /// and empty strings for the rest. Since minor 36.
    pub name: Str,
    /// One sentence on what the mod does. Since minor 36.
    pub description: Str,
    /// The authors' names. Since minor 36.
    pub authors: *const Str,
    /// How many names `authors` points at. Since minor 36.
    pub author_count: usize,
    /// An SPDX licence expression. Since minor 36.
    pub license: Str,
    /// Where players read about the mod. Since minor 36.
    pub homepage: Str,
    /// In [`HostApi::replaced`](super::HostApi::replaced), the id of the mod that replaces this one;
    /// empty elsewhere. Since minor 54.
    pub replaced_by: Str,
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
        replaced_by: Str::EMPTY,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ModHandle(pub usize);

impl ModHandle {
    /// No mod.
    pub const NONE: Self = Self(0);
}
