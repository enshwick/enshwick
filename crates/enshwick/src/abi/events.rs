//! Events the loader raises, with the data each carries.

use core::ffi::c_void;

use crate::abi::Header;

/// Something the loader tells subscribed mods about, with
/// [`HostApi::subscribe`](super::HostApi::subscribe).
///
/// # Examples
///
/// ```
/// use enshwick::abi::Event;
///
/// assert_ne!(Event::SETTINGS_CHANGED, Event::LOADER_SETTINGS_CHANGED);
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Event(pub u32);

impl Event {
    /// The mod's settings changed: its file on disk, a setting set by
    /// the console or the Mods screen, or a client's join values; no data. The
    /// thread is not fixed: a loader thread about a second after a file
    /// edit, the thread that made any other change.
    pub const SETTINGS_CHANGED: Self = Self(1);
    /// The loader's settings changed, log levels among them; no data. Raised
    /// on a loader thread.
    pub const LOADER_SETTINGS_CHANGED: Self = Self(2);
    /// A simulation step ran; data is a [`TickData`]. Raised on the server's
    /// game thread after the step, about 20 times a second with nobody online
    /// and 60 with players, from before [`Event::WORLD_LOADED`]. Server only.
    pub const TICKED: Self = Self(3);
    /// The world finished loading and the server runs; no data. Raised on the
    /// game thread. Server only.
    pub const WORLD_LOADED: Self = Self(4);
    /// A save of the world finished writing to disk; no data. Raised on the
    /// game thread; the final save runs after [`Event::SHUTDOWN_STARTED`]. Server only.
    pub const WORLD_SAVED: Self = Self(5);
    /// A player's character entered the world; data is a [`PlayerData`].
    /// Raised on the game thread. Server only.
    pub const PLAYER_JOINED: Self = Self(6);
    /// A player who joined is leaving, with their character still in the
    /// world; data is a [`PlayerData`]. Raised on the game thread. Server only.
    pub const PLAYER_LEFT: Self = Self(7);
    /// The server begins an orderly stop with the world and players still
    /// alive; no data. Raised on the game thread; a killed process raises
    /// nothing. Server only.
    pub const SHUTDOWN_STARTED: Self = Self(8);
    /// A player placed a building piece or prop; data is a [`BuildingData`].
    /// Raised on the game thread after the step that placed it. Server only.
    pub const BUILDING_PLACED: Self = Self(9);
    /// A player picked up or tore down a placed piece; data is a
    /// [`BuildingData`] whose entity no longer exists. Raised on the game
    /// thread after the step. Server only.
    pub const BUILDING_REMOVED: Self = Self(10);
    /// The join check kept this client out of a server that runs join mods
    /// the client lacks or runs in another version; no data.
    /// [`HostApi::join_missing`](super::HostApi::join_missing) lists those mods with the versions the
    /// server runs and their pages. Raised on the game thread once the player
    /// closed the loader's dialog. Client only. Since minor 10.
    pub const JOIN_REFUSED: Self = Self(11);
    /// The Flame Altars of the server's world changed: one was placed or
    /// removed, moved, raised a level or had its zone built anew; no data.
    /// [`HostApi::altars`](super::HostApi::altars) lists them as they are now. Raised on the game
    /// thread after the step that noticed it, while a mod has opened altars
    /// ([`HostApi::altars_open`](super::HostApi::altars_open)). Server only. Since minor 15.
    pub const ALTARS_CHANGED: Self = Self(12);
    /// The day began in the server's world: the clock passed the hour day
    /// starts at, or a mod set it into the day; no data. Raised on the game
    /// thread after the step that noticed it. Server only. Since minor 28.
    pub const DAY_STARTED: Self = Self(13);
    /// The night began in the server's world, as [`Event::DAY_STARTED`] the
    /// day; no data. Server only. Since minor 28.
    pub const NIGHT_STARTED: Self = Self(14);
    /// A knowledge value of the server's world or of a connected player
    /// changed: a recipe learnt, a quest step, a place found; data is a
    /// [`KnowledgeData`]. Raised on the game thread after the step that
    /// applied it. Server only. Since minor 32.
    pub const KNOWLEDGE_CHANGED: Self = Self(15);
    /// A player completed a quest and got its rewards; data is a
    /// [`QuestData`]. Raised on the game thread after the step. Server
    /// only. Since minor 32.
    pub const QUEST_COMPLETED: Self = Self(16);
    /// A chunk of the server's world loaded: players came near, and its
    /// entities exist now; data is a [`ChunkData`]. Raised on the game
    /// thread after the step. Server only. Since minor 33.
    pub const CHUNK_LOADED: Self = Self(17);
    /// A chunk of the server's world unloaded: no player is near, and its
    /// entities are gone; data is a [`ChunkData`]. Raised on the game thread
    /// after the step. Server only. Since minor 33.
    pub const CHUNK_UNLOADED: Self = Self(18);
    /// The settings a mod reads were replaced whole, as a world loads in this
    /// process (its own settings from then on) or stops (the settings file
    /// again); no data. The settings that wait for a start apply too.
    /// Raised on the server's game thread. Since minor 46.
    pub const SETTINGS_REPLACED: Self = Self(19);
    /// A player died, from a hit, a fall, the shroud or drowning; data is a
    /// [`PlayerDeathData`]. Raised on the game thread after the step.
    /// Deaths the game keeps no record of, such as freezing or an
    /// administrator's kill, raise none. Server only. Since minor 60.
    pub const PLAYER_DIED: Self = Self(20);
}

