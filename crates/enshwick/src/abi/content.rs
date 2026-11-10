//! Game resources and recipe rules a mod patches or serves.

use core::ffi::c_void;

use crate::abi::{Header, Status, Str};

#[doc(inline)]
pub use enshwick_values::abi::ResourceKey;

/// One content rename for [`HostApi::rename_content`](super::HostApi::rename_content):
/// content saved under `from` reads as the content now under `to`.
///
/// Frozen: written as an array.
///
/// # Examples
///
/// ```
/// use enshwick::abi::ContentRename;
///
/// let rename = ContentRename { from: [1; 16], to: [2; 16] };
/// assert_ne!(rename.from, rename.to);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContentRename {
    /// The GUID the content had, in memory order.
    pub from: [u8; 16],
    /// The GUID the content has now, in memory order.
    pub to: [u8; 16],
}

/// Builds a served resource from its base: `base` is the base resource's bytes,
/// valid for the call.
///
/// On [`Status::OK`] the function writes a pointer and length of its
/// output: bytes in a buffer from
/// [`HostApi::resource_buffer`](super::HostApi::resource_buffer), which the
/// loader frees once no build reads them, or bytes of its own, which stay
/// valid and writable for the life of the process. A rebuild
/// ([`HostApi::rebuild_resource`](super::HostApi::rebuild_resource)) writes
/// over the bytes the game's first build kept.
///
/// The loader calls it on the thread that builds: the game's resource
/// thread for the game's first build, the calling thread for a rebuild or a
/// served read ([`HostApi::resource_served`](super::HostApi::resource_served)).
/// It runs again for each served read, possibly on two threads at once,
/// and nested inside another key's patch on the same thread, though never
/// inside a build of its own key there. A failure returned from
/// the game's build turns the mod off; a panic must not leave the
/// function. `user` is the pointer the mod served with.
///
/// # Examples
///
/// The safe layer wraps it in [`Load::serve`](crate::Load::serve).
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

/// An attribute of a reflected type or field, in a [`TypeEntry`] or
/// [`TypeField`]: the engine's name for it, the type of its value and the
/// value as the engine's text.
///
/// Frozen: read as an array. Its strings are the loader's, valid and
/// unchanged for the life of the process.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Str, TypeAttribute};
///
/// let tooltip = TypeAttribute { name: Str::new("tooltip"), value_type: Str::EMPTY, value: Str::new("Damage") };
/// assert_eq!(tooltip.name.len(), 7);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct TypeAttribute {
    /// The attribute's name.
    pub name: Str,
    /// The qualified name of its value's type; empty for a flag.
    pub value_type: Str,
    /// The value as the engine's text; empty for a flag.
    pub value: Str,
}

/// A named constant of a reflected enum, in a [`TypeEntry`].
///
/// Frozen: read as an array. Its name is the loader's, valid and unchanged
/// for the life of the process.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Str, TypeConstant};
///
/// let invalid = TypeConstant { name: Str::new("Invalid"), value: 0 };
/// assert_eq!(invalid.value, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct TypeConstant {
    /// The constant's name, without its type.
    pub name: Str,
    /// Its value; for the flag enum of a bitmask, the flag's bit index.
    pub value: u64,
}

/// A field of a reflected struct, in a [`TypeEntry`].
///
/// Frozen: read as an array. Its strings and attributes are the loader's,
/// valid and unchanged for the life of the process.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Str, TypeField};
///
/// let health = TypeField {
///     name: Str::new("health"),
///     type_name: Str::new("demo::float32"),
///     offset: 4,
///     attribute_count: 0,
///     attributes: core::ptr::null(),
/// };
/// assert_eq!(health.offset, 4);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct TypeField {
    /// The field's name.
    pub name: Str,
    /// The qualified name of its type; empty when the engine does not
    /// reflect it.
    pub type_name: Str,
    /// Its offset from the start of the struct.
    pub offset: u32,
    /// How many attributes `attributes` points at.
    pub attribute_count: u32,
    /// Its attributes; null when it has none.
    pub attributes: *const TypeAttribute,
}

/// A reflected type of the running game, as [`HostApi::types`](super::HostApi::types)
/// lists them: its names, layout, kind and the arrays of its fields,
/// constants and attributes.
///
/// Every string and array an entry points at is the loader's, valid and
/// unchanged for the life of the process; a null array has no elements.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, TypeEntry};
///
/// let entry = TypeEntry::EMPTY;
/// assert_eq!(entry.header, Header::of::<TypeEntry>());
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct TypeEntry {
    /// Size and version of this struct.
    pub header: Header,
    /// The short name, such as `TemplateResource`.
    pub name: Str,
    /// The name the engine's scripts use, such as `ecs.TemplateResource`.
    pub impact_name: Str,
    /// The full name, such as `demo::ecs::TemplateResource`.
    pub qualified_name: Str,
    /// The qualified name of a struct's base or of the type an array,
    /// optional, typedef or bitmask holds; empty for none.
    pub inner: Str,
    /// The hash the engine keeps beside the qualified name's.
    pub internal_hash: u32,
    /// The size in bytes.
    pub size: u32,
    /// The alignment in bytes.
    pub align: u16,
    /// The alignment as an array element.
    pub element_align: u16,
    /// The descriptor's kind byte, as [`Kind::from_byte`](crate::resources::Kind::from_byte)
    /// reads it.
    pub kind: u8,
    /// The descriptor's flags byte.
    pub flags: u8,
    /// Zero.
    pub reserved: [u8; 2],
    /// The engine's count: of fields, of constants, or a static array's
    /// length.
    pub count: u32,
    /// How many fields `fields` points at.
    pub field_count: u32,
    /// How many constants `constants` points at.
    pub constant_count: u32,
    /// How many attributes `attributes` points at.
    pub attribute_count: u32,
    /// A struct's own fields; null for none.
    pub fields: *const TypeField,
    /// An enum's constants; null for none.
    pub constants: *const TypeConstant,
    /// The type's attributes; null for none.
    pub attributes: *const TypeAttribute,
}

impl_headed!(TypeEntry);

impl TypeEntry {
    /// An entry with nothing known, its header set.
    pub const EMPTY: Self = Self {
        header: Header::of::<Self>(),
        name: Str::EMPTY,
        impact_name: Str::EMPTY,
        qualified_name: Str::EMPTY,
        inner: Str::EMPTY,
        internal_hash: 0,
        size: 0,
        align: 0,
        element_align: 0,
        kind: 0,
        flags: 0,
        reserved: [0; 2],
        count: 0,
        field_count: 0,
        constant_count: 0,
        attribute_count: 0,
        fields: core::ptr::null(),
        constants: core::ptr::null(),
        attributes: core::ptr::null(),
    };
}
