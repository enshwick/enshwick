//! A fake Enshwick loader for a mod's unit tests.
//!
//! [`FakeHost`] is configured with what the game and the loader would answer:
//! symbols, systems, constants, reflected types, the running world's
//! settings files, the side, the players. [`FakeHost::install`] hands the loader's table to the test as an
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
//! use std::sync::atomic::{AtomicU32, Ordering};
//!
//! use enshwick::abi::{Level, Status};
//! use enshwick::{Load, Mod, Next, Off, events, hook};
//!
//! /// The bonus settings.
//! #[enshwick::config]
//! #[derive(Default)]
//! struct Settings {
//!     /// What each reward gains.
//!     bonus: u32,
//! }
//!
//! static BONUS: AtomicU32 = AtomicU32::new(0);
//!
//! struct Bonus;
//!
//! #[hook]
//! fn reward(next: Next<'_, extern "system" fn(u32) -> u32>, base: u32) -> u32 {
//!     next.call(base) + BONUS.load(Ordering::Relaxed)
//! }
//!
//! impl Mod for Bonus {
//!     fn init(load: &mut Load<'_>) -> Result<Self, Off> {
//!         load.on_settings(|settings: &Settings| BONUS.store(settings.bonus, Ordering::Relaxed))?;
//!         let reward = load.symbol("quest.reward")?;
//!         // SAFETY: the game's reward function has the type `REWARD` declares.
//!         unsafe { load.hook(&REWARD, reward) }?;
//!         load.on::<events::WorldLoaded>(|()| enshwick::info!("world loaded"))?;
//!         Ok(Self)
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
//! let status = unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
//! assert_eq!(status, Status::OK);
//!
//! let reward = loader.detour(&REWARD).ok_or("not linked")?;
//! assert_eq!(reward(10), 10, "no world yet: the defaults");
//! assert_eq!(loader.raise::<events::SettingsReplaced>(&()), 1);
//! assert_eq!(reward(10), 15);
//!
//! assert_eq!(loader.raise::<events::WorldLoaded>(&()), 1);
//! assert_eq!(loader.logs(), [(Level::INFO, "world loaded".to_owned())]);
//! # Ok::<(), &str>(())
//! ```

mod calls;
mod chat;
mod content;
#[cfg(feature = "unstable")]
mod controls;
mod events;
pub mod facts;
mod gameplay;
mod hooks;
mod inventory;
mod items;
mod pacing;
mod players;
mod points;
mod presets;
#[cfg(feature = "typed")]
mod progression;
mod props;
mod rates;
#[cfg(feature = "typed")]
mod removal;
mod saves;
mod settings;
mod stats;
pub(crate) mod table;
#[cfg(feature = "typed")]
mod tags;
mod templates;
mod ticks;
mod transactions;
mod ui;
mod voxel_edits;
mod world;

pub use content::{LiveChange, ServedResource};
pub use hooks::LinkedHook;
pub use inventory::SubmittedRequest;
#[cfg(feature = "typed")]
pub use progression::{
    ATTRIBUTE_NODE, PROGRESSION, QUEST_ITEM, ROOT_NODE, SKILL_ROOM, VANILLA_EXPERIENCE,
    VANILLA_PERK, VANILLA_PLAYER_KNOWLEDGE, VANILLA_QUEST, VANILLA_QUEST_EXPERIENCE,
    VANILLA_WORLD_KNOWLEDGE,
};
#[cfg(feature = "typed")]
pub use removal::{ENTITY_ITEM, FILL_TABLE, LOOT_ITEM, OTHER_ITEM, RANDOM_LOOT, SPARE_ITEM};
pub use settings::SettingsRefused;
#[cfg(feature = "typed")]
pub use tags::TAG_CONTAINER;
pub use templates::{
    BITE, BOSS_CREATURE, CRAFTING_PROP_ITEM, CREATURE, ENEMY, ENEMY_ARSENAL, HOWL, LUNGE, NEST,
    NEST_GROUP, OTHER_ARSENAL, PROP_ITEM, VANILLA_CRAFTING_PROPS,
};
pub use transactions::{FakeChange, FakeTransaction};
pub use ui::{Drawn, FakeMarker, Input, ShownDialog};

