//! Game resources and recipe rules a mod patches or serves.

use core::ffi::c_void;

use crate::abi::{Header, Status, Str};

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

/// The vanilla value a [`DataChange`] changes; the field says what its
/// `entry` is. A later minor may add fields, which an older loader refuses
/// with [`Status::BAD_ARGS`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::DataField;
///
/// assert_ne!(DataField::MAX_STACK, DataField(0));
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, bytemuck::Zeroable)]
pub struct DataField(pub u32);

impl DataField {
    /// How many of an item one inventory slot holds, 1 to 65535; the entry
    /// is an item.
    pub const MAX_STACK: Self = Self(1);
    /// How many of each item a recipe makes; the entry is a recipe.
    pub const RECIPE_OUTPUT: Self = Self(2);
    /// How many of each ingredient a recipe takes; the entry is a recipe.
    pub const RECIPE_COST: Self = Self(3);
    /// How many of the ingredient `item` a recipe takes, 0 taking it out; the
    /// entry is a recipe.
    pub const RECIPE_INGREDIENT: Self = Self(4);
    /// How long a recipe takes, in seconds; the entry is a recipe.
    pub const RECIPE_TIME: Self = Self(5);
}

/// How a [`DataChange`] changes its value.
///
/// # Examples
///
/// ```
/// use enshwick::abi::ChangeOp;
///
/// assert_ne!(ChangeOp::SET, ChangeOp::TIMES);
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, bytemuck::Zeroable)]
pub struct ChangeOp(pub u32);

impl ChangeOp {
    /// The value becomes `value`.
    pub const SET: Self = Self(0);
    /// The value is multiplied by `value`, a finite factor from 0 to 100.
    pub const TIMES: Self = Self(1);
}

/// One change of a vanilla value, as
/// [`HostApi::change_data`](super::HostApi::change_data) takes it. The
/// loader copies it; `source` is borrowed for the call.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{ChangeOp, DataChange, DataField, Header, Str};
///
/// let change = DataChange {
///     header: Header::of::<DataChange>(),
///     entry: [0x3a; 16],
///     field: DataField::MAX_STACK,
///     op: ChangeOp::SET,
///     value: 999.0,
///     source: Str::new("items.toml:3"),
///     item: [0; 16],
/// };
/// assert_eq!(change.value, 999.0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct DataChange {
    /// Size and version of this struct.
    pub header: Header,
    /// The GUID of the vanilla entry, in memory order.
    pub entry: [u8; 16],
    /// The value changed.
    pub field: DataField,
    /// Whether `value` replaces it or scales it.
    pub op: ChangeOp,
    /// The new value or the factor.
    pub value: f64,
    /// Where the change comes from, such as `items.toml:12`, for the reports
    /// of conflicts; may be empty.
    pub source: Str,
    /// The item the field names inside its entry, such as a recipe's
    /// ingredient; all zero for none. Since minor 77.
    pub item: [u8; 16],
}

impl_headed!(DataChange);

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
