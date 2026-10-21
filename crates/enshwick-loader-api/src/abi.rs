//! The binary interface between the loader and a mod DLL.
//!
//! Everything here is `repr(C)` and crosses DLL boundaries, so the layout is
//! the contract: structs that the other side may outgrow start with a
//! [`Header`] and only ever grow at the end, enums are integers, strings are
//! borrowed for the call, and no function unwinds. Mods normally use the safe
//! layer of this crate instead of these types.

use core::ffi::c_void;
use core::sync::atomic::AtomicUsize;

/// The major version of this interface. A loader refuses a mod built for
/// another major.
pub const ABI_MAJOR: u16 = 1;

/// The minor version of this interface. A newer minor only appends fields.
pub const ABI_MINOR: u16 = 64;

/// Whether `id` is a valid mod id: 1 to 64 characters of `[a-z0-9-]`.
///
/// The id names the mod's DLL and settings file, so it stays portable.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::is_valid_id;
///
/// assert!(is_valid_id("flame-altar-zones"));
/// assert!(!is_valid_id("Flame Altar Zones"));
/// ```
#[must_use]
pub const fn is_valid_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    if bytes.is_empty() || bytes.len() > 64 {
        return false;
    }
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if !(b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-') {
            return false;
        }
        i += 1;
    }
    true
}

/// The start of every struct the other side may know an older or newer
/// version of.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Header, Manifest};
///
/// let header = Header::of::<Manifest>();
/// assert_eq!(header.abi_major, enshwick_loader_api::abi::ABI_MAJOR);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    /// The size of the whole struct as its writer knew it.
    pub size: u32,
    /// [`ABI_MAJOR`] of the writer.
    pub abi_major: u16,
    /// [`ABI_MINOR`] of the writer.
    pub abi_minor: u16,
}

impl Header {
    /// The header for a struct of type `T` written by this build.
    ///
    /// # Panics
    ///
    /// Never at run time: a struct over 4 GiB fails to compile.
    ///
    /// # Examples
    ///
    /// See [`Header`].
    #[must_use]
    pub const fn of<T>() -> Self {
        let size = size_of::<T>();
        assert!(size <= u32::MAX as usize, "an ABI struct fits in u32");
        #[expect(
            clippy::cast_possible_truncation,
            reason = "checked by the assert above"
        )]
        Self {
            size: size as u32,
            abi_major: ABI_MAJOR,
            abi_minor: ABI_MINOR,
        }
    }

    /// Whether the struct this header starts carries a field that ends at
    /// byte `end`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::{Header, Manifest};
    ///
    /// let header = Header::of::<Manifest>();
    /// assert!(header.covers(size_of::<Manifest>()));
    /// assert!(!header.covers(size_of::<Manifest>() + 1));
    /// ```
    #[must_use]
    pub const fn covers(self, end: usize) -> bool {
        end <= self.size as usize
    }
}

/// Copies a struct that starts with a [`Header`] from the other side, which
/// may know an older or a newer version of it.
///
/// Reads only the bytes the writer's header covers; the fields it does not
/// know stay zero. `None` when `raw` is null or the writer's struct ends
/// before byte `required`, the end of the fields every version has. The copy
/// keeps the writer's header, so check [`Header::covers`] before trusting a
/// field newer than `required` that zero does not default well.
///
/// # Safety
///
/// `raw` is null or points at a struct that starts with a [`Header`] and is
/// readable for the header's size; `T` is `repr(C)`, starts with a
/// [`Header`], and an all-zero `T` is a valid value.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Header, TickData, read_headed};
///
/// let mut old = TickData { header: Header::of::<TickData>(), step: 7 };
/// old.header.size = 8;
/// // SAFETY: a valid tick; an all-zero tick is valid.
/// let tick = unsafe { read_headed(&raw const old, 8) }.ok_or("too short")?;
/// assert_eq!(tick.step, 0);
/// # Ok::<(), &str>(())
/// ```
#[must_use]
pub unsafe fn read_headed<T: Copy>(raw: *const T, required: usize) -> Option<T> {
    if raw.is_null() {
        return None;
    }
    // SAFETY: the caller's struct starts with a header.
    let header = unsafe { raw.cast::<Header>().read_unaligned() };
    if !header.covers(required.max(size_of::<Header>())) {
        return None;
    }
    let known = (header.size as usize).min(size_of::<T>());
    let mut out = core::mem::MaybeUninit::<T>::zeroed();
    // SAFETY: the writer's struct is readable for its header's size and `out`
    // for `size_of::<T>()`; `known` is at most both.
    unsafe { core::ptr::copy_nonoverlapping(raw.cast::<u8>(), out.as_mut_ptr().cast(), known) };
    // SAFETY: an all-zero `T` is valid and the copied bytes are a `T`'s.
    Some(unsafe { out.assume_init() })
}

/// Writes `value`, a struct that starts with a [`Header`], to the other side,
/// which may know an older or a newer version of it.
///
/// Writes only the bytes both sides know, and in the written header the
/// size of what it wrote. `false`, writing nothing, when `out` is null or its
/// header is shorter than a [`Header`].
///
/// # Safety
///
/// `out` is null or points at a struct that starts with a [`Header`] whose
/// size is that of the struct, writable for that many bytes; `T` is
/// `repr(C)` and starts with a [`Header`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Header, TickData, write_headed};
///
/// let mut old = TickData { header: Header::of::<TickData>(), step: 0 };
/// old.header.size = 8;
/// let new = TickData { header: Header::of::<TickData>(), step: 7 };
/// // SAFETY: a valid tick of the size its header says.
/// assert!(unsafe { write_headed(&raw mut old, new) });
/// assert_eq!((old.header.size, old.step), (8, 0));
/// ```
#[must_use]
pub unsafe fn write_headed<T: Copy>(out: *mut T, value: T) -> bool {
    if out.is_null() {
        return false;
    }
    // SAFETY: the caller's struct starts with a header.
    let room = unsafe { out.cast::<Header>().read_unaligned() }.size as usize;
    if room < size_of::<Header>() {
        return false;
    }
    let known = room.min(size_of::<T>());
    // SAFETY: the caller's struct is writable for `room` bytes and `value`
    // readable for `size_of::<T>()`; `known` is at most both.
    unsafe {
        core::ptr::copy_nonoverlapping((&raw const value).cast::<u8>(), out.cast(), known);
    };
    // `known` is at most `room`, a `u32`.
    let size = u32::try_from(known).unwrap_or(u32::MAX);
    // SAFETY: the header's size is the struct's first field.
    unsafe { out.cast::<u32>().write_unaligned(size) };
    true
}

/// A UTF-8 string borrowed for the duration of a call, or for as long as its
/// owner documents.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::Str;
///
/// let id = Str::new("flame-altar-zones");
/// // SAFETY: `id` borrows a `'static` string.
/// assert_eq!(unsafe { id.as_str() }, Some("flame-altar-zones"));
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Str {
    ptr: *const u8,
    len: usize,
}

// SAFETY: a `Str` is a borrowed view of immutable bytes; sharing it across
// threads shares only the view.
unsafe impl Send for Str {}
// SAFETY: as for `Send`, the bytes are never written through a `Str`.
unsafe impl Sync for Str {}

impl Str {
    /// The empty string.
    pub const EMPTY: Self = Self::new("");

    /// A view of `text`.
    ///
    /// # Examples
    ///
    /// See [`Str`].
    #[must_use]
    pub const fn new(text: &str) -> Self {
        Self {
            ptr: text.as_ptr(),
            len: text.len(),
        }
    }

    /// The length in bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::Str;
    ///
    /// assert_eq!(Str::new("abc").len(), 3);
    /// ```
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether the string is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::Str;
    ///
    /// assert!(Str::EMPTY.is_empty());
    /// ```
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The text, or `None` when the bytes are not UTF-8 or the pointer is
    /// null or the length impossible.
    ///
    /// # Safety
    ///
    /// Unless the pointer is null, the bytes must be alive and unchanged for
    /// the lifetime `'a` the caller picks.
    ///
    /// # Examples
    ///
    /// See [`Str`].
    #[must_use]
    pub unsafe fn as_str<'a>(&self) -> Option<&'a str> {
        if self.len == 0 {
            return Some("");
        }
        if self.ptr.is_null() || isize::try_from(self.len).is_err() {
            return None;
        }
        // SAFETY: the caller keeps `len` bytes at `ptr` alive for `'a`.
        let bytes = unsafe { core::slice::from_raw_parts(self.ptr, self.len) };
        core::str::from_utf8(bytes).ok()
    }
}

/// The outcome of a call across the interface.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::Status;
///
/// assert!(Status::OK.is_ok());
/// assert!(!Status::MISSING.is_ok());
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Status(pub u32);

impl Status {
    /// Success.
    pub const OK: Self = Self(0);
    /// What was asked for does not exist on this game build.
    pub const MISSING: Self = Self(1);
    /// More than one candidate answers a lookup.
    pub const AMBIGUOUS: Self = Self(2);
    /// The request conflicts with another mod or with the loader's rules.
    pub const REFUSED: Self = Self(3);
    /// An argument is malformed.
    pub const BAD_ARGS: Self = Self(4);
    /// The call came in a phase where it is not allowed, such as a hook after
    /// arming.
    pub const TOO_LATE: Self = Self(5);
    /// The mod has faulted and is off.
    pub const FAULTED: Self = Self(6);
    /// Called from a thread the function does not serve, such as a game
    /// read off the server's game thread, or a world change inside a step.
    pub const WRONG_THREAD: Self = Self(7);

    /// Whether this is [`Status::OK`].
    ///
    /// # Examples
    ///
    /// See [`Status`].
    #[must_use]
    pub const fn is_ok(self) -> bool {
        self.0 == Self::OK.0
    }
}

/// Which side of the game a mod or a binary is.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::Side;
///
/// assert!(Side::BOTH.includes(Side::CLIENT));
/// assert!(!Side::SERVER.includes(Side::CLIENT));
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Side(pub u8);

impl Side {
    /// The dedicated server; as a mod's side, server features, which the
    /// client also loads to run while it hosts a game (solo play included).
    pub const SERVER: Self = Self(1);
    /// The game client.
    pub const CLIENT: Self = Self(2);
    /// Both binaries.
    pub const BOTH: Self = Self(3);

    /// Whether `self` covers every side in `other`.
    ///
    /// # Examples
    ///
    /// See [`Side`].
    #[must_use]
    pub const fn includes(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// The severity of a log line.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::Level;
///
/// assert!(Level::ERROR.0 < Level::DEBUG.0);
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Level(pub u8);

impl Level {
    /// Something failed and a feature is off.
    pub const ERROR: Self = Self(0);
    /// Something unexpected that the mod handled.
    pub const WARN: Self = Self(1);
    /// A milestone worth one line.
    pub const INFO: Self = Self(2);
    /// Detail for development, off in releases.
    pub const DEBUG: Self = Self(3);

    /// The level's name as settings and log lines spell it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::Level;
    ///
    /// assert_eq!(Level::WARN.name(), "warn");
    /// ```
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::ERROR => "error",
            Self::WARN => "warn",
            Self::INFO => "info",
            _ => "debug",
        }
    }
}

impl Default for Level {
    fn default() -> Self {
        Self::INFO
    }
}

impl core::str::FromStr for Level {
    type Err = ParseLevelError;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "error" => Ok(Self::ERROR),
            "warn" => Ok(Self::WARN),
            "info" => Ok(Self::INFO),
            "debug" => Ok(Self::DEBUG),
            _ => Err(ParseLevelError),
        }
    }
}

impl serde::Serialize for Level {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.name())
    }
}

impl<'de> serde::Deserialize<'de> for Level {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        name.parse().map_err(serde::de::Error::custom)
    }
}

/// Something the loader tells subscribed mods about, with
/// [`HostApi::subscribe`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::Event;
///
/// assert_ne!(Event::SETTINGS_CHANGED, Event::LOADER_SETTINGS_CHANGED);
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Event(pub u32);

impl Event {
    /// The mod's settings changed: its file on disk, a setting set by
    /// `/config` or the Mods screen, or a client's join values; no data. The
    /// thread is not fixed: a loader thread about a second after a file
    /// edit, the thread that made any other change.
    pub const SETTINGS_CHANGED: Self = Self(1);
    /// The loader's settings changed, log levels among them; no data. Raised
    /// on a loader thread.
    pub const LOADER_SETTINGS_CHANGED: Self = Self(2);
    /// A simulation step ran; data is a [`TickData`]. Raised on the server's
    /// game thread after the step, about 20 times a second with nobody online
    /// and 60 with players, from before [`Event::WORLD_LOADED`]. Server only.
    pub const TICK: Self = Self(3);
    /// The world finished loading and the server runs; no data. Raised on the
    /// game thread. Server only.
    pub const WORLD_LOADED: Self = Self(4);
    /// A save of the world finished writing to disk; no data. Raised on the
    /// game thread; the final save runs after [`Event::SHUTDOWN`]. Server only.
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
    pub const SHUTDOWN: Self = Self(8);
    /// A player placed a building piece or prop; data is a [`BuildingData`].
    /// Raised on the game thread after the step that placed it. Server only.
    pub const BUILDING_PLACED: Self = Self(9);
    /// A player picked up or tore down a placed piece; data is a
    /// [`BuildingData`] whose entity no longer exists. Raised on the game
    /// thread after the step. Server only.
    pub const BUILDING_REMOVED: Self = Self(10);
    /// The join check kept this client out of a server that runs join mods
    /// the client lacks or runs in another version; no data.
    /// [`HostApi::join_missing`] lists those mods with the versions the
    /// server runs and where they come from. Raised on the game thread once
    /// the player closed the loader's note: a handler leaves downloads to a
    /// thread of its own. Client only. Since minor 10.
    pub const JOIN_MISSING: Self = Self(11);
    /// The Flame Altars of the server's world changed: one was placed or
    /// removed, moved, raised a level or had its zone built anew; no data.
    /// [`HostApi::altars`] lists them as they are now. Raised on the game
    /// thread after the step that noticed it, while a mod has opened altars
    /// ([`HostApi::altars_open`]). Server only. Since minor 15.
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
    /// The settings a mod reads changed whole, as a world loads in this
    /// process (its own settings from then on) or stops (the settings file
    /// again); no data. The settings that wait for a start apply too.
    /// Raised on the server's game thread. Since minor 46.
    pub const WORLD_SETTINGS: Self = Self(19);
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
/// use enshwick_loader_api::abi::{Header, PlayerDeathData};
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
#[derive(Clone, Copy, Debug, PartialEq)]
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
    /// `HitHistory`, as [`crate::game_events::Died`] reads it; 0 for other
    /// causes, or when the history names none. Since minor 62.
    pub killer_entity: u32,
    /// Zero.
    pub reserved_tail: u32,
}

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
/// use enshwick_loader_api::abi::{Header, KnowledgeData};
///
/// let learnt = KnowledgeData { header: Header::of::<KnowledgeData>(), entity: 2, id: 0x1d, value: 3 };
/// assert_eq!(learnt.value, 3);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

/// The data of [`Event::QUEST_COMPLETED`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Header, QuestData};
///
/// let done = QuestData { header: Header::of::<QuestData>(), entity: 2, quest: 0x51 };
/// assert_eq!(done.entity, 2);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuestData {
    /// Size and version of this struct.
    pub header: Header,
    /// The player's character.
    pub entity: u32,
    /// The quest's id, a player knowledge id.
    pub quest: u32,
}

/// Knowledge a client asks the server for in its input, handed to the
/// filters registered with [`HostApi::knowledge_input_filter`] with the
/// verdict so far.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Header, KnowledgeInfo, KnowledgeInput};
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KnowledgeInput {
    /// Size and version of this struct.
    pub header: Header,
    /// The asking player's character.
    pub player: u32,
    /// The knowledge id.
    pub id: u32,
    /// The kind the client names: [`KnowledgeInfo::BOOL`],
    /// [`KnowledgeInfo::COUNTER`] or [`KnowledgeInfo::FLAGS`].
    pub kind: u8,
    /// [`KnowledgeInfo::WORLD`] or [`KnowledgeInfo::PLAYER`], as the id's
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
/// use enshwick_loader_api::abi::{ChunkData, Header};
///
/// let chunk = ChunkData { header: Header::of::<ChunkData>(), index: 41, x: 1, z: 1, size: 256 };
/// assert_eq!(chunk.x * chunk.size, 256, "it starts 256 m east of the origin");
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

/// The data of [`Event::BUILDING_PLACED`] and [`Event::BUILDING_REMOVED`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{BuildingData, Header};
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
#[derive(Clone, Copy, Debug)]
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

impl BuildingData {
    /// Placed by a player.
    pub const PLACED: u8 = 0;
    /// Picked up, its item returned.
    pub const PICKED_UP: u8 = 1;
    /// Torn down.
    pub const TORN_DOWN: u8 = 2;
}

/// A connected player, from [`HostApi::player`] or [`HostApi::players`].
///
/// Unlike other structs with a [`Header`], its layout is frozen and it never
/// grows: [`CommandCall`] and [`RenameCall`] carry it before fields of their
/// own, and [`HostApi::players`] writes an array of it with no entry size.
/// Player data a later minor adds goes into a new struct. The caller still
/// starts each one it passes with [`Header::of`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Header, PlayerInfo};
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
#[derive(Clone, Copy, Debug)]
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

/// The data of [`Event::TICK`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Header, TickData};
///
/// let tick = TickData { header: Header::of::<TickData>(), step: 1 };
/// assert_eq!(tick.step, 1);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TickData {
    /// Size and version of this struct.
    pub header: Header,
    /// Steps since the loader started counting, from 1.
    pub step: u64,
}

/// The data of [`Event::PLAYER_JOINED`] and [`Event::PLAYER_LEFT`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Header, PlayerData};
///
/// let player = PlayerData { header: Header::of::<PlayerData>(), slot: 0, entity: 7, key: 42 };
/// assert_eq!(player.entity, 7);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
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

/// A mod's handler for an [`Event`]: the event, its data (null for events
/// without data, valid for the call) and the pointer given at subscription.
pub type EventFn = unsafe extern "system" fn(event: Event, data: *const c_void, user: *mut c_void);

/// A chat command a player typed, handed to the mod that registered its name
/// with [`HostApi::command`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{CommandCall, Header, PlayerInfo, Str};
///
/// let call = CommandCall {
///     header: Header::of::<CommandCall>(),
///     player: PlayerInfo {
///         header: Header::of::<PlayerInfo>(),
///         slot: 0,
///         entity: 2,
///         key: 7,
///         permissions: 0,
///         name_len: 0,
///         name: [0; 64],
///     },
///     name: Str::new("safe"),
///     args: Str::new("allow Ann"),
/// };
/// // SAFETY: `args` borrows a `'static` string.
/// assert_eq!(unsafe { call.args.as_str() }, Some("allow Ann"));
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct CommandCall {
    /// Size and version of this struct.
    pub header: Header,
    /// The player who typed it.
    pub player: PlayerInfo,
    /// The command's name, without the `/`.
    pub name: Str,
    /// The rest of the line after the name, trimmed; valid for the call.
    pub args: Str,
}

/// A mod's handler for a chat command: the call, the pointer given at
/// registration, and where to write the reply for the typing player. The
/// reply stays valid until the handler runs again; empty for none.
pub type CommandCallFn =
    unsafe extern "system" fn(call: *const CommandCall, user: *mut c_void, reply: *mut Str);

/// A mod's status source: the pointer given at registration and where to
/// write the mod's lines for the loader's status summary. The text stays
/// valid until the source runs again; empty for none.
pub type StatusFn = unsafe extern "system" fn(user: *mut c_void, out: *mut Str);

/// A player renaming an entity, handed to the filters registered with
/// [`HostApi::rename_filter`] before the game applies it.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Header, PlayerInfo, RenameCall, Str};
///
/// let call = RenameCall {
///     header: Header::of::<RenameCall>(),
///     entity: 4096,
///     known: 0,
///     player: PlayerInfo {
///         header: Header::of::<PlayerInfo>(),
///         slot: 0,
///         entity: 0,
///         key: 0,
///         permissions: 0,
///         name_len: 0,
///         name: [0; 64],
///     },
///     text: Str::new("Tools"),
/// };
/// assert_eq!(call.known, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RenameCall {
    /// Size and version of this struct.
    pub header: Header,
    /// The entity being renamed.
    pub entity: u32,
    /// 1 when `player` holds the renaming player, 0 when the loader could
    /// not tell who sent the rename.
    pub known: u8,
    /// The renaming player, when `known` is 1.
    pub player: PlayerInfo,
    /// The new name as the client sent it; valid for the call.
    pub text: Str,
}

/// A mod's rename filter: the call and the pointer given at registration;
/// returns 1 to let the rename through, 0 to refuse it.
pub type RenameCallFn = unsafe extern "system" fn(call: *const RenameCall, user: *mut c_void) -> u8;

/// A game resource's key: its GUID in memory order, its type hash and part.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::ResourceKey;
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

/// Builds a served resource from its base: `base` is the base resource's
/// bytes, valid for the call. On [`Status::OK`] the function writes a pointer
/// and length of bytes that stay valid and writable for the life of the
/// process: a rebuild ([`HostApi::rebuild_resource`]) writes over the last
/// patch's bytes.
pub type ResourcePatchFn = unsafe extern "system" fn(
    base: *const u8,
    base_len: usize,
    user: *mut c_void,
    out: *mut *const u8,
    out_len: *mut usize,
) -> Status;

/// A level name other than `error`, `warn`, `info` or `debug`.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::Level;
///
/// assert!("loud".parse::<Level>().is_err());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("a log level is one of \"error\", \"warn\", \"info\" or \"debug\"")]
pub struct ParseLevelError;

