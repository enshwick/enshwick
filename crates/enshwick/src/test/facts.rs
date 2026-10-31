//! The engine facts the [`FakeHost`](super::FakeHost) presets set that a test
//! lays its blobs out by: read each back through
//! [`EngineFacts`](crate::content::EngineFacts) with [`Fact::id`].

use crate::engine_ids as ids;

/// One engine fact a preset sets, by enshwick's id: a type's hash or size,
/// or a field's offset, for a test to read back rather than copy.
///
/// # Examples
///
/// ```
/// use enshwick::content::EngineFacts;
/// use enshwick::test::facts;
///
/// let host = enshwick::test::FakeHost::new().world();
/// let transform = host.hash(facts::CURRENT_TRANSFORM.id())?;
/// let orientation = host.offset(facts::WORLD_TRANSFORM_ORIENTATION.id())?;
/// assert!(orientation >= host.offset(facts::WORLD_TRANSFORM_POSITION.id())? + 24);
/// let loader = host.placed(transform, enshwick::Entity(2), [0.0; 3]).install();
/// # let _ = loader;
/// # Ok::<(), enshwick::Off>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Fact(&'static str);

impl Fact {
    /// Its enshwick id, which [`EngineFacts`](crate::content::EngineFacts)
    /// answers by.
    ///
    /// # Examples
    ///
    /// See [`Fact`].
    #[must_use]
    pub const fn id(self) -> &'static str {
        self.0
    }
}

/// The component of an entity's place in the world, set by
/// [`FakeHost::world`](super::FakeHost::world).
pub const CURRENT_TRANSFORM: Fact = Fact(ids::CURRENT_TRANSFORM);
/// Where the transform lies in a [`CURRENT_TRANSFORM`].
pub const CURRENT_TRANSFORM_TRANSFORM: Fact = Fact(ids::CURRENT_TRANSFORM_TRANSFORM);
/// Where a transform keeps its position, three 32.32 fixed-point `i64`.
pub const WORLD_TRANSFORM_POSITION: Fact = Fact(ids::WORLD_TRANSFORM_POSITION);
/// Where a transform keeps its orientation, a quaternion of four `f32`.
pub const WORLD_TRANSFORM_ORIENTATION: Fact = Fact(ids::WORLD_TRANSFORM_ORIENTATION);
/// Where a transform keeps its scale, an `f32`.
pub const WORLD_TRANSFORM_SCALE: Fact = Fact(ids::WORLD_TRANSFORM_SCALE);
/// The component naming the item an entity was placed from, a `u32`, set by
/// [`FakeHost::world`](super::FakeHost::world).
pub const USED_ITEM: Fact = Fact(ids::USED_ITEM);
/// Where an item's static data keeps the template it places, set by
/// [`FakeHost::items`](super::FakeHost::items).
pub const ITEM_INFO_PLACED_ENTITY: Fact = Fact(ids::ITEM_INFO_PLACED_ENTITY);

/// The component of an entity's inventory setup, set by
/// [`FakeHost::inventory`](super::FakeHost::inventory).
pub const INVENTORY_SETUP: Fact = Fact(ids::INVENTORY_SETUP);
/// Where a setup keeps its linked entities, `u32` each.
pub const INVENTORY_SETUP_LINKS: Fact = Fact(ids::INVENTORY_SETUP_LINKS);
/// Where a setup keeps its links' categories, a byte each.
pub const INVENTORY_SETUP_CATEGORIES: Fact = Fact(ids::INVENTORY_SETUP_CATEGORIES);
/// Where a setup keeps the byte that marks it initialized.
pub const INVENTORY_SETUP_INITIALIZED: Fact = Fact(ids::INVENTORY_SETUP_INITIALIZED);
/// The component of a sub-inventory's stacks.
pub const INVENTORY: Fact = Fact(ids::INVENTORY);
/// A stack in a sub-inventory, its `u32` item first; its size is set.
pub const ITEM_STACK: Fact = Fact(ids::ITEM_STACK);
/// Where a stack keeps its `u32` count.
pub const ITEM_STACK_COUNT: Fact = Fact(ids::ITEM_STACK_COUNT);
/// Where a stack keeps its `u32` item entity, its PIDE.
pub const ITEM_STACK_PIDE: Fact = Fact(ids::ITEM_STACK_PIDE);
/// The component of an item's durability, its current `u16` first.
pub const DURABILITY: Fact = Fact(ids::DURABILITY);
/// Where a durability keeps its `u16` maximum.
pub const DURABILITY_MAX: Fact = Fact(ids::DURABILITY_MAX);
/// The component of an item's fuel, its current `u16` first.
pub const FUEL: Fact = Fact(ids::FUEL);
/// Where a fuel keeps its `u16` maximum.
pub const FUEL_MAX: Fact = Fact(ids::FUEL_MAX);
/// The component relating an entity to its owner, the owner's `u32` entity.
pub const OWNER: Fact = Fact(ids::OWNER);
