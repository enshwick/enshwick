//! The world: game events, entities, teleports, spawns, daytime, damage, knowledge, weather and terrain.

use core::ffi::c_void;

use crate::abi::Header;

/// A game event handed to a [`GameEventFn`], from the ECS event streams
/// of the server's systems.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{GameEventData, Header};
///
/// let payload = [0u8; 16];
/// let died = GameEventData {
///     header: Header::of::<GameEventData>(),
///     type_hash: 0xd8ec_982a,
///     size: 16,
///     data: payload.as_ptr(),
/// };
/// assert_eq!(died.size, 16);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct GameEventData {
    /// Size and version of this struct.
    pub header: Header,
    /// The event's reflected type hash.
    pub type_hash: u32,
    /// How many bytes `data` holds: the event type's reflected size.
    pub size: u32,
    /// The event as the engine wrote it, valid for the call.
    pub data: *const u8,
}

impl_headed!(GameEventData);

/// A mod's handler for game events: the event and the pointer given at
/// subscription. It must not unwind.
pub type GameEventFn = unsafe extern "system" fn(event: *const GameEventData, user: *mut c_void);

/// An entity placed in the world, from [`HostApi::entities`](super::HostApi::entities).
///
/// Like [`PlayerInfo`](super::PlayerInfo), its layout is frozen: [`HostApi::entities`](super::HostApi::entities) writes
/// an array of it with no entry size.
///
/// # Examples
///
/// ```
/// use enshwick::abi::EntityPlace;
///
/// let chest = EntityPlace { entity: 4096, reserved: 0, position: [1 << 32, 0, 0] };
/// assert_eq!(chest.position[0] >> 32, 1);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct EntityPlace {
    /// The entity.
    pub entity: u32,
    /// Zero.
    pub reserved: u32,
    /// Where it is, in 32.32 fixed-point metres.
    pub position: [i64; 3],
}

/// A request to move an entity, for [`HostApi::teleport`](super::HostApi::teleport).
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, TeleportRequest};
///
/// let to_spawn = TeleportRequest {
///     header: Header::of::<TeleportRequest>(),
///     entity: 2,
///     flags: TeleportRequest::BEST_SPAWN,
///     position: [100 << 32, 40 << 32, -(20 << 32)],
///     orientation: [0.0, 0.0, 0.0, 1.0],
/// };
/// assert_eq!(to_spawn.flags, TeleportRequest::BEST_SPAWN);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Zeroable)]
pub struct TeleportRequest {
    /// Size and version of this struct.
    pub header: Header,
    /// The entity to move.
    pub entity: u32,
    /// [`TeleportRequest::BEST_SPAWN`], [`TeleportRequest::ORIENTED`] or
    /// none.
    pub flags: u32,
    /// Where to, in 32.32 fixed-point metres.
    pub position: [i64; 3],
    /// The orientation to arrive in, a quaternion `x, y, z, w`, with
    /// [`TeleportRequest::ORIENTED`]; otherwise the entity keeps its own.
    pub orientation: [f32; 4],
}

impl_headed!(TeleportRequest);

impl TeleportRequest {
    /// The game looks for a free spot near `position` to stand on, as it
    /// does for respawns.
    pub const BEST_SPAWN: u32 = 1;
    /// The entity arrives facing `orientation`.
    pub const ORIENTED: u32 = 2;
}

/// A request to spawn an entity from a template, for [`HostApi::spawn`](super::HostApi::spawn).
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, SpawnRequest};
///
/// let wolf = SpawnRequest {
///     header: Header::of::<SpawnRequest>(),
///     template: [0x3a; 16],
///     position: [100 << 32, 40 << 32, 0],
///     orientation: [0.0, 0.0, 0.0, 1.0],
///     level: 12,
///     reserved: 0,
///     lifetime_ns: 0,
/// };
/// assert_eq!(wolf.level, 12);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Zeroable)]
pub struct SpawnRequest {
    /// Size and version of this struct.
    pub header: Header,
    /// The template GUID, in the order [`HostApi::entity_template`](super::HostApi::entity_template) writes.
    pub template: [u8; 16],
    /// Where, in 32.32 fixed-point metres.
    pub position: [i64; 3],
    /// The orientation, a quaternion `x, y, z, w`.
    pub orientation: [f32; 4],
    /// The level the entity spawns at, as the game scales enemies; 0 for
    /// the template's own.
    pub level: u32,
    /// Zero.
    pub reserved: u32,
    /// Game time in nanoseconds after which the game removes the entity; 0
    /// to keep it.
    pub lifetime_ns: u64,
}

