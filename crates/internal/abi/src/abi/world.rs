//! The world: game events, entities, teleports, spawns, daytime, damage, knowledge, weather, build rights and joins.

use core::ffi::c_void;

use crate::abi::Metres;
use crate::abi::{Entity, Header, ItemId, KnowledgeId, PlayerKey, RecipeId, Str};
use crate::content::Guid;

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
///     type_hash: 0x0042,
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

/// One game event type of the running build, as [`HostApi::event_types`]
/// writes it. `name` is the loader's, valid for the rest of the process.
///
/// [`HostApi::event_types`]: crate::abi::HostApi::event_types
///
/// # Examples
///
/// ```
/// use enshwick::abi::{EventTypeInfo, Header, Str};
///
/// let info = EventTypeInfo {
///     header: Header::of::<EventTypeInfo>(),
///     name: Str::new("you::Event"),
///     hash: 1,
///     size: 16,
/// };
/// assert_eq!(info.size, 16);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct EventTypeInfo {
    /// Size and version of this struct.
    pub header: Header,
    /// The reflected name.
    pub name: Str,
    /// The reflected type hash, which each event of the type carries.
    pub hash: u32,
    /// The bytes each event of the type carries.
    pub size: u32,
}

impl_headed!(EventTypeInfo);

/// A mod's handler for game events: the event and the pointer given at
/// subscription. It must not unwind.
///
/// Null is never a callback: the loader refuses a null one with
/// [`Status::BAD_ARGS`](crate::abi::Status::BAD_ARGS) and keeps nothing.
/// Runs on the thread that steps the event's world, at the start of a step.
///
/// # Examples
///
/// The safe layer wraps it in [`Load::on`](../struct.Load.html#method.on), for a game event's type.
pub type GameEventFn = unsafe extern "system" fn(event: *const GameEventData, user: *mut c_void);

/// An entity placed in the world, from [`HostApi::entities`](super::HostApi::entities).
///
/// Frozen, written as an array by
/// [`HostApi::entities`](super::HostApi::entities): an entity and its place
/// are a plain value, as the list rule on [`HostApi`](super::HostApi) says.
///
/// # Examples
///
/// ```
/// use enshwick::abi::EntityPlace;
///
/// let chest = EntityPlace { entity: enshwick::Entity(4096), reserved: 0, position: [1.0, 0.0, 0.0] };
/// assert_eq!(chest.position[0], 1.0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EntityPlace {
    /// The entity.
    pub entity: Entity,
    /// Zero.
    pub reserved: u32,
    /// Where it is, in metres.
    pub position: Metres,
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
///     entity: enshwick::Entity(2),
///     flags: TeleportRequest::BEST_SPAWN,
///     position: [100.0, 40.0, -20.0],
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
    pub entity: Entity,
    /// [`TeleportRequest::BEST_SPAWN`], [`TeleportRequest::ORIENTED`] or
    /// none.
    pub flags: u32,
    /// Where to, in metres.
    pub position: Metres,
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

/// A player's respawn point, which
/// [`HostApi::respawn_point`](super::HostApi::respawn_point) fills: the spawn
/// point entity the character respawns at and where it stands.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, RespawnPointInfo};
///
/// let altar = RespawnPointInfo {
///     header: Header::of::<RespawnPointInfo>(),
///     entity: enshwick::Entity(40),
///     kind: RespawnPointInfo::BASE,
///     position: [100.0, 40.0, -20.0],
///     orientation: [0.0, 0.0, 0.0, 1.0],
/// };
/// assert_eq!(altar.kind, RespawnPointInfo::BASE);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Zeroable)]
pub struct RespawnPointInfo {
    /// Size and version of this struct.
    pub header: Header,
    /// The spawn point entity.
    pub entity: Entity,
    /// The spawn point's kind: [`RespawnPointInfo::START_LOCATION`],
    /// [`RespawnPointInfo::SAVE_POINT`], [`RespawnPointInfo::BASE`],
    /// [`RespawnPointInfo::NO_RESPAWN`] or [`RespawnPointInfo::OTHER`].
    pub kind: u32,
    /// Where the character respawns, in metres.
    pub position: Metres,
    /// The orientation it respawns in, a quaternion `x, y, z, w`.
    pub orientation: [f32; 4],
}