/// Exports a mod as [`declare_mod!`](crate::declare_mod) does, with its
/// `[package.metadata.enshwick]` table given as TOML, so one test binary may
/// declare several mods.
///
/// The text takes the keys of the table, read by the same rules; the
/// version and the rest of `[package]` still come from the crate. Each
/// mod's manifest carries the schemas of the binary's points, events and
/// calls named with its id and marked `export`.
///
/// # Examples
///
/// ```rust,standalone_crate
/// struct Lockers;
///
/// impl enshwick::Mod for Lockers {
///     fn init(_load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
///         Ok(Self)
///     }
/// }
///
/// enshwick::test::declare_mod!(
///     Lockers,
///     r#"
///     id = "you.lockers"
///     side = "server"
///
///     [after]
///     lights = "you.lanterns"
///     "#
/// );
///
/// // SAFETY: `enshwick_mod_query` returns the manifest of
/// // `declare_mod!`, valid for the process.
/// let manifest = unsafe { &*enshwick_mod_query() };
/// // SAFETY: `declare_mod!` points the id at a string literal, alive for the process.
/// assert_eq!(unsafe { manifest.id.as_str() }, Some("you.lockers"));
/// assert_eq!(manifest.dependency_count, 1);
/// ```
pub use enshwick_macros::declare_test_mod as declare_mod;

/// The modding book's chapter on tests, whose examples run as doctests here.
#[cfg(doctest)]
#[doc = include_str!("../../../book/src/testing.md")]
#[expect(dead_code, reason = "the doc comment carries the chapter")]
struct BookTesting;

use std::sync::MutexGuard;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::FnPtr;
use crate::abi::{
    CallerFrame, EntityPlace, Fingerprint, HostApi, ItemData, Level, ModHandle, ModInfo,
    PlayerInfo, Side, Status,
};

use players::{FakeService, ask, console};

/// A mod's `enshwick_mod_init` export, as [`declare_mod!`](crate::declare_mod)
/// writes it.
///
/// # Examples
///
/// See [`Installed::start`], which takes one.
pub type InitFn = unsafe extern "system" fn(*const HostApi, ModHandle) -> Status;

/// A mod's `enshwick_mod_ready` export, as [`declare_mod!`](crate::declare_mod)
/// writes it.
///
/// # Examples
///
/// See [`Installed::start`], which takes one.
pub type ReadyFn = unsafe extern "system" fn(*const HostApi, ModHandle);

/// A mod's `enshwick_mod_query` export, as [`declare_mod!`](crate::declare_mod)
/// writes it: the manifest the fake reads before init, as the loader does.
///
/// # Examples
///
/// See [`Installed::start`], which takes one.
pub type QueryFn = extern "system" fn() -> *const crate::abi::Manifest;

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

/// Whether the fake's game build has the base code the loader hooks to
/// raise the base limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BaseCode {
    /// The platform knows it.
    Known,
    /// The platform raises the zones but cannot hold bases past 16.
    ZonesOnly,
    /// An unknown build: the game keeps its own limits.
    Unknown,
}