impl_headed!(SpawnRequest);

/// A player's craft request, handed to the filters registered with
/// [`HostApi::craft_filter`](super::HostApi::craft_filter) before the server crafts.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{CraftCheck, Header};
///
/// let craft = CraftCheck { header: Header::of::<CraftCheck>(), player: 2, station: 0, recipe: 0x51, amount: 3 };
/// assert_eq!(craft.station, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable)]
pub struct CraftCheck {
    /// Size and version of this struct.
    pub header: Header,
    /// The crafting player's character.
    pub player: u32,
    /// The station, or 0 for crafting by hand.
    pub station: u32,
    /// The recipe id.
    pub recipe: u32,
    /// How many.
    pub amount: u32,
}

impl_headed!(CraftCheck);

/// A mod's craft filter: the request and the pointer given at registration;
/// returns 1 to let it through, 0 to refuse it.
pub type CraftCheckFn = unsafe extern "system" fn(call: *const CraftCheck, user: *mut c_void) -> u8;

/// A placement the game's cursor check allows, handed to the filters
/// registered with [`HostApi::build_filter`](super::HostApi::build_filter).
///
/// # Examples
///
/// ```
/// use enshwick::abi::{BuildCheck, Header};
///
/// let wall = BuildCheck { header: Header::of::<BuildCheck>(), player: 2, item: 0x1d, position: [0; 3] };
/// assert_eq!(wall.item, 0x1d);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable)]
pub struct BuildCheck {
    /// Size and version of this struct.
    pub header: Header,
    /// The building player's character.
    pub player: u32,
    /// The item placed: a building block, a prop or another placeable.
    pub item: u32,
    /// Where, in 32.32 fixed-point metres.
    pub position: [i64; 3],
}

impl_headed!(BuildCheck);

/// A mod's build filter: the placement and the pointer given at
/// registration; returns 1 to let it through, 0 to refuse it.
pub type BuildCheckFn = unsafe extern "system" fn(call: *const BuildCheck, user: *mut c_void) -> u8;

/// The clock of the server's world, written by [`HostApi::time_of_day`](super::HostApi::time_of_day).
///
/// Times are game nanoseconds of the day, 0 up to 24 hours; lengths are
/// real nanoseconds.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{DaytimeInfo, Header};
///
/// let noon = DaytimeInfo {
///     header: Header::of::<DaytimeInfo>(),
///     time: 12 * 3_600_000_000_000,
///     day_start: 6 * 3_600_000_000_000,
///     day_end: 18 * 3_600_000_000_000,
///     day_length: 1_800_000_000_000,
///     night_length: 720_000_000_000,
///     speed: 1.0,
///     flags: DaytimeInfo::DAY,
/// };
/// assert_eq!(noon.flags & DaytimeInfo::DAY, DaytimeInfo::DAY);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Zeroable)]
pub struct DaytimeInfo {
    /// Size and version of this struct.
    pub header: Header,
    /// The time of day.
    pub time: i64,
    /// The time of day the day starts at.
    pub day_start: i64,
    /// The time of day the night starts at.
    pub day_end: i64,
    /// How long the day lasts in real time at speed 1.
    pub day_length: i64,
    /// How long the night lasts in real time at speed 1.
    pub night_length: i64,
    /// How fast the clock runs: 1 normally, faster while players sleep, and
    /// times every mod's factor ([`HostApi::set_time_speed`](super::HostApi::set_time_speed)).
    pub speed: f32,
    /// [`DaytimeInfo::DAY`] or none.
    pub flags: u32,
}

impl_headed!(DaytimeInfo);

impl DaytimeInfo {
    /// It is day.
    pub const DAY: u32 = 1;
}

