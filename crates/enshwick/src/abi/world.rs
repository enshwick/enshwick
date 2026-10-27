//! The world: game events, entities, teleports, spawns, daytime, damage, knowledge, weather, terrain, voxels, build rights and transactions.

use core::ffi::c_void;

use crate::abi::{Header, Status, Str};

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

/// A voxel of the server's world, written by
/// [`HostApi::voxel_read`](super::HostApi::voxel_read). The loader decodes
/// the game's voxel into it, so a mod never reads the game's own encoding.
///
/// Its layout is frozen: [`HostApi::voxel_read`](super::HostApi::voxel_read)
/// writes an array of it with no entry size.
///
/// # Examples
///
/// ```
/// use enshwick::abi::Voxel;
///
/// let rock = Voxel { kind: Voxel::TERRAIN, material: 9, fill: 0xff, reserved: 0 };
/// assert_ne!(rock.kind, Voxel::AIR);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, bytemuck::Zeroable)]
pub struct Voxel {
    /// [`Voxel::AIR`], [`Voxel::TERRAIN`] or [`Voxel::BUILDING`]; a reader
    /// treats a kind it does not know as neither.
    pub kind: u8,
    /// The game's material id: 0 for air, a terrain material's id as
    /// [`TerrainMaterialInfo::id`] has it, or a building material's id.
    pub material: u8,
    /// How full the voxel is, 0 to 255: enshwick's name for the high byte
    /// of the game's voxel, which the game does not name.
    pub fill: u8,
    /// Zero.
    pub reserved: u8,
}

impl Voxel {
    /// Empty: no ground and no building.
    pub const AIR: u8 = 0;
    /// Ground of a terrain material.
    pub const TERRAIN: u8 = 1;
    /// A building block's material.
    pub const BUILDING: u8 = 2;
    /// The most voxels one [`HostApi::voxel_read`](super::HostApi::voxel_read)
    /// reads: a box of 64 voxels, 32 metres, a side.
    pub const READ_MAX: u64 = 64 * 64 * 64;
    /// The end of the coordinates a box covers on any axis, in voxels:
    /// every voxel of a box lies below it. 2³⁰, far past any world's edge.
    pub const END: u32 = 1 << 30;

    /// The voxels in the box of `size` from `min` when one
    /// [`HostApi::voxel_read`](super::HostApi::voxel_read) takes it: `None`
    /// for an empty box, one past [`Voxel::END`] or more than
    /// [`Voxel::READ_MAX`] voxels.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::Voxel;
    ///
    /// assert_eq!(Voxel::read_count([0; 3], [2, 3, 4]), Some(24));
    /// assert_eq!(Voxel::read_count([0; 3], [0, 3, 4]), None);
    /// assert_eq!(Voxel::read_count([Voxel::END, 0, 0], [1; 3]), None);
    /// ```
    #[must_use]
    pub const fn read_count(min: [u32; 3], size: [u32; 3]) -> Option<u64> {
        let mut count = 1u64;
        let mut axis = 0;
        while axis < 3 {
            let within = match min[axis].checked_add(size[axis]) {
                Some(end) => end <= Self::END,
                None => false,
            };
            if size[axis] == 0 || !within {
                return None;
            }
            count = match count.checked_mul(size[axis] as u64) {
                Some(product) if product <= Self::READ_MAX => product,
                _ => return None,
            };
            axis += 1;
        }
        Some(count)
    }
}

/// Whether a player may edit a box of the server's world, for
/// [`HostApi::build_rights`](crate::abi::HostApi::build_rights): the mod
/// fills `player`, `min` and `max`, and the loader writes `refusal`.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{BuildRights, Header};
///
/// let check = BuildRights {
///     header: Header::of::<BuildRights>(),
///     player: 2,
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
    pub player: u32,
    /// [`BuildRights::ALLOWED`] or why the server refuses, written by the
    /// loader.
    pub refusal: u32,
    /// The box's lowest corner, in metres.
    pub min: [f64; 3],
    /// The box's highest corner, in metres, at least `min` on every axis.
    pub max: [f64; 3],
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

/// One change of a transaction, handed to
/// [`HostApi::transaction_commit`](super::HostApi::transaction_commit) in an
/// array: its kind names the record `data` points at.
///
/// Its layout is frozen: the commit reads an array of it with no entry
/// size. A new kind of change gets a new kind, never a new field.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, TerrainEdit, TransactionOp};
///
/// let dig = TerrainEdit {
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
/// let op = TransactionOp { kind: TransactionOp::TERRAIN, reserved: 0, data: (&raw const dig).cast() };
/// assert_eq!(op.kind, TransactionOp::TERRAIN);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable)]
pub struct TransactionOp {
    /// One of the kinds below; the loader refuses a kind it does not know.
    pub kind: u32,
    /// Zero; the loader refuses another value.
    pub reserved: u32,
    /// The change, borrowed for the call: the record its kind names.
    pub data: *const c_void,
}

