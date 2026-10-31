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
///
/// # Examples
///
/// See [`HostApi::serve_resource`](super::HostApi::serve_resource), which takes it.
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
    /// None of the kinds below: equipment, functional props, materials and
    /// the rest.
    pub const OTHER: Self = Self(0);
    /// A building block.
    pub const BUILDING_BLOCK: Self = Self(1);
    /// A roof block.
    pub const ROOF: Self = Self(2);
    /// An overgrowth material.
    pub const OVERGROWTH: Self = Self(3);
    /// A terrain material.
    pub const TERRAIN: Self = Self(4);
    /// A placed prop the game files under its Decorative crafting group.
    pub const DECORATIVE: Self = Self(5);
    /// A placed prop the game files under its Comfort Furniture crafting
    /// group.
    pub const COMFORT_FURNITURE: Self = Self(6);
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
    /// How many slots the container an item places holds; the entry is the
    /// item.
    pub const CONTAINER_SLOTS: Self = Self(6);
    /// A weapon's damage factor; the entry is the item.
    pub const DAMAGE_MOD: Self = Self(7);
    /// A weapon's attack speed factor; the entry is the item.
    pub const SPEED_MOD: Self = Self(8);
    /// Every armour value of an item, scaled only; the entry is the item.
    pub const ARMOR: Self = Self(9);
    /// How long an item burns as fuel, in seconds; the entry is the item.
    pub const BURN_TIME: Self = Self(10);
    /// The energy an item gives as fuel; the entry is the item.
    pub const ENERGY_OUTPUT: Self = Self(11);
    /// The comfort a placed item gives; the entry is the item.
    pub const COMFORT_AMOUNT: Self = Self(12);
    /// The stack size random loot rolls for an item; the entry is the item.
    pub const LOOT_STACK_SIZE: Self = Self(13);
    /// The count fill tables give of an item; the entry is the item.
    pub const LOOT_COUNT: Self = Self(14);
    /// The chance fill tables give an item; the entry is the item.
    pub const LOOT_SPAWN_PROBABILITY: Self = Self(15);
    /// The weight of an item in fill tables; the entry is the item.
    pub const LOOT_WEIGHT: Self = Self(16);
    /// The workshop a recipe is made at, by its game id.
    pub const RECIPE_STATION: Self = Self(17);
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
    /// The mod's value is taken out, as if it never set one; `value` is
    /// ignored. Only [`HostApi::set_change`](super::HostApi::set_change)
    /// takes it.
    pub const RESET: Self = Self(2);
}

/// A change a mod declared with
/// [`HostApi::declare_change`](super::HostApi::declare_change), whose value it
/// sets later; numbered by the loader, unique in the process.
///
/// # Examples
///
/// ```
/// use enshwick::abi::ChangeId;
///
/// assert_ne!(ChangeId(1), ChangeId(2));
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, bytemuck::Zeroable)]
pub struct ChangeId(pub u32);

/// One change of a vanilla value, as
/// [`HostApi::change_data`](super::HostApi::change_data) takes it. The
/// loader copies it; `source` is borrowed for the call.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{ChangeOp, DataChange, DataField, DataSelect, Header, Str};
///
/// let change = DataChange {
///     header: Header::of::<DataChange>(),
///     entry: [0x3a; 16],
///     field: DataField::MAX_STACK,
///     op: ChangeOp::SET,
///     value: 999.0,
///     source: Str::new("items.toml:3"),
///     item: [0; 16],
///     select: DataSelect::ENTRY,
///     categories: 0,
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
    /// ingredient; all zero for none.
    pub item: [u8; 16],
    /// Which vanilla entries the change reaches: `entry` alone, or every
    /// item or recipe a selection picks, with `entry` all zero but for
    /// [`DataSelect::WORKSHOP`].
    pub select: DataSelect,
    /// The item categories [`DataSelect::CATEGORY`] picks, a bit per the
    /// game's `ItemCategory`, or the [`RecipeKind`] code
    /// [`DataSelect::RECIPE_KIND`] picks; 0 for any other selection.
    pub categories: u32,
}

impl_headed!(DataChange);

/// Which vanilla items a [`DataChange`] reaches.
///
/// # Examples
///
/// ```
/// use enshwick::abi::DataSelect;
///
/// assert_ne!(DataSelect::ENTRY, DataSelect::STACKABLE);
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, bytemuck::Zeroable)]
pub struct DataSelect(pub u32);

impl DataSelect {
    /// The entry the change names.
    pub const ENTRY: Self = Self(0);
    /// Every item of the categories in [`DataChange::categories`].
    pub const CATEGORY: Self = Self(1);
    /// Every item one slot holds more than one of.
    pub const STACKABLE: Self = Self(2);
    /// Every item that places a container.
    pub const CONTAINERS: Self = Self(3);
    /// Every recipe whose output is of the kind whose
    /// [`RecipeKind`] code is [`DataChange::categories`].
    pub const RECIPE_KIND: Self = Self(4);
    /// Every recipe made at the workshop whose GUID is
    /// [`DataChange::entry`].
    pub const WORKSHOP: Self = Self(5);
    /// Every backpack.
    pub const BACKPACKS: Self = Self(6);
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