/// Damage about to reach an entity's health, handed to the filters
/// registered with [`HostApi::damage_filter`](super::HostApi::damage_filter), which may change it.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{DamageCheck, Header};
///
/// let mut fall = DamageCheck {
///     header: Header::of::<DamageCheck>(),
///     cause: DamageCheck::FALL,
///     target: 2,
///     source: 0,
///     attacker: 0,
///     amounts: [0.0; 13],
///     total: 40.0,
/// };
/// fall.total *= 0.5;
/// assert_eq!(fall.total, 20.0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Zeroable)]
pub struct DamageCheck {
    /// Size and version of this struct.
    pub header: Header,
    /// [`DamageCheck::HIT`], [`DamageCheck::FALL`], [`DamageCheck::SHROUD`],
    /// [`DamageCheck::SHROUD_DEATH`], [`DamageCheck::DROWNING`],
    /// [`DamageCheck::DROWNING_DEATH`], [`DamageCheck::FREEZING`] or
    /// [`DamageCheck::ADMIN_KILL`].
    pub cause: u32,
    /// The entity that takes the damage.
    pub target: u32,
    /// What dealt a hit: the attacker, or its projectile; 0 for other
    /// causes.
    pub source: u32,
    /// Who dealt a hit, the root of the impact that hit (the archer for an
    /// arrow), or `source` when the game names none; 0 for other causes.
    /// Since minor 45.
    pub attacker: u32,
    /// A hit's damage by type (`DamageTypeNew`: cut 0, pierce 1, blunt 2,
    /// fire 3, ice 4, shroud 5, lightning 6, healing 7, poison 8, wood 9,
    /// stone 10, metal 11, explosion 12), after the target's resistances
    /// and before its armour; healing is what heals. Zero for other causes.
    pub amounts: [f32; 13],
    /// The health the damage takes, for causes other than a hit; for a hit,
    /// the sum of its damage types but healing, which the loader does not
    /// read back.
    pub total: f32,
}

impl_headed!(DamageCheck);

impl DamageCheck {
    /// A hit from a weapon, a spell, a creature or an explosion.
    pub const HIT: u32 = 0;
    /// A fall.
    pub const FALL: u32 = 1;
    /// The shroud draining a character whose time in it ran out.
    pub const SHROUD: u32 = 2;
    /// The deadly shroud, which kills at once.
    pub const SHROUD_DEATH: u32 = 3;
    /// A character out of breath under water.
    pub const DROWNING: u32 = 4;
    /// A creature out of breath under water, which dies at once.
    pub const DROWNING_DEATH: u32 = 5;
    /// A character frozen through, out of body heat.
    pub const FREEZING: u32 = 6;
    /// An administrator's kill command, of chosen enemies or of whole
    /// factions or players: `total` is 1, and the kill goes ahead unless
    /// the filters leave 0.
    pub const ADMIN_KILL: u32 = 7;
}

/// A mod's damage filter: the damage, which it may change, and the pointer
/// given at registration. It runs on the game's worker threads.
pub type DamageFilterFn = unsafe extern "system" fn(check: *mut DamageCheck, user: *mut c_void);

/// A knowledge value, written by [`HostApi::knowledge`](super::HostApi::knowledge).
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, KnowledgeInfo};
///
/// let recipe = KnowledgeInfo {
///     header: Header::of::<KnowledgeInfo>(),
///     id: 0x1d,
///     kind: KnowledgeInfo::FLAGS,
///     scope: KnowledgeInfo::PLAYER,
///     reserved: 0,
///     value: 1,
///     flags: 3,
/// };
/// assert_eq!(recipe.value & 1, 1);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable)]
pub struct KnowledgeInfo {
    /// Size and version of this struct.
    pub header: Header,
    /// The knowledge id.
    pub id: u32,
    /// [`KnowledgeInfo::BOOL`], [`KnowledgeInfo::COUNTER`] or
    /// [`KnowledgeInfo::FLAGS`].
    pub kind: u8,
    /// [`KnowledgeInfo::WORLD`] or [`KnowledgeInfo::PLAYER`].
    pub scope: u8,
    /// Zero.
    pub reserved: u16,
    /// The value: 0 or 1, a count, or flag bits.
    pub value: u32,
    /// The flag bits the game expects of a flags value; 0 for any.
    pub flags: u32,
}

impl_headed!(KnowledgeInfo);

impl KnowledgeInfo {
    /// Set or not.
    pub const BOOL: u8 = 0;
    /// A count.
    pub const COUNTER: u8 = 1;
    /// Bits.
    pub const FLAGS: u8 = 2;
    /// The world's, shared by every player.
    pub const WORLD: u8 = 0;
    /// Each player's own.
    pub const PLAYER: u8 = 1;
}

/// A front of weather in the server's world: the current phenomenon of one
/// of its weather layers, written by [`HostApi::weather_fronts`](super::HostApi::weather_fronts).
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, WeatherFront};
///
/// let storm = WeatherFront {
///     header: Header::of::<WeatherFront>(),
///     layer: 0,
///     effect: WeatherFront::RAIN,
///     position: [0; 3],
///     radius: 900.0,
///     reserved: 0,
///     remaining: 300_000_000_000,
/// };
/// assert_eq!(storm.effect, WeatherFront::RAIN);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Zeroable)]
pub struct WeatherFront {
    /// Size and version of this struct.
    pub header: Header,
    /// The weather layer it belongs to.
    pub layer: u32,
    /// [`WeatherFront::NONE`], [`WeatherFront::RAIN`] or
    /// [`WeatherFront::BLIZZARD`].
    pub effect: u32,
    /// Where it is, in 32.32 fixed-point metres.
    pub position: [i64; 3],
    /// How far it reaches, in metres.
    pub radius: f32,
    /// Zero.
    pub reserved: u32,
    /// How long it has left, in game nanoseconds.
    pub remaining: i64,
}