impl TransactionOp {
    /// A terrain add, dig or flatten: `data` points at a [`TerrainEdit`]
    /// without [`TerrainEdit::DRY_RUN`].
    pub const TERRAIN: u32 = 1;
    /// Voxels written over a box: `data` points at a [`VoxelWrite`]. Since
    /// minor 89.
    pub const VOXELS: u32 = 2;
    /// The building voxels of a box cleared to air: `data` points at a
    /// [`VoxelRegion`]. Since minor 89.
    pub const ERASE: u32 = 3;
    /// A building block placed: `data` points at a [`BlockPlace`]. Since
    /// minor 89.
    pub const BLOCK: u32 = 4;
    /// A prop placed: `data` points at a [`PropPlace`]. Since minor 90.
    pub const PLACE_PROP: u32 = 5;
    /// A prop removed: `data` points at a [`PropRemove`]. Since minor 90.
    pub const REMOVE_PROP: u32 = 6;
}

/// A prop a transaction places, as a player places one from an item, for
/// [`TransactionOp::PLACE_PROP`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, PropPlace};
///
/// let chest = PropPlace {
///     header: Header::of::<PropPlace>(),
///     item: 0x1234,
///     reserved: 0,
///     position: [100.0, 40.0, 100.0],
///     orientation: [0.0, 0.0, 0.0, 1.0],
/// };
/// assert_eq!(chest.item, 0x1234);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Zeroable)]
pub struct PropPlace {
    /// Size and version of this struct.
    pub header: Header,
    /// The item whose placed template the prop is.
    pub item: u32,
    /// Zero.
    pub reserved: u32,
    /// Where it stands, in metres.
    pub position: [f64; 3],
    /// Which way it faces, a quaternion x, y, z, w.
    pub orientation: [f32; 4],
}

impl_headed!(PropPlace);

/// A prop a transaction removes, for [`TransactionOp::REMOVE_PROP`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, PropRemove};
///
/// let gone = PropRemove { header: Header::of::<PropRemove>(), entity: 77, flags: 0 };
/// assert_eq!(gone.flags & PropRemove::DISCARD_STATE, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable)]
pub struct PropRemove {
    /// Size and version of this struct.
    pub header: Header,
    /// The prop's entity.
    pub entity: u32,
    /// [`PropRemove::DISCARD_STATE`] or none.
    pub flags: u32,
}

impl_headed!(PropRemove);

impl PropRemove {
    /// Removes the prop even when it holds state a player would lose (see
    /// [`TransactionInfo::holds`]); without it, such a prop is refused.
    pub const DISCARD_STATE: u32 = 1;
}

/// Voxels a transaction writes over a box, for
/// [`TransactionOp::VOXELS`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, Voxel, VoxelWrite};
///
/// let stone = [Voxel { kind: Voxel::BUILDING, material: 0x83, fill: 0, reserved: 0 }; 8];
/// let write = VoxelWrite {
///     header: Header::of::<VoxelWrite>(),
///     min: [200, 80, 200],
///     size: [2, 2, 2],
///     voxels: stone.as_ptr(),
///     count: 8,
/// };
/// assert_eq!(write.count, 8);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable)]
pub struct VoxelWrite {
    /// Size and version of this struct.
    pub header: Header,
    /// The box's lowest voxel, in voxels of half a metre from the world's
    /// corner.
    pub min: [u32; 3],
    /// The box's voxels along each axis.
    pub size: [u32; 3],
    /// The box's voxels, x fastest, then y, then z, borrowed for the call;
    /// each kind must be the one its material names.
    pub voxels: *const Voxel,
    /// The voxels at `voxels`: `size[0] * size[1] * size[2]`.
    pub count: u64,
}

impl_headed!(VoxelWrite);

/// A box of voxels, for [`TransactionOp::ERASE`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, VoxelRegion};
///
/// let region = VoxelRegion { header: Header::of::<VoxelRegion>(), min: [200, 80, 200], size: [8, 4, 8] };
/// assert_eq!(region.size[1], 4);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable)]
pub struct VoxelRegion {
    /// Size and version of this struct.
    pub header: Header,
    /// The box's lowest voxel, in voxels of half a metre.
    pub min: [u32; 3],
    /// The box's voxels along each axis.
    pub size: [u32; 3],
}

impl_headed!(VoxelRegion);

/// A building block a transaction places, as a player's building tool
/// does, for [`TransactionOp::BLOCK`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::{BlockPlace, Header};
///
/// let block = BlockPlace {
///     header: Header::of::<BlockPlace>(),
///     shape: 0x1234,
///     material: 0x5678,
///     position: [100.0, 40.0, 100.0],
///     rotation: [0.0, 0.0, 0.0, 1.0],
/// };
/// assert_eq!(block.rotation[3], 1.0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Zeroable)]
pub struct BlockPlace {
    /// Size and version of this struct.
    pub header: Header,
    /// The shape's item: one with a `VoxelBlueprintItem`.
    pub shape: u32,
    /// The material's item: one whose `ChangeVoxelData` places a building
    /// material.
    pub material: u32,
    /// Where the block's middle lies, in metres.
    pub position: [f64; 3],
    /// The block's rotation, a quaternion x, y, z, w; the game keeps the
    /// nearest quarter turns.
    pub rotation: [f32; 4],
}