/// What the fake loader answers: a builder, installed with
/// [`FakeHost::install`].
///
/// Lookups the test did not configure answer [`Status::MISSING`], as a game
/// build without them would. A world's settings files are empty, so the mod
/// reads its own defaults; the side is [`Side::SERVER`] and the log level
/// [`Level::INFO`].
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
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is a switch of its own that a test sets through its builder"
)]
pub struct FakeHost {
    side: Side,
    base: u64,
    fingerprint: Fingerprint,
    table: crate::abi::BuildTable,
    handle: ModHandle,
    symbols: FxHashMap<String, u64>,
    systems: FxHashMap<String, u64>,
    constants: FxHashMap<String, u64>,
    field_offsets: FxHashMap<(String, String), u64>,
    type_sizes: FxHashMap<String, u32>,
    type_hashes: FxHashMap<String, u32>,
    event_types: Vec<(&'static str, u32, u32)>,
    /// The running world's settings files of the mod, by name.
    settings: std::collections::BTreeMap<String, String>,
    data_dir: String,
    mod_dir: String,
    log_level: Level,
    /// The levels of data packs that differ from `log_level`.
    pack_log_levels: Vec<(ModHandle, Level)>,
    caller_frame: CallerFrame,
    components: FxHashMap<(u32, u32), (usize, usize)>,
    templates: FxHashMap<u32, [u8; 16]>,
    players: Vec<PlayerInfo>,
    mods: Vec<ModInfo>,
    hosting: bool,
    developer_tools: bool,
    host_key: Option<crate::PlayerKey>,
    server_world: u64,
    join_missing: Vec<ModInfo>,
    /// What other mods export, by id; a mod left out exports every name.
    exports: FxHashMap<String, Vec<String>>,
    language: u32,
    services: Vec<FakeService>,
    refused_hooks: FxHashSet<u64>,
    code: (u64, u64),
    workshop_recipes: u32,
    items: FxHashMap<u32, ItemData>,
    no_inventories: bool,
    no_entity_commands: bool,
    base_code: BaseCode,
    console_answers: FxHashMap<String, String>,
    placed: Vec<(u32, EntityPlace)>,
    world_key: Option<String>,
    fog: Option<(crate::abi::FogInfo, Vec<u8>)>,
    map_markers: Vec<crate::abi::MapMarkerInfo>,
    map_frame: Option<crate::abi::MapFrameInfo>,
    map_marker_types: Option<Vec<crate::abi::MapMarkerTypeInfo>>,
    refused_writes: FxHashSet<u32>,
    static_halves: FxHashSet<u32>,
    net_peers: FxHashSet<u64>,
    icons_ready: bool,
    archive: std::collections::BTreeMap<[u32; 4], Vec<u8>>,
    resources: FxHashMap<([u8; 16], u32, u32), Vec<u8>>,
    types: Option<std::sync::Arc<crate::resources::TypeTable>>,
    /// The types of every [`FakeHost::types`] call, which `types` is built from.
    type_list: Vec<crate::resources::Type<'static>>,
    /// The player's keys by control name, in place of the mod's defaults.
    #[cfg(feature = "unstable")]
    bindings: Vec<(String, usize, Option<crate::controls::Key>)>,
    game_version: Option<&'static str>,
    game_texts: FxHashMap<(u32, u32), &'static str>,
    disabled_recipes: FxHashSet<u32>,
    recipe_kinds: FxHashMap<u32, crate::abi::RecipeKind>,
    missing_rates: FxHashSet<crate::abi::RateId>,
    voxels: Vec<world::VoxelGrid>,
    zones: world::FakeZones,
    extent: Option<[u32; 3]>,
    blueprints: FxHashMap<u32, [u32; 3]>,
    props: Vec<props::FakeProp>,
    /// The items whose props are logic triggers.
    trigger_items: Vec<u32>,
    /// The items whose props are plants.
    growing_items: Vec<u32>,
    /// The items that place something that is no prop: a creature, a
    /// flame altar.
    no_prop_items: Vec<u32>,
    /// Each item's placement box, lowest and highest corner before it
    /// turns.
    placement_boxes: Vec<(u32, [f32; 3], [f32; 3])>,
    building_materials: FxHashMap<u32, u8>,
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
            symbols: FxHashMap::default(),
            systems: FxHashMap::default(),
            constants: FxHashMap::default(),
            field_offsets: FxHashMap::default(),
            type_sizes: FxHashMap::default(),
            type_hashes: FxHashMap::default(),
            event_types: Vec::new(),
            settings: std::collections::BTreeMap::new(),
            data_dir: "world0-mod-data/test".to_owned(),
            mod_dir: "mods/test".to_owned(),
            log_level: Level::INFO,
            pack_log_levels: Vec::new(),
            caller_frame: CallerFrame::default(),
            components: FxHashMap::default(),
            templates: FxHashMap::default(),
            players: Vec::new(),
            mods: Vec::new(),
            hosting: false,
            developer_tools: false,
            host_key: None,
            server_world: 0,
            join_missing: Vec::new(),
            exports: FxHashMap::default(),
            language: 0,
            services: Vec::new(),
            refused_hooks: FxHashSet::default(),
            code: (0, 0),
            workshop_recipes: 512,
            items: FxHashMap::default(),
            no_inventories: false,
            no_entity_commands: false,
            base_code: BaseCode::Known,
            console_answers: FxHashMap::default(),
            placed: Vec::new(),
            world_key: None,
            fog: None,
            map_markers: Vec::new(),
            map_frame: None,
            map_marker_types: None,
            refused_writes: FxHashSet::default(),
            static_halves: FxHashSet::default(),
            net_peers: FxHashSet::default(),
            icons_ready: false,
            archive: std::collections::BTreeMap::new(),
            resources: FxHashMap::default(),
            types: None,
            type_list: Vec::new(),
            #[cfg(feature = "unstable")]
            bindings: Vec::new(),
            game_version: None,
            game_texts: FxHashMap::default(),
            disabled_recipes: FxHashSet::default(),
            recipe_kinds: FxHashMap::default(),
            missing_rates: FxHashSet::default(),
            voxels: Vec::new(),
            zones: world::FakeZones::default(),
            extent: None,
            blueprints: FxHashMap::default(),
            props: Vec::new(),
            trigger_items: Vec::new(),
            growing_items: Vec::new(),
            no_prop_items: Vec::new(),
            placement_boxes: Vec::new(),
            building_materials: FxHashMap::default(),
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

    /// Whether developer tools are on, as the file
    /// [`DEVELOPER_TOOLS`](crate::mods::DEVELOPER_TOOLS) turns them on; off
    /// by default, as in a game players use. While they are off the loader
    /// turns a mod with `developer: true` off before its `init`, so the
    /// fake's init returns [`Status::REFUSED`](crate::abi::Status::REFUSED)
    /// with the reason in [`Installed::faults`].
    ///
    /// # Examples
    ///
    /// ```
    /// let _loader = enshwick::test::FakeHost::new()
    ///     .developer_tools(true)
    ///     .install();
    /// ```
    #[must_use]
    pub const fn developer_tools(mut self, on: bool) -> Self {
        self.developer_tools = on;
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
        let status = unsafe { init(self.api(), self.handle) };
        // The loader arms once every mod is initialised; what only init may
        // ask for comes too late from here on.
        table::with((), |s| s.registration = table::Registration::Closed);
        status
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
        table::with((), |s| s.base_limit.armed = true);
        // SAFETY: the caller guarantees `ready` is the `enshwick_mod_ready` of
        // a `declare_mod!` whose init succeeded on this fake.
        unsafe { ready(self.api(), self.handle) }
    }

    /// Reads the mod's manifest, initialises the mod unless the loader would
    /// refuse it first, and, when init agrees, tells it its hooks are armed;
    /// returns the answer of `init`, or [`Status::REFUSED`] for a refused mod.
    ///
    /// A mod with `developer: true` is refused before its init while
    /// [`FakeHost::developer_tools`] is off, and a mod whose side is not
    /// [`Side::SERVER`](crate::abi::Side::SERVER) or
    /// [`Side::BOTH`](crate::abi::Side::BOTH) always, as the loader refuses
    /// them.
    ///
    /// # Safety
    ///
    /// - `query`, `init` and `ready` are the `enshwick_mod_query`,
    ///   `enshwick_mod_init` and `enshwick_mod_ready` exports of one mod,
    ///   such as one [`declare_mod!`](crate::declare_mod) writes.
    ///
    /// # Examples
    ///
    /// See [`crate`].
    pub unsafe fn start(&self, query: QueryFn, init: InitFn, ready: ReadyFn) -> Status {
        // SAFETY: `query` returns null or a manifest valid for the process.
        if let Some(manifest) = unsafe { query().as_ref() }
            && (table::refuses_side(self.api(), manifest)
                || table::refuses_developer(self.api(), manifest))
        {
            return Status::REFUSED;
        }
        // SAFETY: the caller guarantees `init` is the `enshwick_mod_init` of
        // the mod whose manifest `query` returned.
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

    /// Every log line and fault line with the handle it came under, so a
    /// test tells a data pack's lines from the loader's.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.sent().is_empty());
    /// ```
    #[must_use]
    pub fn sent(&self) -> Vec<(ModHandle, Level, String)> {
        table::with(Vec::new(), |s| s.sent.clone())
    }
}