impl_headed!(RespawnPointInfo);

impl RespawnPointInfo {
    /// The world's start location.
    pub const START_LOCATION: u32 = 0;
    /// A save point out in the world.
    pub const SAVE_POINT: u32 = 1;
    /// A Flame Altar's base.
    pub const BASE: u32 = 2;
    /// A spawn point nobody respawns at.
    pub const NO_RESPAWN: u32 = 3;
    /// A kind the loader does not know on this build.
    pub const OTHER: u32 = u32::MAX;
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
///     template: enshwick::abi::Guid([0x3a; 16]),
///     position: [100.0, 40.0, 0.0],
///     orientation: [0.0, 0.0, 0.0, 1.0],
///     level: 12,
///     lifetime_ns: 0,
///     flags: 0,
/// };
/// assert_eq!(wolf.level, 12);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Zeroable)]
pub struct SpawnRequest {
    /// Size and version of this struct.
    pub header: Header,
    /// The template GUID.
    pub template: Guid,
    /// Where, in metres.
    pub position: Metres,
    /// The orientation, a quaternion `x, y, z, w`.
    pub orientation: [f32; 4],
    /// The level the entity spawns at, as the game scales enemies; 0 for
    /// the template's own.
    pub level: u32,
    /// [`SpawnRequest::DO_NOT_SAVE`], or 0.
    pub flags: u32,
    /// Game time in nanoseconds after which the game removes the entity; 0
    /// to keep it.
    pub lifetime_ns: u64,
}

impl_headed!(SpawnRequest);

impl SpawnRequest {
    /// The entity stays out of the world's save, as the game's own passing
    /// animals do: it is gone once the world loads again.
    pub const DO_NOT_SAVE: u32 = 1;
}

/// A player's craft request, handed to the filters registered with
/// [`HostApi::craft_filter`](super::HostApi::craft_filter) before the server crafts.
///
/// The name is enshwick's; the fields are the game's `CraftRecipeEvent`.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{CraftCheck, Header};
///
/// let craft = CraftCheck { header: Header::of::<CraftCheck>(), crafting_operator_id: enshwick::Entity(2), crafting_station_id: enshwick::Entity(0), recipe_id: enshwick::RecipeId::new(0x51), recipe_amount: 3, reserved: 0 };
/// assert_eq!(craft.crafting_station_id, enshwick::Entity(0));
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable)]
pub struct CraftCheck {
    /// Size and version of this struct.
    pub header: Header,
    /// The crafting player's character.
    pub crafting_operator_id: Entity,
    /// The station, or 0 for crafting by hand.
    pub crafting_station_id: Entity,
    /// The recipe id.
    pub recipe_id: RecipeId,
    /// How many.
    pub recipe_amount: u16,
    /// Zero.
    pub reserved: u16,
}

impl_headed!(CraftCheck);

/// A mod's craft filter: the request and the pointer given at registration;
/// returns [`FILTER_REFUSE`](super::FILTER_REFUSE) to refuse it, else [`FILTER_PASS`](super::FILTER_PASS).
///
/// Null is never a callback: the loader refuses a null one with
/// [`Status::BAD_ARGS`](crate::abi::Status::BAD_ARGS) and keeps nothing.
/// Runs on the server's game thread, at the start of a step.
///
/// # Examples
///
/// The safe layer wraps it in [`Load::on_craft`](../struct.Load.html#method.on_craft).
pub type CraftFilterFn =
    unsafe extern "system" fn(call: *const CraftCheck, user: *mut c_void) -> u8;

