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
pub const ABI_MINOR: u16 = 9;

/// Whether `id` is a valid mod id: 1 to 64 characters of `[a-z0-9-]`.
///
/// The id names the mod's DLL and settings file, so it stays portable.
///
/// # Examples
///
/// ```
/// use enshwick_api::abi::is_valid_id;
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
/// use enshwick_api::abi::{Header, Manifest};
///
/// let header = Header::of::<Manifest>();
/// assert_eq!(header.abi_major, enshwick_api::abi::ABI_MAJOR);
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
    /// use enshwick_api::abi::{Header, Manifest};
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

/// A UTF-8 string borrowed for the duration of a call, or for as long as its
/// owner documents.
///
/// # Examples
///
/// ```
/// use enshwick_api::abi::Str;
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
    /// use enshwick_api::abi::Str;
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
    /// use enshwick_api::abi::Str;
    ///
    /// assert!(Str::EMPTY.is_empty());
    /// ```
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The text, or `None` when the bytes are not UTF-8.
    ///
    /// # Safety
    ///
    /// The bytes must be alive and unchanged for the lifetime `'a` the caller
    /// picks.
    ///
    /// # Examples
    ///
    /// See [`Str`].
    #[must_use]
    pub unsafe fn as_str<'a>(&self) -> Option<&'a str> {
        if self.len == 0 {
            return Some("");
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
/// use enshwick_api::abi::Status;
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
    /// read off the server's game thread.
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
/// use enshwick_api::abi::Side;
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
/// use enshwick_api::abi::Level;
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
    /// use enshwick_api::abi::Level;
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
    type Err = UnknownLevel;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "error" => Ok(Self::ERROR),
            "warn" => Ok(Self::WARN),
            "info" => Ok(Self::INFO),
            "debug" => Ok(Self::DEBUG),
            _ => Err(UnknownLevel),
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
/// use enshwick_api::abi::Event;
///
/// assert_ne!(Event::SETTINGS_CHANGED, Event::LOADER_SETTINGS_CHANGED);
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Event(pub u32);

impl Event {
    /// The mod's settings file changed on disk; no data. Raised on a loader
    /// thread, about a second after the change.
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
}

/// The data of [`Event::BUILDING_PLACED`] and [`Event::BUILDING_REMOVED`].
///
/// # Examples
///
/// ```
/// use enshwick_api::abi::{BuildingData, Header};
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
/// # Examples
///
/// ```
/// use enshwick_api::abi::{Header, PlayerInfo};
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
/// use enshwick_api::abi::{Header, TickData};
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
/// use enshwick_api::abi::{Header, PlayerData};
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
/// use enshwick_api::abi::{CommandCall, Header, PlayerInfo, Str};
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
pub type CommandFn =
    unsafe extern "system" fn(call: *const CommandCall, user: *mut c_void, reply: *mut Str);

/// A player renaming an entity, handed to the filters registered with
/// [`HostApi::rename_filter`] before the game applies it.
///
/// # Examples
///
/// ```
/// use enshwick_api::abi::{Header, PlayerInfo, RenameCall, Str};
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
pub type RenameFn = unsafe extern "system" fn(call: *const RenameCall, user: *mut c_void) -> u8;

/// A game resource's key: its GUID in memory order, its type hash and part.
///
/// # Examples
///
/// ```
/// use enshwick_api::abi::ResourceKey;
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
/// and length of bytes that stay valid for the life of the process.
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
/// use enshwick_api::abi::Level;
///
/// assert!("loud".parse::<Level>().is_err());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("a log level is one of \"error\", \"warn\", \"info\" or \"debug\"")]
pub struct UnknownLevel;

/// A game build's identity: the CodeView GUID, in file order, and age.
///
/// # Examples
///
/// ```
/// use enshwick_api::abi::Fingerprint;
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
/// use enshwick_api::abi::{BuildInfo, Fingerprint, Header, Side};
///
/// let build = BuildInfo {
///     header: Header::of::<BuildInfo>(),
///     side: Side::SERVER,
///     base: 0x1_4000_0000,
///     fingerprint: Fingerprint { guid: [0; 16], age: 0 },
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
}

/// One symbol pinned for one game build: a name and its address relative to
/// the image base.
///
/// # Examples
///
/// ```
/// use enshwick_api::abi::{Str, Symbol};
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
/// use enshwick_api::abi::guid;
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
/// use enshwick_api::abi::{BuildRow, Fingerprint, Side, Str, Symbol};
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
/// use enshwick_api::abi::Relation;
///
/// assert_ne!(Relation::REQUIRES, Relation::CONFLICTS);
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Relation(pub u8);

impl Relation {
    /// The other mod must be present in the version range; it loads first.
    pub const REQUIRES: Self = Self(0);
    /// When the other mod is present, it loads first; it may be absent.
    pub const AFTER: Self = Self(1);
    /// The two mods never load together.
    pub const CONFLICTS: Self = Self(2);
}

/// A relation to another mod, by id and SemVer range.
///
/// # Examples
///
/// ```
/// use enshwick_api::abi::{Dependency, Relation, Str};
///
/// let zones = Dependency {
///     id: Str::new("flame-altar-zones"),
///     range: Str::new("^0.1"),
///     relation: Relation::REQUIRES,
/// };
/// assert_eq!(zones.relation, Relation::REQUIRES);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Dependency {
    /// The other mod's id.
    pub id: Str,
    /// A Cargo-style version requirement, such as `^0.1`; empty for any.
    pub range: Str,
    /// How the two relate.
    pub relation: Relation,
}

