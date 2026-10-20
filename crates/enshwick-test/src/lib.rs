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
//! use enshwick_api::abi::{Level, Status};
//! use enshwick_api::{Load, Mod, Next, Off, events, hook};
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
//!     fn manifest() -> &'static enshwick_api::abi::Manifest {
//!         enshwick_api::manifest!(id: "bonus", side: SERVER)
//!     }
//!
//!     fn init(load: &mut Load<'_>) -> Result<Self, Off> {
//!         let settings: Settings = load.settings("bonus = 1\n")?;
//!         load.hook(&REWARD, load.symbol("quest.reward")?)?;
//!         load.on::<events::WorldLoaded>(|()| enshwick_api::info!("world loaded"))?;
//!         Ok(Self { bonus: settings.bonus })
//!     }
//! }
//!
//! enshwick_api::declare_mod!(Bonus);
//!
//! extern "system" fn vanilla(base: u32) -> u32 {
//!     base
//! }
//!
//! let loader = enshwick_test::FakeHost::new()
//!     .symbol("quest.reward", enshwick_test::address(vanilla as extern "system" fn(u32) -> u32))
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

use std::collections::{HashMap, HashSet};
use std::sync::MutexGuard;

use enshwick_api::abi::{
    CallerFrame, CommandCall, Event, Fingerprint, Header, HookFlags, HostApi, InventoryOp,
    InventoryOpResult, InventoryResult, ItemData, Level, ModHandle, ModInfo, PlayerInfo,
    RenameCall, ResourceKey, Side, Status, Str,
};
use enshwick_api::events::Kind;
use enshwick_api::{FnPtr, Hook};

/// A mod's `enshwick_mod_init` export, as [`declare_mod!`](enshwick_api::declare_mod)
/// writes it.
pub type InitFn = unsafe extern "system" fn(*const HostApi, ModHandle) -> Status;