/// A placement the game's cursor check allows, handed to the filters
/// registered with [`HostApi::build_filter`](super::HostApi::build_filter).
///
/// # Examples
///
/// ```
/// use enshwick::abi::{BuildCheck, Header};
///
/// let wall = BuildCheck {
///     header: Header::of::<BuildCheck>(),
///     character: enshwick::Entity(2),
///     item: enshwick::ItemId::new(0x1d),
///     position: [0.0; 3],
///     reason: core::ptr::null_mut(),
///     account: enshwick::PlayerKey::new(7),
/// };
/// assert_eq!(wall.item, enshwick::ItemId::new(0x1d));
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Zeroable)]
pub struct BuildCheck {
    /// Size and version of this struct.
    pub header: Header,
    /// The building player's character.
    pub character: Entity,
    /// The item placed: a building block, a prop or another placeable.
    pub item: ItemId,
    /// Where, in metres.
    pub position: Metres,
    /// Null, or where a filter that refuses writes why: a key of its mod's
    /// texts or plain words, which the builder reads. The text it points at
    /// stays valid until the filter's next call on the same thread; the
    /// loader copies it before then, on that thread.
    pub reason: *mut Str,
    /// The building player's account, which a new character of the same
    /// player keeps; 0 where the loader cannot tell, such as in the own
    /// prediction of a client that joined a server. A hosting client's own
    /// prediction gets its player's account. A mod reads it only where
    /// `header` covers it.
    pub account: PlayerKey,
}

impl_headed!(BuildCheck);

/// A mod's build filter: the placement and the pointer given at
/// registration; returns [`FILTER_REFUSE`](super::FILTER_REFUSE) to refuse it, else [`FILTER_PASS`](super::FILTER_PASS).
///
/// Null is never a callback: the loader refuses a null one with
/// [`Status::BAD_ARGS`](crate::abi::Status::BAD_ARGS) and keeps nothing.
/// Runs on the worker threads of the game's cursor check, on the server and on
/// a client for its own prediction, so several calls may run at once.
///
/// # Examples
///
/// The safe layer wraps it in [`Load::on_build`](../struct.Load.html#method.on_build).
pub type BuildFilterFn =
    unsafe extern "system" fn(call: *const BuildCheck, user: *mut c_void) -> u8;

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
///     time_ns: 12 * 3_600_000_000_000,
///     day_start_ns: 6 * 3_600_000_000_000,
///     day_end_ns: 18 * 3_600_000_000_000,
///     day_length_ns: 1_800_000_000_000,
///     night_length_ns: 720_000_000_000,
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
    pub time_ns: i64,
    /// The time of day the day starts at.
    pub day_start_ns: i64,
    /// The time of day the night starts at.
    pub day_end_ns: i64,
    /// How long the day lasts in real time at speed 1.
    pub day_length_ns: i64,
    /// How long the night lasts in real time at speed 1.
    pub night_length_ns: i64,
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

/// The damage types this ABI has names for, the values of the game's
/// `DamageTypeNew` today. A hit may carry more on a later game build:
/// [`DamageCheck::amount_count`] says how many.
pub const DAMAGE_TYPES: usize = 13;

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
///     cause: DamageCheck::FALL_DAMAGE,
///     target: enshwick::Entity(2),
///     source: enshwick::Entity(0),
///     attacker: enshwick::Entity(0),
///     total: 40.0,
///     amount_count: 0,
///     amounts: core::ptr::null_mut(),
/// };
/// fall.total *= 0.5;
/// assert_eq!(fall.total, 20.0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Zeroable)]
pub struct DamageCheck {
    /// Size and version of this struct.
    pub header: Header,
    /// [`DamageCheck::IMPACT`], [`DamageCheck::FALL_DAMAGE`], [`DamageCheck::FOG`],
    /// [`DamageCheck::DEADLY_FOG`], [`DamageCheck::SUFFOCATED`],
    /// [`DamageCheck::DROWNING_DEATH`], [`DamageCheck::FREEZING`] or
    /// [`DamageCheck::ADMIN_KILL`].
    pub cause: u32,
    /// The entity that takes the damage.
    pub target: Entity,
    /// What dealt a hit: the attacker, or its projectile; 0 for other
    /// causes.
    pub source: Entity,
    /// Who dealt a hit, the root of the impact that hit (the archer for an
    /// arrow), or `source` when the game names none; 0 for other causes.
    pub attacker: Entity,
    /// The health the damage takes, for causes other than a hit; for a hit,
    /// the sum of its damage types but healing, which the loader does not
    /// read back.
    pub total: f32,
    /// How many floats `amounts` points at: the values of the running
    /// game's `DamageTypeNew`, read from its reflection; 0 for causes other
    /// than a hit.
    pub amount_count: u32,
    /// A hit's damage by type, in the game's order of its damage types
    /// (its `DamageTypeNew` enum), after the target's resistances and before
    /// its armour; healing is what heals. The filter may change them; valid
    /// for the call. Null for other causes.
    pub amounts: *mut f32,
}