/// A game build's identity: the CodeView GUID, in file order, and age.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::Fingerprint;
///
/// let build = Fingerprint { guid: [0x1c; 16], age: 387 };
/// assert_eq!(build.age, 387);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Fingerprint {
    /// The GUID in file order.
    pub guid: [u8; 16],
    /// The linker's age counter.
    pub age: u32,
}

/// The running game, as the loader identified it.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{BuildInfo, BuildTable, Fingerprint, Header, Side};
///
/// let build = BuildInfo {
///     header: Header::of::<BuildInfo>(),
///     side: Side::SERVER,
///     base: 0x1_4000_0000,
///     fingerprint: Fingerprint { guid: [0; 16], age: 0 },
///     table: BuildTable::EXACT,
/// };
/// assert_eq!(build.side, Side::SERVER);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuildInfo {
    /// Size and version of this struct.
    pub header: Header,
    /// [`Side::SERVER`] or [`Side::CLIENT`].
    pub side: Side,
    /// Where the game's image is loaded.
    pub base: u64,
    /// The build fingerprint.
    pub fingerprint: Fingerprint,
    /// Whether the loader's build table was made for this build. Since
    /// minor 51; older loaders run only on builds with their own table.
    pub table: BuildTable,
}

/// Where the loader's build table for the running build comes from.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::BuildTable;
///
/// assert_ne!(BuildTable::EXACT, BuildTable::PATTERNS);
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct BuildTable(pub u8);

impl BuildTable {
    /// The table was made for this build: every entry it pins is there.
    pub const EXACT: Self = Self(0);
    /// The build has no table yet: the newest table of its side found what
    /// it could by code patterns, and the rest is missing until the table
    /// for this build arrives.
    pub const PATTERNS: Self = Self(1);
}

/// One symbol pinned for one game build: a name and its address relative to
/// the image base.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Str, Symbol};
///
/// let symbol = Symbol { name: Str::new("npc.lookup"), rva: 0x20_2e80 };
/// assert_eq!(symbol.rva, 0x20_2e80);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Symbol {
    /// The symbolic id, such as `npc.lookup`.
    pub name: Str,
    /// The address relative to the image base.
    pub rva: u64,
}

/// A build fingerprint's GUID from its 32 lowercase hex digits, for build rows
/// written in source.
///
/// # Panics
///
/// When `hex` is not 32 lowercase hex digits; in a `const` or `static` that
/// fails the build.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::guid;
///
/// assert_eq!(guid("1c129660cd6efc4f9f0b21652d5ff2bf")[0], 0x1c);
/// ```
#[must_use]
pub const fn guid(hex: &str) -> [u8; 16] {
    let bytes = hex.as_bytes();
    assert!(bytes.len() == 32, "a GUID is 32 hex digits");
    let mut out = [0u8; 16];
    let mut i = 0;
    while i < 16 {
        out[i] = (digit(bytes[2 * i]) << 4) | digit(bytes[2 * i + 1]);
        i += 1;
    }
    out
}

#[expect(
    clippy::panic,
    reason = "used in const evaluation, where a bad literal fails the build"
)]
const fn digit(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        _ => panic!("a GUID is lowercase hex"),
    }
}

/// The symbols a mod pins itself for one game build, beside the loader's own
/// table.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{BuildRow, Fingerprint, Side, Str, Symbol};
///
/// static SYMBOLS: [Symbol; 1] = [Symbol { name: Str::new("npc.update"), rva: 0x20_55f0 }];
/// let row = BuildRow {
///     side: Side::SERVER,
///     fingerprint: Fingerprint { guid: [0; 16], age: 0 },
///     symbols: SYMBOLS.as_ptr(),
///     symbol_count: SYMBOLS.len(),
///     constants: core::ptr::null(),
///     constant_count: 0,
/// };
/// assert_eq!(row.symbol_count, 1);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BuildRow {
    /// The binary the row is for.
    pub side: Side,
    /// The build the row is for.
    pub fingerprint: Fingerprint,
    /// The pinned symbols.
    pub symbols: *const Symbol,
    /// How many symbols `symbols` points at.
    pub symbol_count: usize,
    /// Engine constants the mod pins for the build, such as offsets in the
    /// caller frames it reads; the `rva` field holds the value.
    pub constants: *const Symbol,
    /// How many constants `constants` points at.
    pub constant_count: usize,
}

// SAFETY: a build row is immutable static data pointing at static symbols.
unsafe impl Sync for BuildRow {}

/// How a mod relates to another mod it names.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::Relation;
///
/// assert_ne!(Relation::REQUIRES, Relation::BREAKS);
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Relation(pub u8);

impl Relation {
    /// The other mod must be present in the version range; it loads first.
    pub const REQUIRES: Self = Self(0);
    /// When the other mod is present, it loads first; it may be absent.
    pub const AFTER: Self = Self(1);
    /// This mod does not work beside the other mod in the version range (any
    /// version when the range is empty): when that one loads, this one is
    /// skipped.
    pub const BREAKS: Self = Self(2);
    /// This mod works better beside the other mod in the version range: it
    /// loads first when present, and the loader notes when it is missing or
    /// in another version.
    pub const RECOMMENDS: Self = Self(3);
    /// This mod takes the place of the other mod, an old id of the same mod
    /// (any version; the range stays empty): while this one is present, the
    /// loader never starts the other and names its DLL to delete. Since
    /// minor 54.
    pub const REPLACES: Self = Self(4);
}

/// A relation to another mod, by id and SemVer range.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Dependency, Relation, Str};
///
/// let zones = Dependency {
///     id: Str::new("flame-altar-zones"),
///     range: Str::new(">=0.1, <1.0"),
///     relation: Relation::REQUIRES,
/// };
/// assert_eq!(zones.relation, Relation::REQUIRES);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Dependency {
    /// The other mod's id.
    pub id: Str,
    /// A Cargo-style version requirement, such as `>=0.1, <1.0`; empty for any.
    pub range: Str,
    /// How the two relate.
    pub relation: Relation,
}

/// Flags of a [`Manifest`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::ManifestFlags;
///
/// assert!(ManifestFlags::JOIN.contains(ManifestFlags::JOIN));
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ManifestFlags(pub u32);

impl ManifestFlags {
    /// No flag.
    pub const NONE: Self = Self(0);
    /// Server and clients must run the same version of this mod to play
    /// together; it counts in the join check.
    pub const JOIN: Self = Self(1);
    /// The mod may be reloaded while the game runs, for development: it
    /// keeps no state it cannot hand over, stops its own threads in
    /// `shutdown` and uses only what a reload can unlink (hooks, events,
    /// lookups, settings, logging). Since minor 53; see the modding book.
    pub const RELOAD: Self = Self(2);

    /// Whether every flag of `other` is set.
    ///
    /// # Examples
    ///
    /// See [`ManifestFlags`].
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// Where an [`UpdateSource`] looks for new releases.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::UpdateKind;
///
/// assert_ne!(UpdateKind::GITHUB, UpdateKind::URL);
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UpdateKind(pub u32);

impl UpdateKind {
    /// No source: the mod is updated by hand.
    pub const NONE: Self = Self(0);
    /// GitHub releases of `location`, an `owner/name` repository, whose tags
    /// name `component`'s version, such as `<component>-v1.2.0` as
    /// release-please writes them for a workspace.
    pub const GITHUB: Self = Self(1);
    /// A JSON document at `location`, an `https://` URL, that lists the mod's
    /// versions; `component` is empty. The updater documents the format.
    pub const URL: Self = Self(2);
}

/// Where a mod's new releases come from, declared in its [`Manifest`] with
/// `updates:` in [`manifest!`](crate::manifest).
///
/// The loader only checks and lists it; an updater mod reads it from
/// [`HostApi::mods`] and fetches, compares and stages files itself.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Header, Str, UpdateKind, UpdateSource};
///
/// let safes = UpdateSource {
///     header: Header::of::<UpdateSource>(),
///     kind: UpdateKind::GITHUB,
///     location: Str::new("themaiby/enshwick"),
///     component: Str::new("enshwick-safes"),
/// };
/// assert_eq!(safes.kind, UpdateKind::GITHUB);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct UpdateSource {
    /// Size and version of this struct.
    pub header: Header,
    /// What `location` and `component` mean.
    pub kind: UpdateKind,
    /// The repository, `owner/name`, for [`UpdateKind::GITHUB`]; the URL for
    /// [`UpdateKind::URL`].
    pub location: Str,
    /// The tag prefix before `-v<semver>` for [`UpdateKind::GITHUB`]; empty
    /// otherwise.
    pub component: Str,
}

impl UpdateSource {
    /// No source.
    pub const NONE: Self = Self {
        header: Header::of::<Self>(),
        kind: UpdateKind::NONE,
        location: Str::EMPTY,
        component: Str::EMPTY,
    };

    /// GitHub releases of `repository` whose tags name `component`'s
    /// version, such as `<component>-v1.2.0`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::{UpdateKind, UpdateSource};
    ///
    /// let source = UpdateSource::github("themaiby/enshwick", "enshwick-safes");
    /// assert_eq!(source.kind, UpdateKind::GITHUB);
    /// ```
    #[must_use]
    pub const fn github(repository: &'static str, component: &'static str) -> Self {
        Self {
            kind: UpdateKind::GITHUB,
            location: Str::new(repository),
            component: Str::new(component),
            ..Self::NONE
        }
    }

    /// The JSON document at `url` that lists the mod's versions.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::{UpdateKind, UpdateSource};
    ///
    /// let source = UpdateSource::url("https://example.com/my-mod/latest.json");
    /// assert_eq!(source.kind, UpdateKind::URL);
    /// ```
    #[must_use]
    pub const fn url(url: &'static str) -> Self {
        Self {
            kind: UpdateKind::URL,
            location: Str::new(url),
            ..Self::NONE
        }
    }
}

/// What a mod is, read by the loader before it runs any of the mod's code.
///
/// It is static data returned by `enshwick_mod_query`.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Header, Manifest, ManifestFlags, Side, Str};
///
/// let manifest = Manifest {
///     header: Header::of::<Manifest>(),
///     id: Str::new("flame-altar-count"),
///     version: Str::new("0.1.0"),
///     side: Side::BOTH,
///     flags: ManifestFlags::JOIN,
///     dependencies: core::ptr::null(),
///     dependency_count: 0,
///     builds: core::ptr::null(),
///     build_count: 0,
///     updates: core::ptr::null(),
///     signed_by: [Str::EMPTY; 2],
///     name: Str::new("Flame Altar Count"),
///     description: Str::EMPTY,
///     authors: core::ptr::null(),
///     author_count: 0,
///     license: Str::new("MIT"),
///     homepage: Str::EMPTY,
/// };
/// assert!(manifest.flags.contains(ManifestFlags::JOIN));
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Manifest {
    /// Size and version of this struct.
    pub header: Header,
    /// `[a-z0-9-]+`; names the DLL and the settings file.
    pub id: Str,
    /// The mod's SemVer version.
    pub version: Str,
    /// The sides the mod runs on.
    pub side: Side,
    /// Manifest flags.
    pub flags: ManifestFlags,
    /// Relations to other mods.
    pub dependencies: *const Dependency,
    /// How many entries `dependencies` points at.
    pub dependency_count: usize,
    /// Symbols the mod pins itself, per build.
    pub builds: *const BuildRow,
    /// How many rows `builds` points at.
    pub build_count: usize,
    /// Where new releases come from; null for none. Since minor 5.
    pub updates: *const UpdateSource,
    /// The minisign public keys the author signs releases with, each the
    /// base64 line of a `.pub` file (`RW...`); empty strings for none. An
    /// updater installs a new release of the mod only when one of them
    /// signed it. Two, so an author can rotate keys. Since minor 14.
    pub signed_by: [Str; 2],
    /// The name players read, such as `Survivors per Altar`; empty for the
    /// id. Since minor 36.
    pub name: Str,
    /// One sentence on what the mod does. Since minor 36.
    pub description: Str,
    /// The authors' names. Since minor 36.
    pub authors: *const Str,
    /// How many names `authors` points at. Since minor 36.
    pub author_count: usize,
    /// An SPDX licence expression, such as `MIT OR Apache-2.0`. Since minor
    /// 36.
    pub license: Str,
    /// Where players read about the mod, an `https://` address. Since minor
    /// 36.
    pub homepage: Str,
}

// SAFETY: a manifest is immutable static data; its pointers refer to statics.
unsafe impl Sync for Manifest {}

/// Flags of a [`HookSpec`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::HookFlags;
///
/// let flags = HookFlags(HookFlags::EXCLUSIVE.0 | HookFlags::CALLER_FRAME.0);
/// assert!(flags.contains(HookFlags::CALLER_FRAME));
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HookFlags(pub u32);

impl HookFlags {
    /// No flag.
    pub const NONE: Self = Self(0);
    /// No other mod may link on this target.
    pub const EXCLUSIVE: Self = Self(1);
    /// The link reads the game's call site through
    /// [`HostApi::caller_frame`].
    pub const CALLER_FRAME: Self = Self(2);

    /// Whether every flag of `other` is set.
    ///
    /// # Examples
    ///
    /// See [`HookFlags`].
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// A request to link a detour into the chain of a game function.
///
/// The loader points `next` at the following link or at the original
/// function when it arms the chain; until then `next` holds 0 and the detour
/// is never called.
///
/// # Examples
///
/// ```
/// use core::sync::atomic::AtomicUsize;
/// use enshwick_loader_api::abi::{HookFlags, HookSpec};
///
/// static NEXT: AtomicUsize = AtomicUsize::new(0);
/// extern "system" fn detour() {}
///
/// let spec = HookSpec {
///     target: 0x1_4009_0a60,
///     detour: detour as usize,
///     next: &NEXT,
///     priority: 0,
///     flags: HookFlags::NONE,
/// };
/// assert_eq!(spec.priority, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct HookSpec {
    /// The absolute address of the game function.
    pub target: u64,
    /// The detour, with the target's signature and the `system` ABI.
    pub detour: usize,
    /// A static slot the loader keeps pointing at what comes next.
    pub next: *const AtomicUsize,
    /// Lower runs earlier; equal priorities run in load order.
    pub priority: i32,
    /// Hook flags.
    pub flags: HookFlags,
}

/// The game caller's context at entry of a hook chain: its stack pointer and
/// callee-saved registers, for links that tell callers apart.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::CallerFrame;
///
/// let frame = CallerFrame { rsp: 0x1000, ..CallerFrame::default() };
/// assert_eq!(frame.stack(), 0x1008);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CallerFrame {
    /// The stack pointer at entry; it points at the return address.
    pub rsp: u64,
    /// `rbx` at entry.
    pub rbx: u64,
    /// `rbp` at entry.
    pub rbp: u64,
    /// `rdi` at entry.
    pub rdi: u64,
    /// `rsi` at entry.
    pub rsi: u64,
    /// `r12` at entry.
    pub r12: u64,
    /// `r13` at entry.
    pub r13: u64,
    /// `r14` at entry.
    pub r14: u64,
    /// `r15` at entry.
    pub r15: u64,
}

impl CallerFrame {
    /// The caller's stack above the return address, where it keeps its locals.
    ///
    /// # Examples
    ///
    /// See [`CallerFrame`].
    #[must_use]
    pub const fn stack(&self) -> u64 {
        self.rsp + 8
    }

    /// The address the hooked function returns to, which names its caller.
    ///
    /// # Safety
    ///
    /// The frame was taken in the running chain, so `rsp` points at a live
    /// return address.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::CallerFrame;
    ///
    /// let stack = [0x1_4004_05cb_u64, 0];
    /// let frame = CallerFrame { rsp: stack.as_ptr() as u64, ..CallerFrame::default() };
    /// // SAFETY: `rsp` points at a readable `u64`.
    /// assert_eq!(unsafe { frame.return_address() }, 0x1_4004_05cb);
    /// ```
    #[must_use]
    pub unsafe fn return_address(&self) -> u64 {
        // SAFETY: the caller guarantees `rsp` points at the return address.
        unsafe { (self.rsp as *const u64).read_unaligned() }
    }
}

/// A mod the loader loaded, from [`HostApi::mods`]; the loader itself comes
/// first, with the id `enshwick`.
///
/// Every string stays valid for the life of the process.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Header, ManifestFlags, ModInfo, Side, Str, UpdateSource};
///
/// let safes = ModInfo {
///     header: Header::of::<ModInfo>(),
///     id: Str::new("safes"),
///     version: Str::new("0.1.0"),
///     side: Side::BOTH,
///     flags: ManifestFlags::JOIN,
///     off: 0,
///     file: Str::new("mods/enshwick_safes.dll"),
///     updates: UpdateSource::github("themaiby/enshwick", "enshwick-safes"),
///     signed_by: [Str::new("RWTjqgtfpy3lSOLC4uaUXuvvKSLQKOFf6nUnEytoiFjSzxMEhLnoAdJF"), Str::EMPTY],
///     name: Str::new("Safes"),
///     description: Str::EMPTY,
///     authors: core::ptr::null(),
///     author_count: 0,
///     license: Str::new("MIT"),
///     homepage: Str::EMPTY,
///     replaced_by: Str::EMPTY,
/// };
/// assert_eq!(safes.off, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ModInfo {
    /// Size and version of this struct.
    pub header: Header,
    /// The mod id.
    pub id: Str,
    /// The mod's SemVer version.
    pub version: Str,
    /// The sides the mod runs on.
    pub side: Side,
    /// The manifest's flags; [`ManifestFlags::JOIN`] when it counts in the
    /// join check.
    pub flags: ManifestFlags,
    /// 1 when the mod is off: refused at init or faulted since.
    pub off: u8,
    /// The DLL's path relative to the game's folder, with `/` separators,
    /// such as `mods/enshwick_safes.dll`; the path a staged update goes to.
    pub file: Str,
    /// Where new releases come from; [`UpdateKind::NONE`] for none.
    pub updates: UpdateSource,
    /// The author's minisign public keys from the manifest, checked; empty
    /// strings for none. In [`HostApi::join_missing`], the keys the server
    /// advertises for the mod. Since minor 14.
    pub signed_by: [Str; 2],
    /// The manifest's human metadata, as in [`Manifest`]; in
    /// [`HostApi::join_missing`], the name the server advertises and empty
    /// strings for the rest. Since minor 36.
    pub name: Str,
    /// One sentence on what the mod does. Since minor 36.
    pub description: Str,
    /// The authors' names. Since minor 36.
    pub authors: *const Str,
    /// How many names `authors` points at. Since minor 36.
    pub author_count: usize,
    /// An SPDX licence expression. Since minor 36.
    pub license: Str,
    /// Where players read about the mod. Since minor 36.
    pub homepage: Str,
    /// In [`HostApi::replaced`], the id of the mod that replaces this one;
    /// empty elsewhere. Since minor 54.
    pub replaced_by: Str,
}

impl ModInfo {
    /// A listing with every field empty, to fill with `..ModInfo::EMPTY`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::{ModInfo, Str};
    ///
    /// let safes = ModInfo { id: Str::new("safes"), ..ModInfo::EMPTY };
    /// assert!(safes.version.is_empty());
    /// ```
    pub const EMPTY: Self = Self {
        header: Header::of::<Self>(),
        id: Str::EMPTY,
        version: Str::EMPTY,
        side: Side(0),
        flags: ManifestFlags::NONE,
        off: 0,
        file: Str::EMPTY,
        updates: UpdateSource::NONE,
        signed_by: [Str::EMPTY; 2],
        name: Str::EMPTY,
        description: Str::EMPTY,
        authors: core::ptr::null(),
        author_count: 0,
        license: Str::EMPTY,
        homepage: Str::EMPTY,
        replaced_by: Str::EMPTY,
    };
}

// SAFETY: a listing's strings and its author list stay valid and unchanged
// for the life of the process, as `Str` itself is shared.
unsafe impl Send for ModInfo {}
// SAFETY: as above.
unsafe impl Sync for ModInfo {}

/// An opaque handle the loader gives each mod; every call names its caller
/// with it.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::ModHandle;
///
/// assert_eq!(ModHandle::NONE, ModHandle(0));
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ModHandle(pub usize);

impl ModHandle {
    /// No mod.
    pub const NONE: Self = Self(0);
}

/// The general-purpose registers and flags at a mid-function hook, as the
/// interrupted game code had them.
///
/// The loader saves them before it calls the handlers of a hook placed with
/// [`HostApi::hook_at`] and loads them back afterwards, so a handler that
/// writes a field changes what the game code sees next. `rsp` is read-only:
/// the stack stays where it was.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::Registers;
///
/// let mut registers = Registers { rcx: 7, ..Registers::default() };
/// registers.rax = registers.rcx * 2;
/// assert_eq!(registers.rax, 14);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Registers {
    /// `rax`.
    pub rax: u64,
    /// `rcx`.
    pub rcx: u64,
    /// `rdx`.
    pub rdx: u64,
    /// `rbx`.
    pub rbx: u64,
    /// The stack pointer at the hooked instruction; writes are ignored.
    pub rsp: u64,
    /// `rbp`.
    pub rbp: u64,
    /// `rsi`.
    pub rsi: u64,
    /// `rdi`.
    pub rdi: u64,
    /// `r8`.
    pub r8: u64,
    /// `r9`.
    pub r9: u64,
    /// `r10`.
    pub r10: u64,
    /// `r11`.
    pub r11: u64,
    /// `r12`.
    pub r12: u64,
    /// `r13`.
    pub r13: u64,
    /// `r14`.
    pub r14: u64,
    /// `r15`.
    pub r15: u64,
    /// `rflags`.
    pub rflags: u64,
}

/// A handler of a mid-function hook: the saved registers, writable, and the
/// pointer it was registered with. It must not unwind.
pub type MidHookFn = unsafe extern "system" fn(registers: *mut Registers, user: *mut c_void);