/// The data of [`Event::PLAYER_DIED`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, PlayerDeathData};
///
/// let fell = PlayerDeathData {
///     header: Header::of::<PlayerDeathData>(),
///     entity: 2,
///     cause: PlayerDeathData::FALL,
///     key: 7,
///     position: [10.0, 50.0, -3.0],
///     killer: [0; 16],
///     deadly_shroud: 0,
///     drowned: 0,
///     reserved: 0,
///     killer_entity: 0,
///     reserved_tail: 0,
/// };
/// assert_eq!(fell.cause, PlayerDeathData::FALL);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Zeroable, crate::Script)]
pub struct PlayerDeathData {
    /// Size and version of this struct.
    pub header: Header,
    /// The player's character; 0 when the account left in the same step.
    pub entity: u32,
    /// [`PlayerDeathData::HIT`], [`PlayerDeathData::FALL`],
    /// [`PlayerDeathData::SHROUD`] or [`PlayerDeathData::SUFFOCATED`].
    pub cause: u32,
    /// The account, as [`PlayerInfo::key`].
    pub key: u64,
    /// Where, in metres.
    pub position: [f32; 3],
    /// For a hit, the template GUID of what dealt it, a creature's or an
    /// impact's; zero for other causes.
    pub killer: [u8; 16],
    /// 1 when the shroud was the deadly kind.
    pub deadly_shroud: u8,
    /// 1 when the player drowned.
    pub drowned: u8,
    /// Zero.
    pub reserved: u16,
    /// For a hit, the attacker of the last hit the player took, from its
    /// `HitHistory`, as [`crate::game_events::EntityDiedEvent`] reads it; 0 for other
    /// causes, or when the history names none. Since minor 62.
    pub killer_entity: u32,
    /// Zero.
    pub reserved_tail: u32,
}

impl_headed!(PlayerDeathData);

impl PlayerDeathData {
    /// A hit.
    pub const HIT: u32 = 0;
    /// A fall.
    pub const FALL: u32 = 1;
    /// The shroud.
    pub const SHROUD: u32 = 2;
    /// Out of breath, under water.
    pub const SUFFOCATED: u32 = 3;
}

/// The data of [`Event::KNOWLEDGE_CHANGED`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, KnowledgeData};
///
/// let learnt = KnowledgeData { header: Header::of::<KnowledgeData>(), entity: 2, id: 0x1d, value: 3 };
/// assert_eq!(learnt.value, 3);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable, crate::Script)]
pub struct KnowledgeData {
    /// Size and version of this struct.
    pub header: Header,
    /// The player's character, or 0 for the world's knowledge.
    pub entity: u32,
    /// The knowledge id.
    pub id: u32,
    /// The value now.
    pub value: u32,
}

impl_headed!(KnowledgeData);

/// The data of [`Event::QUEST_COMPLETED`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, QuestData};
///
/// let done = QuestData { header: Header::of::<QuestData>(), entity: 2, quest: 0x51 };
/// assert_eq!(done.entity, 2);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable, crate::Script)]
pub struct QuestData {
    /// Size and version of this struct.
    pub header: Header,
    /// The player's character.
    pub entity: u32,
    /// The quest's id, a player knowledge id.
    pub quest: u32,
}

impl_headed!(QuestData);