impl_headed!(DamageCheck);

impl DamageCheck {
    /// A hit from a weapon, a spell, a creature or an explosion.
    pub const IMPACT: u32 = 0;
    /// A fall.
    pub const FALL_DAMAGE: u32 = 1;
    /// The shroud draining a character whose time in it ran out.
    pub const FOG: u32 = 2;
    /// The deadly shroud, which kills at once.
    pub const DEADLY_FOG: u32 = 3;
    /// A character out of breath under water.
    pub const SUFFOCATED: u32 = 4;
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
///
/// Null is never a callback: the loader refuses a null one with
/// [`Status::BAD_ARGS`](crate::abi::Status::BAD_ARGS) and keeps nothing.
///
/// # Examples
///
/// The safe layer wraps it in `Load::on_damage`, behind the `unstable` feature.
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
///     id: enshwick::knowledge::KnowledgeId::new(0x1d),
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
    pub id: KnowledgeId,
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

/// One grant or revoke of a knowledge value, for
/// [`HostApi::change_knowledge`](super::HostApi::change_knowledge): of the
/// character `entity` in the game, of the account `account` whether it plays
/// now or later, or of the world when both are 0.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, KnowledgeChange, KnowledgeInfo, PlayerKey};
///
/// let unflag = KnowledgeChange {
///     header: Header::of::<KnowledgeChange>(),
///     account: PlayerKey(7),
///     entity: enshwick::Entity(0),
///     id: enshwick::knowledge::KnowledgeId::new(0x1d),
///     value: 1,
///     kind: KnowledgeInfo::FLAGS,
///     op: KnowledgeChange::REVOKE,
///     reserved: 0,
/// };
/// assert_eq!(unflag.op, KnowledgeChange::REVOKE);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable)]
pub struct KnowledgeChange {
    /// Size and version of this struct.
    pub header: Header,
    /// The account whose knowledge changes, by its
    /// [`PlayerInfo::key`](super::PlayerInfo::key): now while its character
    /// is in the game, else once its next character loads in this world; 0
    /// for none.
    pub account: PlayerKey,
    /// The character of a player in the game whose knowledge changes now; 0
    /// for none.
    pub entity: Entity,
    /// The knowledge id.
    pub id: KnowledgeId,
    /// Ignored for [`KnowledgeInfo::BOOL`], the count to add or take for
    /// [`KnowledgeInfo::COUNTER`], the bit to set or clear for
    /// [`KnowledgeInfo::FLAGS`].
    pub value: u32,
    /// The id's kind: [`KnowledgeInfo::BOOL`], [`KnowledgeInfo::COUNTER`]
    /// or [`KnowledgeInfo::FLAGS`].
    pub kind: u8,
    /// [`KnowledgeChange::GRANT`] or [`KnowledgeChange::REVOKE`].
    pub op: u8,
    /// Zero.
    pub reserved: u16,
}

impl_headed!(KnowledgeChange);

impl KnowledgeChange {
    /// Grants the value, as the game grants it.
    pub const GRANT: u8 = 0;
    /// Takes the value back.
    pub const REVOKE: u8 = 1;
    /// A notice of [`HostApi::knowledge_set`](super::HostApi::knowledge_set):
    /// the value written with every bit marked seen, as a save loads it.
    pub const QUIET: u8 = 0;
    /// A notice of [`HostApi::knowledge_set`](super::HostApi::knowledge_set):
    /// what the value gains granted as the game grants it, what it loses
    /// written with the mark of flags kept and none for a bool or a counter.
    pub const SHOWN: u8 = 1;
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
///     position: [0.0; 3],
///     radius: 900.0,
///     reserved: 0,
///     remaining_ns: 300_000_000_000,
///     phenomenon: enshwick::abi::Guid::NIL,
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
    /// Where it is, in metres.
    pub position: Metres,
    /// How far it reaches, in metres.
    pub radius: f32,
    /// Zero.
    pub reserved: u32,
    /// How long it has left, in game nanoseconds.
    pub remaining_ns: i64,
    /// The GUID of its phenomenon in the weather resource, all zero when
    /// the loader did not find it.
    pub phenomenon: Guid,
}

impl_headed!(WeatherFront);

/// How the weather weighs at one place, as weights that add up to 1, written
/// by [`HostApi::weather_at`](super::HostApi::weather_at).
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, WeatherSample};
///
/// let sample = WeatherSample {
///     header: Header::of::<WeatherSample>(),
///     clear: 0.75,
///     rain: 0.25,
///     snow: 0.0,
///     blizzard: 0.0,
/// };
/// assert_eq!(sample.clear + sample.rain, 1.0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Zeroable)]
pub struct WeatherSample {
    /// Size and version of this struct.
    pub header: Header,
    /// How clear it is.
    pub clear: f32,
    /// How rainy it is.
    pub rain: f32,
    /// How snowy it is.
    pub snow: f32,
    /// How stormy it is, in a blizzard.
    pub blizzard: f32,
}