/// A request to run a handler at an instruction inside a game function.
///
/// Every mod's handler on one address runs, by priority and then load order,
/// each seeing the writes of the ones before; then the instructions the jump
/// replaced run and the function goes on.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{HookFlags, MidHookSpec, Registers};
///
/// unsafe extern "system" fn handler(registers: *mut Registers, _user: *mut core::ffi::c_void) {
///     // SAFETY: the loader passes its saved registers, valid for the call.
///     if let Some(registers) = unsafe { registers.as_mut() } {
///         registers.rax = 0;
///     }
/// }
///
/// let spec = MidHookSpec {
///     address: 0x1_4009_0a6c,
///     handler,
///     user: core::ptr::null_mut(),
///     priority: 0,
///     flags: HookFlags::NONE,
/// };
/// assert_eq!(spec.priority, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct MidHookSpec {
    /// The absolute address of the instruction to run the handler before.
    pub address: u64,
    /// The handler.
    pub handler: MidHookFn,
    /// Passed to the handler on every call.
    pub user: *mut c_void,
    /// Lower runs earlier; equal priorities run in load order.
    pub priority: i32,
    /// [`HookFlags::EXCLUSIVE`] or none.
    pub flags: HookFlags,
}

/// One operation of an inventory transaction, in [`InventoryRequest::ops`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::InventoryOp;
///
/// // Everything from a chest (entity 40) to a player (entity 2).
/// let op = InventoryOp {
///     kind: InventoryOp::MOVE_ALL,
///     flags: 0,
///     target: 40,
///     other: 2,
///     item: 0,
///     count: 0,
/// };
/// assert_eq!(op.kind, 5);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct InventoryOp {
    /// What to do: one of the constants on this type.
    pub kind: u32,
    /// [`InventoryOp::INCLUDE_SPECIAL`] or none.
    pub flags: u32,
    /// The inventory entity, or for slot operations the slot reference
    /// (`sub-inventory | index << 32`).
    pub target: u64,
    /// The destination inventory entity of a move; for a give with
    /// [`InventoryOp::FILLED`], the bits of the `f32` fill each new item's
    /// meter starts with; 0 otherwise.
    pub other: u64,
    /// The item id of a give or take.
    pub item: u32,
    /// How many to give or take; for a slot move, how many to move, 0 for the
    /// whole stack.
    pub count: u32,
}

impl InventoryOp {
    /// Adds `count` new items `item` to the inventory `target`.
    pub const GIVE: u32 = 1;
    /// Removes `count` items `item` from the inventory `target`.
    pub const TAKE: u32 = 2;
    /// Destroys the stack in slot `target`.
    pub const DESTROY_SLOT: u32 = 3;
    /// Moves the stack in slot `target`, or `count` of it, into the inventory
    /// `other`.
    pub const MOVE_SLOT: u32 = 4;
    /// Moves every stack of the inventory `target` into the inventory `other`.
    pub const MOVE_ALL: u32 = 5;
    /// For [`InventoryOp::MOVE_ALL`]: also move the source's action bar,
    /// equipment and locked slots.
    pub const INCLUDE_SPECIAL: u32 = 1;
    /// Replaces the stack in slot `target` with `count` new items `item`,
    /// each with its own entity, given to the inventory in `other`'s low
    /// half and asked for the same slot; `other`'s high half holds the
    /// `f32` fill of their meter. Since minor 42.
    pub const RENEW: u32 = 6;
    /// For [`InventoryOp::GIVE`]: `other` holds the fill of each new item's
    /// meter, the game's own per-item fuel fill. Since minor 37.
    pub const FILLED: u32 = 2;
}

/// Inventory operations to run in one transaction, for
/// [`HostApi::inventory_submit`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Header, InventoryOp, InventoryRequest};
///
/// let ops = [InventoryOp { kind: InventoryOp::GIVE, target: 2, item: 0x1234, count: 5, ..Default::default() }];
/// let request = InventoryRequest {
///     header: Header::of::<InventoryRequest>(),
///     flags: InventoryRequest::NOTIFY,
///     actor: 2,
///     ops: ops.as_ptr(),
///     op_count: 1,
///     reserved: 0,
/// };
/// assert_eq!(request.op_count, 1);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct InventoryRequest {
    /// Size and version of this struct.
    pub header: Header,
    /// [`InventoryRequest::ALLOW_PARTIAL`], [`InventoryRequest::NOTIFY`].
    pub flags: u32,
    /// The player the game credits with the change, for pickup and
    /// inventory-full messages; 0 for whichever player the game is serving.
    pub actor: u32,
    /// The operations, read during the call.
    pub ops: *const InventoryOp,
    /// How many operations `ops` holds.
    pub op_count: u32,
    /// Zero.
    pub reserved: u32,
}

impl InventoryRequest {
    /// Commit even when a give or take leaves some of its count undone.
    pub const ALLOW_PARTIAL: u32 = 1;
    /// Show the actor the pickup and inventory-full messages.
    pub const NOTIFY: u32 = 2;
}

/// The outcome of one operation in an [`InventoryResult`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::InventoryOpResult;
///
/// let done = InventoryOpResult { error: 0, remaining: 0 };
/// assert_ne!(done.error, InventoryOpResult::NOT_RUN);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct InventoryOpResult {
    /// The game's error code, 0 for none, or [`InventoryOpResult::NOT_RUN`].
    pub error: u32,
    /// How much of the count the operation could not do.
    pub remaining: u32,
}

impl InventoryOpResult {
    /// The operation did not run: one before it failed.
    pub const NOT_RUN: u32 = 0x100;
}

/// What became of an [`InventoryRequest`], passed to its
/// [`InventoryResultFn`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::InventoryResult;
///
/// assert_ne!(InventoryResult::COMMITTED, InventoryResult::ROLLED_BACK);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct InventoryResult {
    /// Size and version of this struct.
    pub header: Header,
    /// One of the constants on this type.
    pub outcome: u32,
    /// How many results `ops` holds: one per operation, or none when the
    /// request never ran.
    pub op_count: u32,
    /// The operations' results, valid for the call.
    pub ops: *const InventoryOpResult,
}

impl InventoryResult {
    /// Every operation succeeded and the transaction was committed.
    pub const COMMITTED: u32 = 0;
    /// An operation failed or fell short; the transaction was rolled back
    /// and nothing changed.
    pub const ROLLED_BACK: u32 = 1;
    /// The request could not run: this game build lacks a piece, or its hook
    /// was not armed. Nothing changed.
    pub const UNAVAILABLE: u32 = 2;
    /// The game did not run inventories in time, as with nobody online.
    /// Nothing changed.
    pub const TIMED_OUT: u32 = 3;
}

/// Receives the [`InventoryResult`] of a request, on the server's game thread
/// after a simulation step; it must not unwind.
pub type InventoryResultFn =
    unsafe extern "system" fn(result: *const InventoryResult, user: *mut c_void);

/// An item's static data, from [`HostApi::item_info`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Header, ItemData};
///
/// let sword = ItemData {
///     header: Header::of::<ItemData>(),
///     id: 7,
///     max_stack: 1,
///     category: 2,
///     rarity: 0,
///     flags: ItemData::HAS_PIDE,
/// };
/// assert_ne!(sword.flags & ItemData::HAS_PIDE, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ItemData {
    /// Size and version of this struct.
    pub header: Header,
    /// The item id: FNV-1a of the item's GUID.
    pub id: u32,
    /// How many fit one stack.
    pub max_stack: u16,
    /// The game's `ItemCategory`: 1 equipment, 2 weapons, 3 tools and so
    /// on. Since minor 48; zero before.
    pub category: u8,
    /// The game's `ItemRarity` of the type. Since minor 48; zero before.
    pub rarity: u8,
    /// `ItemInfo` flags: [`ItemData::HAS_PIDE`] and the others on this type.
    pub flags: u32,
}

impl ItemData {
    /// Each item has an entity of its own, with durability or a level.
    pub const HAS_PIDE: u32 = 1 << 1;
    /// The item is a container.
    pub const IS_CONTAINER: u32 = 1 << 2;
    /// The item is a blueprint.
    pub const IS_BLUEPRINT: u32 = 1 << 3;
    /// Each item keeps a count the game shows as a bar on its slot, as the
    /// water pouch keeps its water (the game's fuel).
    pub const USES_FUEL: u32 = 1 << 13;
}

/// A box of whole cells or of 32.32 fixed-point metres, as its user says:
/// `min` inclusive, `max` exclusive on each axis; axis 1 is up.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::Bounds;
///
/// let zone = Bounds { min: [0, 0, 0], max: [80, 80, 80] };
/// assert_eq!(zone.max[0] - zone.min[0], 80);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Bounds {
    /// The inclusive lower corner.
    pub min: [i64; 3],
    /// The exclusive upper corner.
    pub max: [i64; 3],
}

/// One Flame Altar of the server's world, from [`HostApi::altars`].
///
/// Zones are in whole 0.5 m cells, rounded outwards, as the engine builds
/// them from the altar's trigger shape.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{AltarInfo, Bounds, Header};
///
/// let mut altar = AltarInfo::EMPTY;
/// altar.entity = 4096;
/// altar.level = 1;
/// altar.level_count = 1;
/// altar.zone = Bounds { min: [0, 0, 0], max: [80, 80, 80] };
/// altar.levels[0] = altar.zone;
/// assert_eq!(altar.header, Header::of::<AltarInfo>());
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AltarInfo {
    /// Size and version of this struct.
    pub header: Header,
    /// The altar's entity.
    pub entity: u32,
    /// Its Flame level; 0 until the engine has reported it.
    pub level: u8,
    /// 1 when the altar is tilted off the vertical: its zone is known, its
    /// level zones are not, and it keeps the engine's size.
    pub tilted: u8,
    /// How many entries of `levels` hold a zone.
    pub level_count: u8,
    /// Zero.
    pub reserved: u8,
    /// The zone as the engine has built it now, growth by any mod included.
    pub zone: Bounds,
    /// The vanilla zone at each level from 1 to `level_count`.
    pub levels: [Bounds; 16],
    /// Where the altar stands, in metres.
    pub position: [f64; 3],
}

impl AltarInfo {
    /// An altar with nothing known, its header set.
    pub const EMPTY: Self = Self {
        header: Header::of::<Self>(),
        entity: 0,
        level: 0,
        tilted: 0,
        level_count: 0,
        reserved: 0,
        zone: Bounds {
            min: [0; 3],
            max: [0; 3],
        },
        levels: [Bounds {
            min: [0; 3],
            max: [0; 3],
        }; 16],
        position: [0.0; 3],
    };
}

/// The size a mod that controls altar zones gives one altar, for
/// [`HostApi::set_altar_zones`]: its vanilla zone at `level` grown by `grow`
/// cells on every horizontal side and made `height` cells tall. The loader
/// applies it while the altar is at `level`, the vanilla size otherwise.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::AltarZoneSize;
///
/// let size = AltarZoneSize { altar: 4096, level: 2, reserved: [0; 3], grow: 6, height: 120 };
/// assert_eq!(size.grow, 6);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AltarZoneSize {
    /// The altar's entity.
    pub altar: u32,
    /// The level the size was worked out for.
    pub level: u8,
    /// Zero.
    pub reserved: [u8; 3],
    /// Cells added on every horizontal side, at least 0.
    pub grow: i64,
    /// The zone's height in cells.
    pub height: i64,
}

/// A placement the game refused because the object's box does not lie in a
/// single building zone, as [`HostApi::zone_check_filter`] hands it on.
/// Coordinates are 32.32 fixed-point metres.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Bounds, Header, ZoneCheck};
///
/// let check = ZoneCheck {
///     header: Header::of::<ZoneCheck>(),
///     zones: core::ptr::null(),
///     zone_count: 0,
///     object: Bounds::default(),
///     margin: 0,
/// };
/// assert_eq!(check.zone_count, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ZoneCheck {
    /// Size and version of this struct.
    pub header: Header,
    /// The building zones, half-open boxes, valid for the call.
    pub zones: *const Bounds,
    /// How many boxes `zones` holds.
    pub zone_count: usize,
    /// The object's box, both corners included.
    pub object: Bounds,
    /// How far the game lets an object reach past a zone, on every side.
    pub margin: i64,
}

/// A mod's zone check filter: the check and the pointer given at
/// registration; returns 1 to accept the placement after all.
pub type ZoneCheckFn = unsafe extern "system" fn(check: *const ZoneCheck, user: *mut c_void) -> u8;

/// A game system asking which entity is a survivor, as
/// [`HostApi::survivor_lookup`] hands it to the mod that answers.
///
/// The game keeps one entity per survivor; a mod that keeps several, such as
/// one per base, answers each asker with the one that suits it. Cells are
/// whole 0.5 m cells.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Header, SurvivorLookup};
///
/// let lookup = SurvivorLookup {
///     header: Header::of::<SurvivorLookup>(),
///     survivor: 0x47a1_5e4a,
///     vanilla: 4096,
///     purpose: SurvivorLookup::SPAWN,
///     entity: 0,
///     cell: [10, 0, 10],
///     bed_of: None,
///     token: core::ptr::null_mut(),
/// };
/// assert_eq!(lookup.purpose, SurvivorLookup::SPAWN);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SurvivorLookup {
    /// Size and version of this struct.
    pub header: Header,
    /// The survivor's id.
    pub survivor: u32,
    /// The game's own answer, 0 for none.
    pub vanilla: u32,
    /// Who asks; one of the constants on this type.
    pub purpose: u32,
    /// The entity the asker works on, as its purpose says; 0 for none.
    pub entity: u32,
    /// The cell its purpose names.
    pub cell: [i64; 3],
    /// The bed an entity sleeps in, 0 for none; valid during the call with
    /// `token`.
    pub bed_of: Option<unsafe extern "system" fn(token: *mut c_void, entity: u32) -> u32>,
    /// The argument of `bed_of`.
    pub token: *mut c_void,
}

impl SurvivorLookup {
    /// The pass that keeps one entity per survivor: `entity` is the one it
    /// looks at, `cell` its home. Answer `entity` to keep it, or another
    /// entity, which makes the game remove `entity`.
    pub const UNIQUENESS: u32 = 1;
    /// A summoning staff used at `cell`: answer the entity to move there, or
    /// 0 to spawn a new one.
    pub const SPAWN: u32 = 2;
    /// A bed at `cell` binding its sleeper: answer the entity that may sleep
    /// there, 0 for none.
    pub const BED_OWNER: u32 = 3;
    /// A player giving bed `entity` at `cell` to the survivor with its staff:
    /// answer the entity that gets it.
    pub const ASSIGN_BED: u32 = 4;
    /// The same assignment asking who slept in bed `entity` at `cell` before:
    /// answer that entity, 0 for none.
    pub const PREVIOUS_OCCUPANT: u32 = 5;
    /// A survivor's idle behaviour: `entity` is the survivor, `cell` where it
    /// stands; answer the entity whose base it keeps to.
    pub const ROAMING: u32 = 6;
    /// The first binding of beds after a load, before beds bind their
    /// sleepers.
    pub const BED_BINDING: u32 = 7;
    /// The cursor measuring how far a bed lies from the survivor before it
    /// offers to assign it; 0 skips the measure.
    pub const CURSOR: u32 = 8;
}

/// The mod's survivor lookup: the call and the pointer given at
/// registration; returns 1 with the answer in `out`, or 0 for the game's own.
pub type SurvivorLookupFn =
    unsafe extern "system" fn(call: *const SurvivorLookup, user: *mut c_void, out: *mut u32) -> u8;

/// A survivor's own check that its bed is still its, after the game kept it,
/// as [`HostApi::survivor_bed_check`] hands it on. Cells are whole 0.5 m
/// cells.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{BedCheck, Header};
///
/// let check = BedCheck { header: Header::of::<BedCheck>(), survivor: 7, entity: 4096, bed: 4100, home: [0; 3], bed_at: [4, 0, 4] };
/// assert_eq!(check.bed, 4100);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BedCheck {
    /// Size and version of this struct.
    pub header: Header,
    /// The survivor's id.
    pub survivor: u32,
    /// The survivor's entity.
    pub entity: u32,
    /// The bed.
    pub bed: u32,
    /// Where the survivor belongs: its saved spawn cell, or where it stands.
    pub home: [i64; 3],
    /// Where the bed stands.
    pub bed_at: [i64; 3],
}

/// A filter of survivors' beds: returns 0 to make the game clear the bed.
pub type BedCheckFn = unsafe extern "system" fn(check: *const BedCheck, user: *mut c_void) -> u8;

/// A summoning staff's use about to spawn or move a survivor, as
/// [`HostApi::summon_filter`] hands it on, and once it worked, as
/// [`HostApi::summoned`] does.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Header, Summon};
///
/// let summon = Summon {
///     header: Header::of::<Summon>(),
///     item: 0x5e01,
///     ammo: 0,
///     template: [0; 16],
///     target: [0; 3],
///     slot: 0x3_0000_0041,
///     pide: 900,
///     meter: [2, 3],
///     consumed: 0,
///     reserved: [0; 3],
/// };
/// assert_eq!(summon.meter, [2, 3]);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Summon {
    /// Size and version of this struct.
    pub header: Header,
    /// The item used.
    pub item: u32,
    /// The ammunition it used, 0 for none.
    pub ammo: u32,
    /// The template GUID it places, in file order.
    pub template: [u8; 16],
    /// The target cell.
    pub target: [i64; 3],
    /// The slot the used item lies in, `sub-inventory | index << 32`.
    /// Since minor 37.
    pub slot: u64,
    /// The used item's own entity, 0 for an item without one. Since minor
    /// 37.
    pub pide: u32,
    /// The used item's meter as clients see it, current and maximum;
    /// `[0, 0]` for an item without one. Since minor 37.
    pub meter: [u16; 2],
    /// After the use: 1 when it took the item. Since minor 37.
    pub consumed: u8,
    /// Zero.
    pub reserved: [u8; 3],
}

impl Summon {
    /// A filter's answer: the use takes the item.
    pub const TAKE: u8 = 1;
    /// A filter's answer: the use is refused; nothing spawns, moves or is
    /// taken. Since minor 42.
    pub const REFUSE: u8 = 2;
}

/// A summon filter: returns [`Summon::TAKE`] to make the game take the
/// item in the same transaction, and only when the spawn or move works,
/// [`Summon::REFUSE`] to refuse the use, or 0 to leave it.
pub type SummonFn = unsafe extern "system" fn(summon: *const Summon, user: *mut c_void) -> u8;

/// The client's survivor menu merging the statuses of every entity of one
/// survivor: the survivor, `count` status bytes at `statuses`, valid for the
/// call, and the pointer given at registration; returns
/// [`SURVIVOR_STATUS_SET`] or-ed with the byte to show, or 0 to leave the
/// game's.
pub type SurvivorStatusFn = unsafe extern "system" fn(
    survivor: u32,
    statuses: *const u8,
    count: usize,
    user: *mut c_void,
) -> u32;

/// The bit of a [`SurvivorStatusFn`] answer that says its low byte is the
/// status to show.
pub const SURVIVOR_STATUS_SET: u32 = 0x100;

/// A player about to interact with an entity, as
/// [`HostApi::interaction_filter`] hands it on: open it, move items in or out
/// of it, sort it, craft or build from it, dismantle it, or flip its Quick
/// Stash or upgrade switch.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Header, Interaction};
///
/// let call = Interaction {
///     header: Header::of::<Interaction>(),
///     kind: Interaction::OPEN,
///     player: 4096,
///     target: 5000,
///     item: 0x5e01,
///     known: 1,
///     reserved: [0; 3],
///     position: [1.0, 2.0, 3.0],
/// };
/// assert_eq!(call.kind, Interaction::OPEN);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Interaction {
    /// Size and version of this struct.
    pub header: Header,
    /// What the player does; one of the constants on this type.
    pub kind: u32,
    /// The player's character entity.
    pub player: u32,
    /// The entity they act on: a chest, a placed piece, the inventory a slot
    /// belongs to.
    pub target: u32,
    /// The item the target was placed from (`UsedItem`), 0 when unknown.
    pub item: u32,
    /// 1 when `item` and `position` were read from the target.
    pub known: u8,
    /// Zero.
    pub reserved: [u8; 3],
    /// Where the target stands, in metres, when `known`.
    pub position: [f64; 3],
}

impl Interaction {
    /// The game offers to open the target.
    pub const OPEN: u32 = 1;
    /// Items move out of or into the target.
    pub const MOVE: u32 = 2;
    /// The target's inventory is sorted.
    pub const SORT: u32 = 3;
    /// The target lends its stock to crafting or building nearby; asked
    /// without the target's item or position.
    pub const STOCK: u32 = 4;
    /// The cursor offers to pick up or tear down the target.
    pub const DISMANTLE: u32 = 5;
    /// The player flips the target's Quick Stash switch.
    pub const QUICK_STASH: u32 = 6;
    /// The player upgrades the target into another piece.
    pub const UPGRADE: u32 = 7;
}

/// A mod's interaction filter: the call and the pointer given at
/// registration; returns 0 to refuse, which the game handles as its own
/// refusal, so nothing moves and nothing is lost.
pub type InteractionFn =
    unsafe extern "system" fn(call: *const Interaction, user: *mut c_void) -> u8;

/// The game asking whether a character unlocked a skill node, as
/// [`HostApi::skill_override`] hands it on.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Header, SkillCheck};
///
/// let check = SkillCheck { header: Header::of::<SkillCheck>(), node: 0x4475_e8dd, gate: SkillCheck::FLY_MODE, unlocked: 0, reserved: [0; 3] };
/// assert_eq!(check.gate, SkillCheck::FLY_MODE);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SkillCheck {
    /// Size and version of this struct.
    pub header: Header,
    /// The skill node.
    pub node: u32,
    /// The gate that asks, one of the constants on this type.
    pub gate: u32,
    /// 1 when the character unlocked it.
    pub unlocked: u8,
    /// Zero.
    pub reserved: [u8; 3],
}