impl_headed!(BlockPlace);

/// Where a transaction stands, written by
/// [`HostApi::transaction_status`](super::HostApi::transaction_status).
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, Status, TransactionInfo};
///
/// let info = TransactionInfo {
///     header: Header::of::<TransactionInfo>(),
///     state: TransactionInfo::APPLIED,
///     reason: Status::OK,
///     ops: 2,
///     applied: 2,
///     bricks: 16,
///     refusal: 0,
///     reserved: 0,
///     holds: 0,
///     created: 0,
/// };
/// assert_eq!(info.applied, info.ops);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable)]
pub struct TransactionInfo {
    /// Size and version of this struct.
    pub header: Header,
    /// One of the states below; a reader treats a state it does not know as
    /// [`TransactionInfo::PENDING`].
    pub state: u32,
    /// Why it failed, or why its last undo was refused:
    /// [`Status::NOT_LOADED`] for a part of the world not loaded,
    /// [`Status::OUTSIDE_WORLD`] for a change that leaves the world,
    /// [`Status::REFUSED`] for a region changed since, [`Status::MISSING`]
    /// when the world ended or the build lacks a piece; [`Status::OK`]
    /// otherwise.
    pub reason: Status,
    /// The changes it holds.
    pub ops: u32,
    /// The changes written to the world and not undone yet.
    pub applied: u32,
    /// The 8³-voxel bricks whose earlier state the loader keeps for its
    /// undo, 1 KiB each.
    pub bricks: u64,
    /// When the server refused the player the transaction acts for, its
    /// reason, one of the [`BuildRights`] refusals; [`BuildRights::ALLOWED`]
    /// otherwise. Since minor 89.
    pub refusal: u32,
    /// Zero.
    pub reserved: u32,
    /// When a prop's state stopped it, or stopped its last undo, the kinds
    /// of state the prop holds, a set of the `HOLDS_` bits; 0 otherwise.
    /// Since minor 90.
    pub holds: u32,
    /// The props it placed and still holds, which
    /// [`HostApi::transaction_entities`](super::HostApi::transaction_entities)
    /// lists. Since minor 90.
    pub created: u32,
}

impl_headed!(TransactionInfo);

impl TransactionInfo {
    /// Queued, or written in part: the loader spreads a large one over the
    /// server's steps.
    pub const PENDING: u32 = 0;
    /// Written whole; it can be undone.
    pub const APPLIED: u32 = 1;
    /// An undo is queued or written in part.
    pub const UNDOING: u32 = 2;
    /// Undone, or dropped before it wrote anything.
    pub const UNDONE: u32 = 3;
    /// It stopped before or right after a change it could not write or
    /// read back, for `reason`; the changes written stay and can be undone
    /// while `bricks` is above 0. An undo never fails: it is refused.
    pub const FAILED: u32 = 4;
    /// Written whole, or with its undo refused, but the loader dropped what
    /// its undo needs, to stay within its memory cap or because the world
    /// ended; it can no longer be undone.
    pub const EXPIRED: u32 = 5;
    /// The prop's inventory holds items, in any of its linked slots.
    pub const HOLDS_INVENTORY: u32 = 1;
    /// The plant has grown: its `Level` is above its start, as `Growth`
    /// keeps it.
    pub const HOLDS_GROWTH: u32 = 1 << 1;
    /// The `FactoryStation` has a recipe running or queued.
    pub const HOLDS_FACTORY_STATION: u32 = 1 << 2;
    /// A survivor is assigned to the `NpcBed`.
    pub const HOLDS_NPC_BED: u32 = 1 << 3;
    /// Survivors are assigned to it: `StoreAssociatedNpcIds`.
    pub const HOLDS_STORE_ASSOCIATED_NPC_IDS: u32 = 1 << 4;
    /// A player wrote its text: `CustomPlayerString`.
    pub const HOLDS_CUSTOM_PLAYER_STRING: u32 = 1 << 5;
    /// It holds more `Fuel` charges than a fresh one starts with.
    pub const HOLDS_FUEL: u32 = 1 << 6;
    /// It is wired to other logic: `LogicGraphNodeConnected`.
    pub const HOLDS_LOGIC_GRAPH_NODE_CONNECTED: u32 = 1 << 7;
    /// It is a logic trigger, whose settings a player may have changed:
    /// `LogicGraphTrigger` or `LogicTriggerShape`. Any trigger for a
    /// removal; for the undo of a placement, one whose settings differ
    /// from those it first stood with.
    pub const HOLDS_LOGIC_GRAPH_TRIGGER: u32 = 1 << 8;
    /// Its last undo stopped for `reason`: before writing anything, or at a
    /// brick changed since or not loaded for the loader's wait limit, with
    /// what it put back kept. The rest stays written and can be undone
    /// again.
    pub const UNDO_REFUSED: u32 = 6;
}