impl_headed!(WeatherSample);

/// A world's save on disk, written by
/// [`HostApi::save_info`](super::HostApi::save_info).
///
/// The mod owns it and passes it as `out`, its header set with
/// a header of its own size and ABI version; the loader writes it on the calling thread during the
/// call, up to the size the header states, and keeps no pointer to it. It
/// holds no pointers. Layout grows only by appending; read a later field
/// once the header's `size` covers it.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, SaveInfo};
///
/// let info = SaveInfo {
///     header: Header::of::<SaveInfo>(),
///     world: enshwick::abi::Guid([7; 16]),
///     saved_at: 1_762_300_000,
///     size: 4_096,
///     backups: 2,
///     reserved: 0,
/// };
/// assert_eq!(info.backups, 2);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable)]
pub struct SaveInfo {
    /// Size and version of this struct.
    pub header: Header,
    /// The world's GUID, which stays the same across its saves.
    pub world: Guid,
    /// When the save was written, in seconds since the Unix epoch.
    pub saved_at: u64,
    /// The save file's size in bytes.
    pub size: u64,
    /// The backups the game keeps beside it, 0 to 10.
    pub backups: u32,
    /// Zero.
    pub reserved: u32,
}

impl_headed!(SaveInfo);

impl WeatherFront {
    /// Clouds without rain, or nothing.
    pub const NONE: u32 = 0;
    /// Rain, snow where it is cold.
    pub const RAIN: u32 = 1;
    /// A blizzard.
    pub const BLIZZARD: u32 = 2;
}

/// Whether a player may edit a box of the server's world, for
/// [`HostApi::build_rights`](crate::abi::HostApi::build_rights): the mod
/// fills `character`, `min` and `max`, and the loader writes `refusal`.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{BuildRights, Header};
///
/// let check = BuildRights {
///     header: Header::of::<BuildRights>(),
///     character: enshwick::Entity(2),
///     refusal: BuildRights::ALLOWED,
///     min: [100.0, 40.0, 100.0],
///     max: [104.0, 44.0, 104.0],
/// };
/// assert_eq!(check.refusal, BuildRights::ALLOWED);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Zeroable)]
pub struct BuildRights {
    /// Size and version of this struct.
    pub header: Header,
    /// The player's character entity.
    pub character: Entity,
    /// [`BuildRights::ALLOWED`] or why the server refuses, written by the
    /// loader.
    pub refusal: u32,
    /// The box's lowest corner, in metres.
    pub min: Metres,
    /// The box's highest corner, in metres, at least `min` on every axis.
    pub max: Metres,
}

impl_headed!(BuildRights);

impl BuildRights {
    /// The server lets the player edit the box.
    pub const ALLOWED: u32 = 0;
    /// The player lacks `CanEditWorld`, which every edit needs.
    pub const NO_CAN_EDIT_WORLD: u32 = 1;
    /// The player lacks `CanEditBase`, and the box overlaps a building zone.
    pub const NO_CAN_EDIT_BASE: u32 = 2;
    /// A corner of the box lies outside the world's building area.
    pub const OUTSIDE_BUILDING_ZONE: u32 = 3;
    /// The box reaches into a strict no-build zone.
    pub const INSIDE_STRICT_BUILD_INHIBITOR_ZONE: u32 = 4;
}