/// A mod's `enshwick_mod_ready` export, as [`declare_mod!`](enshwick_api::declare_mod)
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
/// let target = enshwick_test::address(vanilla as extern "system" fn());
/// assert_ne!(target, 0);
/// ```
#[must_use]
pub fn address<F: FnPtr>(function: F) -> u64 {
    // SAFETY: `FnPtr` implementors are function pointers the size of `usize`.
    let address: usize = unsafe { core::mem::transmute_copy(&function) };
    address as u64
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
/// use enshwick_api::abi::{Level, Side};
///
/// let loader = enshwick_test::FakeHost::new()
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
    /// use enshwick_api::abi::Side;
    ///
    /// let loader = enshwick_test::FakeHost::new().side(Side::CLIENT).hosting(true).install();
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
    /// let loader = enshwick_test::FakeHost::new().server_world(0x2000).install();
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
    /// let loader = enshwick_test::FakeHost::new().base(0x1_4000_0000).install();
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
    /// use enshwick_api::abi::{Fingerprint, guid};
    ///
    /// let build = Fingerprint { guid: guid("1c129660cd6efc4f9f0b21652d5ff2bf"), age: 387 };
    /// let loader = enshwick_test::FakeHost::new().fingerprint(build).install();
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
    /// use enshwick_api::abi::ModHandle;
    ///
    /// let loader = enshwick_test::FakeHost::new().handle(ModHandle(7)).install();
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
    /// let loader = enshwick_test::FakeHost::new()
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
    /// let loader = enshwick_test::FakeHost::new().data_dir("target/mod-data/bonus").install();
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
    /// use enshwick_api::abi::CallerFrame;
    ///
    /// let frame = CallerFrame { rsp: 0x1000, ..CallerFrame::default() };
    /// let loader = enshwick_test::FakeHost::new().caller_frame(frame).install();
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
    /// let loader = enshwick_test::FakeHost::new()
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
    /// use enshwick_api::abi::{Header, PlayerInfo};
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
    /// let loader = enshwick_test::FakeHost::new().player(player).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn player(mut self, player: PlayerInfo) -> Self {
        self.players.push(player);
        self
    }

    /// A loaded mod `enshwick_api::mods()` lists, in order; the loader
    /// itself comes first in the game.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_api::abi::{Header, ManifestFlags, ModInfo, Side, Str, UpdateSource};
    ///
    /// let loader = enshwick_test::FakeHost::new()
    ///     .loaded(ModInfo {
    ///         header: Header::of::<ModInfo>(),
    ///         id: Str::new("enshwick"),
    ///         version: Str::new("0.2.0"),
    ///         side: Side::BOTH,
    ///         flags: ManifestFlags::NONE,
    ///         off: 0,
    ///         file: Str::new("mods/enshwick/enshwick.dll"),
    ///         updates: UpdateSource::NONE,
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
    /// `enshwick_api::mods::join_missing()` lists, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_api::abi::{Header, ManifestFlags, ModInfo, Side, Str, UpdateSource};
    ///
    /// let loader = enshwick_test::FakeHost::new()
    ///     .join_missing(ModInfo {
    ///         header: Header::of::<ModInfo>(),
    ///         id: Str::new("safes"),
    ///         version: Str::new("0.2.0"),
    ///         side: Side::BOTH,
    ///         flags: ManifestFlags::JOIN,
    ///         off: 1,
    ///         file: Str::EMPTY,
    ///         updates: UpdateSource::github("themaiby/enshwick", "enshwick-safes"),
    ///     })
    ///     .install();
    /// // SAFETY: the installed fake's table.
    /// let api = unsafe { &*loader.api() };
    /// let mut count = 0;
    /// let size = u32::try_from(size_of::<ModInfo>()).unwrap();
    /// let status = (api.join_missing)(loader.handle(), core::ptr::null_mut(), size, 0, &mut count);
    /// assert!(status.is_ok());
    /// assert_eq!(count, 1);
    /// ```
    #[must_use]
    pub fn join_missing(mut self, info: ModInfo) -> Self {
        self.join_missing.push(info);
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
    /// let loader = enshwick_test::FakeHost::new()
    ///     .service("flame-altar-zones.zones", 1, (&raw const ZONES).cast())
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
    /// let loader = enshwick_test::FakeHost::new().refuse_hook(0x1_4009_0a60).install();
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
    /// let loader = enshwick_test::FakeHost::new().code(&CODE).install();
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
    /// let loader = enshwick_test::FakeHost::new().workshop_recipes(u32::MAX).install();
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
    /// use enshwick_api::abi::ItemData;
    ///
    /// let loader = enshwick_test::FakeHost::new().item(0x5e01, 1, ItemData::HAS_PIDE).install();
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
    /// let loader = enshwick_test::FakeHost::new().without_inventories().install();
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
    /// let loader = enshwick_test::FakeHost::new().install();
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
    /// [`declare_mod!`](enshwick_api::declare_mod) exported.
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
    /// [`declare_mod!`](enshwick_api::declare_mod) exported, and its
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
    /// [`declare_mod!`](enshwick_api::declare_mod).
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
    /// let loader = enshwick_test::FakeHost::new().install();
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
    /// let loader = enshwick_test::FakeHost::new().install();
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
    /// let loader = enshwick_test::FakeHost::new().install();
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
    /// let loader = enshwick_test::FakeHost::new().install();
    /// let mut registers = enshwick_api::Registers::default();
    /// assert_eq!(loader.run_at(0x1_4000_1000, &mut registers), 0);
    /// ```
    #[must_use = "the count says whether the mod hooks the address"]
    pub fn run_at(&self, address: u64, registers: &mut enshwick_api::Registers) -> usize {
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
    /// let loader = enshwick_test::FakeHost::new().install();
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
    /// use enshwick_api::abi::{Header, TickData};
    /// use enshwick_api::events;
    ///
    /// let loader = enshwick_test::FakeHost::new().install();
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
    /// use enshwick_api::abi::Event;
    ///
    /// let loader = enshwick_test::FakeHost::new().install();
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

    /// Replaces the text of the mod's settings file, as a player's edit
    /// would; raise [`events::SettingsChanged`](enshwick_api::events::SettingsChanged)
    /// after it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_api::events;
    ///
    /// let loader = enshwick_test::FakeHost::new().settings("limit = 7\n").install();
    /// loader.set_settings("limit = 9\n");
    /// assert_eq!(loader.raise::<events::SettingsChanged>(&()), 0);
    /// ```
    pub fn set_settings(&self, text: &str) {
        table::with((), |s| s.host.settings = Some(text.to_owned()));
    }

    /// Sets the join values of the server this client joined, which the mod
    /// gets over its file's on its next read, or `None` for none, as after
    /// joining a server without them; raise
    /// [`events::SettingsChanged`](enshwick_api::events::SettingsChanged)
    /// after it, as the loader does.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_test::FakeHost::new().install();
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
    /// let loader = enshwick_test::FakeHost::new().install();
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
    /// let loader = enshwick_test::FakeHost::new().install();
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
    /// use enshwick_api::abi::{ResourceKey, Status};
    ///
    /// let loader = enshwick_test::FakeHost::new().install();
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
    /// let loader = enshwick_test::FakeHost::new().install();
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
    /// let loader = enshwick_test::FakeHost::new().install();
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
    /// use enshwick_api::abi::{Header, PlayerInfo};
    ///
    /// let loader = enshwick_test::FakeHost::new().install();
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
        Some(unsafe { reply.as_str() }.unwrap_or("").to_owned())
    }

    /// The commands the mod registered, as `(name, help)`, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_test::FakeHost::new().install();
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
    /// let loader = enshwick_test::FakeHost::new().install();
    /// assert!(loader.chat().is_empty());
    /// ```
    #[must_use]
    pub fn chat(&self) -> Vec<(u64, String)> {
        table::with(Vec::new(), |s| s.chat.clone())
    }

    /// The labels the mod set, as `(entity, text)`, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_test::FakeHost::new().install();
    /// assert!(loader.labels().is_empty());
    /// ```
    #[must_use]
    pub fn labels(&self) -> Vec<(u32, String)> {
        table::with(Vec::new(), |s| s.labels.clone())
    }

    /// Asks the mod's rename filters about `player` renaming `entity` to
    /// `text`; whether every filter lets it through.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick_test::FakeHost::new().install();
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
    /// let loader = enshwick_test::FakeHost::new().install();
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
    /// let loader = enshwick_test::FakeHost::new().install();
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
    /// use enshwick_api::abi::InventoryResult;
    ///
    /// let loader = enshwick_test::FakeHost::new().install();
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