impl SkillCheck {
    /// A caller the platform does not name.
    pub const OTHER: u32 = 0;
    /// The building fly mode gate, on both sides.
    pub const FLY_MODE: u32 = 1;
    /// A [`SkillCheckFn`] answer: the game's own.
    pub const VANILLA: u8 = 0;
    /// A [`SkillCheckFn`] answer: the skill counts as unlocked.
    pub const UNLOCKED: u8 = 1;
    /// A [`SkillCheckFn`] answer: the skill counts as locked.
    pub const LOCKED: u8 = 2;
}

/// A skill override: the check and the pointer given at registration;
/// returns [`SkillCheck::VANILLA`], [`SkillCheck::UNLOCKED`] or
/// [`SkillCheck::LOCKED`].
pub type SkillCheckFn =
    unsafe extern "system" fn(check: *const SkillCheck, user: *mut c_void) -> u8;

/// A comfort provider in a base registering its comfort for its category,
/// as [`HostApi::comfort_filter`] hands it on.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Comfort, Header};
///
/// let comfort = Comfort { header: Header::of::<Comfort>(), category: 3, value: 5, position: [0.0; 3] };
/// assert_eq!(comfort.value, 5);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Comfort {
    /// Size and version of this struct.
    pub header: Header,
    /// The comfort category; a base keeps the best provider per category.
    pub category: u32,
    /// The comfort it registers.
    pub value: i32,
    /// Where the provider stands, in metres.
    pub position: [f32; 3],
}

/// A comfort filter: the registration and the pointer given at
/// registration; returns the comfort to register.
pub type ComfortFn = unsafe extern "system" fn(comfort: *const Comfort, user: *mut c_void) -> i32;

/// One character's experience before the game applies it, in an
/// [`ExperiencePass`]. Its layout is frozen: it is passed as an array.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::ExperienceRow;
///
/// let row = ExperienceRow { entity: 4096, level: 3, progress: 10, need: 100, gained: 5 };
/// assert_eq!(row.gained, 5);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExperienceRow {
    /// The character's entity.
    pub entity: u32,
    /// Its level.
    pub level: u32,
    /// Experience inside the level.
    pub progress: u32,
    /// What the level needs.
    pub need: u32,
    /// Gained experience the game applies next, at most one level a tick;
    /// a filter may change it.
    pub gained: u32,
}

/// Every spawned character's experience before the server applies it, as
/// [`HostApi::experience_filter`] hands it on.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{ExperiencePass, Header};
///
/// let pass = ExperiencePass { header: Header::of::<ExperiencePass>(), rows: core::ptr::null_mut(), row_count: 0, level_cap: 25, need_for: None, token: core::ptr::null_mut() };
/// assert_eq!(pass.level_cap, 25);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ExperiencePass {
    /// Size and version of this struct.
    pub header: Header,
    /// The characters, valid and writable for the call.
    pub rows: *mut ExperienceRow,
    /// How many rows there are.
    pub row_count: usize,
    /// The highest level.
    pub level_cap: u32,
    /// What level `level` (1 or more) needs; valid during the call with
    /// `token`.
    pub need_for: Option<unsafe extern "system" fn(token: *mut c_void, level: u32) -> u32>,
    /// The argument of `need_for`.
    pub token: *mut c_void,
}

/// An experience filter: the pass and the pointer given at registration.
pub type ExperiencePassFn = unsafe extern "system" fn(pass: *mut ExperiencePass, user: *mut c_void);

/// A mod's altar limit: the Flame level and the pointer given at
/// registration; returns how many altars that level allows.
pub type AltarLimitFn = unsafe extern "system" fn(flame_level: u32, user: *mut c_void) -> u8;

/// A game event handed to a [`GameEventFn`], from the ECS event streams
/// of the server's systems.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{GameEventData, Header};
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
#[derive(Clone, Copy, Debug)]
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

/// A mod's handler for game events: the event and the pointer given at
/// subscription. It must not unwind.
pub type GameEventFn = unsafe extern "system" fn(event: *const GameEventData, user: *mut c_void);

/// An entity placed in the world, from [`HostApi::entities`].
///
/// Like [`PlayerInfo`], its layout is frozen: [`HostApi::entities`] writes
/// an array of it with no entry size.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::EntityPlace;
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

/// A request to move an entity, for [`HostApi::teleport`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Header, TeleportRequest};
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
#[derive(Clone, Copy, Debug, PartialEq)]
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

impl TeleportRequest {
    /// The game looks for a free spot near `position` to stand on, as it
    /// does for respawns.
    pub const BEST_SPAWN: u32 = 1;
    /// The entity arrives facing `orientation`.
    pub const ORIENTED: u32 = 2;
}

/// A request to spawn an entity from a template, for [`HostApi::spawn`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Header, SpawnRequest};
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
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpawnRequest {
    /// Size and version of this struct.
    pub header: Header,
    /// The template GUID, in the order [`HostApi::entity_template`] writes.
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

/// A player's craft request, handed to the filters registered with
/// [`HostApi::craft_filter`] before the server crafts.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{CraftCheck, Header};
///
/// let craft = CraftCheck { header: Header::of::<CraftCheck>(), player: 2, station: 0, recipe: 0x51, amount: 3 };
/// assert_eq!(craft.station, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

/// A mod's craft filter: the request and the pointer given at registration;
/// returns 1 to let it through, 0 to refuse it.
pub type CraftCheckFn = unsafe extern "system" fn(call: *const CraftCheck, user: *mut c_void) -> u8;

/// A placement the game's cursor check allows, handed to the filters
/// registered with [`HostApi::build_filter`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{BuildCheck, Header};
///
/// let wall = BuildCheck { header: Header::of::<BuildCheck>(), player: 2, item: 0x1d, position: [0; 3] };
/// assert_eq!(wall.item, 0x1d);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

/// A mod's build filter: the placement and the pointer given at
/// registration; returns 1 to let it through, 0 to refuse it.
pub type BuildCheckFn = unsafe extern "system" fn(call: *const BuildCheck, user: *mut c_void) -> u8;

/// A message on a mod's channel, handed to the handler registered with
/// [`HostApi::net_channel`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Header, NetMessage};
///
/// let bytes = [1u8, 2, 3];
/// let message = NetMessage {
///     header: Header::of::<NetMessage>(),
///     channel: 0x1234,
///     reserved: 0,
///     peer: NetMessage::SERVER,
///     data: bytes.as_ptr(),
///     size: 3,
///     reserved_tail: 0,
/// };
/// assert_eq!(message.peer, NetMessage::SERVER);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct NetMessage {
    /// Size and version of this struct.
    pub header: Header,
    /// The channel, as [`HostApi::net_channel`] wrote it.
    pub channel: u32,
    /// Zero.
    pub reserved: u32,
    /// Who sent it: a player's account key on the server,
    /// [`NetMessage::SERVER`] on a client.
    pub peer: u64,
    /// The message, valid for the call.
    pub data: *const u8,
    /// How many bytes `data` holds.
    pub size: u32,
    /// Zero.
    pub reserved_tail: u32,
}

impl NetMessage {
    /// The server, as a peer: what a client hears from and sends to.
    pub const SERVER: u64 = u64::MAX;
}

/// A mod's handler for messages on its channel: the message and the pointer
/// given at registration. It must not unwind.
pub type NetMessageFn = unsafe extern "system" fn(message: *const NetMessage, user: *mut c_void);

/// The clock of the server's world, written by [`HostApi::time_of_day`].
///
/// Times are game nanoseconds of the day, 0 up to 24 hours; lengths are
/// real nanoseconds.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{DaytimeInfo, Header};
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
#[derive(Clone, Copy, Debug, PartialEq)]
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
    /// times every mod's factor ([`HostApi::set_time_speed`]).
    pub speed: f32,
    /// [`DaytimeInfo::DAY`] or none.
    pub flags: u32,
}

impl DaytimeInfo {
    /// It is day.
    pub const DAY: u32 = 1;
}

/// Damage about to reach an entity's health, handed to the filters
/// registered with [`HostApi::damage_filter`], which may change it.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{DamageCheck, Header};
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
#[derive(Clone, Copy, Debug, PartialEq)]
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

/// A stat kind's definition, for [`HostApi::stat_open`] and
/// [`HostApi::stat_info`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Header, StatDefData, Str};
///
/// let crit = StatDefData {
///     header: Header::of::<StatDefData>(),
///     ty: StatDefData::ADDITIVE,
///     format: StatDefData::PERCENT,
///     reserved: 0,
///     default: 0.0,
///     min: 0.0,
///     max: 1.0,
///     text: Str::new("stat.crit"),
/// };
/// assert!(crit.min <= crit.default && crit.default <= crit.max);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct StatDefData {
    /// Size and version of this struct.
    pub header: Header,
    /// [`StatDefData::ADDITIVE`], [`StatDefData::MULTIPLICATIVE`] or
    /// [`StatDefData::FLAG`].
    pub ty: u8,
    /// How the value reads: [`StatDefData::NUMBER`],
    /// [`StatDefData::SIGNED`] or [`StatDefData::PERCENT`].
    pub format: u8,
    /// Zero.
    pub reserved: u16,
    /// The value without modifiers.
    pub default: f32,
    /// The least value.
    pub min: f32,
    /// The greatest value.
    pub max: f32,
    /// The text key that names the stat, in its mod's texts.
    pub text: Str,
}

impl StatDefData {
    /// A sum of points, such as extra health.
    pub const ADDITIVE: u8 = 0;
    /// A factor, 1 without modifiers, such as a damage multiplier.
    pub const MULTIPLICATIVE: u8 = 1;
    /// On or off: any value but 0 is on.
    pub const FLAG: u8 = 2;
    /// A plain number.
    pub const NUMBER: u8 = 0;
    /// A number with its sign.
    pub const SIGNED: u8 = 1;
    /// A fraction shown as a percentage.
    pub const PERCENT: u8 = 2;
}

/// A stat kind as the loader numbers it: its index for calls and its key,
/// the FNV-1a 32 hash of its full id, for what is stored.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::StatRef;
///
/// let health = StatRef { index: 0, key: 0x1234 };
/// assert_eq!(health.index, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct StatRef {
    /// The index in this process.
    pub index: u32,
    /// The FNV-1a 32 hash of the full id, the same in every process.
    pub key: u32,
}

/// One modifier of a stat, by index; written as arrays.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::StatModifierData;
///
/// let more = StatModifierData { stat: 0, op: StatModifierData::ADD, reserved: [0; 3], value: 50.0 };
/// assert_eq!(more.op, StatModifierData::ADD);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StatModifierData {
    /// The stat's index.
    pub stat: u32,
    /// [`StatModifierData::ADD`], [`StatModifierData::MULTIPLY`],
    /// [`StatModifierData::MAX`] or [`StatModifierData::OVERRIDE`].
    pub op: u8,
    /// Zero.
    pub reserved: [u8; 3],
    /// The modifier's value.
    pub value: f32,
}

impl StatModifierData {
    /// Adds the value.
    pub const ADD: u8 = 0;
    /// Multiplies by the value.
    pub const MULTIPLY: u8 = 1;
    /// Raises the result to at least the value.
    pub const MAX: u8 = 2;
    /// Sets the result.
    pub const OVERRIDE: u8 = 3;
}

/// Sets or removes a mod's modifier on an entity, for
/// [`HostApi::stat_modify`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Header, StatModifierData, StatModify};
///
/// let blessing = StatModify {
///     header: Header::of::<StatModify>(),
///     entity: 2,
///     key: 7,
///     modifier: StatModifierData { stat: 0, op: StatModifierData::ADD, reserved: [0; 3], value: 50.0 },
///     flags: 0,
///     duration_ms: 60_000,
/// };
/// assert_eq!(blessing.flags & StatModify::REMOVE, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StatModify {
    /// Size and version of this struct.
    pub header: Header,
    /// The entity.
    pub entity: u32,
    /// The mod's own key; a modifier with the same mod, key and stat
    /// replaces the one before.
    pub key: u32,
    /// The modifier.
    pub modifier: StatModifierData,
    /// [`StatModify::REMOVE`], [`StatModify::SAVED`].
    pub flags: u32,
    /// How long it lasts in milliseconds; 0 until removed.
    pub duration_ms: u64,
}

impl StatModify {
    /// Removes the modifier instead.
    pub const REMOVE: u32 = 1;
    /// Kept with the world for the player's account, and given back when
    /// the player joins again; players only.
    pub const SAVED: u32 = 1 << 1;
}

/// A stat whose value changed on an entity, for the handlers registered
/// with [`HostApi::stats_changed`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Header, StatChange, StatRef};
///
/// let change = StatChange { header: Header::of::<StatChange>(), entity: 2, stat: StatRef { index: 0, key: 0x1234 }, old: 0.0, new: 50.0, reserved: 0 };
/// assert!(change.new > change.old);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StatChange {
    /// Size and version of this struct.
    pub header: Header,
    /// The entity.
    pub entity: u32,
    /// The stat.
    pub stat: StatRef,
    /// The value before.
    pub old: f32,
    /// The value now.
    pub new: f32,
    /// Zero.
    pub reserved: u32,
}

/// A handler of stat changes: the change and the pointer given at
/// registration. It runs on the game thread, after a step.
pub type StatChangeFn = unsafe extern "system" fn(change: *const StatChange, user: *mut c_void);

/// One modifier that counts toward an entity's stat, and where it comes
/// from, written by [`HostApi::stat_sources`] as an array.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{StatModifierData, StatSource};
///
/// let ring = StatSource { kind: StatSource::ITEM, op: StatModifierData::ADD, reserved: 0, stat: 0, value: 20.0, id: 900, owner: 0x5e01 };
/// assert_eq!(ring.kind, StatSource::ITEM);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StatSource {
    /// [`StatSource::ITEM`], [`StatSource::BUFF`] or
    /// [`StatSource::ENTITY`].
    pub kind: u8,
    /// The operation, as [`StatModifierData::op`].
    pub op: u8,
    /// Zero.
    pub reserved: u16,
    /// The stat's index.
    pub stat: u32,
    /// The modifier's value.
    pub value: f32,
    /// The item's entity, the buff id or the modifier's key.
    pub id: u32,
    /// The item id, 0, or the index of the mod that set the modifier.
    pub owner: u32,
}

/// An affix's definition, for [`HostApi::affix_open`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{AffixDefData, AffixModifierData, Header, StatModifierData, Str};
///
/// let ranges = [AffixModifierData { stat: 0, op: StatModifierData::ADD, reserved: [0; 3], min: 10.0, max: 30.0, step: 1.0 }];
/// let sturdy = AffixDefData {
///     header: Header::of::<AffixDefData>(),
///     text: Str::new("affix.sturdy"),
///     group: Str::EMPTY,
///     modifiers: ranges.as_ptr(),
///     modifier_count: 1,
///     weight: 100,
///     categories: 1 << 1,
///     rarity: 0,
///     reserved: [0; 3],
/// };
/// assert_eq!(sturdy.modifier_count, 1);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AffixDefData {
    /// Size and version of this struct.
    pub header: Header,
    /// The text key that names the affix, in its mod's texts.
    pub text: Str,
    /// Affixes of one group never roll together on an item; empty for none.
    pub group: Str,
    /// Its modifiers' ranges.
    pub modifiers: *const AffixModifierData,
    /// How many.
    pub modifier_count: u32,
    /// How likely it rolls against the others.
    pub weight: u32,
    /// The item categories it rolls on, a bit per `ItemCategory`.
    pub categories: u32,
    /// Its rarity, 0 (common) to 4 (legendary).
    pub rarity: u8,
    /// Zero.
    pub reserved: [u8; 3],
}

/// One modifier of an affix with the range its value rolls in; read as
/// arrays.
///
/// # Examples
///
/// See [`AffixDefData`].
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AffixModifierData {
    /// The stat's index.
    pub stat: u32,
    /// The operation, as [`StatModifierData::op`].
    pub op: u8,
    /// Zero.
    pub reserved: [u8; 3],
    /// The least value.
    pub min: f32,
    /// The greatest value.
    pub max: f32,
    /// The value rolls in steps of this from `min`; 0 for any value.
    pub step: f32,
}

/// Rolls affixes onto an item, for [`HostApi::affix_roll`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{AffixRoll, Header};
///
/// let roll = AffixRoll { header: Header::of::<AffixRoll>(), pide: 900, item: 7, min: 1, max: 2, max_rarity: 4, flags: AffixRoll::ONCE, seed: 42 };
/// assert!(roll.min <= roll.max);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AffixRoll {
    /// Size and version of this struct.
    pub header: Header,
    /// The item's entity.
    pub pide: u32,
    /// The item id it holds.
    pub item: u32,
    /// The fewest affixes.
    pub min: u8,
    /// The most affixes.
    pub max: u8,
    /// The rarest an affix may be, 0 to 4.
    pub max_rarity: u8,
    /// [`AffixRoll::ONCE`].
    pub flags: u8,
    /// The seed of the roll: the same seed, item and affixes give the same
    /// result.
    pub seed: u64,
}

impl AffixRoll {
    /// Rolls only an item that never rolled; others answer
    /// [`Status::REFUSED`].
    pub const ONCE: u8 = 1;
}

/// One line of what an item gives, written by [`HostApi::item_lines`] as an
/// array.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{ItemLine, StatModifierData};
///
/// let line = ItemLine { modifier: StatModifierData { stat: 0, op: StatModifierData::ADD, reserved: [0; 3], value: 20.0 }, affix: 0 };
/// assert_eq!(line.affix, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ItemLine {
    /// The modifier.
    pub modifier: StatModifierData,
    /// The index of the affix it comes from, plus one; 0 for the item's own.
    pub affix: u32,
}

impl StatSource {
    /// An item the entity wears.
    pub const ITEM: u8 = 0;
    /// A buff the entity has.
    pub const BUFF: u8 = 1;
    /// A modifier a mod set on the entity.
    pub const ENTITY: u8 = 2;
}

/// A buff or debuff type of the running game, written by
/// [`HostApi::buff_types`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{BuffTypeInfo, Header};
///
/// let rested = BuffTypeInfo {
///     header: Header::of::<BuffTypeInfo>(),
///     id: 0xcdbe_3e80,
///     slot: BuffTypeInfo::RESTED,
///     flags: BuffTypeInfo::SAVED,
///     name: 1_237_250_642,
///     duration: 5_000_000_000,
/// };
/// assert_eq!(rested.slot, BuffTypeInfo::RESTED);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuffTypeInfo {
    /// Size and version of this struct.
    pub header: Header,
    /// The type's id: FNV-1a 32 of its GUID's bytes.
    pub id: u32,
    /// Where an entity holds it: [`BuffTypeInfo::FOOD`],
    /// [`BuffTypeInfo::BUFF`], [`BuffTypeInfo::DEBUFF`],
    /// [`BuffTypeInfo::RESTED`] or [`BuffTypeInfo::TERRAIN`].
    pub slot: u16,
    /// [`BuffTypeInfo::SAVED`], [`BuffTypeInfo::HIDDEN`] or none.
    pub flags: u16,
    /// The `LocaTag` of its name, which a client shows.
    pub name: u32,
    /// How long it lasts by default, in game nanoseconds.
    pub duration: i64,
}

impl BuffTypeInfo {
    /// A food buff.
    pub const FOOD: u16 = 0;
    /// A buff.
    pub const BUFF: u16 = 1;
    /// A debuff.
    pub const DEBUFF: u16 = 2;
    /// Rested.
    pub const RESTED: u16 = 3;
    /// A debuff of the ground the entity stands in.
    pub const TERRAIN: u16 = 4;
    /// The game saves it with the character.
    pub const SAVED: u16 = 1;
    /// The game shows it nowhere.
    pub const HIDDEN: u16 = 2;
}

/// A buff an entity has, written by [`HostApi::buffs`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{ActiveBuff, Header};
///
/// let fed = ActiveBuff {
///     header: Header::of::<ActiveBuff>(),
///     id: 7,
///     entity: 4100,
///     slot: 0,
///     level: 0,
///     duration: 600_000_000_000,
///     remaining: 120_000_000_000,
/// };
/// assert!(fed.remaining < fed.duration);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActiveBuff {
    /// Size and version of this struct.
    pub header: Header,
    /// Its type's id.
    pub id: u32,
    /// The buff's own entity, which carries its effect.
    pub entity: u32,
    /// Where the entity holds it, as [`BuffTypeInfo::slot`].
    pub slot: u32,
    /// Its level; 0 for none.
    pub level: u32,
    /// How long it lasts in all, in game nanoseconds.
    pub duration: i64,
    /// How long it has left, in game nanoseconds.
    pub remaining: i64,
}

/// A buff to give an entity, for [`HostApi::apply_buff`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{BuffRequest, Header};
///
/// let request = BuffRequest {
///     header: Header::of::<BuffRequest>(),
///     entity: 2,
///     id: 7,
///     level: 0,
///     reserved: 0,
///     duration: 0,
/// };
/// assert_eq!(request.duration, 0, "the type's own duration");
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuffRequest {
    /// Size and version of this struct.
    pub header: Header,
    /// The entity to give it.
    pub entity: u32,
    /// The type's id.
    pub id: u32,
    /// Its level, 1 to 255; 0 for none.
    pub level: u32,
    /// Zero.
    pub reserved: u32,
    /// How long it lasts, in game nanoseconds; 0 for the type's own.
    pub duration: i64,
}

/// A knowledge value, written by [`HostApi::knowledge`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Header, KnowledgeInfo};
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
/// of its weather layers, written by [`HostApi::weather_fronts`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Header, WeatherFront};
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
#[derive(Clone, Copy, Debug, PartialEq)]
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

