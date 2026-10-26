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
//! # One mod life per install
//!
//! Every [`FakeHost::install`] starts a new mod life: the mod's `init` runs
//! again and the platform's state starts empty. Under `cargo test` the tests
//! of one binary share the process; the fake serves one test at a time, and
//! a second test waits until the first drops its [`Installed`]. State a mod
//! keeps in a `static` starts afresh only in a
//! [`ModLocal`](crate::state::ModLocal).
//!
//! # Examples
//!
//! ```rust,standalone_crate
//! use enshwick::abi::{Level, Status};
//! use enshwick::{Load, Mod, Next, Off, events, hook};
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
//!     fn manifest() -> &'static enshwick::abi::Manifest {
//!         enshwick::manifest!(id: "you.bonus", side: SERVER)
//!     }
//!
//!     fn init(load: &mut Load<'_>) -> Result<Self, Off> {
//!         let settings: Settings = load.settings("bonus = 1\n")?;
//!         let reward = load.symbol("quest.reward")?;
//!         // SAFETY: the game's reward function has the type `REWARD` declares.
//!         unsafe { load.hook(&REWARD, reward) }?;
//!         load.on::<events::WorldLoaded>(|()| enshwick::info!("world loaded"))?;
//!         Ok(Self { bonus: settings.bonus })
//!     }
//! }
//!
//! enshwick::declare_mod!(Bonus);
//!
//! extern "system" fn vanilla(base: u32) -> u32 {
//!     base
//! }
//!
//! let loader = enshwick::test::FakeHost::new()
//!     .symbol("quest.reward", enshwick::test::address(vanilla as extern "system" fn(u32) -> u32))
//!     .settings("bonus = 5\n")
//!     .install();
//! // SAFETY: `enshwick_mod_init` and `enshwick_mod_ready` are the exports
//! // `declare_mod!(Bonus)` generated in this example.
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

mod content;
mod events;
mod gameplay;
mod hooks;
mod inventory;
mod players;
mod points;
mod rates;
mod settings;
mod stats;
mod table;
mod ui;
mod world;

pub use content::ServedResource;
pub use hooks::LinkedHook;
pub use inventory::SubmittedRequest;
pub use settings::SettingsRefused;
pub use ui::{Drawn, FakeMarker, Input, ShownDialog};

/// The modding book's chapter on tests, whose examples run as doctests here.
#[cfg(doctest)]
#[doc = include_str!("../../../book/src/testing.md")]
#[expect(dead_code, reason = "the doc comment carries the chapter")]
struct BookTesting;

/// The modding book's chapter on migrations, which ends with a test.
#[cfg(doctest)]
#[doc = include_str!("../../../book/src/migrations.md")]
#[expect(dead_code, reason = "the doc comment carries the chapter")]
struct BookMigrations;

use std::collections::{HashMap, HashSet};
use std::sync::MutexGuard;

use crate::FnPtr;
use crate::abi::{
    CallerFrame, EntityPlace, Fingerprint, HostApi, ItemData, Level, ModHandle, ModInfo,
    PlayerInfo, Side, Status,
};

use players::{FakeService, ask, console};
use settings::FakeMigrations;

/// A mod's `enshwick_mod_init` export, as [`declare_mod!`](crate::declare_mod)
/// writes it.
pub type InitFn = unsafe extern "system" fn(*const HostApi, ModHandle) -> Status;

/// A mod's `enshwick_mod_ready` export, as [`declare_mod!`](crate::declare_mod)
/// writes it.
pub type ReadyFn = unsafe extern "system" fn(*const HostApi, ModHandle);

