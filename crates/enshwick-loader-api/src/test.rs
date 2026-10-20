//! A fake Enshwick loader for a mod's unit tests.
//!
//! [`FakeHost`] is configured with what the game and the loader would answer:
//! symbols, systems, constants, reflected types, settings text, the side, the
//! players. [`FakeHost::install`] hands the loader's table to the test as an
//! [`Installed`] fake, which starts the mod through its exports, records what
//! the mod asks for, raises events and calls a linked detour as the game
//! would. A linked hook's chain continues to its target address, so a target
//! set to [`address`] of a plain function stands in for the game's original.
//!
//! # One mod per process
//!
//! A mod initialises once per process, as in the game: its state and the
//! table it talks to are process-wide. Write one test per mod life and run
//! the tests with `cargo nextest`, which gives each test its own process.
//! Under `cargo test` the tests of one binary share the process; the fake
//! then serves one test at a time, and a second test waits until the first
//! drops its [`Installed`].
//!
//! # Examples
//!
//! ```
//! use enshwick_loader_api::abi::{Level, Status};
//! use enshwick_loader_api::{Load, Mod, Next, Off, events, hook};
//!
//! #[derive(serde::Deserialize)]
//! struct Settings {
//!     bonus: u32,
//! }
//!
//! struct Bonus {
//!     bonus: u32,
//! }
//!
//! #[hook]
//! fn reward(next: Next<extern "system" fn(u32) -> u32>, base: u32) -> u32 {
//!     next.get()(base) + Bonus::instance().map_or(0, |state| state.bonus)
//! }
//!
//! impl Mod for Bonus {
//!     fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
//!         enshwick_loader_api::manifest!(id: "bonus", side: SERVER)
//!     }
//!
//!     fn init(load: &mut Load<'_>) -> Result<Self, Off> {
//!         let settings: Settings = load.settings("bonus = 1\n")?;
//!         load.hook(&REWARD, load.symbol("quest.reward")?)?;
//!         load.on::<events::WorldLoaded>(|()| enshwick_loader_api::info!("world loaded"))?;
//!         Ok(Self { bonus: settings.bonus })
//!     }
//! }
//!
//! enshwick_loader_api::declare_mod!(Bonus);
//!
//! extern "system" fn vanilla(base: u32) -> u32 {
//!     base
//! }
//!
//! let loader = enshwick_loader_api::test::FakeHost::new()
//!     .symbol("quest.reward", enshwick_loader_api::test::address(vanilla as extern "system" fn(u32) -> u32))
//!     .settings("bonus = 5\n")
//!     .install();
//! // SAFETY: the exports of `declare_mod!` above.
//! let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
//! assert_eq!(status, Status::OK);
//!
//! let reward = loader.detour(&REWARD).ok_or("not linked")?;
//! assert_eq!(reward(10), 15);
//!
//! assert_eq!(loader.raise::<events::WorldLoaded>(&()), 1);
//! assert_eq!(loader.logs(), [(Level::INFO, "world loaded".to_owned())]);
//! # Ok::<(), &str>(())
//! ```

mod table;

/// The modding book's chapter on tests, whose examples run as doctests here.
#[cfg(doctest)]
#[doc = include_str!("../../../book/src/testing.md")]
#[expect(dead_code, reason = "the doc comment carries the chapter")]
struct BookTesting;

use std::collections::{HashMap, HashSet};
use std::sync::MutexGuard;

use crate::abi::{
    AltarInfo, AltarZoneSize, BedCheck, Bounds, CallerFrame, Comfort, CommandCall, EntityPlace,
    Event, ExperiencePass, ExperienceRow, Fingerprint, Header, HookFlags, HostApi, Interaction,
    InventoryOp, InventoryOpResult, InventoryResult, ItemData, Level, ModHandle, ModInfo,
    PlayerInfo, RenameCall, ResourceKey, Side, SkillCheck, Status, Str, Summon, SurvivorLookup,
    ZoneCheck,
};
use crate::events::Kind;
use crate::{FnPtr, Hook};

/// A widget a frame callback drew, as [`Installed::draw_frame`] reports it.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::test::Drawn;
///
/// let drawn = Drawn::Button { text: "Close".into(), enabled: true };
/// assert!(matches!(drawn, Drawn::Button { .. }));
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Drawn {
    /// A window started.
    Window {
        /// Its id.
        id: String,
        /// Its title.
        title: String,
    },
    /// The window ended.
    End,
    /// A label.
    Label {
        /// Its text.
        text: String,
        /// Its style, `UiWidget::STYLE_*`.
        style: u32,
    },
    /// A button.
    Button {
        /// Its text.
        text: String,
        /// Whether it takes clicks.
        enabled: bool,
    },
    /// A toggle row.
    Toggle {
        /// Its label.
        label: String,
        /// Its state after the frame.
        on: bool,
        /// Whether it takes changes.
        read_only: bool,
    },
    /// A slider row.
    Slider {
        /// Its label.
        label: String,
        /// Its value after the frame.
        value: f32,
        /// Its least value.
        min: f32,
        /// Its greatest value.
        max: f32,
        /// Its stops, 0 for continuous.
        steps: u32,
        /// Whether it takes changes.
        read_only: bool,
    },
    /// A dropdown row.
    Dropdown {
        /// Its label.
        label: String,
        /// Its options.
        options: Vec<String>,
        /// Its option after the frame.
        index: usize,
        /// Whether it takes changes.
        read_only: bool,
    },
    /// A progress bar.
    Progress {
        /// How full, from 0 to 1.
        fraction: f32,
    },
    /// A sound, `UiWidget::SOUND_*`.
    Sound(u32),
}

/// A marker the mod set on the map, as [`Installed::markers`] reports it.
///
/// # Examples
///
/// See [`Installed::markers`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FakeMarker {
    /// Where, in 32.32 fixed-point metres.
    pub position: [i64; 3],
    /// The label.
    pub label: String,
    /// The colour, `UiMarker::WHITE` and on.
    pub color: u32,
}

/// What the player does to a widget, given with [`Installed::input`].
///
/// # Examples
///
/// See [`Installed::input`].
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Input {
    /// Clicks a button.
    Click,
    /// Switches a toggle row.
    Toggle(bool),
    /// Moves a slider row.
    Value(f32),
    /// Picks a dropdown row's option.
    Pick(usize),
}

/// A dialog the mod queued, as [`Installed::dialogs`] reports it.
///
/// # Examples
///
/// See [`Installed::dialogs`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShownDialog {
    /// The title.
    pub title: String,
    /// The message.
    pub message: String,
    /// The detail.
    pub detail: String,
    /// The buttons, bit `i` for answer `i`.
    pub buttons: u32,
}

/// A mod's `enshwick_mod_init` export, as [`declare_mod!`](crate::declare_mod)
/// writes it.
pub type InitFn = unsafe extern "system" fn(*const HostApi, ModHandle) -> Status;

/// A mod's `enshwick_mod_ready` export, as [`declare_mod!`](crate::declare_mod)
/// writes it.
pub type ReadyFn = unsafe extern "system" fn(*const HostApi, ModHandle);

/// The address of `function`, to stand in for a game function's.
///
/// A hook linked on it continues its chain into `function`, so the test sees
/// the detour wrap a plain function as it would wrap the game's.
///
/// # Examples
///
/// ```
/// extern "system" fn vanilla() {}
///
/// let target = enshwick_loader_api::test::address(vanilla as extern "system" fn());
/// assert_ne!(target, 0);
/// ```
#[must_use]
pub fn address<F: FnPtr>(function: F) -> u64 {
    // SAFETY: `FnPtr` implementors are function pointers the size of `usize`.
    let address: usize = unsafe { core::mem::transmute_copy(&function) };
    address as u64
}

/// The server's console as a player: an administrator with no character.
pub(crate) fn console() -> PlayerInfo {
    let mut name = [0u8; 64];
    name[..7].copy_from_slice(b"console");
    PlayerInfo {
        header: Header::of::<PlayerInfo>(),
        slot: u32::MAX,
        entity: 0,
        key: 0,
        permissions: PlayerInfo::CAN_KICK_BAN,
        name_len: 7,
        name,
    }
}

/// Calls a command handler the mod registered; its reply.
pub(crate) fn run_handler(
    callback: crate::abi::CommandCallFn,
    user: usize,
    name: &str,
    player: &PlayerInfo,
    args: &str,
) -> String {
    let call = CommandCall {
        header: Header::of::<CommandCall>(),
        player: *player,
        name: Str::new(name),
        args: Str::new(args),
    };
    let mut reply = Str::EMPTY;
    // SAFETY: a valid call; `user` is the pointer the mod registered with.
    unsafe {
        callback(
            &raw const call,
            core::ptr::with_exposed_provenance_mut(user),
            &raw mut reply,
        );
    };
    // SAFETY: the reply stays valid until the handler runs again.
    unsafe { reply.as_str() }.unwrap_or("").to_owned()
}