impl WeatherFront {
    /// Clouds without rain, or nothing.
    pub const NONE: u32 = 0;
    /// Rain, snow where it is cold.
    pub const RAIN: u32 = 1;
    /// A blizzard.
    pub const BLIZZARD: u32 = 2;
}

/// A terrain material of the running game, written by
/// [`HostApi::terrain_materials`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Header, TerrainMaterialInfo};
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

impl TerrainMaterialInfo {
    /// A player's tool can flatten it.
    pub const FLATTENS: u8 = 1;
}

/// A terrain edit, for [`HostApi::terrain_edit`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Header, TerrainEdit};
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
#[derive(Clone, Copy, Debug, PartialEq)]
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

/// A frame of the client's interface, handed to the callbacks registered with
/// [`HostApi::ui_frame`]; widgets drawn with [`HostApi::ui_widget`] name its
/// `token`.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Header, UiFrame};
///
/// let frame = UiFrame {
///     header: Header::of::<UiFrame>(),
///     token: 7,
///     screen: UiFrame::IN_GAME,
///     reserved: 0,
/// };
/// assert_eq!(frame.screen, UiFrame::IN_GAME);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct UiFrame {
    /// Size and version of this struct.
    pub header: Header,
    /// Names this frame in [`HostApi::ui_widget`]; valid until the callback
    /// returns.
    pub token: u64,
    /// Where the player is: [`UiFrame::MENU`], [`UiFrame::LOADING`] or
    /// [`UiFrame::IN_GAME`].
    pub screen: u32,
    /// Zero.
    pub reserved: u32,
}

impl UiFrame {
    /// The title screen and the menus before a game.
    pub const MENU: u32 = 0;
    /// A game loading, or the intro and outro.
    pub const LOADING: u32 = 1;
    /// In a game.
    pub const IN_GAME: u32 = 2;
}

/// A mod's callback for each frame of the client's interface: the frame and
/// the pointer given at registration. It must not unwind.
pub type UiFrameFn = unsafe extern "system" fn(frame: *const UiFrame, user: *mut c_void);

/// One widget, drawn with [`HostApi::ui_widget`]: `kind` says which, and
/// which fields it reads; the loader writes `index`, `value` and `result`
/// back.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Header, Str, UiWidget};
///
/// let button = UiWidget {
///     kind: UiWidget::BUTTON,
///     text: Str::new("Close"),
///     ..UiWidget::EMPTY
/// };
/// assert_eq!(button.header, Header::of::<UiWidget>());
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct UiWidget {
    /// Size and version of this struct.
    pub header: Header,
    /// What to draw: one of the kind constants, [`UiWidget::WINDOW`] to
    /// [`UiWidget::SOUND`].
    pub kind: u32,
    /// [`UiWidget::READ_ONLY`], [`UiWidget::WRAP`], [`UiWidget::CENTER`].
    pub flags: u32,
    /// A label's style ([`UiWidget::STYLE_BODY`] and on) or a sound
    /// ([`UiWidget::SOUND_CLICK`] and on).
    pub style: u32,
    /// A slider's stops between `min` and `max`, both included; 0 for a
    /// continuous slider.
    pub steps: u32,
    /// The text: a label's, a button's, a row's label, a window's id.
    pub text: Str,
    /// A window's title.
    pub detail: Str,
    /// A dropdown's options, `option_count` of them, valid for the call.
    pub options: *const Str,
    /// How many options `options` holds.
    pub option_count: u32,
    /// In and out: a toggle's state (0 or 1), a dropdown's option.
    pub index: u32,
    /// In and out: a slider's value; a progress bar's fill.
    pub value: f32,
    /// A slider's least value, a progress bar's empty value.
    pub min: f32,
    /// A slider's greatest value, a progress bar's full value.
    pub max: f32,
    /// Out: 1 when a button was clicked or a value changed this frame.
    pub result: u32,
    /// Where a HUD widget shows: [`UiWidget::TOP_LEFT`] to
    /// [`UiWidget::BOTTOM_RIGHT`]; read in a HUD callback only.
    pub anchor: u32,
    /// Zero.
    pub reserved: u32,
}

impl UiWidget {
    /// A window in the game's style, a panel over the screen that takes the
    /// input while it shows: `text` is its id, unique in the mod, `detail`
    /// its title. Widgets that follow go into it until [`UiWidget::END`].
    pub const WINDOW: u32 = 1;
    /// Closes the innermost open window.
    pub const END: u32 = 2;
    /// A line of text in `style`; [`UiWidget::WRAP`] wraps it at the width
    /// of the window, [`UiWidget::CENTER`] centres it.
    pub const LABEL: u32 = 3;
    /// A button labelled `text`; `result` 1 when clicked.
    pub const BUTTON: u32 = 4;
    /// A settings row labelled `text` with an on and off switch, `index`.
    pub const TOGGLE: u32 = 5;
    /// A settings row labelled `text` with a slider over `min..=max` in
    /// `steps` stops, `value`.
    pub const SLIDER: u32 = 6;
    /// A settings row labelled `text` with a choice among `options`, `index`.
    pub const DROPDOWN: u32 = 7;
    /// A bar filled from `min` to `max` at `value`.
    pub const PROGRESS: u32 = 8;
    /// Plays the interface sound `style`.
    pub const SOUND: u32 = 9;

    /// The row shows its value and does not take changes; a button does not
    /// take clicks.
    pub const READ_ONLY: u32 = 1;
    /// A label wraps at the width of its window.
    pub const WRAP: u32 = 2;
    /// A label is centred.
    pub const CENTER: u32 = 4;

    /// The game's body text.
    pub const STYLE_BODY: u32 = 0;
    /// The game's dimmer, secondary text.
    pub const STYLE_SECONDARY: u32 = 1;
    /// The game's largest heading.
    pub const STYLE_H1: u32 = 2;
    /// The game's second heading.
    pub const STYLE_H2: u32 = 3;
    /// The game's third heading.
    pub const STYLE_H3: u32 = 4;
    /// The game's warning text.
    pub const STYLE_WARNING: u32 = 5;
    /// The game's text for good news.
    pub const STYLE_POSITIVE: u32 = 6;
    /// The game's text for bad news.
    pub const STYLE_NEGATIVE: u32 = 7;

    /// The game's click.
    pub const SOUND_CLICK: u32 = 0;
    /// The game's confirmation.
    pub const SOUND_CONFIRM: u32 = 1;
    /// The game's step back.
    pub const SOUND_BACK: u32 = 2;
    /// The game's refusal.
    pub const SOUND_ERROR: u32 = 3;

    /// A widget of no kind, every field zero: the base for struct updates.
    pub const EMPTY: Self = Self {
        header: Header::of::<Self>(),
        kind: 0,
        flags: 0,
        style: 0,
        steps: 0,
        text: Str::EMPTY,
        detail: Str::EMPTY,
        options: core::ptr::null(),
        option_count: 0,
        index: 0,
        value: 0.0,
        min: 0.0,
        max: 0.0,
        result: 0,
        anchor: 0,
        reserved: 0,
    };

    /// The HUD's top left corner.
    pub const TOP_LEFT: u32 = 0;
    /// The HUD's top edge, centred.
    pub const TOP_CENTER: u32 = 1;
    /// The HUD's top right corner.
    pub const TOP_RIGHT: u32 = 2;
    /// The HUD's bottom left corner.
    pub const BOTTOM_LEFT: u32 = 3;
    /// The HUD's bottom edge, centred.
    pub const BOTTOM_CENTER: u32 = 4;
    /// The HUD's bottom right corner.
    pub const BOTTOM_RIGHT: u32 = 5;
}

/// A marker on the client's map, one of the set [`HostApi::ui_markers`]
/// hands over.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Header, Str, UiMarker};
///
/// let marker = UiMarker {
///     header: Header::of::<UiMarker>(),
///     position: [100 << 32, 40 << 32, -(20 << 32)],
///     label: Str::new("Safe of Bob"),
///     color: UiMarker::GREEN,
///     reserved: 0,
/// };
/// assert_eq!(marker.color, 2);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct UiMarker {
    /// Size and version of this struct.
    pub header: Header,
    /// Where, in 32.32 fixed-point metres, y up.
    pub position: [i64; 3],
    /// The label the map shows beside the icon.
    pub label: Str,
    /// The colour, one of the game's custom marker colours,
    /// [`UiMarker::WHITE`] to [`UiMarker::BLACK`].
    pub color: u32,
    /// Zero.
    pub reserved: u32,
}

impl UiMarker {
    /// White.
    pub const WHITE: u32 = 0;
    /// Red.
    pub const RED: u32 = 1;
    /// Green.
    pub const GREEN: u32 = 2;
    /// Blue.
    pub const BLUE: u32 = 3;
    /// Yellow.
    pub const YELLOW: u32 = 4;
    /// Purple.
    pub const PURPLE: u32 = 5;
    /// Turquoise.
    pub const TURQUOISE: u32 = 6;
    /// Black.
    pub const BLACK: u32 = 7;
}

/// A dialog in the game's own style, queued with [`HostApi::ui_dialog`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Header, Str, UiDialog};
///
/// let dialog = UiDialog {
///     header: Header::of::<UiDialog>(),
///     title: Str::new("Enshwick"),
///     message: Str::new("Download the update?"),
///     detail: Str::EMPTY,
///     buttons: 1 << UiDialog::OK | 1 << UiDialog::CANCEL,
///     flags: 0,
/// };
/// assert_eq!(dialog.buttons, 0xc0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct UiDialog {
    /// Size and version of this struct.
    pub header: Header,
    /// The title.
    pub title: Str,
    /// The message under the title.
    pub message: Str,
    /// Smaller text under the message; empty for none.
    pub detail: Str,
    /// Bit `i` adds the button with the game's label for answer `i`, such
    /// as [`UiDialog::OK`]; the buttons show in the order of their bits. At
    /// least one.
    pub buttons: u32,
    /// [`UiDialog::WAITING`] when the caller blocks until the answer.
    pub flags: u32,
}

impl UiDialog {
    /// "Retry".
    pub const RETRY: u32 = 0;
    /// "Disable".
    pub const DISABLE: u32 = 1;
    /// "Erase".
    pub const ERASE: u32 = 2;
    /// "Reset".
    pub const RESET: u32 = 3;
    /// "Overwrite".
    pub const OVERWRITE: u32 = 5;
    /// "Cancel".
    pub const CANCEL: u32 = 6;
    /// "OK".
    pub const OK: u32 = 7;
    /// "Ignore".
    pub const IGNORE: u32 = 8;
    /// The buttons a dialog may carry.
    pub const BUTTONS: u32 = 0x1ef;
    /// The answer when the dialog closed without one, as when the game
    /// quits.
    pub const CLOSED: u32 = u32::MAX;
    /// The caller blocks until the answer: refused on the thread that
    /// draws the interface, which would never draw it.
    pub const WAITING: u32 = 1;
}

/// A mod's callback for the answer to its dialog: the answer, such as
/// [`UiDialog::OK`] or [`UiDialog::CLOSED`], and the pointer given with the
/// dialog. Called once, on the thread that draws the interface, or on a
/// thread of the loader's when the dialog shows as a system window. It must
/// not unwind.
pub type UiAnswerFn = unsafe extern "system" fn(answer: u32, user: *mut c_void);

/// A mod's answer to the player pressing Update under mod `id` on the Mods
/// screen ([`HostApi::updates_set`]), with the pointer given at
/// registration. It runs on the thread that draws the interface, must
/// return at once and must not unwind.
pub type UpdatePickFn = unsafe extern "system" fn(id: Str, user: *mut c_void);

/// A mod's check of a new text of its settings file, before the loader
/// writes it: [`Status::OK`] to accept it; else the reason, written to
/// `why` and valid until the mod's next call, and [`Status::REFUSED`]. It
/// must not unwind.
pub type ConfigCheckFn =
    unsafe extern "system" fn(text: Str, why: *mut Str, user: *mut c_void) -> Status;

