//! Game resources and recipe rules a mod patches or serves.

use core::ffi::c_void;

use crate::abi::Status;

/// A game resource's key: its GUID in memory order, its type hash and part.
///
/// # Examples
///
/// ```
/// use enshwick::abi::ResourceKey;
///
/// let item = ResourceKey { guid: [0x3a; 16], type_hash: 0xb5ce_8765, part: 0 };
/// assert_eq!(item.part, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResourceKey {
    /// The resource's GUID, in memory order (`bytes_le` of the textual form).
    pub guid: [u8; 16],
    /// The reflected type hash of the resource's root type.
    pub type_hash: u32,
    /// The part of a resource split in parts; 0 for most.
    pub part: u32,
}

/// Builds a served resource from its base: `base` is the base resource's bytes,
/// valid for the call.
///
/// On [`Status::OK`] the function writes a pointer and length of bytes that
/// stay valid and writable for the life of the process: a rebuild
/// ([`HostApi::rebuild_resource`](super::HostApi::rebuild_resource)) writes
/// over the last patch's bytes.
pub type ResourcePatchFn = unsafe extern "system" fn(
    base: *const u8,
    base_len: usize,
    user: *mut c_void,
    out: *mut *const u8,
    out_len: *mut usize,
) -> Status;

/// What a recipe rule of [`HostApi::recipe_rule`](super::HostApi::recipe_rule) names.
///
/// # Examples
///
/// ```
/// use enshwick::abi::RecipeRuleKind;
///
/// assert_ne!(RecipeRuleKind::RECIPE, RecipeRuleKind::OUTPUT);
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RecipeRuleKind(pub u32);

impl RecipeRuleKind {
    /// One recipe: its debug name, content id or GUID.
    pub const RECIPE: Self = Self(0);
    /// Every recipe that makes an item: the item's debug name, content id
    /// or GUID.
    pub const OUTPUT: Self = Self(1);
    /// Every recipe made at a workshop: `hand`, the debug name of the item
    /// the workshop belongs to, the workshop's content id or GUID.
    pub const WORKSHOP: Self = Self(2);
}

/// What the item a recipe makes is, from
/// [`HostApi::recipe_kind`](super::HostApi::recipe_kind); a later minor may
/// add kinds, which a mod reads as [`RecipeKind::OTHER`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::RecipeKind;
///
/// assert_ne!(RecipeKind::ROOF, RecipeKind::BUILDING_BLOCK);
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RecipeKind(pub u32);

impl RecipeKind {
    /// None of the kinds below: equipment, props, materials and the rest.
    pub const OTHER: Self = Self(0);
    /// A building block.
    pub const BUILDING_BLOCK: Self = Self(1);
    /// A roof block.
    pub const ROOF: Self = Self(2);
    /// An overgrowth material.
    pub const OVERGROWTH: Self = Self(3);
    /// A terrain material.
    pub const TERRAIN: Self = Self(4);
}

/// Flags of [`HostApi::entity_data_open`](super::HostApi::entity_data_open).
///
/// # Examples
///
/// ```
/// use enshwick::abi::EntityDataFlags;
///
/// assert_ne!(EntityDataFlags::SENT, EntityDataFlags::NONE);
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EntityDataFlags(pub u32);

impl EntityDataFlags {
    /// The server keeps the values to itself.
    pub const NONE: Self = Self(0);
    /// The clients that run Enshwick get every value.
    pub const SENT: Self = Self(1);
}
