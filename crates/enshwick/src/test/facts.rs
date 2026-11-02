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

/// The hit game event, set by
/// [`FakeHost::game_events`](super::FakeHost::game_events); its size is a
/// hit history's entry.
pub const HIT_EVENT: Fact = Fact(ids::HIT_EVENT);
/// Where a hit keeps the `u32` entity behind its source.
pub const HIT_EVENT_ROOT_SOURCE_ID: Fact = Fact(ids::HIT_EVENT_ROOT_SOURCE_ID);
/// Where a hit keeps its `u32` target.
pub const HIT_EVENT_TARGET_ID: Fact = Fact(ids::HIT_EVENT_TARGET_ID);
/// Where a hit keeps its `u32` source.
pub const HIT_EVENT_SOURCE_ID: Fact = Fact(ids::HIT_EVENT_SOURCE_ID);
/// Where a hit keeps the `u32` entity behind its target.
pub const HIT_EVENT_ROOT_TARGET_ID: Fact = Fact(ids::HIT_EVENT_ROOT_TARGET_ID);
/// Where a hit keeps its `u32` weapon item entity.
pub const HIT_EVENT_WEAPON_PIDE_ID: Fact = Fact(ids::HIT_EVENT_WEAPON_PIDE_ID);
/// Where a hit keeps its `i32` health change.
pub const HIT_EVENT_HEALTH_CHANGE: Fact = Fact(ids::HIT_HEALTH_CHANGE);
/// Where a hit keeps its `i32` stamina change.
pub const HIT_EVENT_STAMINA_CHANGE: Fact = Fact(ids::HIT_STAMINA_CHANGE);
/// Where a hit keeps its damage types, a `u16` of engine bits.
pub const HIT_EVENT_DAMAGE_TYPES: Fact = Fact(ids::HIT_DAMAGE_TYPES);
/// Where a hit keeps its attack category, a byte of engine bits.
pub const HIT_EVENT_ATTACK_CATEGORY: Fact = Fact(ids::HIT_EVENT_ATTACK_CATEGORY);
/// Where a hit keeps its target's faction, a byte.
pub const HIT_EVENT_TARGET_FACTION: Fact = Fact(ids::HIT_TARGET_FACTION);
/// The death game event.
pub const ENTITY_DIED_EVENT: Fact = Fact(ids::ENTITY_DIED_EVENT);
/// Where a death keeps its `u32` dead entity.
pub const ENTITY_DIED_EVENT_DEAD_ENTITY_ID: Fact = Fact(ids::ENTITY_DIED_EVENT_DEAD_ENTITY_ID);
/// The component of the hits an entity took.
pub const HIT_HISTORY: Fact = Fact(ids::HIT_HISTORY);
/// Where a hit history keeps the `u32` index of its last hit.
pub const HIT_HISTORY_LAST_INDEX: Fact = Fact(ids::HIT_HISTORY_LAST_INDEX);
/// Where a hit history's hits start, one [`HIT_EVENT`] each.
pub const HIT_HISTORY_ENTRIES: Fact = Fact(ids::HIT_HISTORY_ENTRIES);
/// The component naming the last player that hit an entity.
pub const EXPERIENCE_CREDIT: Fact = Fact(ids::EXPERIENCE_CREDIT);
/// Where an experience credit keeps that player's `u32` entity.
pub const EXPERIENCE_CREDIT_LAST_PLAYER: Fact = Fact(ids::EXPERIENCE_CREDIT_LAST_PLAYER);
/// The equipment change game event.
pub const HANDLE_EQUIPMENT_SLOT_CHANGED_EVENT: Fact =
    Fact(ids::HANDLE_EQUIPMENT_SLOT_CHANGED_EVENT);
/// Where an equipment change keeps its slot, a byte.
pub const HANDLE_EQUIPMENT_SLOT_CHANGED_EVENT_EQUIPMENT_SLOT: Fact =
    Fact(ids::HANDLE_EQUIPMENT_SLOT_CHANGED_EVENT_EQUIPMENT_SLOT);
/// The engine bit of magical fire damage.
pub const DAMAGE_TYPE_MAGICAL_FIRE: Fact = Fact(ids::DAMAGE_TYPE_MAGICAL_FIRE);
/// The engine bit of physical cut damage.
pub const DAMAGE_TYPE_PHYSICAL_CUT: Fact = Fact(ids::DAMAGE_TYPE_PHYSICAL_CUT);
/// The engine bit of the melee attack category.
pub const ATTACK_CATEGORY_MELEE: Fact = Fact(ids::ATTACK_CATEGORY_MELEE);
/// The engine number of the shield equipment slot.
pub const EQUIPMENT_SLOT_SHIELD: Fact = Fact(ids::EQUIPMENT_SLOT_SHIELD);