/// What the fake loader answers: a builder, installed with
/// [`FakeHost::install`].
///
/// Lookups the test did not configure answer [`Status::MISSING`], as a game
/// build without them would. Settings default to the mod's own defaults, the
/// side to [`Side::SERVER`] and the log level to [`Level::INFO`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Level, Side};
///
/// let loader = enshwick_loader_api::test::FakeHost::new()
///     .side(Side::CLIENT)
///     .system("upgrade_building_range", 0x1_4009_0a60)
///     .constant("ecs.context_row", 0x48)
///     .log_level(Level::DEBUG)
///     .install();
/// assert!(loader.hooks().is_empty());
/// ```
#[derive(Clone, Debug)]
pub struct FakeHost {
    side: Side,
    base: u64,
    fingerprint: Fingerprint,
    handle: ModHandle,
    symbols: HashMap<String, u64>,
    systems: HashMap<String, u64>,
    constants: HashMap<String, u64>,
    field_offsets: HashMap<(String, String), u64>,
    type_sizes: HashMap<String, u32>,
    type_hashes: HashMap<String, u32>,
    settings: Option<String>,
    server_synced: Option<String>,
    data_dir: String,
    log_level: Level,
    caller_frame: CallerFrame,
    components: HashMap<(u32, u32), usize>,
    templates: HashMap<u32, [u8; 16]>,
    players: Vec<PlayerInfo>,
    mods: Vec<ModInfo>,
    hosting: bool,
    server_world: u64,
    join_missing: Vec<ModInfo>,
    services: HashMap<(String, u32), usize>,
    refused_hooks: HashSet<u64>,
    code: (u64, u64),
    workshop_recipes: u32,
    items: HashMap<u32, ItemData>,
    no_inventories: bool,
    foreign_commands: HashMap<String, String>,
    placed: Vec<(u32, EntityPlace)>,
    world_key: Option<String>,
    refused_writes: HashSet<u32>,
    net_peers: HashSet<u64>,
}

impl Default for FakeHost {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeHost {
    /// A loader for a server with nothing configured.
    ///
    /// # Examples
    ///
    /// See [`FakeHost`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            side: Side::SERVER,
            base: 0x1_4000_0000,
            fingerprint: Fingerprint {
                guid: [0; 16],
                age: 0,
            },
            handle: ModHandle(1),
            symbols: HashMap::new(),
            systems: HashMap::new(),
            constants: HashMap::new(),
            field_offsets: HashMap::new(),
            type_sizes: HashMap::new(),
            type_hashes: HashMap::new(),
            settings: None,
            server_synced: None,
            data_dir: "mod-data/test".to_owned(),
            log_level: Level::INFO,
            caller_frame: CallerFrame::default(),
            components: HashMap::new(),
            templates: HashMap::new(),
            players: Vec::new(),
            mods: Vec::new(),
            hosting: false,
            server_world: 0,
            join_missing: Vec::new(),
            services: HashMap::new(),
            refused_hooks: HashSet::new(),
            code: (0, 0),
            workshop_recipes: 512,
            items: HashMap::new(),
            no_inventories: false,
            foreign_commands: HashMap::new(),
            placed: Vec::new(),
            world_key: None,
            refused_writes: HashSet::new(),
            net_peers: HashSet::new(),
        }
    }

    /// The side the game runs as.
    ///
    /// # Examples
    ///
    /// See [`FakeHost`].
    #[must_use]
    pub fn side(mut self, side: Side) -> Self {
        self.side = side;
        self
    }

    /// Whether a client hosts a game now; the dedicated server always runs
    /// one.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::Side;
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new().side(Side::CLIENT).hosting(true).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn hosting(mut self, hosting: bool) -> Self {
        self.hosting = hosting;
        self
    }

    /// The entity world of the running server, which a system context of
    /// that world holds; 0 for none.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().server_world(0x2000).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn server_world(mut self, world: u64) -> Self {
        self.server_world = world;
        self
    }

    /// The game's image base.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().base(0x1_4000_0000).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn base(mut self, base: u64) -> Self {
        self.base = base;
        self
    }

    /// The game build's fingerprint.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::{Fingerprint, guid};
    ///
    /// let build = Fingerprint { guid: guid("1c129660cd6efc4f9f0b21652d5ff2bf"), age: 387 };
    /// let loader = enshwick_loader_api::test::FakeHost::new().fingerprint(build).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn fingerprint(mut self, fingerprint: Fingerprint) -> Self {
        self.fingerprint = fingerprint;
        self
    }

    /// The handle the mod receives; `ModHandle(1)` by default.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::ModHandle;
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new().handle(ModHandle(7)).install();
    /// assert_eq!(loader.handle(), ModHandle(7));
    /// ```
    #[must_use]
    pub fn handle(mut self, handle: ModHandle) -> Self {
        self.handle = handle;
        self
    }

    /// The address `Load::symbol(name)` finds.
    ///
    /// # Examples
    ///
    /// See [`crate`].
    #[must_use]
    pub fn symbol(mut self, name: &str, address: u64) -> Self {
        self.symbols.insert(name.to_owned(), address);
        self
    }

    /// The address `Load::system(name)` finds.
    ///
    /// # Examples
    ///
    /// See [`FakeHost`].
    #[must_use]
    pub fn system(mut self, name: &str, address: u64) -> Self {
        self.systems.insert(name.to_owned(), address);
        self
    }

    /// The value `Load::constant(name)` finds.
    ///
    /// # Examples
    ///
    /// See [`FakeHost`].
    #[must_use]
    pub fn constant(mut self, name: &str, value: u64) -> Self {
        self.constants.insert(name.to_owned(), value);
        self
    }

    /// The offset `Load::field_offset(type_name, field)` finds.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new()
    ///     .field_offset("keen::ecs::TriggerShape", "rangeX", 4)
    ///     .type_size("keen::ecs::TriggerShape", 16)
    ///     .type_hash("keen::ecs::Name", 0x027a_c564)
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn field_offset(mut self, type_name: &str, field: &str, offset: u64) -> Self {
        self.field_offsets
            .insert((type_name.to_owned(), field.to_owned()), offset);
        self
    }

    /// The size `Load::type_size(type_name)` finds.
    ///
    /// # Examples
    ///
    /// See [`FakeHost::field_offset`].
    #[must_use]
    pub fn type_size(mut self, type_name: &str, size: u32) -> Self {
        self.type_sizes.insert(type_name.to_owned(), size);
        self
    }

    /// The ECS type hash of the component type `type_name`.
    ///
    /// # Examples
    ///
    /// See [`FakeHost::field_offset`].
    #[must_use]
    pub fn type_hash(mut self, type_name: &str, hash: u32) -> Self {
        self.type_hashes.insert(type_name.to_owned(), hash);
        self
    }

    /// The text of the mod's settings file; without it the mod reads its own
    /// defaults, as on first start.
    ///
    /// # Examples
    ///
    /// See [`crate`].
    #[must_use]
    pub fn settings(mut self, text: &str) -> Self {
        self.settings = Some(text.to_owned());
        self
    }

    /// The path `Load::data_dir` returns; `mod-data/test` by default. The
    /// fake creates nothing there.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().data_dir("target/mod-data/bonus").install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn data_dir(mut self, path: &str) -> Self {
        path.clone_into(&mut self.data_dir);
        self
    }

    /// The most detailed level the loader keeps for the mod's lines.
    ///
    /// # Examples
    ///
    /// See [`FakeHost`].
    #[must_use]
    pub fn log_level(mut self, level: Level) -> Self {
        self.log_level = level;
        self
    }

    /// The caller's context a hook with
    /// [`HookFlags::CALLER_FRAME`] reads.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::CallerFrame;
    ///
    /// let frame = CallerFrame { rsp: 0x1000, ..CallerFrame::default() };
    /// let loader = enshwick_loader_api::test::FakeHost::new().caller_frame(frame).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn caller_frame(mut self, frame: CallerFrame) -> Self {
        self.caller_frame = frame;
        self
    }

    /// The component of type `type_hash` that `entity` has, by address; it
    /// must stay valid while the fake is installed.
    ///
    /// # Examples
    ///
    /// ```
    /// static HEALTH: [f32; 2] = [80.0, 100.0];
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new()
    ///     .component(4096, 0x027a_c564, HEALTH.as_ptr().cast())
    ///     .template(4096, [0x3a; 16])
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn component(
        mut self,
        entity: u32,
        type_hash: u32,
        data: *const core::ffi::c_void,
    ) -> Self {
        self.components
            .insert((entity, type_hash), data.expose_provenance());
        self
    }

    /// A peer that takes mod messages: a player's account key on the
    /// server, or [`Peer::Server`](crate::net::Peer::Server) on a client.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::net::Peer;
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new().net_peer(Peer::Player(7)).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn net_peer(mut self, peer: crate::net::Peer) -> Self {
        let raw = match peer {
            crate::net::Peer::Server => crate::abi::NetMessage::SERVER,
            crate::net::Peer::Player(key) => key,
        };
        self.net_peers.insert(raw);
        self
    }

    /// Refuses component writes of type `type_hash`, as the loader refuses
    /// the copies the game keeps for clients.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().refuse_write(0x216e_b5f9).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn refuse_write(mut self, type_hash: u32) -> Self {
        self.refused_writes.insert(type_hash);
        self
    }

    /// An entity in the world with a component of type `type_hash`, at
    /// `position` in metres, for [`World::entities`](crate::world::World::entities)
    /// and [`World::near`](crate::world::World::near).
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new()
    ///     .placed(0x027a_c564, 4096, [1.0, 2.0, 3.0])
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn placed(mut self, type_hash: u32, entity: u32, position: [f64; 3]) -> Self {
        self.placed.push((
            type_hash,
            EntityPlace {
                entity,
                reserved: 0,
                position: crate::ecs::fixed(position),
            },
        ));
        self
    }

    /// The key of the world the server runs, for
    /// [`World::key`](crate::world::World::key) and world stores.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().world_key("world0-1a2b3c4d").install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn world_key(mut self, key: &str) -> Self {
        self.world_key = Some(key.to_owned());
        self
    }

    /// The template GUID of `entity`, in file order.
    ///
    /// # Examples
    ///
    /// See [`FakeHost::component`].
    #[must_use]
    pub fn template(mut self, entity: u32, guid: [u8; 16]) -> Self {
        self.templates.insert(entity, guid);
        self
    }

    /// A connected player with a character in the world.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::{Header, PlayerInfo};
    ///
    /// let player = PlayerInfo {
    ///     header: Header::of::<PlayerInfo>(),
    ///     slot: 0,
    ///     entity: 2,
    ///     key: 7,
    ///     permissions: PlayerInfo::CAN_KICK_BAN,
    ///     name_len: 0,
    ///     name: [0; 64],
    /// };
    /// let loader = enshwick_loader_api::test::FakeHost::new().player(player).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn player(mut self, player: PlayerInfo) -> Self {
        self.players.push(player);
        self
    }

    /// A loaded mod `enshwick_loader_api::mods()` lists, in order; the loader
    /// itself comes first in the game.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::{Header, ManifestFlags, ModInfo, Side, Str, UpdateSource};
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new()
    ///     .loaded(ModInfo {
    ///         header: Header::of::<ModInfo>(),
    ///         id: Str::new("enshwick"),
    ///         version: Str::new("0.2.0"),
    ///         side: Side::BOTH,
    ///         flags: ManifestFlags::NONE,
    ///         off: 0,
    ///         file: Str::new("mods/enshwick/enshwick.dll"),
    ///         updates: UpdateSource::NONE,
    ///         signed_by: [Str::new("RWTjqgtfpy3lSOLC4uaUXuvvKSLQKOFf6nUnEytoiFjSzxMEhLnoAdJF"), Str::EMPTY],
    ///         ..ModInfo::EMPTY
    ///     })
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn loaded(mut self, info: ModInfo) -> Self {
        self.mods.push(info);
        self
    }

    /// A server mod the last refused join found missing, which
    /// `enshwick_loader_api::mods::join_missing()` lists, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::{Header, ManifestFlags, ModInfo, Side, Str, UpdateSource};
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new()
    ///     .join_missing(ModInfo {
    ///         header: Header::of::<ModInfo>(),
    ///         id: Str::new("safes"),
    ///         version: Str::new("0.2.0"),
    ///         side: Side::BOTH,
    ///         flags: ManifestFlags::JOIN,
    ///         off: 1,
    ///         file: Str::EMPTY,
    ///         updates: UpdateSource::github("themaiby/enshwick", "enshwick-safes"),
    ///         signed_by: [Str::new("RWTjqgtfpy3lSOLC4uaUXuvvKSLQKOFf6nUnEytoiFjSzxMEhLnoAdJF"), Str::EMPTY],
    ///         ..ModInfo::EMPTY
    ///     })
    ///     .install();
    /// // SAFETY: the installed fake's table.
    /// let api = unsafe { &*loader.api() };
    /// let mut count = 0;
    /// let size = u32::try_from(size_of::<ModInfo>())?;
    /// let status = (api.join_missing)(loader.handle(), core::ptr::null_mut(), size, 0, &mut count);
    /// assert!(status.is_ok());
    /// assert_eq!(count, 1);
    /// # Ok::<(), core::num::TryFromIntError>(())
    /// ```
    #[must_use]
    pub fn join_missing(mut self, info: ModInfo) -> Self {
        self.join_missing.push(info);
        self
    }

    /// The reply of the command `/name`, which another mod or the loader
    /// answers, when the mod runs it as the console with
    /// `enshwick_loader_api::commands::run`.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new()
    ///     .foreign_command("enshwick", "Enshwick 0.0.0-test: no mods")
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn foreign_command(mut self, name: &str, reply: &str) -> Self {
        self.foreign_commands
            .insert(name.to_owned(), reply.to_owned());
        self
    }

    /// A service another mod publishes, by its table's address; the table
    /// must stay valid while the fake is installed.
    ///
    /// # Examples
    ///
    /// ```
    /// #[repr(C)]
    /// struct Prices {
    ///     price: extern "system" fn(item: u32) -> u32,
    /// }
    ///
    /// extern "system" fn price(_item: u32) -> u32 {
    ///     3
    /// }
    ///
    /// static PRICES: Prices = Prices { price };
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new()
    ///     .service("trading-post.prices", 1, (&raw const PRICES).cast())
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn service(mut self, name: &str, version: u32, table: *const core::ffi::c_void) -> Self {
        self.services
            .insert((name.to_owned(), version), table.expose_provenance());
        self
    }

    /// Refuses hooks on `target`, as when another mod holds it exclusively.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().refuse_hook(0x1_4009_0a60).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn refuse_hook(mut self, target: u64) -> Self {
        self.refused_hooks.insert(target);
        self
    }

    /// Answers `code` as the game's code section, for pattern searches.
    ///
    /// # Examples
    ///
    /// ```
    /// static CODE: [u8; 4] = [0x90, 0xb9, 0x00, 0x02];
    /// let loader = enshwick_loader_api::test::FakeHost::new().code(&CODE).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn code(mut self, code: &'static [u8]) -> Self {
        self.code = (code.as_ptr().addr() as u64, code.len() as u64);
        self
    }

    /// The most recipes one workshop takes; 512 unless set, [`u32::MAX`]
    /// for a lifted limit.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().workshop_recipes(u32::MAX).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn workshop_recipes(mut self, limit: u32) -> Self {
        self.workshop_recipes = limit;
        self
    }

    /// The static data of item `item`, for `item_info`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::ItemData;
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new().item(0x5e01, 1, ItemData::HAS_PIDE).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn item(mut self, item: u32, max_stack: u16, flags: u32) -> Self {
        self.items.insert(
            item,
            ItemData {
                header: Header::of::<ItemData>(),
                id: item,
                max_stack,
                reserved: 0,
                flags,
            },
        );
        self
    }

    /// Plays a game build without inventory transactions: `inventory_open`
    /// answers [`Status::MISSING`].
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().without_inventories().install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn without_inventories(mut self) -> Self {
        self.no_inventories = true;
        self
    }

    /// Makes this the loader of the process until the returned fake drops.
    ///
    /// Waits while another test of the process holds an installed fake.
    ///
    /// # Examples
    ///
    /// See [`FakeHost`].
    #[must_use]
    pub fn install(self) -> Installed {
        let handle = self.handle;
        let turn = table::install(self);
        Installed {
            handle,
            _turn: turn,
        }
    }
}