/// Knowledge a client asks the server for in its input, handed to the
/// filters registered with [`HostApi::knowledge_input_filter`](super::HostApi::knowledge_input_filter) with the
/// verdict so far.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, KnowledgeInfo, KnowledgeInput};
///
/// let seen = KnowledgeInput {
///     header: Header::of::<KnowledgeInput>(),
///     player: 2,
///     id: 0x1d,
///     kind: KnowledgeInfo::FLAGS,
///     scope: KnowledgeInfo::PLAYER,
///     reserved: 0,
///     value: 0,
///     refusal: KnowledgeInput::ALLOWED,
///     allowed: 1,
/// };
/// assert_eq!(seen.allowed, 1);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable)]
pub struct KnowledgeInput {
    /// Size and version of this struct.
    pub header: Header,
    /// The asking player's character.
    pub player: u32,
    /// The knowledge id.
    pub id: u32,
    /// The kind the client names: [`KnowledgeInfo::BOOL`](super::KnowledgeInfo::BOOL),
    /// [`KnowledgeInfo::COUNTER`](super::KnowledgeInfo::COUNTER) or [`KnowledgeInfo::FLAGS`](super::KnowledgeInfo::FLAGS).
    pub kind: u8,
    /// [`KnowledgeInfo::WORLD`](super::KnowledgeInfo::WORLD) or [`KnowledgeInfo::PLAYER`](super::KnowledgeInfo::PLAYER), as the id's
    /// definition says; [`KnowledgeInput::NO_SCOPE`] for an id the game
    /// does not know.
    pub scope: u8,
    /// Zero.
    pub reserved: u16,
    /// Ignored for a bool, added for a counter, the bit to set for flags.
    pub value: u32,
    /// Why the loader's knowledge guard would refuse the request, in any
    /// mode: [`KnowledgeInput::ALLOWED`] when it would not.
    pub refusal: u32,
    /// 1 when the request goes through so far: after the guard, which
    /// refuses only when enforcing, and the filters that ran before.
    pub allowed: u32,
}

impl_headed!(KnowledgeInput);

impl KnowledgeInput {
    /// The scope of an id the game does not know.
    pub const NO_SCOPE: u8 = 0xff;
    /// The guard lets it through.
    pub const ALLOWED: u32 = 0;
    /// An id the game does not know.
    pub const UNKNOWN: u32 = 1;
    /// A kind that is not the id's.
    pub const KIND: u32 = 2;
    /// An id of the world's, which every player shares.
    pub const WORLD: u32 = 3;
    /// A counter.
    pub const COUNTER: u32 = 4;
    /// A flag bit other than 0, the bit a client sets for what it has seen.
    pub const FLAG_BIT: u32 = 5;
    /// An id that gives experience.
    pub const EXPERIENCE: u32 = 6;
    /// An id that gives skill points.
    pub const SKILL_POINTS: u32 = 7;
    /// A quest's id, set when the game rewards the quest.
    pub const QUEST: u32 = 8;
}

/// A mod's knowledge input filter: the request and the pointer given at
/// registration; returns 1 to let it through, 0 to refuse it.
pub type KnowledgeInputFn =
    unsafe extern "system" fn(call: *const KnowledgeInput, user: *mut c_void) -> u8;

/// The data of [`Event::CHUNK_LOADED`] and [`Event::CHUNK_UNLOADED`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::{ChunkData, Header};
///
/// let chunk = ChunkData { header: Header::of::<ChunkData>(), index: 41, x: 1, z: 1, size: 256 };
/// assert_eq!(chunk.x * chunk.size, 256, "it starts 256 m east of the origin");
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable, crate::Script)]
pub struct ChunkData {
    /// Size and version of this struct.
    pub header: Header,
    /// The chunk's index in the world's grid: `z * width + x`.
    pub index: u32,
    /// Its column, west to east from 0.
    pub x: u32,
    /// Its row, from 0 along the other axis.
    pub z: u32,
    /// Its side in metres: it spans `x * size` to `(x + 1) * size` on one
    /// axis and `z * size` to `(z + 1) * size` on the other, at any height.
    pub size: u32,
}

impl_headed!(ChunkData);

/// The data of [`Event::BUILDING_PLACED`] and [`Event::BUILDING_REMOVED`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::{BuildingData, Header};
///
/// let placed = BuildingData {
///     header: Header::of::<BuildingData>(),
///     entity: 4096,
///     player: 2,
///     item: 1_302_084_846,
///     kind: BuildingData::PLACED,
///     replay: 0,
///     template: [0; 16],
///     position: [1.0, 2.0, 3.0],
/// };
/// assert_eq!(placed.kind, BuildingData::PLACED);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable, crate::Script)]
pub struct BuildingData {
    /// Size and version of this struct.
    pub header: Header,
    /// The placed entity; for a removal, the entity as it was.
    pub entity: u32,
    /// The acting player's character entity.
    pub player: u32,
    /// The item placed, which the entity keeps as its `UsedItem`; 0 for a
    /// removal.
    pub item: u32,
    /// [`BuildingData::PLACED`], [`BuildingData::PICKED_UP`] or
    /// [`BuildingData::TORN_DOWN`].
    pub kind: u8,
    /// 1 when undo or redo replayed the action.
    pub replay: u8,
    /// The template GUID of the placed entity, in file order; zero for a
    /// removal.
    pub template: [u8; 16],
    /// Where it was placed, in metres; zero for a removal.
    pub position: [f32; 3],
}