/// Flags of a [`Manifest`].
///
/// # Examples
///
/// ```
/// use enshwick_api::abi::ManifestFlags;
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
/// use enshwick_api::abi::UpdateKind;
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
    /// are `<component>-v<semver>`, as release-please writes them for a
    /// workspace.
    pub const GITHUB: Self = Self(1);
    /// A JSON document at `location`, an `https://` URL, that names the latest
    /// release; `component` is empty. The updater documents the format.
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
/// use enshwick_api::abi::{Header, Str, UpdateKind, UpdateSource};
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

    /// GitHub releases of `repository` tagged `<component>-v<semver>`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_api::abi::{UpdateKind, UpdateSource};
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

    /// The JSON document at `url` that names the latest release.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_api::abi::{UpdateKind, UpdateSource};
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
/// use enshwick_api::abi::{Header, Manifest, ManifestFlags, Side, Str};
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
}

// SAFETY: a manifest is immutable static data; its pointers refer to statics.
unsafe impl Sync for Manifest {}

/// Flags of a [`HookSpec`].
///
/// # Examples
///
/// ```
/// use enshwick_api::abi::HookFlags;
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
/// use enshwick_api::abi::{HookFlags, HookSpec};
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
/// use enshwick_api::abi::CallerFrame;
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
    /// use enshwick_api::abi::CallerFrame;
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
/// use enshwick_api::abi::{Header, ManifestFlags, ModInfo, Side, Str, UpdateSource};
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
}

/// An opaque handle the loader gives each mod; every call names its caller
/// with it.
///
/// # Examples
///
/// ```
/// use enshwick_api::abi::ModHandle;
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
    /// Writes the running game's identity.
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
    /// it is missing; the text stays valid until the mod's next call to
    /// `settings`.
    pub settings: extern "system" fn(me: ModHandle, defaults: Str, out: *mut Str) -> Status,
    /// Publishes a service: a versioned table of functions other mods may use.
    pub publish:
        extern "system" fn(me: ModHandle, name: Str, version: u32, table: *const c_void) -> Status,
    /// Finds a service another mod published, of a compatible version.
    pub service: extern "system" fn(
        me: ModHandle,
        name: Str,
        version: u32,
        out: *mut *const c_void,
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
    /// until the mod's next call to `sync`. Since minor 6.
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
        callback: CommandFn,
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
        extern "system" fn(me: ModHandle, callback: RenameFn, user: *mut c_void) -> Status,
}

/// The exports a mod DLL provides, by name.
///
/// # Examples
///
/// ```
/// assert_eq!(enshwick_api::abi::exports::INIT, "enshwick_mod_init");
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
    assert!(size_of::<HostApi>() == 24 + 34 * 8);
    assert!(core::mem::offset_of!(Manifest, updates) == 80);
    assert!(size_of::<UpdateSource>() == 48);
    assert!(size_of::<ResourceKey>() == 24);
};