/// A hook the mod linked, as the loader received it.
///
/// # Examples
///
/// See [`Installed::hooks`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct LinkedHook {
    /// The address of the hooked function.
    pub target: u64,
    /// The address of the detour's entry point.
    pub detour: u64,
    /// Lower runs earlier.
    pub priority: i32,
    /// The hook's flags.
    pub flags: HookFlags,
}

/// An inventory request the mod submitted, as the loader received it.
///
/// # Examples
///
/// See [`Installed::inventory_requests`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SubmittedRequest {
    /// The operations, in order.
    pub ops: Vec<InventoryOp>,
    /// `InventoryRequest` flags.
    pub flags: u32,
    /// The player credited, 0 for the game's choice.
    pub actor: u32,
}

/// A resource the mod serves, as the loader received it.
///
/// # Examples
///
/// See [`Installed::resources`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ServedResource {
    /// The resource served.
    pub key: ResourceKey,
    /// The resource it is built from.
    pub base: ResourceKey,
}

/// The installed fake loader: starts the mod and shows what it asked for.
///
/// Dropping it uninstalls the fake; calls the mod makes afterwards answer
/// [`Status::MISSING`] and are not recorded.
///
/// # Examples
///
/// See [`crate`].
#[derive(Debug)]
pub struct Installed {
    handle: ModHandle,
    _turn: MutexGuard<'static, ()>,
}