/// The id the fake loader gives the mod it runs: the namespace its stats,
/// item data and services take.
///
/// # Examples
///
/// ```
/// assert!(enshwick::abi::is_valid_id(enshwick::test::MOD_ID));
/// ```
pub const MOD_ID: &str = "test.fake";

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
/// let target = enshwick::test::address(vanilla as extern "system" fn());
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
/// use enshwick::abi::{Level, Side};
///
/// let loader = enshwick::test::FakeHost::new()
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
    table: crate::abi::BuildTable,
    handle: ModHandle,
    symbols: HashMap<String, u64>,
    systems: HashMap<String, u64>,
    constants: HashMap<String, u64>,
    field_offsets: HashMap<(String, String), u64>,
    type_sizes: HashMap<String, u32>,
    type_hashes: HashMap<String, u32>,
    event_types: Vec<(&'static str, u32, u32)>,
    settings: Option<String>,
    other_settings: HashMap<String, String>,
    server_synced: Option<String>,
    data_dir: String,
    log_level: Level,
    caller_frame: CallerFrame,
    components: HashMap<(u32, u32), (usize, usize)>,
    templates: HashMap<u32, [u8; 16]>,
    players: Vec<PlayerInfo>,
    mods: Vec<ModInfo>,
    hosting: bool,
    server_world: u64,
    join_missing: Vec<ModInfo>,
    replaced: Vec<ModInfo>,
    language: u32,
    services: Vec<FakeService>,
    refused_hooks: HashSet<u64>,
    code: (u64, u64),
    workshop_recipes: u32,
    items: HashMap<u32, ItemData>,
    no_inventories: bool,
    console_answers: HashMap<String, String>,
    placed: Vec<(u32, EntityPlace)>,
    world_key: Option<String>,
    refused_writes: HashSet<u32>,
    net_peers: HashSet<u64>,
    icons_ready: bool,
    archive: std::collections::BTreeMap<[u32; 4], Vec<u8>>,
    resources: HashMap<([u8; 16], u32, u32), Vec<u8>>,
    game_texts: HashMap<(u32, u32), &'static str>,
    disabled_recipes: HashSet<u32>,
    missing_rates: HashSet<crate::abi::RateId>,
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
            table: crate::abi::BuildTable::EXACT,
            handle: ModHandle(1),
            symbols: HashMap::new(),
            systems: HashMap::new(),
            constants: HashMap::new(),
            field_offsets: HashMap::new(),
            type_sizes: HashMap::new(),
            type_hashes: HashMap::new(),
            event_types: Vec::new(),
            settings: None,
            other_settings: HashMap::new(),
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
            replaced: Vec::new(),
            language: 0,
            services: Vec::new(),
            refused_hooks: HashSet::new(),
            code: (0, 0),
            workshop_recipes: 512,
            items: HashMap::new(),
            no_inventories: false,
            console_answers: HashMap::new(),
            placed: Vec::new(),
            world_key: None,
            refused_writes: HashSet::new(),
            net_peers: HashSet::new(),
            icons_ready: false,
            archive: std::collections::BTreeMap::new(),
            resources: HashMap::new(),
            game_texts: HashMap::new(),
            disabled_recipes: HashSet::new(),
            missing_rates: HashSet::new(),
        }
    }

    /// The handle the mod receives; `ModHandle(1)` by default.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::ModHandle;
    ///
    /// let loader = enshwick::test::FakeHost::new().handle(ModHandle(7)).install();
    /// assert_eq!(loader.handle(), ModHandle(7));
    /// ```
    #[must_use]
    pub fn handle(mut self, handle: ModHandle) -> Self {
        self.handle = handle;
        self
    }

    /// The language the game shows its texts in, which
    /// `enshwick::i18n::language()` answers; the fallback by
    /// default.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::i18n::Language;
    ///
    /// let _loader = enshwick::test::FakeHost::new()
    ///     .language(Language::UK_UA)
    ///     .install();
    /// ```
    #[must_use]
    pub const fn language(mut self, language: crate::i18n::Language) -> Self {
        self.language = language.0;
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
        crate::fresh::renew();
        Installed {
            handle,
            _turn: turn,
        }
    }
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
    /// let loader = enshwick::test::FakeHost::new().install();
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
    /// - `init` is the `enshwick_mod_init` that
    ///   [`declare_mod!`](crate::declare_mod) exported.
    ///
    /// # Examples
    ///
    /// See [`crate`].
    pub unsafe fn init(&self, init: InitFn) -> Status {
        // SAFETY: the caller guarantees `init` is the `enshwick_mod_init` of a
        // `declare_mod!`, which takes the static fake table and a handle.
        unsafe { init(self.api(), self.handle) }
    }

    /// Tells the mod its hooks are armed.
    ///
    /// # Safety
    ///
    /// - `ready` is the `enshwick_mod_ready` that
    ///   [`declare_mod!`](crate::declare_mod) exported.
    /// - Its `enshwick_mod_init` returned [`Status::OK`] on this fake.
    ///
    /// # Examples
    ///
    /// See [`crate`].
    pub unsafe fn ready(&self, ready: ReadyFn) {
        // SAFETY: the caller guarantees `ready` is the `enshwick_mod_ready` of
        // a `declare_mod!` whose init succeeded on this fake.
        unsafe { ready(self.api(), self.handle) }
    }

    /// Initialises the mod and, when it agrees, tells it its hooks are armed;
    /// returns the answer of `init`.
    ///
    /// # Safety
    ///
    /// - `init` and `ready` are the `enshwick_mod_init` and
    ///   `enshwick_mod_ready` exports of one
    ///   [`declare_mod!`](crate::declare_mod).
    ///
    /// # Examples
    ///
    /// See [`crate`].
    pub unsafe fn start(&self, init: InitFn, ready: ReadyFn) -> Status {
        // SAFETY: the caller guarantees `init` is the `enshwick_mod_init` of a
        // `declare_mod!`.
        let status = unsafe { self.init(init) };
        if status.is_ok() {
            // SAFETY: the caller guarantees `ready` is the matching
            // `enshwick_mod_ready`, and `status` says its init returned `OK`.
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
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.faults().is_empty());
    /// ```
    #[must_use]
    pub fn faults(&self) -> Vec<String> {
        table::with(Vec::new(), |s| s.faults.clone())
    }
}
