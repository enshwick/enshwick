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
    callback: crate::abi::CommandFn,
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
    /// struct Zones {
    ///     count: extern "system" fn() -> u32,
    /// }
    ///
    /// extern "system" fn count() -> u32 {
    ///     3
    /// }
    ///
    /// static ZONES: Zones = Zones { count };
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new()
    ///     .service("trading-post.prices", 1, (&raw const ZONES).cast())
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
    /// assert_eq!(loader.raise::<events::Tick>(&tick), 0);
    /// assert_eq!(loader.raise::<events::Shutdown>(&()), 0);
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

    /// The engine event types the mod asked for, by hash, 0 for every type.
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

    /// Hands the engine event of `type_hash` with `bytes` to every handler of
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
    /// mod that opened altars.
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
    /// let summon = Summon { header: Header::of::<Summon>(), item: 1, ammo: 0, template: [0; 16], target: [0; 3] };
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
        (answer & 0x100 != 0).then(|| answer.to_le_bytes()[0])
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
    /// first answer for the node, 0 for none, 1 unlocked, 2 locked.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::{Header, SkillCheck};
    ///
    /// let loader = enshwick_loader_api::test::FakeHost::new().install();
    /// let check = SkillCheck { header: Header::of::<SkillCheck>(), node: 7, gate: SkillCheck::OTHER, unlocked: 0, reserved: [0; 3] };
    /// assert_eq!(loader.skill(&check), 0);
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
            .find(|&answer| answer != 0)
            .unwrap_or(0)
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