impl Drop for Installed {
    fn drop(&mut self) {
        table::uninstall();
    }
}

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// The table the mod is given.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(!loader.api().is_null());
    /// ```
    #[must_use]
    pub fn api(&self) -> *const HostApi {
        &raw const table::TABLE
    }

    /// The handle the mod is given.
    ///
    /// # Examples
    ///
    /// See [`FakeHost::handle`].
    #[must_use]
    pub fn handle(&self) -> ModHandle {
        self.handle
    }

    /// Initialises the mod as the loader does, and returns its answer.
    ///
    /// # Safety
    ///
    /// `init` is the `enshwick_mod_init` that
    /// [`declare_mod!`](crate::declare_mod) exported.
    ///
    /// # Examples
    ///
    /// See [`crate`].
    pub unsafe fn init(&self, init: InitFn) -> Status {
        // SAFETY: the table is static, and `init` is a mod's export as the
        // caller guarantees.
        unsafe { init(self.api(), self.handle) }
    }

    /// Tells the mod its hooks are armed.
    ///
    /// # Safety
    ///
    /// `ready` is the `enshwick_mod_ready` that
    /// [`declare_mod!`](crate::declare_mod) exported, and its
    /// `enshwick_mod_init` returned [`Status::OK`].
    ///
    /// # Examples
    ///
    /// See [`crate`].
    pub unsafe fn ready(&self, ready: ReadyFn) {
        // SAFETY: as the caller guarantees.
        unsafe { ready(self.api(), self.handle) }
    }

    /// Initialises the mod and, when it agrees, tells it its hooks are armed;
    /// returns the answer of `init`.
    ///
    /// # Safety
    ///
    /// `init` and `ready` are the exports of one
    /// [`declare_mod!`](crate::declare_mod).
    ///
    /// # Examples
    ///
    /// See [`crate`].
    pub unsafe fn start(&self, init: InitFn, ready: ReadyFn) -> Status {
        // SAFETY: as the caller guarantees.
        let status = unsafe { self.init(init) };
        if status.is_ok() {
            // SAFETY: as the caller guarantees, after a successful init.
            unsafe { self.ready(ready) };
        }
        status
    }

    /// The lines the mod logged, at the levels the loader kept.
    ///
    /// # Examples
    ///
    /// See [`crate`].
    #[must_use]
    pub fn logs(&self) -> Vec<(Level, String)> {
        table::with(Vec::new(), |s| s.logs.clone())
    }

    /// The faults the mod reported, such as a panic in a hook.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(loader.faults().is_empty());
    /// ```
    #[must_use]
    pub fn faults(&self) -> Vec<String> {
        table::with(Vec::new(), |s| s.faults.clone())
    }

    /// The hooks the mod linked, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(loader.hooks().iter().all(|hook| hook.priority == 0));
    /// ```
    #[must_use]
    pub fn hooks(&self) -> Vec<LinkedHook> {
        table::with(Vec::new(), |s| s.hooks.clone())
    }

    /// The addresses the mod placed mid-function hooks at, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(loader.mid_hooks().is_empty());
    /// ```
    #[must_use]
    pub fn mid_hooks(&self) -> Vec<u64> {
        table::with(Vec::new(), |s| s.mid_hooks.iter().map(|h| h.0).collect())
    }

    /// Runs the mod's mid-function hooks at `address` on `registers`, as the
    /// game reaching that instruction would; returns how many ran.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// let mut registers = enshwick_loader_api::Registers::default();
    /// assert_eq!(loader.run_at(0x1_4000_1000, &mut registers), 0);
    /// ```
    #[must_use = "the count says whether the mod hooks the address"]
    pub fn run_at(&self, address: u64, registers: &mut crate::Registers) -> usize {
        let handlers: Vec<_> = table::with(Vec::new(), |s| {
            s.mid_hooks
                .iter()
                .filter(|h| h.0 == address)
                .map(|h| (h.1, h.2))
                .collect()
        });
        for &(handler, user) in &handlers {
            // SAFETY: the registers are valid for the call, and `user` is the
            // pointer the mod registered the handler with.
            unsafe { handler(registers, core::ptr::with_exposed_provenance_mut(user)) };
        }
        handlers.len()
    }

    /// The entry point of `hook` when the mod linked it: call it to run the
    /// detour as the game would, its chain continuing to the target.
    ///
    /// # Examples
    ///
    /// See [`crate`].
    #[must_use]
    pub fn detour<F: FnPtr>(&self, hook: &'static Hook<F>) -> Option<F> {
        let entry = address(hook.entry());
        self.hooks()
            .iter()
            .any(|linked| linked.detour == entry)
            .then(|| hook.entry())
    }

    /// The events the mod subscribed to, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(loader.subscriptions().is_empty());
    /// ```
    #[must_use]
    pub fn subscriptions(&self) -> Vec<Event> {
        table::with(Vec::new(), |s| {
            s.subscriptions.iter().map(|s| s.0).collect()
        })
    }

    /// Raises the event `E` with `data` to every handler of the mod, on this
    /// thread; returns how many ran.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::{Header, TickData};
    /// use enshwick_loader_api::events;
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// let tick = TickData { header: Header::of::<TickData>(), step: 1 };
    /// assert_eq!(loader.raise::<events::Ticked>(&tick), 0);
    /// assert_eq!(loader.raise::<events::ShutdownStarted>(&()), 0);
    /// ```
    #[must_use = "the count says whether the mod handles the event"]
    pub fn raise<E: Kind>(&self, data: &E::Data) -> usize {
        // SAFETY: `data` is the event's data and lives for the call.
        unsafe { self.raise_raw(E::EVENT, core::ptr::from_ref(data).cast()) }
    }

    /// Raises `event` with raw `data`, for events this crate has no type for;
    /// returns how many handlers ran.
    ///
    /// # Safety
    ///
    /// `data` is what the loader passes with `event`: null or the event's
    /// data struct, valid for the call.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::Event;
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// // SAFETY: the event carries no data.
    /// let ran = unsafe { loader.raise_raw(Event::WORLD_SAVED, core::ptr::null()) };
    /// assert_eq!(ran, 0);
    /// ```
    #[must_use = "the count says whether the mod handles the event"]
    pub unsafe fn raise_raw(&self, event: Event, data: *const core::ffi::c_void) -> usize {
        let handlers: Vec<_> = table::with(Vec::new(), |s| {
            s.subscriptions
                .iter()
                .filter(|s| s.0 == event)
                .map(|s| (s.1, s.2))
                .collect()
        });
        for &(callback, user) in &handlers {
            // SAFETY: `data` is valid as the caller guarantees, and `user` is
            // the pointer the mod subscribed with.
            unsafe { callback(event, data, core::ptr::with_exposed_provenance_mut(user)) };
        }
        handlers.len()
    }

    /// The game event types the mod asked for, by hash, 0 for every type.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(loader.game_event_types().is_empty());
    /// ```
    #[must_use]
    pub fn game_event_types(&self) -> Vec<u32> {
        table::with(Vec::new(), |s| s.game_events.iter().map(|g| g.0).collect())
    }

    /// Hands the game event of `type_hash` with `bytes` to every handler of
    /// the mod for that type or for every type, on this thread; returns how
    /// many ran.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert_eq!(loader.raise_game_event(0xd8ec_982a, &[0; 16]), 0);
    /// ```
    #[must_use = "the count says whether the mod handles the event"]
    pub fn raise_game_event(&self, type_hash: u32, bytes: &[u8]) -> usize {
        let handlers: Vec<_> = table::with(Vec::new(), |s| {
            s.game_events
                .iter()
                .filter(|g| g.0 == 0 || g.0 == type_hash)
                .map(|g| (g.1, g.2))
                .collect()
        });
        let data = crate::abi::GameEventData {
            header: Header::of::<crate::abi::GameEventData>(),
            type_hash,
            size: u32::try_from(bytes.len()).unwrap_or(u32::MAX),
            data: bytes.as_ptr(),
        };
        for &(callback, user) in &handlers {
            // SAFETY: `data` lives for the call and `user` is the pointer the
            // mod subscribed with.
            unsafe {
                callback(
                    &raw const data,
                    core::ptr::with_exposed_provenance_mut(user),
                );
            };
        }
        handlers.len()
    }

    /// Replaces the text of the mod's settings file, as a player's edit
    /// would; raise [`events::SettingsChanged`](crate::events::SettingsChanged)
    /// after it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::events;
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new().settings("limit = 7\n").install();
    /// loader.set_settings("limit = 9\n");
    /// assert_eq!(loader.raise::<events::SettingsChanged>(&()), 0);
    /// ```
    pub fn set_settings(&self, text: &str) {
        table::with((), |s| s.host.settings = Some(text.to_owned()));
    }

    /// Sets the join values of the server this client joined, which the mod
    /// gets over its file's on its next read, or `None` for none, as after
    /// joining a server without them; raise
    /// [`events::SettingsChanged`](crate::events::SettingsChanged)
    /// after it, as the loader does.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// loader.set_server_synced(Some("{ altars = [2, 4] }"));
    /// assert!(loader.reported_synced().is_empty());
    /// ```
    pub fn set_server_synced(&self, synced: Option<&str>) {
        table::with((), |s| s.host.server_synced = synced.map(str::to_owned));
    }

    /// The join values the mod reported each time it read its settings, as
    /// its file holds them.
    ///
    /// # Examples
    ///
    /// See [`Installed::set_server_synced`].
    #[must_use]
    pub fn reported_synced(&self) -> Vec<String> {
        table::with(Vec::new(), |s| s.synced.clone())
    }

    /// The defaults the mod passed each time it read its settings.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(loader.settings_defaults().is_empty());
    /// ```
    #[must_use]
    pub fn settings_defaults(&self) -> Vec<String> {
        table::with(Vec::new(), |s| s.defaults.clone())
    }

    /// The resources the mod serves, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(loader.resources().is_empty());
    /// ```
    #[must_use]
    pub fn resources(&self) -> Vec<ServedResource> {
        table::with(Vec::new(), |s| s.served.iter().map(|s| s.0).collect())
    }

    /// Builds the served resource `key` from `base` with the mod's patch, as
    /// the game's first read of it would.
    ///
    /// # Errors
    ///
    /// [`Status::MISSING`] when the mod serves no `key`, or the patch's own
    /// status when it fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::{ResourceKey, Status};
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// let key = ResourceKey { guid: [0x3a; 16], type_hash: 0xb5ce_8765, part: 0 };
    /// assert_eq!(loader.build_resource(&key, &[]), Err(Status::MISSING));
    /// ```
    pub fn build_resource(&self, key: &ResourceKey, base: &[u8]) -> Result<Vec<u8>, Status> {
        let served = table::with(None, |s| {
            s.served
                .iter()
                .find(|s| s.0.key == *key)
                .map(|s| (s.1, s.2))
        });
        let (patch, user) = served.ok_or(Status::MISSING)?;
        let mut out = core::ptr::null();
        let mut out_len = 0;
        // SAFETY: `base` is readable for the call, the outs are valid, and
        // `user` is the pointer the mod served with.
        let status = unsafe {
            patch(
                base.as_ptr(),
                base.len(),
                core::ptr::with_exposed_provenance_mut(user),
                &raw mut out,
                &raw mut out_len,
            )
        };
        if !status.is_ok() {
            return Err(status);
        }
        if out.is_null() {
            return Ok(Vec::new());
        }
        // SAFETY: on success the patch wrote bytes that live for the process.
        Ok(unsafe { core::slice::from_raw_parts(out, out_len) }.to_vec())
    }

    /// The code patches the mod asked for, as `(address, expected,
    /// replacement)`, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(loader.code_patches().is_empty());
    /// ```
    #[must_use]
    pub fn code_patches(&self) -> Vec<(u64, Vec<u8>, Vec<u8>)> {
        table::with(Vec::new(), |s| s.code_patches.clone())
    }

    /// The texts the mod added, as `(tag, language, text)`, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(loader.texts().is_empty());
    /// ```
    #[must_use]
    pub fn texts(&self) -> Vec<(u32, u32, String)> {
        table::with(Vec::new(), |s| s.texts.clone())
    }

    /// Runs the mod's chat command `/name` as `player` typed it with `args`;
    /// the reply, or `None` when the mod has no such command.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::{Header, PlayerInfo};
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// let player = PlayerInfo {
    ///     header: Header::of::<PlayerInfo>(),
    ///     slot: 0,
    ///     entity: 2,
    ///     key: 7,
    ///     permissions: 0,
    ///     name_len: 0,
    ///     name: [0; 64],
    /// };
    /// assert_eq!(loader.command("safe", &player, "list"), None);
    /// ```
    #[must_use]
    pub fn command(&self, name: &str, player: &PlayerInfo, args: &str) -> Option<String> {
        let (callback, user) = table::with(None, |s| {
            s.commands.iter().find(|c| c.0 == name).map(|c| (c.2, c.3))
        })?;
        Some(run_handler(callback, user, name, player, args))
    }

    /// The command lines the mod ran as the console with
    /// `enshwick_loader_api::commands::run`, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(loader.console().is_empty());
    /// ```
    #[must_use]
    pub fn console(&self) -> Vec<String> {
        table::with(Vec::new(), |s| s.console.clone())
    }

    /// The lines the mod's status sources give now, each source's text in
    /// registration order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(loader.status().is_empty());
    /// ```
    #[must_use]
    pub fn status(&self) -> Vec<String> {
        let sources = table::with(Vec::new(), |s| s.status_sources.clone());
        sources
            .into_iter()
            .map(|(callback, user)| {
                let mut out = Str::EMPTY;
                // SAFETY: `user` is the pointer the mod registered with; a
                // writable out pointer.
                unsafe { callback(core::ptr::with_exposed_provenance_mut(user), &raw mut out) };
                // SAFETY: the text stays valid until the source runs again.
                unsafe { out.as_str() }.unwrap_or("").to_owned()
            })
            .collect()
    }

    /// The commands the mod registered, as `(name, help)`, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(loader.commands().is_empty());
    /// ```
    #[must_use]
    pub fn commands(&self) -> Vec<(String, String)> {
        table::with(Vec::new(), |s| {
            s.commands
                .iter()
                .map(|c| (c.0.clone(), c.1.clone()))
                .collect()
        })
    }

    /// The chat lines the mod sent, as `(account key, text)`; key 0 for
    /// everyone.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(loader.chat().is_empty());
    /// ```
    #[must_use]
    pub fn chat(&self) -> Vec<(u64, String)> {
        table::with(Vec::new(), |s| s.chat.clone())
    }

    /// Makes `altars` the altars of the world, as the loader lists them to a
    /// mod that opened altars, and raises
    /// [`AltarsChanged`](crate::events::AltarsChanged) as the loader does.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::AltarInfo;
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// let mut altar = AltarInfo::EMPTY;
    /// altar.entity = 4096;
    /// loader.set_altars(&[altar]);
    /// ```
    pub fn set_altars(&self, altars: &[AltarInfo]) {
        table::with((), |s| s.altars = altars.to_vec());
        let _handled = self.raise::<crate::events::AltarsChanged>(&());
    }

    /// Sets the clock the fake's `time_of_day` reports; until then the
    /// clock is missing, as before a world runs.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::{DaytimeInfo, Header};
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// loader.set_daytime(DaytimeInfo {
    ///     header: Header::of::<DaytimeInfo>(),
    ///     time: 0,
    ///     day_start: 6 * 3_600_000_000_000,
    ///     day_end: 18 * 3_600_000_000_000,
    ///     day_length: 1_800_000_000_000,
    ///     night_length: 720_000_000_000,
    ///     speed: 1.0,
    ///     flags: 0,
    /// });
    /// assert_eq!(loader.daytime().map(|info| info.time), Some(0));
    /// ```
    pub fn set_daytime(&self, info: crate::abi::DaytimeInfo) {
        table::with((), |s| s.daytime = Some(info));
    }

    /// The clock as the mod left it; a time it set moves the clock and its
    /// day flag.
    ///
    /// # Examples
    ///
    /// See [`Installed::set_daytime`].
    #[must_use]
    pub fn daytime(&self) -> Option<crate::abi::DaytimeInfo> {
        table::with(None, |s| s.daytime)
    }

    /// The mod's factor on the clock's speed, once it asked to change it.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert_eq!(loader.clock_speed(), None);
    /// ```
    #[must_use]
    pub fn clock_speed(&self) -> Option<f32> {
        table::with(None, |s| s.clock_speed)
    }

    /// Sets the game settings the fake reports, `keen::ecs::GameSettings`
    /// as bytes of its reflected size, and their preset; until then no
    /// world runs. A change the mod makes replaces them and makes the
    /// preset custom (4).
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// loader.set_game_settings(vec![0; 144], 0);
    /// assert_eq!(loader.game_settings().map(|(bytes, _)| bytes.len()), Some(144));
    /// ```
    pub fn set_game_settings(&self, bytes: Vec<u8>, preset: u32) {
        table::with((), |s| s.game_settings = Some((bytes, preset)));
    }

    /// The game settings' bytes and preset as the mod left them.
    ///
    /// # Examples
    ///
    /// See [`Installed::set_game_settings`].
    #[must_use]
    pub fn game_settings(&self) -> Option<(Vec<u8>, u32)> {
        table::with(None, |s| s.game_settings.clone())
    }

    /// Runs the mod's damage filters on `check` as the loader does before
    /// the game applies damage; returns what they left.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::{DamageCheck, Header};
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// let fall = DamageCheck {
    ///     header: Header::of::<DamageCheck>(),
    ///     cause: DamageCheck::FALL,
    ///     target: 2,
    ///     source: 0,
    ///     reserved: 0,
    ///     amounts: [0.0; 13],
    ///     total: 40.0,
    /// };
    /// assert_eq!(loader.damage(fall), fall, "no filters, no change");
    /// ```
    #[must_use]
    pub fn damage(&self, mut check: crate::abi::DamageCheck) -> crate::abi::DamageCheck {
        let filters = table::with(Vec::new(), |s| s.damage_filters.clone());
        for (callback, user) in filters {
            // SAFETY: the filter the mod registered, with its pointer.
            unsafe { callback(&raw mut check, core::ptr::with_exposed_provenance_mut(user)) };
        }
        check
    }

    /// The settings schema the mod handed the loader, which
    /// [`Load::config`](crate::Load::config) does; `None` before.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(loader.config_schema().is_none());
    /// ```
    #[must_use]
    pub fn config_schema(&self) -> Option<crate::config::Schema> {
        table::with(None, |s| s.config_schema.clone())
            .and_then(|text| crate::config::Schema::parse(&text).ok())
    }

    /// Sets the buff types the fake knows and the entities that hold buffs,
    /// with none yet; a buff the mod gives starts with the duration asked,
    /// or the type's, and one it removes goes at once.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::{BuffTypeInfo, Header};
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// let ty = BuffTypeInfo { header: Header::of::<BuffTypeInfo>(), id: 7, slot: 1, flags: 0, name: 0, duration: 60_000_000_000 };
    /// loader.set_buffs(&[ty], &[2]);
    /// assert_eq!(loader.buffs_of(2), Some(Vec::new()));
    /// ```
    pub fn set_buffs(&self, types: &[crate::abi::BuffTypeInfo], holders: &[u32]) {
        table::with((), |s| {
            s.buff_types = types.to_vec();
            s.buffs = holders.iter().map(|&e| (e, Vec::new())).collect();
        });
    }

    /// The buffs `entity` holds as the mod left them; `None` for one that
    /// holds no buffs.
    ///
    /// # Examples
    ///
    /// See [`Installed::set_buffs`].
    #[must_use]
    pub fn buffs_of(&self, entity: u32) -> Option<Vec<crate::abi::ActiveBuff>> {
        table::with(None, |s| s.buffs.get(&entity).cloned())
    }

    /// Sets the knowledge the fake knows, `(character, value)`; a world
    /// value's character is ignored. A grant is recorded, not applied.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::{Header, KnowledgeInfo};
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// let flag = KnowledgeInfo { header: Header::of::<KnowledgeInfo>(), id: 9, kind: KnowledgeInfo::BOOL, scope: KnowledgeInfo::WORLD, reserved: 0, value: 1, flags: 0 };
    /// loader.set_knowledge(&[(0, flag)], &[]);
    /// assert!(loader.grants().is_empty());
    /// ```
    pub fn set_knowledge(&self, values: &[(u32, crate::abi::KnowledgeInfo)], quests: &[u32]) {
        table::with((), |s| {
            s.knowledge = values.to_vec();
            s.quests = quests.to_vec();
        });
    }

    /// The knowledge grants the mod asked for: character, id, kind, value.
    ///
    /// # Examples
    ///
    /// See [`Installed::set_knowledge`].
    #[must_use]
    pub fn grants(&self) -> Vec<(u32, u32, u32, u32)> {
        table::with(Vec::new(), |s| s.grants.clone())
    }

    /// Sets the weather the fake reports: the weights everywhere (clear,
    /// rain, snow, blizzard) and the fronts; until then no world runs.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// loader.set_weather([1.0, 0.0, 0.0, 0.0], &[]);
    /// assert!(loader.forecasts().is_empty());
    /// ```
    pub fn set_weather(&self, weights: [f32; 4], fronts: &[crate::abi::WeatherFront]) {
        table::with((), |s| s.weather = Some((weights, fronts.to_vec())));
    }

    /// The weather the mod started: effect (`u32::MAX` for the game's
    /// choice) and duration in game nanoseconds.
    ///
    /// # Examples
    ///
    /// See [`Installed::set_weather`].
    #[must_use]
    pub fn forecasts(&self) -> Vec<(u32, i64)> {
        table::with(Vec::new(), |s| s.forecasts.clone())
    }

    /// Sets the terrain materials the fake lists; until then no world runs.
    /// Every edit the mod asks for is recorded and moves one cubic metre.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// loader.set_terrain(&[]);
    /// assert!(loader.terrain_edits().is_empty());
    /// ```
    pub fn set_terrain(&self, materials: &[crate::abi::TerrainMaterialInfo]) {
        table::with((), |s| s.terrain = Some(materials.to_vec()));
    }

    /// The terrain edits the mod asked for, in order.
    ///
    /// # Examples
    ///
    /// See [`Installed::set_terrain`].
    #[must_use]
    pub fn terrain_edits(&self) -> Vec<crate::abi::TerrainEdit> {
        table::with(Vec::new(), |s| s.terrain_edits.clone())
    }

    /// Whether the mod opened altars, and whether it controls their zones.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert_eq!(loader.altars_opened(), (false, false));
    /// ```
    #[must_use]
    pub fn altars_opened(&self) -> (bool, bool) {
        table::with((false, false), |s| (s.altars_opened, s.zone_control))
    }

    /// The altar zone sizes the mod set last.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(loader.zone_sizes().is_empty());
    /// ```
    #[must_use]
    pub fn zone_sizes(&self) -> Vec<AltarZoneSize> {
        table::with(Vec::new(), |s| s.zone_sizes.clone())
    }

    /// Asks the mod's zone check filters about a placement the game refused,
    /// as the loader would; whether any accepts it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::Bounds;
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(!loader.zone_check(&[], Bounds::default(), 0));
    /// ```
    #[must_use]
    pub fn zone_check(&self, zones: &[Bounds], object: Bounds, margin: i64) -> bool {
        let filters = table::with(Vec::new(), |s| s.zone_filters.clone());
        let call = ZoneCheck {
            header: Header::of::<ZoneCheck>(),
            zones: zones.as_ptr(),
            zone_count: zones.len(),
            object,
            margin,
        };
        filters.iter().any(|&(callback, user)| {
            let user = core::ptr::with_exposed_provenance_mut(user);
            // SAFETY: a valid call; `user` is the pointer the mod registered with.
            unsafe { callback(&raw const call, user) != 0 }
        })
    }

    /// Asks the mod's survivor lookup as the game would: survivor
    /// `survivor`, the game's answer `vanilla`, `purpose` one of the
    /// [`SurvivorLookup`] constants with its `entity` and `cell`; `beds`
    /// answers [`Lookup::bed_of`](crate::survivors::Lookup::bed_of). `None`
    /// for the game's own answer.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::SurvivorLookup;
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert_eq!(loader.survivor_lookup(7, 4096, SurvivorLookup::SPAWN, 0, [0; 3], &[]), None);
    /// ```
    #[must_use]
    pub fn survivor_lookup(
        &self,
        survivor: u32,
        vanilla: u32,
        purpose: u32,
        entity: u32,
        cell: [i64; 3],
        beds: &[(u32, u32)],
    ) -> Option<u32> {
        unsafe extern "system" fn bed_of(token: *mut core::ffi::c_void, entity: u32) -> u32 {
            // SAFETY: the token is the `beds` slice of the running call.
            let beds = unsafe { &*token.cast::<&[(u32, u32)]>() };
            beds.iter()
                .find(|(e, _)| *e == entity)
                .map_or(0, |(_, bed)| *bed)
        }
        let (answer, user) = table::with(None, |s| s.survivor_lookup)?;
        let mut beds = beds;
        let lookup = SurvivorLookup {
            header: Header::of::<SurvivorLookup>(),
            survivor,
            vanilla,
            purpose,
            entity,
            cell,
            bed_of: Some(bed_of),
            token: (&raw mut beds).cast(),
        };
        let mut out = 0;
        let user = core::ptr::with_exposed_provenance_mut(user);
        // SAFETY: a valid call and out pointer; `user` is the mod's pointer.
        let answered = unsafe { answer(&raw const lookup, user, &raw mut out) };
        (answered != 0).then_some(out)
    }

    /// Asks the mod's bed checks as the game would; whether every one keeps
    /// the bed.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::{BedCheck, Header};
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// let check = BedCheck { header: Header::of::<BedCheck>(), survivor: 7, entity: 1, bed: 2, home: [0; 3], bed_at: [0; 3] };
    /// assert!(loader.survivor_bed_check(&check));
    /// ```
    #[must_use]
    pub fn survivor_bed_check(&self, check: &BedCheck) -> bool {
        let filters = table::with(Vec::new(), |s| s.bed_checks.clone());
        filters.iter().all(|&(callback, user)| {
            let user = core::ptr::with_exposed_provenance_mut(user);
            // SAFETY: a valid check; `user` is the mod's pointer.
            unsafe { callback(check, user) != 0 }
        })
    }

    /// Asks the mod's summon filters as the game would; whether one makes
    /// the use take the item.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::{Header, Summon};
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// let summon = Summon {
    ///     header: Header::of::<Summon>(),
    ///     item: 1,
    ///     ammo: 0,
    ///     template: [0; 16],
    ///     target: [0; 3],
    ///     slot: 0,
    ///     pide: 0,
    ///     meter: [0; 2],
    ///     consumed: 0,
    ///     reserved: [0; 3],
    /// };
    /// assert!(!loader.summon(&summon));
    /// ```
    #[must_use]
    pub fn summon(&self, summon: &Summon) -> bool {
        let filters = table::with(Vec::new(), |s| s.summons.clone());
        filters.iter().any(|&(callback, user)| {
            let user = core::ptr::with_exposed_provenance_mut(user);
            // SAFETY: a valid summon; `user` is the mod's pointer.
            unsafe { callback(summon, user) != 0 }
        })
    }

    /// Tells the mod's summon handlers that a use worked, as the game does
    /// after the spawn or move; returns how many handlers ran.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::{Header, Summon};
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// let summon = Summon {
    ///     header: Header::of::<Summon>(),
    ///     item: 1,
    ///     ammo: 0,
    ///     template: [0; 16],
    ///     target: [0; 3],
    ///     slot: 0,
    ///     pide: 0,
    ///     meter: [0; 2],
    ///     consumed: 1,
    ///     reserved: [0; 3],
    /// };
    /// assert_eq!(loader.summoned(&summon), 0);
    /// ```
    #[must_use]
    pub fn summoned(&self, summon: &Summon) -> usize {
        let handlers = table::with(Vec::new(), |s| s.summoned.clone());
        for &(callback, user) in &handlers {
            let user = core::ptr::with_exposed_provenance_mut(user);
            // SAFETY: a valid summon; `user` is the mod's pointer.
            unsafe { callback(summon, user) };
        }
        handlers.len()
    }

    /// The item data kinds the mod opened, by full id, and every value set:
    /// `(kind id, entity, item, bytes)`.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(loader.item_data().is_empty());
    /// ```
    #[must_use]
    pub fn item_data(&self) -> Vec<(String, u32, u32, Vec<u8>)> {
        table::with(Vec::new(), |s| {
            s.item_values
                .iter()
                .map(|(&(kind, pide), (item, bytes))| {
                    let id = s.item_kinds.get(kind as usize).cloned().unwrap_or_default();
                    (id, pide, *item, bytes.clone())
                })
                .collect()
        })
    }

    /// The items the mod filed as summoning staffs, as `(item, survivor)`.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(loader.summon_items().is_empty());
    /// ```
    #[must_use]
    pub fn summon_items(&self) -> Vec<(u32, u32)> {
        table::with(Vec::new(), |s| s.summon_items.clone())
    }

    /// Asks the mod's survivor menu merge about `statuses`; the byte to show,
    /// or `None` for the game's.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert_eq!(loader.survivor_status(7, &[1, 3]), None);
    /// ```
    #[must_use]
    pub fn survivor_status(&self, survivor: u32, statuses: &[u8]) -> Option<u8> {
        let (callback, user) = table::with(None, |s| s.survivor_status)?;
        let user = core::ptr::with_exposed_provenance_mut(user);
        // SAFETY: valid statuses for the call; `user` is the mod's pointer.
        let answer = unsafe { callback(survivor, statuses.as_ptr(), statuses.len(), user) };
        (answer & crate::abi::SURVIVOR_STATUS_SET != 0).then(|| answer.to_le_bytes()[0])
    }

    /// Asks the mod's interaction filters about `call` as the loader would;
    /// whether every one allows it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::{Header, Interaction};
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// let call = Interaction {
    ///     header: Header::of::<Interaction>(),
    ///     kind: Interaction::OPEN,
    ///     player: 1,
    ///     target: 2,
    ///     item: 0,
    ///     known: 0,
    ///     reserved: [0; 3],
    ///     position: [0.0; 3],
    /// };
    /// assert!(loader.interaction(&call));
    /// ```
    #[must_use]
    pub fn interaction(&self, call: &Interaction) -> bool {
        let filters = table::with(Vec::new(), |s| s.interaction_filters.clone());
        filters.iter().all(|&(callback, user)| {
            let user = core::ptr::with_exposed_provenance_mut(user);
            // SAFETY: a valid call; `user` is the mod's pointer.
            unsafe { callback(call, user) != 0 }
        })
    }

    /// Asks the mod's skill overrides about `check` as the loader would: the
    /// first answer for the node, [`SkillCheck::VANILLA`] for none, else
    /// [`SkillCheck::UNLOCKED`] or [`SkillCheck::LOCKED`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::{Header, SkillCheck};
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// let check = SkillCheck { header: Header::of::<SkillCheck>(), node: 7, gate: SkillCheck::OTHER, unlocked: 0, reserved: [0; 3] };
    /// assert_eq!(loader.skill(&check), SkillCheck::VANILLA);
    /// ```
    #[must_use]
    pub fn skill(&self, check: &SkillCheck) -> u8 {
        let skills = table::with(Vec::new(), |s| s.skills.clone());
        skills
            .iter()
            .filter(|(node, _, _)| *node == check.node)
            .map(|&(_, callback, user)| {
                let user = core::ptr::with_exposed_provenance_mut(user);
                // SAFETY: a valid check; `user` is the mod's pointer.
                unsafe { callback(check, user) }
            })
            .find(|&answer| answer != SkillCheck::VANILLA)
            .unwrap_or(SkillCheck::VANILLA)
    }

    /// Runs the mod's comfort filters on `comfort` as the loader would; the
    /// comfort registered.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::{Comfort, Header};
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// let comfort = Comfort { header: Header::of::<Comfort>(), category: 1, value: 3, position: [0.0; 3] };
    /// assert_eq!(loader.comfort(&comfort), 3);
    /// ```
    #[must_use]
    pub fn comfort(&self, comfort: &Comfort) -> i32 {
        let filters = table::with(Vec::new(), |s| s.comfort_filters.clone());
        filters
            .iter()
            .fold(comfort.value, |value, &(callback, user)| {
                let call = Comfort { value, ..*comfort };
                let user = core::ptr::with_exposed_provenance_mut(user);
                // SAFETY: a valid registration; `user` is the mod's pointer.
                unsafe { callback(&raw const call, user) }
            })
    }

    /// Runs the mod's experience filters on `rows` as the loader would, with
    /// a level table of `cap` levels, level `l` needing `need(l)`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::ExperienceRow;
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// let mut rows = [ExperienceRow { entity: 1, level: 1, progress: 0, need: 100, gained: 5 }];
    /// loader.experience(&mut rows, 25, |level| level * 100);
    /// assert_eq!(rows[0].gained, 5);
    /// ```
    pub fn experience(&self, rows: &mut [ExperienceRow], cap: u32, need: fn(u32) -> u32) {
        unsafe extern "system" fn need_for(token: *mut core::ffi::c_void, level: u32) -> u32 {
            // SAFETY: the token is the `need` function of the running call.
            let need = unsafe { *token.cast::<fn(u32) -> u32>() };
            need(level)
        }
        let filters = table::with(Vec::new(), |s| s.experience_filters.clone());
        let mut need = need;
        for &(callback, user) in &filters {
            let mut pass = ExperiencePass {
                header: Header::of::<ExperiencePass>(),
                rows: rows.as_mut_ptr(),
                row_count: rows.len(),
                level_cap: cap,
                need_for: Some(need_for),
                token: (&raw mut need).cast(),
            };
            let user = core::ptr::with_exposed_provenance_mut(user);
            // SAFETY: a valid pass; `user` is the mod's pointer.
            unsafe { callback(&raw mut pass, user) };
        }
    }

    /// Asks the mod's altar limit about `flame_level`; `None` without one.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert_eq!(loader.altar_limit(3), None);
    /// ```
    #[must_use]
    pub fn altar_limit(&self, flame_level: u32) -> Option<u8> {
        let (callback, user) = table::with(None, |s| s.altar_limit)?;
        let user = core::ptr::with_exposed_provenance_mut(user);
        // SAFETY: `user` is the mod's pointer.
        Some(unsafe { callback(flame_level, user) })
    }

    /// Whether the mod raised the base limit.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(!loader.base_limit_raised());
    /// ```
    #[must_use]
    pub fn base_limit_raised(&self) -> bool {
        table::with(false, |s| s.base_limit_raised)
    }

    /// Makes the last grouping find `zones` altar zones in `bases` bases.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// loader.set_grouping(17, 16);
    /// ```
    pub fn set_grouping(&self, zones: u32, bases: u32) {
        table::with((), |s| s.grouping = Some((zones, bases)));
    }

    /// The labels the mod set, as `(entity, text)`, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(loader.labels().is_empty());
    /// ```
    #[must_use]
    pub fn labels(&self) -> Vec<(u32, String)> {
        table::with(Vec::new(), |s| s.labels.clone())
    }

    /// The entities the mod asked the world to destroy, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(loader.destroyed().is_empty());
    /// ```
    #[must_use]
    pub fn destroyed(&self) -> Vec<u32> {
        table::with(Vec::new(), |s| s.destroyed.clone())
    }

    /// The components the mod asked to write, `(entity, type hash)`, in
    /// order; the fake hands out the data [`FakeHost::component`] gave.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(loader.writes().is_empty());
    /// ```
    #[must_use]
    pub fn writes(&self) -> Vec<(u32, u32)> {
        table::with(Vec::new(), |s| s.writes.clone())
    }

    /// The game events the mod raised, `(type hash, bytes)`, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(loader.emits().is_empty());
    /// ```
    #[must_use]
    pub fn emits(&self) -> Vec<(u32, Vec<u8>)> {
        table::with(Vec::new(), |s| s.emits.clone())
    }

    /// The teleports the mod asked for, in order; an entity the fake does
    /// not know is refused.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(loader.teleports().is_empty());
    /// ```
    #[must_use]
    pub fn teleports(&self) -> Vec<crate::abi::TeleportRequest> {
        table::with(Vec::new(), |s| s.teleports.clone())
    }

    /// The spawns the mod asked for, with the entity ids the fake handed
    /// out, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(loader.spawns().is_empty());
    /// ```
    #[must_use]
    pub fn spawns(&self) -> Vec<(u32, crate::abi::SpawnRequest)> {
        table::with(Vec::new(), |s| s.spawns.clone())
    }

    /// Asks the mod's craft filters about `call`, as the server does
    /// before it crafts; whether the craft goes ahead.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::{CraftCheck, Header};
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// let craft = CraftCheck { header: Header::of::<CraftCheck>(), player: 2, station: 0, recipe: 1, amount: 1 };
    /// assert!(loader.craft(&craft));
    /// ```
    #[must_use]
    pub fn craft(&self, call: &crate::abi::CraftCheck) -> bool {
        let filters = table::with(Vec::new(), |s| s.craft_filters.clone());
        filters.iter().all(|&(callback, user)| {
            // SAFETY: a filter the mod registered with its own pointer.
            unsafe { callback(call, core::ptr::with_exposed_provenance_mut(user)) != 0 }
        })
    }

    /// Asks the mod's build filters about `call`, as the cursor check does
    /// after it allowed a placement; whether the placement goes ahead.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::{BuildCheck, Header};
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// let wall = BuildCheck { header: Header::of::<BuildCheck>(), player: 2, item: 7, position: [0; 3] };
    /// assert!(loader.build(&wall));
    /// ```
    #[must_use]
    pub fn build(&self, call: &crate::abi::BuildCheck) -> bool {
        let filters = table::with(Vec::new(), |s| s.build_filters.clone());
        filters.iter().all(|&(callback, user)| {
            // SAFETY: a filter the mod registered with its own pointer.
            unsafe { callback(call, core::ptr::with_exposed_provenance_mut(user)) != 0 }
        })
    }

    /// The messages the mod sent: the channel's full name (`name@version`),
    /// the raw peer ([`NetMessage::SERVER`](crate::abi::NetMessage::SERVER),
    /// a player's key, or 0 for every client) and the encoded bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(loader.net_sent().is_empty());
    /// ```
    #[must_use]
    pub fn net_sent(&self) -> Vec<(String, u64, Vec<u8>)> {
        table::with(Vec::new(), |s| {
            s.net_sent
                .iter()
                .map(|(id, peer, bytes)| {
                    let name = s
                        .channels
                        .iter()
                        .find(|c| c.1 == *id)
                        .map_or_else(String::new, |c| c.0.clone());
                    (name, *peer, bytes.clone())
                })
                .collect()
        })
    }

    /// Hands `message` from `from` to the mod's channel `name@version`, as
    /// the loader does when one comes in, encoded as the channel encodes
    /// it; false when the mod opened no such channel.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::net::Peer;
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(!loader.net_deliver("markers@1", Peer::Server, &5u32));
    /// ```
    #[must_use]
    pub fn net_deliver<M: serde::Serialize>(
        &self,
        name: &str,
        from: crate::net::Peer,
        message: &M,
    ) -> bool {
        let Ok(bytes) = postcard::to_allocvec(message) else {
            return false;
        };
        let channel = table::with(None, |s| {
            s.channels
                .iter()
                .find(|c| c.0 == name)
                .map(|c| (c.1, c.2, c.3))
        });
        let Some((id, callback, user)) = channel else {
            return false;
        };
        let peer = match from {
            crate::net::Peer::Server => crate::abi::NetMessage::SERVER,
            crate::net::Peer::Player(key) => key,
        };
        let raw = crate::abi::NetMessage {
            header: Header::of::<crate::abi::NetMessage>(),
            channel: id,
            reserved: 0,
            peer,
            data: bytes.as_ptr(),
            size: u32::try_from(bytes.len()).unwrap_or(0),
            reserved_tail: 0,
        };
        // SAFETY: a handler the mod registered with its own pointer.
        unsafe { callback(&raw const raw, core::ptr::with_exposed_provenance_mut(user)) };
        true
    }

    /// Runs the mod's frame callbacks once, in game, and returns what they
    /// drew, in order; inputs given with [`Installed::input`] reach their
    /// widgets in this frame.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::Side;
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new().side(Side::CLIENT).install();
    /// assert!(loader.draw_frame().is_empty());
    /// ```
    #[must_use]
    pub fn draw_frame(&self) -> Vec<Drawn> {
        let frames = table::with(Vec::new(), |s| s.frames.clone());
        let mut drawn = Vec::new();
        for (token, (callback, user)) in (1u64..).zip(frames) {
            table::with((), |s| s.frame = Some((token, false, Vec::new())));
            let frame = crate::abi::UiFrame {
                header: Header::of::<crate::abi::UiFrame>(),
                token,
                screen: crate::abi::UiFrame::IN_GAME,
                reserved: 0,
            };
            // SAFETY: a callback the mod registered with its own pointer.
            unsafe {
                callback(
                    &raw const frame,
                    core::ptr::with_exposed_provenance_mut(user),
                );
            };
            if let Some((_, _, list)) = table::with(None, |s| s.frame.take()) {
                drawn.extend(list);
            }
        }
        drawn
    }

    /// Runs the mod's HUD callbacks once and returns what they drew, each
    /// item with its anchor (`UiWidget::TOP_LEFT` and on), in order.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::Side;
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new().side(Side::CLIENT).install();
    /// assert!(loader.draw_hud().is_empty());
    /// ```
    #[must_use]
    pub fn draw_hud(&self) -> Vec<(u32, Drawn)> {
        let huds = table::with(Vec::new(), |s| s.huds.clone());
        let mut drawn = Vec::new();
        for (token, (callback, user)) in (1u64..).zip(huds) {
            table::with((), |s| s.hud = Some((token, Vec::new())));
            let frame = crate::abi::UiFrame {
                header: Header::of::<crate::abi::UiFrame>(),
                token,
                screen: crate::abi::UiFrame::IN_GAME,
                reserved: 0,
            };
            // SAFETY: a callback the mod registered with its own pointer.
            unsafe {
                callback(
                    &raw const frame,
                    core::ptr::with_exposed_provenance_mut(user),
                );
            }
            if let Some((_, list)) = table::with(None, |s| s.hud.take()) {
                drawn.extend(list);
            }
        }
        drawn
    }

    /// Runs the mod's check of a new settings text, as the loader does
    /// before it writes a change: `Ok` when the mod accepts `text`, else
    /// its reason; `Ok` for a mod without a check.
    ///
    /// # Errors
    ///
    /// The mod's reason for refusing `text`.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert_eq!(loader.check_settings("x = 1"), Ok(()));
    /// ```
    pub fn check_settings(&self, text: &str) -> Result<(), String> {
        let Some((callback, user)) = table::with(None, |s| s.config_check) else {
            return Ok(());
        };
        let mut why = Str::EMPTY;
        // SAFETY: the mod's check with its own pointer; the text lives for
        // the call.
        let status = unsafe {
            callback(
                Str::new(text),
                &raw mut why,
                core::ptr::with_exposed_provenance_mut(user),
            )
        };
        if status.is_ok() {
            return Ok(());
        }
        // SAFETY: the reason stays valid until the mod's next call.
        Err(unsafe { why.as_str() }.unwrap_or("refused").to_owned())
    }

    /// The mod's markers on the map, as it set them last.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(loader.markers().is_empty());
    /// ```
    #[must_use]
    pub fn markers(&self) -> Vec<FakeMarker> {
        table::with(Vec::new(), |s| s.markers.clone())
    }

    /// Gives the widget whose text or label is `text` an input from the
    /// player in the next frame drawn.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::test::Input;
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// loader.input("Close", Input::Click);
    /// ```
    pub fn input(&self, text: &str, input: Input) {
        table::with((), |s| s.inputs.push((text.to_owned(), input)));
    }

    /// The dialogs waiting for an answer, oldest first.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(loader.dialogs().is_empty());
    /// ```
    #[must_use]
    pub fn dialogs(&self) -> Vec<ShownDialog> {
        table::with(Vec::new(), |s| {
            s.dialogs.iter().map(|(d, _, _)| d.clone()).collect()
        })
    }

    /// Answers the oldest waiting dialog with `answer`, `None` for closed
    /// without one; false when none waits.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::ui::Response;
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(!loader.answer(Some(Response::Ok)));
    /// ```
    pub fn answer(&self, answer: Option<crate::ui::Response>) -> bool {
        let first = table::with(None, |s| {
            (!s.dialogs.is_empty()).then(|| s.dialogs.remove(0))
        });
        let Some((_, callback, user)) = first else {
            return false;
        };
        let raw = answer.map_or(crate::abi::UiDialog::CLOSED, crate::ui::Response::to_raw);
        // SAFETY: the mod's answer callback with its own pointer, once.
        unsafe { callback(raw, core::ptr::with_exposed_provenance_mut(user)) };
        true
    }

    /// The notices the mod showed on screen, `(account key, text)`, in
    /// order; a player not given with [`FakeHost::net_peer`] has a client
    /// without Enshwick, and gets chat lines instead.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(loader.notices().is_empty());
    /// ```
    #[must_use]
    pub fn notices(&self) -> Vec<(u64, String)> {
        table::with(Vec::new(), |s| s.notices.clone())
    }

    /// How many saves the mod asked for.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert_eq!(loader.saves(), 0);
    /// ```
    #[must_use]
    pub fn saves(&self) -> usize {
        table::with(0, |s| s.saves)
    }

    /// Asks the mod's rename filters about `player` renaming `entity` to
    /// `text`; whether every filter lets it through.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(loader.rename(4096, None, "Tools"));
    /// ```
    #[must_use]
    pub fn rename(&self, entity: u32, player: Option<&PlayerInfo>, text: &str) -> bool {
        let filters = table::with(Vec::new(), |s| s.rename_filters.clone());
        let call = RenameCall {
            header: Header::of::<RenameCall>(),
            entity,
            known: u8::from(player.is_some()),
            player: player.copied().unwrap_or(PlayerInfo {
                header: Header::of::<PlayerInfo>(),
                slot: u32::MAX,
                entity: 0,
                key: 0,
                permissions: 0,
                name_len: 0,
                name: [0; 64],
            }),
            text: Str::new(text),
        };
        filters.iter().all(|&(callback, user)| {
            let user = core::ptr::with_exposed_provenance_mut(user);
            // SAFETY: a valid call; `user` is the pointer the mod registered with.
            let allowed = unsafe { callback(&raw const call, user) };
            allowed != 0
        })
    }

    /// The services the mod published, as `(name, version)`, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(loader.published().is_empty());
    /// ```
    #[must_use]
    pub fn published(&self) -> Vec<(String, u32)> {
        table::with(Vec::new(), |s| {
            s.published.iter().map(|p| (p.0.clone(), p.1)).collect()
        })
    }

    /// The inventory requests the mod submitted that are not finished yet, in
    /// order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(loader.inventory_requests().is_empty());
    /// ```
    #[must_use]
    pub fn inventory_requests(&self) -> Vec<SubmittedRequest> {
        table::with(Vec::new(), |s| {
            s.inventory_requests
                .iter()
                .map(|r| r.request.clone())
                .collect()
        })
    }

    /// Finishes the oldest open inventory request as the game would:
    /// `outcome` is an `InventoryResult` constant and `ops` a result per
    /// operation. The mod's callback runs here, as on the game thread.
    /// Returns false when no request is open.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::InventoryResult;
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// assert!(!loader.finish_inventory(InventoryResult::COMMITTED, &[]));
    /// ```
    #[must_use = "false means no request was open"]
    pub fn finish_inventory(&self, outcome: u32, ops: &[InventoryOpResult]) -> bool {
        let open = table::with(None, |s| {
            (!s.inventory_requests.is_empty()).then(|| s.inventory_requests.remove(0))
        });
        let Some(open) = open else {
            return false;
        };
        let result = InventoryResult {
            header: Header::of::<InventoryResult>(),
            outcome,
            op_count: u32::try_from(ops.len()).unwrap_or(0),
            ops: ops.as_ptr(),
        };
        // SAFETY: the mod's callback with its pointer, handed back once; the
        // result lives for the call.
        unsafe {
            (open.done)(
                &raw const result,
                core::ptr::with_exposed_provenance_mut(open.user),
            );
        }
        true
    }
}