/// A map marker of the scene the server runs, one of the list
/// [`HostApi::map_markers`](super::HostApi::map_markers) writes.
///
/// # Examples
///
/// ```
/// use enshwick::abi::MapMarkerInfo;
///
/// let marker = MapMarkerInfo::EMPTY;
/// assert_eq!(marker.label, enshwick::abi::Guid::NIL);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct MapMarkerInfo {
    /// Size and version of this struct.
    pub header: Header,
    /// Where it stands, in metres, y up.
    pub position: Metres,
    /// Its `markerType` reference; zero for none.
    pub marker_type: Guid,
    /// Its label's text tag (`markerId`); zero for
    /// none.
    pub label: Guid,
}

impl_headed!(MapMarkerInfo);

impl MapMarkerInfo {
    /// A marker with nothing known, its header set.
    pub const EMPTY: Self = Self {
        header: Header::of::<Self>(),
        position: [0.0; 3],
        marker_type: Guid::NIL,
        label: Guid::NIL,
    };
}

/// The frame of the map of the scene the server runs, as
/// [`HostApi::map_frame`](super::HostApi::map_frame) writes it: where the
/// map starts and how far it reaches, in metres on the world's x and z.
///
/// # Examples
///
/// ```
/// use enshwick::abi::MapFrameInfo;
///
/// let frame = MapFrameInfo { size: [10240.0; 2], metres_per_cell: 8.0, ..MapFrameInfo::EMPTY };
/// assert_eq!(frame.size[0] / frame.metres_per_cell, 1280.0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct MapFrameInfo {
    /// Size and version of this struct.
    pub header: Header,
    /// The map's corner of lowest x and z, in metres, x then z.
    pub origin: [f64; 2],
    /// How far the map reaches from `origin`, in metres, x then z.
    pub size: [f64; 2],
    /// The metres one cell of the game's map covers on each axis.
    pub metres_per_cell: f64,
}

impl_headed!(MapFrameInfo);

impl MapFrameInfo {
    /// A frame with nothing known, its header set.
    pub const EMPTY: Self = Self {
        header: Header::of::<Self>(),
        origin: [0.0; 2],
        size: [0.0; 2],
        metres_per_cell: 0.0,
    };
}

/// An account action of [`HostApi::account_action`](super::HostApi::account_action),
/// with the values of the game's own `AccountActionType`.
///
/// # Examples
///
/// ```
/// use enshwick::abi::AccountActionType;
///
/// assert_ne!(AccountActionType::BAN, AccountActionType::UNBAN);
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, bytemuck::Zeroable)]
pub struct AccountActionType(pub u8);

impl AccountActionType {
    /// Removes a player online from the game.
    pub const KICK: Self = Self(0);
    /// Puts the account on the ban list, and removes its player when
    /// online.
    pub const BAN: Self = Self(1);
    /// Takes the account off the ban list.
    pub const UNBAN: Self = Self(2);
}

/// An account joining the game, handed to the rules registered with
/// [`HostApi::join_rule`](super::HostApi::join_rule).
///
/// The rules see it before any player of its machine enters the world.
/// Layout grows only by appending; read a later field once
/// the header's `size` covers it.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, JoinCall};
///
/// let call = JoinCall {
///     header: Header::of::<JoinCall>(),
///     account: enshwick::PlayerKey::new(7),
/// };
/// assert_eq!(call.account.get(), 7);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct JoinCall {
    /// Size and version of this struct.
    pub header: Header,
    /// The joining account's key, the one its player and the server's ban
    /// list carry; zero for an account the game gives no key.
    pub account: PlayerKey,
}

impl_headed!(JoinCall);

/// A mod's join rule, registered with
/// [`HostApi::join_rule`](super::HostApi::join_rule).
///
/// It is called on the thread the session reads its messages on, not the
/// game thread, once for each account a joining machine brings; it returns
/// [`FILTER_REFUSE`](super::FILTER_REFUSE) to refuse the account, any other answer letting it
/// in. `call` is borrowed for the call and never null; `user` is the
/// pointer given at registration, passed as it is. Never re-entered; it must
/// not unwind.
///
/// Null is never a callback: the loader refuses a null one with
/// [`Status::BAD_ARGS`](crate::abi::Status::BAD_ARGS) and keeps nothing.
///
/// # Examples
///
/// The safe layer wraps it in `Load::on_join`.
pub type JoinRuleFn = unsafe extern "system" fn(call: *const JoinCall, user: *mut c_void) -> u8;