impl_headed!(WeatherFront);

impl WeatherFront {
    /// Clouds without rain, or nothing.
    pub const NONE: u32 = 0;
    /// Rain, snow where it is cold.
    pub const RAIN: u32 = 1;
    /// A blizzard.
    pub const BLIZZARD: u32 = 2;
}

/// A terrain material of the running game, written by
/// [`HostApi::terrain_materials`](super::HostApi::terrain_materials).
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, TerrainMaterialInfo};
///
/// let dirt = TerrainMaterialInfo {
///     header: Header::of::<TerrainMaterialInfo>(),
///     id: 1,
///     flags: TerrainMaterialInfo::FLATTENS,
///     hardness: 0,
///     reserved: 0,
///     item: 0x0d9b_7a1c,
///     health: 100,
/// };
/// assert_eq!(dirt.flags & TerrainMaterialInfo::FLATTENS, TerrainMaterialInfo::FLATTENS);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable)]
pub struct TerrainMaterialInfo {
    /// Size and version of this struct.
    pub header: Header,
    /// The material's voxel id, 1 to 127.
    pub id: u8,
    /// [`TerrainMaterialInfo::FLATTENS`] or none.
    pub flags: u8,
    /// How hard it is to dig, the game's hardness category from 0.
    pub hardness: u8,
    /// Zero.
    pub reserved: u8,
    /// The item digging it yields, as an item id.
    pub item: u32,
    /// The health a voxel of it has against tools.
    pub health: i32,
}

impl_headed!(TerrainMaterialInfo);

impl TerrainMaterialInfo {
    /// A player's tool can flatten it.
    pub const FLATTENS: u8 = 1;
}

/// A terrain edit, for [`HostApi::terrain_edit`](super::HostApi::terrain_edit).
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, TerrainEdit};
///
/// let hole = TerrainEdit {
///     header: Header::of::<TerrainEdit>(),
///     kind: TerrainEdit::DIG,
///     shape: TerrainEdit::ROUND,
///     flags: 0,
///     material: 0,
///     position: [100.0, 40.0, 100.0],
///     radius: 3.0,
///     height: 0.0,
///     protect: [0; 16],
/// };
/// assert_eq!(hole.kind, TerrainEdit::DIG);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Zeroable)]
pub struct TerrainEdit {
    /// Size and version of this struct.
    pub header: Header,
    /// [`TerrainEdit::ADD`], [`TerrainEdit::DIG`] or
    /// [`TerrainEdit::FLATTEN`].
    pub kind: u32,
    /// [`TerrainEdit::ROUND`] or [`TerrainEdit::SQUARE`].
    pub shape: u32,
    /// [`TerrainEdit::DRY_RUN`] or none.
    pub flags: u32,
    /// The terrain material added or filled with, 1 to 127; unused by a
    /// dig.
    pub material: u32,
    /// Where, in metres; every axis 0 or more.
    pub position: [f32; 3],
    /// How far it reaches, in metres, above 0 and up to 16: a mound's or a
    /// flattened disc's radius, a dug sphere's.
    pub radius: f32,
    /// A mound's height at its middle, in metres, above 0 and up to 16;
    /// unused otherwise.
    pub height: f32,
    /// Terrain materials a dig leaves, a bit for each id (bit `id % 8` of
    /// byte `id / 8`): the dig stops at the first shell that meets one.
    pub protect: [u8; 16],
}

impl_headed!(TerrainEdit);

impl TerrainEdit {
    /// Adds a mound of a material, as a player's shovel does.
    pub const ADD: u32 = 0;
    /// Digs out terrain, as a pickaxe does, without its cost.
    pub const DIG: u32 = 1;
    /// Flattens the ground to the level of the position, filling with a
    /// material.
    pub const FLATTEN: u32 = 2;
    /// A disc or a sphere.
    pub const ROUND: u32 = 0;
    /// A square or a cube.
    pub const SQUARE: u32 = 1;
    /// Counts what the edit would change and changes nothing.
    pub const DRY_RUN: u32 = 1;
}