/// The table of services the loader offers a mod.
///
/// Fields are only ever appended; a mod checks [`Header::covers`] before it
/// calls a field added after the minor it was built against.
///
/// # Examples
///
/// The safe layer wraps this table; see [`crate::Load`].
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct HostApi {
    /// Size and version of this struct.
    pub header: Header,
    /// The loader's own version.
    pub host_version: Str,
    /// Writes the running game's identity, as far as the [`Header`] `out`
    /// starts with says the caller's struct reaches.
    pub build: extern "system" fn(out: *mut BuildInfo),
    /// Resolves a symbolic id for the running build: the mod's own rows first,
    /// then the loader's table.
    pub symbol: extern "system" fn(me: ModHandle, name: Str, out: *mut u64) -> Status,
    /// Resolves an ECS system's update function by its reflected name.
    pub system: extern "system" fn(name: Str, out: *mut u64) -> Status,
    /// Resolves a field's offset in a reflected type.
    pub field_offset: extern "system" fn(type_name: Str, field: Str, out: *mut u64) -> Status,
    /// Resolves a reflected type's size.
    pub type_size: extern "system" fn(type_name: Str, out: *mut u32) -> Status,
    /// Links a detour; allowed during `enshwick_mod_init` only.
    pub hook: extern "system" fn(me: ModHandle, spec: *const HookSpec) -> Status,
    /// Writes the game caller's context at entry of the current chain, for
    /// links with [`HookFlags::CALLER_FRAME`]; valid until the link calls
    /// anything hooked.
    pub caller_frame: extern "system" fn(out: *mut CallerFrame),
    /// Reports that the mod failed at run time; the loader unlinks every link
    /// of the mod and turns it off.
    pub fault: extern "system" fn(me: ModHandle, reason: Str),
    /// Writes a line to the log under the mod's id.
    pub log: extern "system" fn(me: ModHandle, level: Level, message: Str),
    /// The text of the mod's settings file, written from `defaults` first when
    /// it is missing; the text stays valid for the life of the process, so
    /// a thread may read it while another asks again.
    pub settings: extern "system" fn(me: ModHandle, defaults: Str, out: *mut Str) -> Status,
    /// Publishes the service `spec` describes, table and layout, for other
    /// mods; [`Status::REFUSED`] when another mod publishes its name and
    /// major. Allowed during `enshwick_mod_init` only. Since minor 47.
    pub publish: extern "system" fn(me: ModHandle, spec: *const ServiceSpec) -> Status,
    /// Finds the service `wanted` describes, its table null, and checks
    /// that the published layout starts with the wanted one; writes the
    /// table and its publisher to `out`, and on [`Status::MISSING`] or
    /// [`Status::REFUSED`] the reason. Since minor 47.
    pub service: extern "system" fn(
        me: ModHandle,
        wanted: *const ServiceSpec,
        out: *mut ServiceFound,
    ) -> Status,
    /// Resolves an engine constant of the running build, such as a field
    /// offset reflection does not describe, by its symbolic id.
    pub constant: extern "system" fn(me: ModHandle, name: Str, out: *mut u64) -> Status,
    /// The mod's own data folder, `mod-data/<id>`, created on first use; the
    /// path stays valid for the process.
    pub data_dir: extern "system" fn(me: ModHandle, out: *mut Str) -> Status,
    /// The most detailed level kept for the mod's lines; the mod skips
    /// formatting anything more detailed. Since minor 1.
    pub log_level: extern "system" fn(me: ModHandle) -> Level,
    /// Calls `callback` with `user` on every `event` for the mod, on the
    /// thread that raises it, until the mod is off; allowed during
    /// `enshwick_mod_init` only. Since minor 2.
    pub subscribe: extern "system" fn(
        me: ModHandle,
        event: Event,
        callback: EventFn,
        user: *mut c_void,
    ) -> Status,
    /// The ECS type hash of a reflected component type, by name. Since minor 3.
    pub type_hash: extern "system" fn(type_name: Str, out: *mut u32) -> Status,
    /// A read-only pointer to `entity`'s component of type `type_hash`, valid
    /// until the game thread moves on; on the server's game thread only.
    /// Since minor 3.
    pub component: extern "system" fn(
        me: ModHandle,
        entity: u32,
        type_hash: u32,
        out: *mut *const c_void,
    ) -> Status,
    /// The template GUID of `entity`, in file order; on the server's game
    /// thread only. Since minor 3.
    pub entity_template:
        extern "system" fn(me: ModHandle, entity: u32, out: *mut [u8; 16]) -> Status,
    /// The connected player whose character is `entity`; on the server's game
    /// thread only. Since minor 3.
    pub player: extern "system" fn(me: ModHandle, entity: u32, out: *mut PlayerInfo) -> Status,
    /// Up to `capacity` connected players with characters in the world, and
    /// how many there are; on the server's game thread only. Since minor 3.
    /// Every entry is as large as the header of the first one says.
    pub players: extern "system" fn(
        me: ModHandle,
        out: *mut PlayerInfo,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Serves the resource `key` as `patch` makes it from the resource
    /// `base`, built the first time the game reads it: a new resource when
    /// the keys differ, a changed vanilla one when they are equal. Allowed
    /// during `enshwick_mod_init` only; one mod per key. Since minor 4.
    pub serve_resource: extern "system" fn(
        me: ModHandle,
        key: *const ResourceKey,
        base: *const ResourceKey,
        patch: ResourcePatchFn,
        user: *mut c_void,
    ) -> Status,
    /// Adds the text of `LocaTag` `id` in `language` (`LanguageId`, 0 for any
    /// language without its own); the client shows it wherever the game asks
    /// for that tag. Allowed during `enshwick_mod_init` only. Since minor 4.
    pub add_text: extern "system" fn(me: ModHandle, id: u32, language: u32, text: Str) -> Status,
    /// Up to `capacity` loaded mods, the loader first, and how many there
    /// are. Entries are `entry_size` bytes apart, the caller's
    /// `size_of::<ModInfo>()`; the loader fills what both sides know of each
    /// and its own [`Header`]. Since minor 5.
    pub mods: extern "system" fn(
        me: ModHandle,
        out: *mut ModInfo,
        entry_size: u32,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Reports the mod's join values as its settings file holds them,
    /// `synced` (an inline TOML table), and writes the values to run with
    /// over them: the server's while this client is joined to a server that
    /// advertises values for the mod, else an empty string. A server
    /// advertises the `synced` of each join mod. The text written stays valid
    /// for the life of the process. Since minor 6.
    pub sync: extern "system" fn(me: ModHandle, synced: Str, out: *mut Str) -> Status,
    /// Writes where the game's code section (`.text`) is loaded and how many
    /// bytes it spans. Since minor 7.
    pub code_range: extern "system" fn(start: *mut u64, len: *mut u64) -> Status,
    /// Replaces `len` bytes of game code at `address`, which must read
    /// `expected` at arming, with `replacement`; both point at `len` bytes
    /// valid for the call. The loader writes the patch with the hooks, refuses
    /// one that overlaps another patch, and skips one that overlaps hooked
    /// bytes or no longer reads `expected`, turning the mod off. Allowed
    /// during `enshwick_mod_init` only. Since minor 7.
    pub patch_code: extern "system" fn(
        me: ModHandle,
        address: u64,
        expected: *const u8,
        replacement: *const u8,
        len: usize,
    ) -> Status,
    /// The most recipes one workshop takes on the running game: 512, the
    /// game's own list size, or [`u32::MAX`] once the loader has lifted that
    /// limit at arming. Since minor 7.
    pub workshop_recipes: extern "system" fn() -> u32,
    /// 1 while this process runs a game server: always on the dedicated
    /// server, on the client while it hosts a game, solo play included.
    /// Since minor 8.
    pub hosting: extern "system" fn() -> u8,
    /// The entity world of the game server this process runs, the value a
    /// system context of that world holds at `ecs.context_entity_world`; 0
    /// while none runs or before its first step. Since minor 8.
    pub server_world: extern "system" fn() -> u64,
    /// Registers the chat command `/name`, `[a-z0-9-]`, 1 to 32 characters,
    /// with a line of help for `/enshwick help`; the server calls `callback`
    /// with `user` on its game thread when a player types it. A name another
    /// mod or the loader holds is refused. Allowed during
    /// `enshwick_mod_init` only. Since minor 9.
    pub command: extern "system" fn(
        me: ModHandle,
        name: Str,
        help: Str,
        callback: CommandCallFn,
        user: *mut c_void,
    ) -> Status,
    /// Sends `text` to the chat of the player whose account key is `to`, or of
    /// everyone for 0, from any thread; the server posts it after the next
    /// step. Lines past 1024 bytes are cut. Since minor 9.
    pub chat: extern "system" fn(me: ModHandle, to: u64, text: Str) -> Status,
    /// Gives `entity` the name `text`, as a player's rename does: shown on
    /// hover, sent to every client and saved with the world; cut at 255
    /// bytes, removed when empty. On the server's game thread only. Since
    /// minor 9.
    pub set_label: extern "system" fn(me: ModHandle, entity: u32, text: Str) -> Status,
    /// Calls `callback` with `user` before the server applies a player's
    /// rename of an entity; any filter returning 0 refuses it. Allowed during
    /// `enshwick_mod_init` only. Since minor 9.
    pub rename_filter:
        extern "system" fn(me: ModHandle, callback: RenameCallFn, user: *mut c_void) -> Status,
    /// Up to `capacity` of the server's join mods that the last refused join
    /// found missing on this client or in another version, and how many
    /// there are; laid out as [`HostApi::mods`] lays out its entries. Each
    /// entry has the server's version, [`ManifestFlags::JOIN`], `off` 1, the
    /// client's DLL path when it has the mod (empty otherwise) and the
    /// source the server advertises, else the client's own source for the
    /// mod. The strings stay valid for the process. Empty before a refused
    /// join; see [`Event::JOIN_MISSING`]. Since minor 10.
    pub join_missing: extern "system" fn(
        me: ModHandle,
        out: *mut ModInfo,
        entry_size: u32,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Runs a handler before the instruction at an address inside a game
    /// function, with the registers there; allowed during
    /// `enshwick_mod_init` only. Since minor 11.
    pub hook_at: extern "system" fn(me: ModHandle, spec: *const MidHookSpec) -> Status,
    /// Asks for inventory transactions and item data; allowed during
    /// `enshwick_mod_init` only. [`Status::MISSING`] when this game build
    /// lacks a piece of them. Requests run in the game server: the dedicated
    /// server, or the client while it hosts. Since minor 12.
    pub inventory_open: extern "system" fn(me: ModHandle) -> Status,
    /// Writes the static data of the item `item`, as far as the [`Header`]
    /// `out` starts with says the caller's struct reaches;
    /// [`Status::MISSING`] for an unknown item, or before the game first ran
    /// its inventories. Any thread. Since minor 12.
    pub item_info: extern "system" fn(me: ModHandle, item: u32, out: *mut ItemData) -> Status,
    /// Queues `request`, from any thread, for the game's inventory system,
    /// which runs it as one transaction: all or nothing. `done` gets the
    /// result with `user` on the game thread after a later step, unless the
    /// mod is off by then. On an error status nothing is queued and `done` is
    /// never called. Since minor 12.
    pub inventory_submit: extern "system" fn(
        me: ModHandle,
        request: *const InventoryRequest,
        done: InventoryResultFn,
        user: *mut c_void,
    ) -> Status,
    /// Adds `callback` with `user` to the mod's status sources: the loader
    /// calls it, on a thread of its own or the game thread, whenever it
    /// writes its status summary (`logs/enshwick/status.log`, `/enshwick
    /// status`) and shows its lines under the mod. Allowed during
    /// `enshwick_mod_init` only. Since minor 13.
    pub status: extern "system" fn(me: ModHandle, callback: StatusFn, user: *mut c_void) -> Status,
    /// Runs the chat command line `line` (`/name args`) as the server's
    /// console, an administrator with no character, and writes its reply
    /// to `out`, valid until the next call on this thread. On the server's
    /// game thread only; [`Status::MISSING`] when no command has the name.
    /// Since minor 13.
    pub run_command: extern "system" fn(me: ModHandle, line: Str, out: *mut Str) -> Status,
    /// Asks the loader to follow the Flame Altars of the server's world, for
    /// [`HostApi::altars`] and [`Event::ALTARS_CHANGED`]. Allowed during
    /// `enshwick_mod_init` only. Since minor 15.
    pub altars_open: extern "system" fn(me: ModHandle) -> Status,
    /// Up to `capacity` altars of the server's world and how many there are.
    /// Entries are `entry_size` bytes apart, the caller's
    /// `size_of::<AltarInfo>()`; the loader fills what both sides know of
    /// each and its own [`Header`]. Any thread; empty while no world runs.
    /// [`Status::REFUSED`] for a mod that did not open altars. Since minor 15.
    pub altars: extern "system" fn(
        me: ModHandle,
        out: *mut AltarInfo,
        entry_size: u32,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Makes the mod the one that sizes altar zones, through
    /// [`HostApi::set_altar_zones`]; it opens altars too. One mod at a time:
    /// [`Status::REFUSED`] when another holds it. Allowed during
    /// `enshwick_mod_init` only. Since minor 15.
    pub altar_zones_control: extern "system" fn(me: ModHandle) -> Status,
    /// Replaces the sizes the loader gives altar zones with the `count`
    /// sizes at `sizes`; an altar without one keeps its vanilla size. Any
    /// thread; the server's next zone pass writes them. [`Status::REFUSED`]
    /// for a mod that does not control altar zones. Since minor 15.
    pub set_altar_zones:
        extern "system" fn(me: ModHandle, sizes: *const AltarZoneSize, count: usize) -> Status,
    /// Calls `callback` with `user` when the game refuses a placement whose
    /// box does not lie in one building zone; any filter returning 1 accepts
    /// it after all. Runs on both sides, since a client predicts placement.
    /// Allowed during `enshwick_mod_init` only. Since minor 15.
    pub zone_check_filter:
        extern "system" fn(me: ModHandle, callback: ZoneCheckFn, user: *mut c_void) -> Status,
    /// Makes `callback` with `user` the answer to the game's survivor
    /// lookups that [`SurvivorLookup`] names; one mod at a time
    /// ([`Status::REFUSED`] for another). Runs on the server's worker
    /// threads, in the dedicated server or a hosting client, and for
    /// [`SurvivorLookup::CURSOR`] on a client alone too. Allowed during
    /// `enshwick_mod_init` only; [`Status::MISSING`] on a build that lacks a
    /// piece. Since minor 16.
    pub survivor_lookup:
        extern "system" fn(me: ModHandle, callback: SurvivorLookupFn, user: *mut c_void) -> Status,
    /// Calls `callback` with `user` after a survivor's own bed check kept its
    /// bed; any filter returning 0 clears it. Server's worker threads.
    /// Allowed during `enshwick_mod_init` only. Since minor 16.
    pub survivor_bed_check:
        extern "system" fn(me: ModHandle, callback: BedCheckFn, user: *mut c_void) -> Status,
    /// Calls `callback` with `user` before a summoning staff spawns or moves
    /// a survivor; any filter returning 1 makes the use take the item.
    /// Server's game thread. Allowed during `enshwick_mod_init` only. Since
    /// minor 16.
    pub summon_filter:
        extern "system" fn(me: ModHandle, callback: SummonFn, user: *mut c_void) -> Status,
    /// Files item `item` as a summoning staff of the survivor `survivor`, so
    /// the game treats it as that survivor's staff, on both sides. Allowed
    /// during `enshwick_mod_init` only. Since minor 16.
    pub summon_item: extern "system" fn(me: ModHandle, item: u32, survivor: u32) -> Status,
    /// Makes `callback` with `user` merge the statuses the client's survivor
    /// menu shows for a survivor with several entities. Client only; one mod
    /// at a time. Allowed during `enshwick_mod_init` only. Since minor 16.
    pub survivor_status:
        extern "system" fn(me: ModHandle, callback: SurvivorStatusFn, user: *mut c_void) -> Status,
    /// Calls `callback` with `user` before a player's interaction that
    /// [`Interaction`] names; any filter returning 0 refuses it. Runs in the
    /// server's world only, on the thread of the system that checks it.
    /// Allowed during `enshwick_mod_init` only; [`Status::MISSING`] on a
    /// build that lacks a piece. Since minor 17.
    pub interaction_filter:
        extern "system" fn(me: ModHandle, callback: InteractionFn, user: *mut c_void) -> Status,
    /// Calls `callback` with `user` when the game asks whether a character
    /// unlocked skill node `node`; the first filter that answers decides.
    /// Runs on both sides, on the asking thread. Allowed during
    /// `enshwick_mod_init` only. Since minor 18.
    pub skill_override: extern "system" fn(
        me: ModHandle,
        node: u32,
        callback: SkillCheckFn,
        user: *mut c_void,
    ) -> Status,
    /// Calls `callback` with `user` when a comfort provider registers its
    /// comfort; filters run in load order, each on the last one's value.
    /// Server's worker threads. Allowed during `enshwick_mod_init` only.
    /// Since minor 18.
    pub comfort_filter:
        extern "system" fn(me: ModHandle, callback: ComfortFn, user: *mut c_void) -> Status,
    /// Calls `callback` with `user` before the server applies the
    /// characters' gained experience, once per batch; filters run in load
    /// order. Server's worker threads. Allowed during `enshwick_mod_init`
    /// only. Since minor 18.
    pub experience_filter:
        extern "system" fn(me: ModHandle, callback: ExperiencePassFn, user: *mut c_void) -> Status,
    /// Makes `callback` with `user` decide how many altars each Flame level
    /// allows, on both sides; one mod at a time. Allowed during
    /// `enshwick_mod_init` only. Since minor 19.
    pub altar_limit:
        extern "system" fn(me: ModHandle, callback: AltarLimitFn, user: *mut c_void) -> Status,
    /// Lets the game group 32 altar zones into its 16 bases instead of 16
    /// zones, refusing an altar that would start a 17th base; the patches
    /// count as the mod's own. [`Status::MISSING`] when this build's code is
    /// not the one the loader knows: the game keeps 16. Allowed during
    /// `enshwick_mod_init` only. Since minor 19.
    pub raise_base_limit: extern "system" fn(me: ModHandle) -> Status,
    /// Writes how many altar zones and bases the game's last grouping found;
    /// [`Status::MISSING`] before the first one or without the raised limit.
    /// Any thread. Since minor 19.
    pub base_zones: extern "system" fn(me: ModHandle, zones: *mut u32, bases: *mut u32) -> Status,
    /// Calls `callback` with `user` on every game event of the reflected
    /// type whose hash is `type_hash`, or on every game event for 0, on
    /// the server's game thread before the systems of the next step run.
    /// Allowed during `enshwick_mod_init` only. Since minor 20.
    pub game_event: extern "system" fn(
        me: ModHandle,
        type_hash: u32,
        callback: GameEventFn,
        user: *mut c_void,
    ) -> Status,
    /// Up to `capacity` entities of the server's world that have a component
    /// of the reflected type `type_hash` and a place in the world, and how
    /// many there are. On the server's game thread only. Since minor 21.
    pub entities: extern "system" fn(
        me: ModHandle,
        type_hash: u32,
        out: *mut EntityPlace,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Asks the server to destroy `entity` after the systems of the next
    /// step; [`Status::REFUSED`] for a player's character,
    /// [`Status::MISSING`] for an entity not in the world. On the server's
    /// game thread only. Since minor 21.
    pub destroy: extern "system" fn(me: ModHandle, entity: u32) -> Status,
    /// Asks the server to save the world now, as its timed saves do;
    /// [`Event::WORLD_SAVED`] follows once the save is on disk. A save under
    /// way counts as asked; [`Status::REFUSED`] while the world may not be
    /// saved, as before it has loaded. On the server's game thread only.
    /// Since minor 21.
    pub save_world: extern "system" fn(me: ModHandle) -> Status,
    /// Writes the key of the world the server runs: its save file's name and
    /// a hash of its folder, `[a-z0-9_-]`, valid for the process.
    /// [`Status::MISSING`] before the server has read or written the world.
    /// Any thread. Since minor 21.
    pub world_key: extern "system" fn(me: ModHandle, out: *mut Str) -> Status,
    /// A writable pointer to `entity`'s component of the reflected type
    /// `type_hash`, valid until the game thread returns to the game; the
    /// loader marks the component changed, as the game's own writes do, so
    /// the game sends it to clients and saves it. [`Status::REFUSED`] for a
    /// copy the game keeps for clients (`Network*`, `Client*`), which it
    /// overwrites; [`Status::MISSING`] when the entity lacks the component.
    /// On the server's game thread only. Since minor 22.
    pub component_write: extern "system" fn(
        me: ModHandle,
        entity: u32,
        type_hash: u32,
        out: *mut *mut c_void,
    ) -> Status,
    /// Raises the game event at `data`, `size` bytes of the reflected type
    /// `type_hash`, its whole layout; the loader stamps it with the game
    /// time and the systems of the next step see it. Only types the loader
    /// allows, which the server's own systems raise: [`Status::REFUSED`]
    /// for any other or when the step's stream is full,
    /// [`Status::BAD_ARGS`] for a size that is not the type's. On the
    /// server's game thread only. Since minor 23.
    pub emit:
        extern "system" fn(me: ModHandle, type_hash: u32, data: *const u8, size: u32) -> Status,
    /// Asks the server to move an entity as its admin teleport does, after
    /// the next step; [`Status::MISSING`] for an entity not in the world,
    /// [`Status::REFUSED`] for one the game's teleport does not move, as a
    /// prop: it moves actors and characters.
    /// On the server's game thread only. Since minor 23.
    pub teleport: extern "system" fn(me: ModHandle, request: *const TeleportRequest) -> Status,
    /// Asks the server to spawn an entity from a template, as its admin
    /// spawn does, and writes the new entity's id to `out`; the entity is in
    /// the world after the next step. [`Status::BAD_ARGS`] for a zero
    /// template; a template the game does not know spawns nothing. On the
    /// server's game thread only. Since minor 24.
    pub spawn:
        extern "system" fn(me: ModHandle, request: *const SpawnRequest, out: *mut u32) -> Status,
    /// Calls `callback` with `user` on each player's craft request in the
    /// server's world, on the game thread before the crafting system reads
    /// it; any filter returning 0 refuses it, and nothing is made or taken.
    /// Allowed during `enshwick_mod_init` only. Since minor 25.
    pub craft_filter:
        extern "system" fn(me: ModHandle, callback: CraftCheckFn, user: *mut c_void) -> Status,
    /// Calls `callback` with `user` after the game's cursor check allows a
    /// placement; any filter returning 0 refuses it as the game refuses a
    /// placement without permission. Runs on both sides, on the cursor
    /// systems' worker threads: the server's check decides, a client's
    /// shows. Allowed during `enshwick_mod_init` only. Since minor 25.
    pub build_filter:
        extern "system" fn(me: ModHandle, callback: BuildCheckFn, user: *mut c_void) -> Status,
    /// Opens the mod's channel `name`, 1 to 64 bytes, between its server
    /// and client parts, and writes its id to `out`; `callback` gets each
    /// message on it with `user`: on the server's game thread, on a client
    /// on the thread that takes the server's chat. Join mods only
    /// ([`Status::REFUSED`] for another), since both ends must run the mod.
    /// Allowed during `enshwick_mod_init` only. Since minor 26.
    pub net_channel: extern "system" fn(
        me: ModHandle,
        name: Str,
        callback: NetMessageFn,
        user: *mut c_void,
        out: *mut u32,
    ) -> Status,
    /// Sends `size` bytes at `data`, at most 4096, on the mod's `channel`:
    /// to the server for [`NetMessage::SERVER`], else from the server to the
    /// client of the player with account key `peer`, or to every client
    /// that runs Enshwick for 0. Any thread. [`Status::MISSING`] while the
    /// peer cannot take messages, [`Status::REFUSED`] over the peer's rate,
    /// [`Status::BAD_ARGS`] for too many bytes or another mod's channel.
    /// Since minor 26.
    pub net_send: extern "system" fn(
        me: ModHandle,
        channel: u32,
        peer: u64,
        data: *const u8,
        size: u32,
    ) -> Status,
    /// [`Status::OK`] when `peer` takes messages: a player's client that
    /// runs Enshwick, on the server; the server, for [`NetMessage::SERVER`]
    /// on a client. [`Status::MISSING`] otherwise. Any thread. Since minor
    /// 26.
    pub net_ready: extern "system" fn(me: ModHandle, peer: u64) -> Status,
    /// Shows `text`, cut at 255 bytes, on the screen of the player with
    /// account key `key`, as the game shows its own "can't do that"
    /// messages; [`Status::MISSING`] when the player's client lacks
    /// Enshwick and cannot show it, and nothing is sent. On the server's
    /// game thread only. Since minor 27.
    pub notify: extern "system" fn(me: ModHandle, key: u64, text: Str) -> Status,
    /// Writes the clock of the server's world, as far as the [`Header`]
    /// `out` starts with says the caller's struct reaches. On the server's
    /// game thread only. Since minor 28.
    pub time_of_day: extern "system" fn(me: ModHandle, out: *mut DaytimeInfo) -> Status,
    /// Sets the clock of the server's world to `time`, game nanoseconds of
    /// the day, taken modulo 24 hours; clients follow after the next step.
    /// On the server's game thread only. Since minor 28.
    pub set_time_of_day: extern "system" fn(me: ModHandle, time: i64) -> Status,
    /// Lets the mod change how fast the clock runs, with
    /// [`HostApi::set_time_speed`]. Allowed during `enshwick_mod_init` only.
    /// Since minor 28.
    pub time_speed_control: extern "system" fn(me: ModHandle) -> Status,
    /// Sets the mod's factor on the clock's speed, from 0 (stopped) to 1000;
    /// the clock runs at the game's own speed times every mod's factor.
    /// [`Status::REFUSED`] for a mod without
    /// [`HostApi::time_speed_control`], [`Status::BAD_ARGS`] for a factor
    /// out of range. On the server's game thread only. Since minor 28.
    pub set_time_speed: extern "system" fn(me: ModHandle, factor: f32) -> Status,
    /// Copies the server's live game settings, `keen::ecs::GameSettings`,
    /// into `out`, `size` bytes, which must be the type's reflected size
    /// ([`Status::BAD_ARGS`] otherwise), and writes the preset the server
    /// runs (`GameSettingsPreset`) to `preset`. On the server's game thread
    /// only. Since minor 29.
    pub game_settings:
        extern "system" fn(me: ModHandle, out: *mut u8, size: u32, preset: *mut u32) -> Status,
    /// Replaces the server's live game settings with `data`, the type's
    /// whole reflected layout, as the game's admin command does: systems
    /// see them from the next step and every client gets them; the preset
    /// becomes custom. Until the server stops: nothing writes them to disk.
    /// [`Status::BAD_ARGS`] for a wrong size or a day or night length of
    /// zero or less. On the server's game thread only. Since minor 29.
    pub set_game_settings: extern "system" fn(me: ModHandle, data: *const u8, size: u32) -> Status,
    /// Calls `callback` with `user` on damage about to reach an entity's
    /// health in the server's world, before the game applies it: hits,
    /// falls and the shroud; what the filters leave in the
    /// [`DamageCheck`] is what the game applies. Runs on the game's worker
    /// threads, in load order. Allowed during `enshwick_mod_init` only.
    /// Since minor 30.
    pub damage_filter:
        extern "system" fn(me: ModHandle, callback: DamageFilterFn, user: *mut c_void) -> Status,
    /// Up to `capacity` buff and debuff types of the running game, and how
    /// many there are. On the server's game thread only. Since minor 31.
    /// Every entry is as large as the header of the first one says.
    pub buff_types: extern "system" fn(
        me: ModHandle,
        out: *mut BuffTypeInfo,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Up to `capacity` buffs `entity` has, and how many there are;
    /// [`Status::MISSING`] for an entity that holds no buffs. On the
    /// server's game thread only. Since minor 31.
    /// Every entry is as large as the header of the first one says.
    pub buffs: extern "system" fn(
        me: ModHandle,
        entity: u32,
        out: *mut ActiveBuff,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Gives an entity a buff, as the game restores a character's saved
    /// buffs: at once, for the duration asked. One the entity has already
    /// is renewed to that duration. [`Status::MISSING`] for an entity that
    /// holds no buffs, [`Status::BAD_ARGS`] for an unknown type,
    /// [`Status::REFUSED`] for a type removed in this step, which the game
    /// clears only in the next. On the server's game thread only. Since
    /// minor 31.
    pub apply_buff: extern "system" fn(me: ModHandle, request: *const BuffRequest) -> Status,
    /// Removes every buff of type `id` from `entity`, as the game cleanses
    /// one: it goes in the next step. Writes 1 to `removed` when the entity
    /// had it, else 0; [`Status::MISSING`] for an entity that holds no
    /// buffs. On the server's game thread only. Since minor 31.
    pub remove_buff:
        extern "system" fn(me: ModHandle, entity: u32, id: u32, removed: *mut u8) -> Status,
    /// Writes knowledge `id` of the player whose character is `entity`, or
    /// of the world for 0 or for an id of the world's;
    /// [`Status::MISSING`] for an id the game does not know,
    /// [`Status::REFUSED`] for a player not in the game. On the server's
    /// game thread only. Since minor 32.
    pub knowledge:
        extern "system" fn(me: ModHandle, entity: u32, id: u32, out: *mut KnowledgeInfo) -> Status,
    /// Grants knowledge `id`, of `kind`, to the player whose character is
    /// `entity`, or to the world for 0, as the game grants it: the next
    /// step applies it with what follows (experience, unlocks, clients,
    /// the save). `value` is ignored for [`KnowledgeInfo::BOOL`], added for
    /// [`KnowledgeInfo::COUNTER`], the bit to set for
    /// [`KnowledgeInfo::FLAGS`]. [`Status::BAD_ARGS`] for an unknown id,
    /// a kind that is not the id's or a bit the game does not expect,
    /// [`Status::REFUSED`] for a player not in the
    /// game or a full queue. On the server's game thread only. Since minor
    /// 32.
    pub grant_knowledge:
        extern "system" fn(me: ModHandle, entity: u32, id: u32, kind: u32, value: u32) -> Status,
    /// Up to `capacity` quest ids of the running game, and how many there
    /// are. On the server's game thread only. Since minor 32.
    pub quests:
        extern "system" fn(me: ModHandle, out: *mut u32, capacity: u32, count: *mut u32) -> Status,
    /// Writes how clear, rainy, snowy and stormy it is at `position`, in
    /// 32.32 fixed-point metres, as weights that add up to 1, as the game's
    /// systems weigh it. On the server's game thread only. Since minor 33.
    pub weather_at:
        extern "system" fn(me: ModHandle, position: *const [i64; 3], out: *mut [f32; 4]) -> Status,
    /// Up to `capacity` current weather fronts of the server's world, one per
    /// layer that has one, and how many there are. On the server's game
    /// thread only. Since minor 33.
    /// Every entry is as large as the header of the first one says.
    pub weather_fronts: extern "system" fn(
        me: ModHandle,
        out: *mut WeatherFront,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Starts new weather in every layer of the server's world: fronts of
    /// `effect` ([`WeatherFront::NONE`] for clear skies) where a layer has
    /// one, clear skies elsewhere, for `duration` game nanoseconds or the
    /// game's own for 0; `u32::MAX` lets the game choose anew. Clients
    /// follow. On the server's game thread only. Since minor 33.
    pub set_weather: extern "system" fn(me: ModHandle, effect: u32, duration: i64) -> Status,
    /// Up to `capacity` terrain materials of the running game, and how many
    /// there are. On the server's game thread only. Since minor 34.
    /// Every entry is as large as the header of the first one says.
    pub terrain_materials: extern "system" fn(
        me: ModHandle,
        out: *mut TerrainMaterialInfo,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Runs a terrain edit in the server's world as the game's terraforming
    /// does, and writes the volume it changed in cubic metres (for a
    /// flatten, 1 when anything changed) to `amount`; clients see it and
    /// the save keeps it. Building voxels are left alone.
    /// [`Status::BAD_ARGS`] for an edit out of range. On the server's game
    /// thread only. Since minor 34.
    pub terrain_edit:
        extern "system" fn(me: ModHandle, edit: *const TerrainEdit, amount: *mut f32) -> Status,
    /// Hands the loader the mod's settings schema, `schema`, the TOML text
    /// of a [`config::Schema`](crate::config::Schema); a later call replaces
    /// it. The loader answers `/config` from it and checks values against it
    /// before it writes them. [`Status::BAD_ARGS`] for text that is not a
    /// schema. Any time. Since minor 35.
    pub config_schema: extern "system" fn(me: ModHandle, schema: Str) -> Status,
    /// Calls `callback` with `user` after a summoning staff's use spawned or
    /// moved a survivor, with [`Summon::consumed`] set; its return value is
    /// ignored. On the thread of the use, as [`HostApi::summon_filter`].
    /// Allowed during `enshwick_mod_init` only. Since minor 37.
    pub summoned:
        extern "system" fn(me: ModHandle, callback: SummonFn, user: *mut c_void) -> Status,
    /// Opens the item data kind `id`, `name` for the mod's own
    /// (`<mod>:<name>`) or `mod:name` for any mod's, each side 1 to 64 of
    /// `[a-z0-9_-]`, and writes its index to `out`; every mod gets the same
    /// index for one id. Allowed during `enshwick_mod_init` only. Since
    /// minor 38.
    pub item_data_open: extern "system" fn(me: ModHandle, id: Str, out: *mut u32) -> Status,
    /// Copies the value of kind `kind` on the item entity `pide` holding
    /// item `item` into `out`, at most `capacity` bytes, and writes its
    /// length to `len`; [`Status::MISSING`] when it has none or its entity
    /// holds another item, [`Status::BAD_ARGS`] when `capacity` is short,
    /// with `len` written. On the server the server's values, on a client
    /// what the server sent. Any thread. Since minor 38.
    pub item_data_get: extern "system" fn(
        me: ModHandle,
        kind: u32,
        pide: u32,
        item: u32,
        out: *mut u8,
        capacity: u32,
        len: *mut u32,
    ) -> Status,
    /// Sets the value of kind `kind` on the item entity `pide` holding item
    /// `item` to `len` bytes at `data`, at most 512, or removes it for 0;
    /// the clients that run Enshwick get the change. [`Status::REFUSED`]
    /// on a client alone, [`Status::MISSING`] before the world has loaded.
    /// Any thread. Since minor 38.
    pub item_data_set: extern "system" fn(
        me: ModHandle,
        kind: u32,
        pide: u32,
        item: u32,
        data: *const u8,
        len: u32,
    ) -> Status,
    /// Calls `callback` with `user` once per frame of the client's
    /// interface, on the thread that draws it, while the mod is on; the
    /// callback draws with [`HostApi::ui_widget`]. Client only
    /// ([`Status::MISSING`] on a server); when the interface could not be
    /// linked, the callback never runs. Allowed during `enshwick_mod_init`
    /// only. Since minor 39.
    pub ui_frame:
        extern "system" fn(me: ModHandle, callback: UiFrameFn, user: *mut c_void) -> Status,
    /// Draws `widget` in the frame `frame` names and writes back its
    /// `index`, `value` and `result`. Only inside the mod's frame callback
    /// ([`Status::WRONG_THREAD`] elsewhere); [`Status::REFUSED`] for a
    /// widget outside a window or an [`UiWidget::END`] without one;
    /// [`Status::MISSING`] for a widget this game build lacks. Since minor
    /// 39.
    pub ui_widget: extern "system" fn(me: ModHandle, frame: u64, widget: *mut UiWidget) -> Status,
    /// Queues `dialog` and calls `callback` with the answer and `user` once
    /// the player gives it; dialogs show one at a time, in order. Any
    /// thread. Client only ([`Status::MISSING`] on a server); when the
    /// interface could not be linked, the dialog shows as a system message
    /// box. [`Status::REFUSED`] when too many wait; [`Status::WRONG_THREAD`]
    /// for [`UiDialog::WAITING`] on the thread that draws the interface. On
    /// an error status `callback` is never called. Since minor 39.
    pub ui_dialog: extern "system" fn(
        me: ModHandle,
        dialog: *const UiDialog,
        callback: UiAnswerFn,
        user: *mut c_void,
    ) -> Status,
    /// Calls `callback` with `user` once per frame while the game draws its
    /// HUD in game, hidden with it; the callback draws labels and bars with
    /// [`HostApi::ui_widget`] at their `anchor`. Client only
    /// ([`Status::MISSING`] on a server). Allowed during
    /// `enshwick_mod_init` only. Since minor 40.
    pub ui_hud: extern "system" fn(me: ModHandle, callback: UiFrameFn, user: *mut c_void) -> Status,
    /// Replaces the mod's markers on the client's map with the `count` at
    /// `markers`, copied; none clears them. Any thread. Client only
    /// ([`Status::MISSING`] on a server); [`Status::BAD_ARGS`] past 256
    /// markers. Since minor 40.
    /// Every entry is as large as the header of the first one says.
    pub ui_markers:
        extern "system" fn(me: ModHandle, markers: *const UiMarker, count: u32) -> Status,
    /// Makes `callback` with `user` the mod's check of a new settings text,
    /// which the loader runs before it writes a change from `/config` or the
    /// Mods screen; a refusal keeps the file. Allowed during
    /// `enshwick_mod_init` only. Since minor 41.
    pub config_check:
        extern "system" fn(me: ModHandle, callback: ConfigCheckFn, user: *mut c_void) -> Status,
    /// Hands the loader the mod's texts, a TOML table per language code as
    /// [`crate::i18n::Texts`] reads them, for the loader's own screens: the
    /// Mods screen labels the mod's settings with them. A later call
    /// replaces them. Since minor 43.
    pub texts: extern "system" fn(me: ModHandle, toml: Str) -> Status,
    /// Serves `len` bytes at `data` as game content, the pixels of a
    /// texture the mod serves, and writes the content hash a resource names
    /// them by to `out`; the same bytes give the same hash. The loader
    /// keeps the bytes for the life of the process. Any thread. Since minor
    /// 44.
    pub content_serve:
        extern "system" fn(me: ModHandle, data: *const u8, len: u32, out: *mut [u32; 4]) -> Status,
    /// Copies the game's own content `hash` from its archive into `out`, at
    /// most `capacity` bytes, and writes its length to `len`;
    /// [`Status::MISSING`] for a hash the archive lacks or keeps
    /// compressed, [`Status::BAD_ARGS`] when `capacity` is short, with
    /// `len` written. Reads the archive's files, never the game's memory.
    /// Any thread. Since minor 44.
    pub content_read: extern "system" fn(
        me: ModHandle,
        hash: *const [u32; 4],
        out: *mut u8,
        capacity: u32,
        len: *mut u32,
    ) -> Status,
    /// [`Status::OK`] when this client shows the UI images mods serve: the
    /// content and UI registry links run and the registry knows the served
    /// images; [`Status::MISSING`] otherwise, and always on a server. Any
    /// thread. Since minor 44.
    pub icons_ready: extern "system" fn(me: ModHandle) -> Status,
    /// Defines the stat kind `id` with `def`, or opens it with a null
    /// `def`, and writes its index and key to `out`. A mod defines kinds in
    /// its own namespace (`name` or `<mod>:name`); any mod opens any kind,
    /// the loader's built-in ones (`enshwick:<name>`) among them. Every mod
    /// gets the same index for one id. [`Status::BAD_ARGS`] for a malformed
    /// id or definition, [`Status::REFUSED`] for a kind defined twice or in
    /// another mod's namespace. Allowed during `enshwick_mod_init` only.
    /// Since minor 45.
    pub stat_open: extern "system" fn(
        me: ModHandle,
        id: Str,
        def: *const StatDefData,
        out: *mut StatRef,
    ) -> Status,
    /// Writes the definition of stat `stat` to `def` and its full id to
    /// `id`, a string the loader keeps for the process; [`Status::MISSING`]
    /// for a kind opened but never defined. Any thread. Since minor 45.
    pub stat_info:
        extern "system" fn(me: ModHandle, stat: u32, def: *mut StatDefData, id: *mut Str) -> Status,
    /// Writes the value of stat `stat` on `entity` to `out`: its default
    /// when nothing modifies it. Values are worked out on the server after
    /// each step a source changed; a client answers defaults. Any thread,
    /// without locks. Since minor 45.
    pub stat_value:
        extern "system" fn(me: ModHandle, entity: u32, stat: u32, out: *mut f32) -> Status,
    /// Sets or removes a modifier the mod keeps on an entity; the value
    /// follows after the next step. [`Status::REFUSED`] on a client alone,
    /// or for a saved modifier on an entity that is no player. Any thread.
    /// Since minor 45.
    pub stat_modify: extern "system" fn(me: ModHandle, request: *const StatModify) -> Status,
    /// Gives the buff type `buff` the `count` modifiers at `modifiers`:
    /// they count toward every watched entity that has the buff. Allowed
    /// during `enshwick_mod_init` only. Since minor 45.
    pub stat_buff: extern "system" fn(
        me: ModHandle,
        buff: u32,
        modifiers: *const StatModifierData,
        count: u32,
    ) -> Status,
    /// Calls `callback` with `user` for every stat value that changed on a
    /// watched entity, on the game thread after the step that changed it.
    /// Allowed during `enshwick_mod_init` only. Since minor 45.
    pub stats_changed:
        extern "system" fn(me: ModHandle, callback: StatChangeFn, user: *mut c_void) -> Status,
    /// Writes up to `capacity` modifiers that count toward stat `stat` on
    /// `entity`, in the order they apply, and how many there are to
    /// `count`; every stat for `u32::MAX`. Any thread. Since minor 45.
    pub stat_sources: extern "system" fn(
        me: ModHandle,
        entity: u32,
        stat: u32,
        out: *mut StatSource,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Watches `entity` for the items it wears and the buffs it has, or
    /// stops for `watch` 0: players are always watched, and so is every
    /// entity a mod keeps a modifier on. Any thread. Since minor 45.
    pub stat_watch: extern "system" fn(me: ModHandle, entity: u32, watch: u8) -> Status,
    /// Defines the mod event `spec` describes, named `<id>.<name>` after
    /// the defining mod, and writes the id [`HostApi::send_event`] takes;
    /// [`Status::REFUSED`] for a name outside the mod's id or defined
    /// twice. Allowed during `enshwick_mod_init` only. Since minor 47.
    pub define_event:
        extern "system" fn(me: ModHandle, spec: *const EventSpec, out: *mut u32) -> Status,
    /// Calls `callback` with `user` and the event's data on every mod event
    /// `spec` names, once its definition, checked against `spec`'s layout
    /// when every mod is initialised, matches. Allowed during
    /// `enshwick_mod_init` only. Since minor 47.
    pub subscribe_event: extern "system" fn(
        me: ModHandle,
        spec: *const EventSpec,
        callback: ModEventFn,
        user: *mut c_void,
    ) -> Status,
    /// Hands `data` to every subscriber of the mod's own event `event`, on
    /// this thread, before it returns; reaches no one before every mod is
    /// initialised. [`Status::REFUSED`] for an event another mod defined.
    /// Any thread. Since minor 47.
    pub send_event: extern "system" fn(me: ModHandle, event: u32, data: *const c_void) -> Status,
    /// Defines the affix `id` in the mod's namespace with `def` and writes
    /// its index and key to `out`. Allowed during `enshwick_mod_init` only.
    /// Since minor 48.
    pub affix_open: extern "system" fn(
        me: ModHandle,
        id: Str,
        def: *const AffixDefData,
        out: *mut StatRef,
    ) -> Status,
    /// Rolls affixes onto an item from every mod's affixes that fit its
    /// category, replacing those it had, and writes how many to `count`;
    /// [`Status::REFUSED`] for a [`AffixRoll::ONCE`] roll of an item that
    /// rolled before, or off the server. On the server's game thread only.
    /// Since minor 48.
    pub affix_roll:
        extern "system" fn(me: ModHandle, roll: *const AffixRoll, count: *mut u32) -> Status,
    /// Writes up to `capacity` lines of what the item gives, its own
    /// modifiers first and then each affix's, and how many to `count`. On a
    /// client, from what the server sent. Any thread. Since minor 48.
    pub item_lines: extern "system" fn(
        me: ModHandle,
        pide: u32,
        item: u32,
        out: *mut ItemLine,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Writes the text key and rarity of affix `affix` (an index) to `text`
    /// and `rarity`. Any thread. Since minor 48.
    pub affix_info:
        extern "system" fn(me: ModHandle, affix: u32, text: *mut Str, rarity: *mut u8) -> Status,
    /// Registers the mod's migrations, `set`, copied: the loader runs the
    /// settings steps on the settings file now, and every step on each
    /// world's files when the world loads, once per world, and records what
    /// ran in `mod-data/enshwick/migrations.toml`. [`Status::REFUSED`] for
    /// a second call or a migration that failed on the settings file, with
    /// the reason logged. Allowed during `enshwick_mod_init` only. Since
    /// minor 49.
    pub migrate: extern "system" fn(me: ModHandle, set: *const MigrationSet) -> Status,
    /// Declares the content id `id`, `<mod>:<name>` in the mod's own
    /// namespace, and writes its GUID to `out`; [`Status::REFUSED`] for a
    /// malformed id, one outside the namespace, or one whose game id
    /// another content id has, which turns the mod off with both ids and
    /// both mods named. Allowed during `enshwick_mod_init` only. Since
    /// minor 50.
    pub declare_content: extern "system" fn(me: ModHandle, id: Str, out: *mut [u8; 16]) -> Status,
    /// Opens the entity data kind `id`, as [`HostApi::item_data_open`] opens
    /// an item kind, and writes its index to `out`; with
    /// [`EntityDataFlags::SENT`] the clients that run Enshwick get its
    /// values. [`HostApi::item_data_get`] and [`HostApi::item_data_set`]
    /// read and write it with `item` 0; a set runs on the server's game
    /// thread. [`Status::REFUSED`] for an id opened as an item kind. Allowed
    /// during `enshwick_mod_init` only. Since minor 52.
    pub entity_data_open:
        extern "system" fn(me: ModHandle, id: Str, flags: EntityDataFlags, out: *mut u32) -> Status,
    /// Lists the mods in `mods/` that a present mod replaces (see
    /// [`Relation::REPLACES`]), which the loader never started, as
    /// [`HostApi::mods`] lists the loaded ones, with
    /// [`ModInfo::replaced_by`] set. Any thread. Since minor 54.
    pub replaced: extern "system" fn(
        me: ModHandle,
        out: *mut ModInfo,
        entry_size: u32,
        capacity: u32,
        count: *mut u32,
    ) -> Status,
    /// Registers a filter on the knowledge clients ask the server for in
    /// their input, which a vanilla game grants unchecked. Filters run in
    /// load order after the loader's guard, each with the verdict so far,
    /// and the last answer decides. It runs on the server's worker threads,
    /// in the dedicated server or a hosting client. [`Status::MISSING`] on
    /// a build without the input site. Allowed during `enshwick_mod_init`
    /// only. Since minor 55.
    pub knowledge_input_filter:
        extern "system" fn(me: ModHandle, callback: KnowledgeInputFn, user: *mut c_void) -> Status,
    /// The game language the client last asked a text in, as the game
    /// numbers them (see [`crate::i18n::Language`]); `0`, the fallback, on a
    /// server and before the client's first text. Any thread. Since minor
    /// 56.
    pub language: extern "system" fn(me: ModHandle) -> u32,
    /// Replaces the live game settings with `data` as
    /// [`HostApi::set_game_settings`] does, and keeps the fields with a
    /// byte set in `mask` (as long as `data`) with the running world: the
    /// loader applies them again each time the world loads, by the end of
    /// its first step. [`Status::MISSING`] while no world runs.
    /// On the server's game thread only. Since minor 57.
    pub keep_game_settings:
        extern "system" fn(me: ModHandle, data: *const u8, mask: *const u8, size: u32) -> Status,
    /// Forgets the game settings kept for the running world and puts back
    /// the values it loaded with. On the server's game thread only. Since
    /// minor 57.
    pub forget_game_settings: extern "system" fn(me: ModHandle) -> Status,
    /// Replaces the updates of installed mods the mod shows on the Mods
    /// screen with `toml`, as [`crate::updates::to_toml`] writes them, each
    /// with a button when it is ready. [`Status::BAD_ARGS`] for text that is
    /// no such list; [`Status::MISSING`] on a server. Any thread. Since
    /// minor 58.
    pub updates_set: extern "system" fn(me: ModHandle, toml: Str) -> Status,
    /// Makes `callback` with `user` the mod's answer to the player picking
    /// one of its updates; [`Status::REFUSED`] for a second callback. Allowed during `enshwick_mod_init` only. Since minor 58.
    pub update_pick:
        extern "system" fn(me: ModHandle, callback: UpdatePickFn, user: *mut c_void) -> Status,
    /// Takes knowledge `id`, of `kind`, back from the player whose character
    /// is `entity`, or from the world for 0 or an id of the world's, and has
    /// the clients sent it again: a [`KnowledgeInfo::BOOL`] cleared, `value`
    /// taken from a [`KnowledgeInfo::COUNTER`] (down to 0), bit `value` of
    /// [`KnowledgeInfo::FLAGS`] cleared. Saved with the next save; what
    /// followed the grant, such as experience, stays.
    /// [`Status::BAD_ARGS`] for an unknown id, a kind that is not the id's or
    /// a bit past 31, [`Status::REFUSED`] for a player not in the game or
    /// whose save has not loaded yet. On the server's game thread only.
    /// Since minor 59.
    pub revoke_knowledge:
        extern "system" fn(me: ModHandle, entity: u32, id: u32, kind: u32, value: u32) -> Status,
    /// Grants knowledge `id` of a player's, as [`HostApi::grant_knowledge`]
    /// does, or takes it back as [`HostApi::revoke_knowledge`] does when
    /// `revoke` is 1, for the account whose [`PlayerInfo::key`] is `key`:
    /// now when its character is in the game, else once its next character
    /// has loaded in this world; the loader keeps waiting changes with the
    /// world. [`Status::BAD_ARGS`] for an unknown id, an id of the world's
    /// or a kind that is not the id's, [`Status::MISSING`] while no world
    /// runs. On the server's game thread only. Since minor 59.
    pub account_knowledge: extern "system" fn(
        me: ModHandle,
        key: u64,
        id: u32,
        kind: u32,
        value: u32,
        revoke: u32,
    ) -> Status,
    /// Copies the game's own resource `key` from its archive into `out`, at
    /// most `capacity` bytes, as the game reads it before it resolves its
    /// references, and writes its length to `len`; [`Status::MISSING`] for
    /// a key the archive lacks, [`Status::BAD_ARGS`] when `capacity` is
    /// short, with `len` written. Reads the archive's files, never the
    /// game's memory. Any thread. Since minor 61.
    pub resource_read: extern "system" fn(
        me: ModHandle,
        key: *const ResourceKey,
        out: *mut u8,
        capacity: u32,
        len: *mut u32,
    ) -> Status,
    /// Writes the game's own text of tag `id` in `language`, as the game
    /// numbers them (0 for English), to `out`, valid for the life of the
    /// process; a text missing in `language` falls back to English as the
    /// game's does. Read from the game's archive, the first call for a
    /// language reads its table. [`Status::MISSING`] for a tag the game
    /// lacks, and always on a server, whose archive has no texts. Any
    /// thread. Since minor 61.
    pub game_text:
        extern "system" fn(me: ModHandle, id: u32, language: u32, out: *mut Str) -> Status,
    /// During `enshwick_mod_init`, by a mod that serves `key`: keeps the
    /// base of `key` when it is first built, so it can be built again.
    /// Afterwards: builds `key` again from that base with the patches of
    /// every mod still on, as values they read changed, and writes the
    /// bytes that differ over the resource the game holds; [`Status::OK`]
    /// with nothing done while `key` was not built yet, as its first build
    /// takes the values in force. Any thread. [`Status::MISSING`] when its
    /// base was not kept, [`Status::REFUSED`] when the new build differs in length, which the
    /// game's copy cannot take, [`Status::BAD_ARGS`] for a key the mod does
    /// not serve. Each rebuild keeps the patches' new bytes for the process.
    /// Since minor 63.
    pub rebuild_resource: extern "system" fn(me: ModHandle, key: *const ResourceKey) -> Status,
    /// Adds the calling mod's rule that disables recipes (`disable` 1), or
    /// takes its own rule of that kind and name back (`disable` 0). `name`
    /// names a recipe, an item every recipe of which is disabled, or a
    /// workshop, as `kind` says, in the forms the loader's recipe file
    /// takes; the loader resolves it against the recipe registry and logs a
    /// name it cannot resolve. The server's recipe file wins over every
    /// mod's rules. Any thread; applied within a second. Since minor 64.
    pub recipe_rule:
        extern "system" fn(me: ModHandle, kind: RecipeRuleKind, name: Str, disable: u32) -> Status,
    /// Writes 1 to `out` when the recipe with game id `recipe` is disabled
    /// in this process now, else 0: on a client joined to a server, by the
    /// server's rules. Any thread. Since minor 64.
    pub recipe_disabled: extern "system" fn(me: ModHandle, recipe: u32, out: *mut u32) -> Status,
}

/// What a recipe rule of [`HostApi::recipe_rule`] names.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::RecipeRuleKind;
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

/// Flags of [`HostApi::entity_data_open`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::EntityDataFlags;
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

/// One step of a mod migration, as [`HostApi::migrate`] takes it.
///
/// Frozen: written as an array.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{MigrationStep, Str};
///
/// let step = MigrationStep {
///     kind: MigrationStep::RENAME_SETTING,
///     from: Str::new("cost"),
///     to: Str::new("cost_multiplier"),
///     from_guid: [0; 16],
///     to_guid: [0; 16],
/// };
/// assert_eq!(step.kind, 1);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct MigrationStep {
    /// What the step does: one of the constants below.
    pub kind: u32,
    /// The setting or store the step reads, or renames from.
    pub from: Str,
    /// The setting or store a rename writes to; empty otherwise.
    pub to: Str,
    /// The GUID content had, for [`MigrationStep::RENAME_CONTENT`].
    pub from_guid: [u8; 16],
    /// The GUID content has now, for [`MigrationStep::RENAME_CONTENT`].
    pub to_guid: [u8; 16],
}

impl MigrationStep {
    /// Renames the setting `from` to `to`, value and comments kept.
    pub const RENAME_SETTING: u32 = 1;
    /// Changes the value of the setting `from` through the mod's
    /// [`MigrateFn`], which gets and returns a TOML value.
    pub const MAP_SETTING: u32 = 2;
    /// Removes the setting `from`.
    pub const REMOVE_SETTING: u32 = 3;
    /// Renames the world store `from` to `to`.
    pub const RENAME_STORE: u32 = 4;
    /// Changes the world store `from` through the mod's [`MigrateFn`], which
    /// gets and returns the whole TOML file.
    pub const MAP_STORE: u32 = 5;
    /// Reads content saved under `from_guid` as the content now under
    /// `to_guid`.
    pub const RENAME_CONTENT: u32 = 6;
}

/// One mod migration: an id and its steps, in order.
///
/// Frozen: written as an array.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Migration, Str};
///
/// let migration = Migration { id: Str::new("2"), steps: core::ptr::null(), count: 0 };
/// assert_eq!(migration.count, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Migration {
    /// The migration's id, unique in the mod; the record keeps it.
    pub id: Str,
    /// Its steps.
    pub steps: *const MigrationStep,
    /// How many `steps` points at.
    pub count: u32,
}

/// Changes a value for a migration step: `migration` and `step` index the
/// set, `input` is a TOML value ([`MigrationStep::MAP_SETTING`]) or a whole
/// TOML file ([`MigrationStep::MAP_STORE`]), and the answer goes to `out`,
/// valid until the next call; an error status with the reason in `out`
/// fails the migration.
pub type MigrateFn = unsafe extern "system" fn(
    user: *mut c_void,
    migration: u32,
    step: u32,
    input: Str,
    out: *mut Str,
) -> Status;

/// Every migration of a mod, oldest first, for [`HostApi::migrate`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Header, MigrationSet};
///
/// assert_eq!(Header::of::<MigrationSet>().size as usize, size_of::<MigrationSet>());
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct MigrationSet {
    /// Size and version of this struct.
    pub header: Header,
    /// The migrations, oldest first.
    pub migrations: *const Migration,
    /// How many `migrations` points at.
    pub count: u32,
    /// Changes values for the steps that map them.
    pub map: MigrateFn,
    /// The pointer `map` takes.
    pub user: *mut c_void,
}

/// One field of a service table or mod event, as its layout hash sees it.
///
/// Frozen: written as an array.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{LayoutField, Str};
///
/// let field = LayoutField { name: Str::new("price"), hash: 7 };
/// assert_eq!(field.hash, 7);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LayoutField {
    /// The field's name.
    pub name: Str,
    /// The hash of the field's name, its type as written, size, alignment
    /// and offset.
    pub hash: u64,
}

/// The layout of a service table or mod event, field by field.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::Layout;
///
/// let layout = Layout { fields: core::ptr::null(), count: 0, size: 0 };
/// assert_eq!(layout.count, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    /// The fields, in declaration order.
    pub fields: *const LayoutField,
    /// How many `fields` points at.
    pub count: u32,
    /// The size of the whole type.
    pub size: u32,
}

/// A service a mod publishes, or the one it asks for.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Header, Layout, ServiceSpec, Str};
///
/// let spec = ServiceSpec {
///     header: Header::of::<ServiceSpec>(),
///     name: Str::new("trading-post.prices"),
///     major: 1,
///     layout: Layout { fields: core::ptr::null(), count: 0, size: 0 },
///     table: core::ptr::null(),
/// };
/// assert_eq!(spec.major, 1);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ServiceSpec {
    /// Size and version of this struct.
    pub header: Header,
    /// The service's name, `<publisher id>.<what>`.
    pub name: Str,
    /// The incompatible version: tables of two majors share nothing.
    pub major: u32,
    /// The table's fields; tables grow by appending.
    pub layout: Layout,
    /// The `'static` table; null when asking.
    pub table: *const c_void,
}

/// What [`HostApi::service`] found.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::ServiceFound;
///
/// let found = ServiceFound::EMPTY;
/// assert!(found.table.is_null());
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ServiceFound {
    /// Size and version of this struct.
    pub header: Header,
    /// The publisher's table, valid for the process.
    pub table: *const c_void,
    /// How many fields the publisher's table has.
    pub fields: u32,
    /// The publisher's mod id, valid for the process.
    pub owner: Str,
    /// The publisher's version, valid for the process.
    pub owner_version: Str,
    /// Why nothing was found, valid for the process; empty on success.
    pub reason: Str,
}

impl ServiceFound {
    /// Nothing found yet, for the loader to fill.
    pub const EMPTY: Self = Self {
        header: Header::of::<Self>(),
        table: core::ptr::null(),
        fields: 0,
        owner: Str::EMPTY,
        owner_version: Str::EMPTY,
        reason: Str::EMPTY,
    };
}

/// A mod event a mod defines, or the one it subscribes to.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{EventSpec, Header, Layout, Str};
///
/// let spec = EventSpec {
///     header: Header::of::<EventSpec>(),
///     name: Str::new("trading-post.sold"),
///     layout: Layout { fields: core::ptr::null(), count: 0, size: 0 },
/// };
/// assert_eq!(spec.layout.size, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct EventSpec {
    /// Size and version of this struct.
    pub header: Header,
    /// The event's name, `<defining mod id>.<what>`.
    pub name: Str,
    /// The event data's fields; events grow by appending.
    pub layout: Layout,
}

/// A mod's handler for a mod event: the event's data, valid for the call,
/// and the pointer given at subscription.
pub type ModEventFn = unsafe extern "system" fn(data: *const c_void, user: *mut c_void);

/// The exports a mod DLL provides, by name.
///
/// # Examples
///
/// ```
/// assert_eq!(enshwick_loader_api::abi::exports::INIT, "enshwick_mod_init");
/// ```
pub mod exports {
    /// `extern "system" fn() -> u32`: the [`ABI_MAJOR`](super::ABI_MAJOR) the mod
    /// was built for.
    pub const ABI: &str = "enshwick_mod_abi";
    /// `extern "system" fn() -> *const Manifest`: static, no side effects.
    pub const QUERY: &str = "enshwick_mod_query";
    /// `extern "system" fn(*const HostApi, ModHandle) -> Status`: registers
    /// hooks, services and settings.
    pub const INIT: &str = "enshwick_mod_init";
    /// `extern "system" fn(*const HostApi, ModHandle)`: after every chain is
    /// armed.
    pub const READY: &str = "enshwick_mod_ready";
    /// `extern "system" fn()`: before a development reload unloads the DLL;
    /// the mod stops what it started. Since minor 53, optional.
    pub const SHUTDOWN: &str = "enshwick_mod_shutdown";
}

const _: () = {
    assert!(size_of::<Header>() == 8);
    assert!(size_of::<Str>() == 16);
    assert!(size_of::<Status>() == 4);
    assert!(size_of::<Fingerprint>() == 20);
    assert!(size_of::<Symbol>() == 24);
    assert!(core::mem::offset_of!(Manifest, header) == 0);
    assert!(core::mem::offset_of!(Manifest, id) == 8);
    assert!(core::mem::offset_of!(Manifest, version) == 24);
    assert!(core::mem::offset_of!(Manifest, side) == 40);
    assert!(core::mem::offset_of!(Manifest, flags) == 44);
    assert!(core::mem::offset_of!(Manifest, dependencies) == 48);
    assert!(core::mem::offset_of!(HookSpec, target) == 0);
    assert!(core::mem::offset_of!(HookSpec, next) == 16);
    assert!(core::mem::offset_of!(HookSpec, flags) == 28);
    assert!(core::mem::offset_of!(HostApi, header) == 0);
    assert!(core::mem::offset_of!(HostApi, host_version) == 8);
    assert!(core::mem::offset_of!(HostApi, build) == 24);
    assert!(core::mem::offset_of!(HostApi, service) == 24 + 11 * 8);
    assert!(core::mem::offset_of!(HostApi, constant) == 24 + 12 * 8);
    assert!(core::mem::offset_of!(HostApi, data_dir) == 24 + 13 * 8);
    assert!(core::mem::offset_of!(HostApi, log_level) == 24 + 14 * 8);
    assert!(core::mem::offset_of!(HostApi, subscribe) == 24 + 15 * 8);
    assert!(core::mem::offset_of!(HostApi, players) == 24 + 20 * 8);
    assert!(core::mem::offset_of!(HostApi, serve_resource) == 24 + 21 * 8);
    assert!(core::mem::offset_of!(HostApi, add_text) == 24 + 22 * 8);
    assert!(core::mem::offset_of!(HostApi, mods) == 24 + 23 * 8);
    assert!(core::mem::offset_of!(HostApi, sync) == 24 + 24 * 8);
    assert!(core::mem::offset_of!(HostApi, code_range) == 24 + 25 * 8);
    assert!(core::mem::offset_of!(HostApi, workshop_recipes) == 24 + 27 * 8);
    assert!(core::mem::offset_of!(HostApi, hosting) == 24 + 28 * 8);
    assert!(core::mem::offset_of!(HostApi, server_world) == 24 + 29 * 8);
    assert!(core::mem::offset_of!(HostApi, command) == 24 + 30 * 8);
    assert!(core::mem::offset_of!(HostApi, rename_filter) == 24 + 33 * 8);
    assert!(core::mem::offset_of!(HostApi, join_missing) == 24 + 34 * 8);
    assert!(core::mem::offset_of!(HostApi, hook_at) == 24 + 35 * 8);
    assert!(core::mem::offset_of!(HostApi, inventory_open) == 24 + 36 * 8);
    assert!(core::mem::offset_of!(HostApi, inventory_submit) == 24 + 38 * 8);
    assert!(core::mem::offset_of!(HostApi, affix_open) == 24 + 118 * 8);
    assert!(size_of::<AffixDefData>() == 8 + 32 + 8 + 16);
    // Frozen: read and written as arrays.
    assert!(size_of::<AffixModifierData>() == 20);
    assert!(size_of::<ItemLine>() == 16);
    assert!(size_of::<AffixRoll>() == 8 + 12 + 4 + 8);
    assert!(core::mem::offset_of!(HostApi, stat_open) == 24 + 107 * 8);
    assert!(size_of::<StatDefData>() == 8 + 16 + 16);
    // Frozen: written as arrays.
    assert!(size_of::<StatRef>() == 8);
    assert!(size_of::<StatModifierData>() == 12);
    assert!(size_of::<StatSource>() == 20);
    assert!(size_of::<StatModify>() == 8 + 8 + 12 + 4 + 8);
    assert!(size_of::<StatChange>() == 8 + 24);
    assert!(core::mem::offset_of!(HostApi, language) == 24 + 127 * 8);
    assert!(size_of::<HostApi>() == 24 + 139 * 8);
    assert!(core::mem::offset_of!(HostApi, recipe_rule) == 24 + 137 * 8);
    assert!(core::mem::offset_of!(HostApi, recipe_disabled) == 24 + 138 * 8);
    assert!(core::mem::offset_of!(HostApi, rebuild_resource) == 24 + 136 * 8);
    assert!(core::mem::offset_of!(HostApi, game_text) == 24 + 135 * 8);
    assert!(core::mem::offset_of!(HostApi, account_knowledge) == 24 + 133 * 8);
    assert!(core::mem::offset_of!(HostApi, update_pick) == 24 + 131 * 8);
    assert!(core::mem::offset_of!(HostApi, replaced) == 24 + 125 * 8);
    assert!(core::mem::offset_of!(HostApi, entity_data_open) == 24 + 124 * 8);
    assert!(core::mem::offset_of!(HostApi, knowledge_input_filter) == 24 + 126 * 8);
    assert!(core::mem::offset_of!(HostApi, forget_game_settings) == 24 + 129 * 8);
    assert!(size_of::<KnowledgeInput>() == 8 + 24);
    assert!(core::mem::offset_of!(HostApi, migrate) == 24 + 122 * 8);
    assert!(core::mem::offset_of!(HostApi, declare_content) == 24 + 123 * 8);
    // Frozen: written as arrays.
    assert!(size_of::<MigrationStep>() == 8 + 16 + 16 + 16 + 16);
    assert!(size_of::<Migration>() == 16 + 8 + 8);
    assert!(size_of::<MigrationSet>() == 8 + 8 + 8 + 8 + 8);
    assert!(core::mem::offset_of!(HostApi, define_event) == 24 + 115 * 8);
    // Frozen: written as an array.
    assert!(size_of::<LayoutField>() == 24);
    assert!(size_of::<Layout>() == 16);
    assert!(size_of::<ServiceSpec>() == 8 + 16 + 8 + 16 + 8);
    assert!(size_of::<ServiceFound>() == 8 + 8 + 8 + 3 * 16);
    assert!(size_of::<EventSpec>() == 8 + 16 + 16);
    assert!(core::mem::offset_of!(HostApi, config_schema) == 24 + 92 * 8);
    assert!(core::mem::offset_of!(HostApi, time_of_day) == 24 + 73 * 8);
    assert!(core::mem::offset_of!(HostApi, set_time_speed) == 24 + 76 * 8);
    assert!(size_of::<DaytimeInfo>() == 8 + 5 * 8 + 8);
    assert!(core::mem::offset_of!(HostApi, game_settings) == 24 + 77 * 8);
    assert!(core::mem::offset_of!(HostApi, damage_filter) == 24 + 79 * 8);
    assert!(size_of::<DamageCheck>() == 8 + 16 + 13 * 4 + 4);
    assert!(core::mem::offset_of!(HostApi, buff_types) == 24 + 80 * 8);
    assert!(core::mem::offset_of!(HostApi, remove_buff) == 24 + 83 * 8);
    // Frozen: written as arrays.
    assert!(size_of::<BuffTypeInfo>() == 8 + 12 + 4 + 8);
    assert!(size_of::<ActiveBuff>() == 8 + 16 + 16);
    assert!(size_of::<BuffRequest>() == 8 + 16 + 8);
    assert!(core::mem::offset_of!(HostApi, knowledge) == 24 + 84 * 8);
    assert!(core::mem::offset_of!(HostApi, quests) == 24 + 86 * 8);
    assert!(size_of::<KnowledgeInfo>() == 8 + 16);
    assert!(size_of::<KnowledgeData>() == 8 + 12);
    assert!(size_of::<PlayerDeathData>() == 8 + 8 + 8 + 12 + 16 + 4 + 8);
    assert!(size_of::<QuestData>() == 8 + 8);
    assert!(core::mem::offset_of!(HostApi, weather_at) == 24 + 87 * 8);
    assert!(core::mem::offset_of!(HostApi, set_weather) == 24 + 89 * 8);
    // Frozen: written as an array.
    assert!(size_of::<WeatherFront>() == 8 + 8 + 24 + 8 + 8);
    assert!(size_of::<ChunkData>() == 8 + 16);
    assert!(core::mem::offset_of!(HostApi, terrain_materials) == 24 + 90 * 8);
    // Frozen: written as an array.
    assert!(size_of::<TerrainMaterialInfo>() == 8 + 12);
    assert!(size_of::<TerrainEdit>() == 8 + 16 + 12 + 8 + 16);
    assert!(size_of::<InventoryOp>() == 32);
    assert!(size_of::<InventoryRequest>() == 32);
    assert!(size_of::<InventoryOpResult>() == 8);
    assert!(size_of::<InventoryResult>() == 24);
    assert!(size_of::<ItemData>() == 20);
    // Frozen: embedded in calls and written as an array.
    assert!(size_of::<PlayerInfo>() == 96);
    assert!(core::mem::offset_of!(PlayerInfo, name) == 26);
    assert!(size_of::<Registers>() == 17 * 8);
    assert!(core::mem::offset_of!(Registers, rsp) == 4 * 8);
    assert!(core::mem::offset_of!(Registers, rflags) == 16 * 8);
    assert!(size_of::<MidHookSpec>() == 32);
    assert!(core::mem::offset_of!(HostApi, status) == 24 + 39 * 8);
    assert!(core::mem::offset_of!(HostApi, run_command) == 24 + 40 * 8);
    assert!(core::mem::offset_of!(HostApi, altars_open) == 24 + 41 * 8);
    assert!(core::mem::offset_of!(HostApi, zone_check_filter) == 24 + 45 * 8);
    assert!(core::mem::offset_of!(HostApi, survivor_lookup) == 24 + 46 * 8);
    assert!(core::mem::offset_of!(HostApi, survivor_status) == 24 + 50 * 8);
    assert!(core::mem::offset_of!(HostApi, interaction_filter) == 24 + 51 * 8);
    assert!(core::mem::offset_of!(HostApi, skill_override) == 24 + 52 * 8);
    assert!(core::mem::offset_of!(HostApi, experience_filter) == 24 + 54 * 8);
    assert!(core::mem::offset_of!(HostApi, base_zones) == 24 + 57 * 8);
    assert!(core::mem::offset_of!(HostApi, game_event) == 24 + 58 * 8);
    assert!(core::mem::offset_of!(HostApi, world_key) == 24 + 62 * 8);
    assert!(core::mem::offset_of!(HostApi, component_write) == 24 + 63 * 8);
    assert!(core::mem::offset_of!(HostApi, teleport) == 24 + 65 * 8);
    assert!(size_of::<TeleportRequest>() == 8 + 8 + 24 + 16);
    assert!(core::mem::offset_of!(HostApi, spawn) == 24 + 66 * 8);
    assert!(size_of::<SpawnRequest>() == 8 + 16 + 24 + 16 + 8 + 8);
    assert!(core::mem::offset_of!(HostApi, build_filter) == 24 + 68 * 8);
    assert!(size_of::<CraftCheck>() == 8 + 16);
    assert!(size_of::<BuildCheck>() == 8 + 8 + 24);
    assert!(core::mem::offset_of!(HostApi, net_ready) == 24 + 71 * 8);
    assert!(size_of::<NetMessage>() == 8 + 8 + 8 + 8 + 8);
    assert!(core::mem::offset_of!(HostApi, notify) == 24 + 72 * 8);
    assert!(core::mem::offset_of!(HostApi, ui_dialog) == 24 + 99 * 8);
    assert!(size_of::<UiFrame>() == 24);
    assert!(size_of::<UiWidget>() == 8 + 16 + 32 + 8 + 32);
    assert!(size_of::<UiMarker>() == 8 + 24 + 16 + 8);
    assert!(core::mem::offset_of!(HostApi, texts) == 24 + 103 * 8);
    assert!(core::mem::offset_of!(HostApi, config_check) == 24 + 102 * 8);
    assert!(size_of::<UiDialog>() == 8 + 48 + 8);
    assert!(size_of::<GameEventData>() == 24);
    // Frozen: written as an array.
    assert!(size_of::<EntityPlace>() == 32);
    assert!(size_of::<SkillCheck>() == 20);
    assert!(size_of::<Comfort>() == 28);
    // Frozen: passed as an array.
    assert!(size_of::<ExperienceRow>() == 20);
    assert!(size_of::<ExperiencePass>() == 48);
    assert!(size_of::<Interaction>() == 8 + 16 + 8 + 24);
    assert!(size_of::<SurvivorLookup>() == 8 + 16 + 24 + 16);
    assert!(size_of::<BedCheck>() == 8 + 12 + 4 + 48);
    assert!(size_of::<Summon>() == 8 + 8 + 16 + 24 + 8 + 4 + 4 + 4 + 4);
    assert!(size_of::<Bounds>() == 48);
    assert!(size_of::<AltarInfo>() == 16 + 17 * 48 + 24);
    assert!(size_of::<AltarZoneSize>() == 24);
    assert!(size_of::<ZoneCheck>() == 8 + 16 + 48 + 8);
    assert!(core::mem::offset_of!(Manifest, updates) == 80);
    assert!(core::mem::offset_of!(Manifest, name) == 120);
    assert!(size_of::<Manifest>() == 120 + 16 * 4 + 16);
    assert!(size_of::<UpdateSource>() == 48);
    assert!(size_of::<ResourceKey>() == 24);
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_null_or_impossible_text_reads_as_none() {
        let null = Str {
            ptr: core::ptr::null(),
            len: 5,
        };
        let huge = Str {
            ptr: c"x".as_ptr().cast(),
            len: usize::MAX,
        };
        // SAFETY: neither is read: both are refused before.
        let (null, huge) = (unsafe { null.as_str() }, unsafe { huge.as_str() });
        assert_eq!((null, huge), (None, None));
    }
}