/// The component of a player's map waypoint, set by
/// [`FakeHost::map_waypoints`](super::FakeHost::map_waypoints).
pub const PLAYER_WAYPOINT: Fact = Fact(ids::PLAYER_WAYPOINT);
/// Where a waypoint keeps the byte that shows it.
pub const PLAYER_WAYPOINT_ENABLED: Fact = Fact(ids::PLAYER_WAYPOINT_ENABLED);
/// Where a waypoint keeps its place, three 32.32 fixed-point `i64`.
pub const PLAYER_WAYPOINT_POSITION: Fact = Fact(ids::PLAYER_WAYPOINT_POSITION);

/// The NPC command game event, set by
/// [`FakeHost::effect_events`](super::FakeHost::effect_events).
pub const NPC_COMMAND_EVENT: Fact = Fact(ids::NPC_COMMAND_EVENT);
/// Where an NPC command keeps its command type, a byte.
pub const NPC_COMMAND_EVENT_TYPE: Fact = Fact(ids::NPC_COMMAND_EVENT_TYPE);
/// Where an NPC command keeps its `u32` NPC.
pub const NPC_COMMAND_EVENT_NPC_ID: Fact = Fact(ids::NPC_COMMAND_EVENT_NPC_ID);
/// Where an NPC command keeps its target, a transform.
pub const NPC_COMMAND_EVENT_TARGET_TRANSFORM: Fact = Fact(ids::NPC_COMMAND_EVENT_TARGET_TRANSFORM);
/// The threat game event.
pub const ADD_THREAT_TARGET_EVENT: Fact = Fact(ids::ADD_THREAT_TARGET_EVENT);
/// Where a threat keeps its `u32` attacker.
pub const ADD_THREAT_TARGET_EVENT_ATTACKER_ID: Fact =
    Fact(ids::ADD_THREAT_TARGET_EVENT_ATTACKER_ID);
/// Where a threat keeps its `u32` target.
pub const ADD_THREAT_TARGET_EVENT_TARGET_ID: Fact = Fact(ids::ADD_THREAT_TARGET_EVENT_TARGET_ID);
/// Where a threat keeps its `f32` value.
pub const ADD_THREAT_TARGET_EVENT_THREAT_VALUE: Fact =
    Fact(ids::ADD_THREAT_TARGET_EVENT_THREAT_VALUE);
/// The ping game event.
pub const UI_PING_EVENT: Fact = Fact(ids::UI_PING_EVENT);
/// Where a ping keeps its `u32` sender.
pub const UI_PING_EVENT_SENDER_ENTITY_ID: Fact = Fact(ids::UI_PING_EVENT_SENDER_ENTITY_ID);
/// Where a ping keeps its place, three 32.32 fixed-point `i64`.
pub const UI_PING_EVENT_POSITION: Fact = Fact(ids::UI_PING_POSITION);

/// The marker of a player character, set by
/// [`FakeHost::container_kinds`](super::FakeHost::container_kinds).
pub const PLAYER_INPUT: Fact = Fact(ids::PLAYER_INPUT);
/// The marker of an NPC.
pub const NPC: Fact = Fact(ids::NPC);
/// The marker of an animal.
pub const ANIMAL: Fact = Fact(ids::ANIMAL);
/// The marker of a drop the game removes once it is looted.
pub const DESTROY_ON_LOOT: Fact = Fact(ids::DESTROY_ON_LOOT);
/// The marker of a container the game fills with loot.
pub const LOOT_FILL: Fact = Fact(ids::LOOT_FILL);
/// The marker of a plant or other resource node.
pub const RESOURCE_NODE: Fact = Fact(ids::RESOURCE_NODE);

/// The slot the single loot roll fills, which no preset sets: the loot tests
/// set its size and fields by hand, as the roll links only where a build
/// names them.
pub const LOOT_SLOT: Fact = Fact(ids::LOOT_SLOT);
/// Where a loot slot keeps its `u32` item.
pub const LOOT_SLOT_ITEM: Fact = Fact(ids::LOOT_SLOT_ITEM);
/// Where a loot slot keeps its `u32` count.
pub const LOOT_SLOT_COUNT: Fact = Fact(ids::LOOT_SLOT_COUNT);