impl_headed!(BuildingData);

impl BuildingData {
    /// Placed by a player.
    pub const PLACED: u8 = 0;
    /// Picked up, its item returned.
    pub const PICKED_UP: u8 = 1;
    /// Torn down.
    pub const TORN_DOWN: u8 = 2;
}

/// A connected player, from [`HostApi::player`](super::HostApi::player) or [`HostApi::players`](super::HostApi::players).
///
/// Unlike other structs with a [`Header`], its layout is frozen and it never
/// grows: [`ActionCall`](super::ActionCall) and [`RenameCall`](super::RenameCall) carry it before fields of their
/// own, and [`HostApi::players`](super::HostApi::players) writes an array of it with no entry size.
/// Player data a later minor adds goes into a new struct. The caller still
/// starts each one it passes with [`Header::of`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, PlayerInfo};
///
/// let mut name = [0u8; 64];
/// name[..3].copy_from_slice(b"Ann");
/// let player = PlayerInfo {
///     header: Header::of::<PlayerInfo>(),
///     slot: 0,
///     entity: 2,
///     key: 7,
///     permissions: PlayerInfo::CAN_ACCESS_INVENTORIES,
///     name_len: 3,
///     name,
/// };
/// assert_eq!(&player.name[..usize::from(player.name_len)], b"Ann");
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct PlayerInfo {
    /// Size and version of this struct.
    pub header: Header,
    /// The server's player slot.
    pub slot: u32,
    /// The player character's entity.
    pub entity: u32,
    /// The same number for the same account in every session.
    pub key: u64,
    /// The server's permission bits for the player, `PlayerInfo::CAN_*`.
    pub permissions: u8,
    /// Bytes of `name` in use.
    pub name_len: u8,
    /// The name the client sent, UTF-8 as sent, cut at 64 bytes.
    pub name: [u8; 64],
}

impl_headed!(PlayerInfo);

impl PlayerInfo {
    /// May kick and ban; the server's administrators.
    pub const CAN_KICK_BAN: u8 = 1;
    /// May open chests and other inventories.
    pub const CAN_ACCESS_INVENTORIES: u8 = 1 << 1;
    /// May build in bases.
    pub const CAN_EDIT_BASE: u8 = 1 << 2;
    /// May change the world outside bases.
    pub const CAN_EDIT_WORLD: u8 = 1 << 3;
    /// May extend bases.
    pub const CAN_EXTEND_BASE: u8 = 1 << 4;
    /// Receives experience.
    pub const CAN_RECEIVE_EXP: u8 = 1 << 5;
}

/// The data of [`Event::TICKED`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, TickData};
///
/// let tick = TickData { header: Header::of::<TickData>(), step: 1 };
/// assert_eq!(tick.step, 1);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable, crate::Script)]
pub struct TickData {
    /// Size and version of this struct.
    pub header: Header,
    /// Steps since the loader started counting, from 1.
    pub step: u64,
}

impl_headed!(TickData);

/// The data of [`Event::PLAYER_JOINED`] and [`Event::PLAYER_LEFT`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, PlayerData};
///
/// let player = PlayerData { header: Header::of::<PlayerData>(), slot: 0, entity: 7, key: 42 };
/// assert_eq!(player.entity, 7);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable, crate::Script)]
pub struct PlayerData {
    /// Size and version of this struct.
    pub header: Header,
    /// The server's player slot, 0 to 15, while the player is connected.
    pub slot: u32,
    /// The player character's entity id.
    pub entity: u32,
    /// The same number for the same player account in every session: a hash
    /// the game makes of the online id.
    pub key: u64,
}

impl_headed!(PlayerData);

/// A mod's handler for an [`Event`]: the event, its data (null for events
/// without data, valid for the call, and no other thread writes it during
/// the call) and the pointer given at subscription.
pub type EventFn = unsafe extern "system" fn(event: Event, data: *const c_void, user: *mut c_void);
