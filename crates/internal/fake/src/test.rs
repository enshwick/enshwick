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
//! [`ModLocal`](enshwick_core::state::ModLocal).
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

// The fake's parts, one per domain of `enshwick-core`'s `domains.toml`,
// lowest first; `table` joins their `HostApi` entries.
mod actions;
mod base;
mod content;
mod inventory;
mod knowledge;
pub(crate) mod table;
mod ui;
mod world;

pub use base::facts;
pub use base::hooks::LinkedHook;
pub use base::settings::SettingsRefused;
#[cfg(feature = "typed")]
pub use content::removal::{
    ENTITY_ITEM, FILL_TABLE, LOOT_ITEM, OTHER_ITEM, RANDOM_LOOT, SPARE_ITEM,
};
pub use content::templates::{
    BITE, BOSS_CREATURE, CRAFTING_PROP_ITEM, CREATURE, ENEMY, ENEMY_ARSENAL, HOWL, LUNGE, NEST,
    NEST_GROUP, OTHER_ARSENAL, PROP_ITEM, VANILLA_CRAFTING_PROPS,
};
pub use content::{LiveChange, ServedResource};
pub use inventory::SubmittedRequest;
pub use inventory::containers::{CHEST_ITEM, CHEST_SLOTS};
#[cfg(feature = "typed")]
pub use knowledge::item_sets::{ITEM_SET_CATEGORY, VANILLA_ITEM_SET, VANILLA_ITEM_SET_REWARD};
#[cfg(feature = "typed")]
pub use knowledge::queries::{
    QUERY_DATABASE, VANILLA_PLAYER_QUERY, VANILLA_QUERY_NAME, VANILLA_WORLD_QUERY,
};
#[cfg(feature = "typed")]
pub use knowledge::rules::KNOWLEDGE_RULES;
#[cfg(feature = "typed")]
pub use knowledge::vanilla::{
    ATTRIBUTE_NODE, BALANCING_TABLE, LORE_CATEGORY, PROGRESSION, QUEST_ITEM, ROOT_NODE, SKILL_ROOM,
    VANILLA_EXPERIENCE, VANILLA_PERK, VANILLA_PLAYER_KNOWLEDGE, VANILLA_QUEST,
    VANILLA_QUEST_EXPERIENCE, VANILLA_UNNAMED_EXPERIENCE, VANILLA_WORLD_KNOWLEDGE,
};
#[cfg(feature = "unstable")]
pub use ui::FakeSound;
pub(crate) use ui::slot_key;
pub use ui::{Drawn, FakeMarker, Input, ShownDialog};
#[cfg(feature = "typed")]
pub use world::tags::TAG_CONTAINER;
pub use world::transactions::{FakeChange, FakeTransaction};

/// Exports a mod as [`declare_mod!`](enshwick_core::declare_mod) does, with its
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

use std::sync::MutexGuard;

use enshwick_core::FnPtr;
use enshwick_core::abi::{HostApi, Level, ModHandle, Status};

use actions::commands::{ask, console};
use base::host::FakeService;

/// A mod's `enshwick_mod_init` export, as [`declare_mod!`](enshwick_core::declare_mod)
/// writes it.
///
/// # Examples
///
/// See [`Installed::start`], which takes one.
pub type InitFn = unsafe extern "system" fn(*const HostApi, ModHandle) -> Status;

/// A mod's `enshwick_mod_ready` export, as [`declare_mod!`](enshwick_core::declare_mod)
/// writes it.
///
/// # Examples
///
/// See [`Installed::start`], which takes one.
pub type ReadyFn = unsafe extern "system" fn(*const HostApi, ModHandle) -> Status;

/// A mod's `enshwick_mod_query` export, as [`declare_mod!`](enshwick_core::declare_mod)
/// writes it: the manifest the fake reads before init, as the loader does.
///
/// # Examples
///
/// See [`Installed::start`], which takes one.
pub type QueryFn = extern "system" fn() -> *const enshwick_core::abi::Manifest;

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
/// build without them would. A world's settings files are empty, so the mod
/// reads its own defaults; the side is
/// [`Side::SERVER`](enshwick_core::abi::Side::SERVER) and the log level
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
pub struct FakeHost {
    base: base::BasePresets,
    content: content::ContentPresets,
    world: world::WorldPresets,
    inventory: inventory::InventoryPresets,
    actions: actions::ActionsPresets,
    ui: ui::UiPresets,
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
            base: base::BasePresets::default(),
            content: content::ContentPresets::default(),
            world: world::WorldPresets::default(),
            inventory: inventory::InventoryPresets::default(),
            actions: actions::ActionsPresets::default(),
            ui: ui::UiPresets::default(),
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
        self.base.handle = handle;
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
    pub const fn language(mut self, language: enshwick_core::i18n::Language) -> Self {
        self.base.language = language.0;
        self
    }

    /// Whether developer tools are on, as the file
    /// [`DEVELOPER_TOOLS`](enshwick_core::mods::DEVELOPER_TOOLS) turns them on; off
    /// by default, as in a game players use. While they are off the loader
    /// turns a mod with `developer: true` off before its `init`, so the
    /// fake's init returns [`Status::REFUSED`](enshwick_core::abi::Status::REFUSED)
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
        self.base.developer_tools = on;
        self
    }

    /// Refuses the periodic tick subscription number `index`, from 0, in
    /// the order the mod asks
    /// ([`Status::REFUSED`](enshwick_core::abi::Status::REFUSED)), as the
    /// loader refuses a tick it cannot list, so a
    /// [`Load::scheduler`](enshwick_core::Load::scheduler) that asks for
    /// that one returns an [`Off`](enshwick_core::Off).
    ///
    /// # Examples
    ///
    /// ```
    /// let _loader = enshwick::test::FakeHost::new().refuse_tick(0).install();
    /// ```
    #[must_use]
    pub const fn refuse_tick(mut self, index: usize) -> Self {
        self.base.refused_tick = Some(index);
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
        let handle = self.base.handle;
        let turn = table::install(self);
        enshwick_core::__fake::check_manifests(table::base::mods::note_manifest);
        enshwick_core::__fake::renew();
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
/// See the [module](crate::test).
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

    /// Initialises the mod as the loader does, and returns its answer. An
    /// answer other than [`Status::OK`] turns the mod off, as the loader
    /// does: none of the callbacks it registered runs from then on.
    ///
    /// # Safety
    ///
    /// - `init` is the `enshwick_mod_init` that
    ///   [`declare_mod!`](enshwick_core::declare_mod) exported.
    ///
    /// # Examples
    ///
    /// See the [module](crate::test).
    pub unsafe fn init(&self, init: InitFn) -> Status {
        table::with((), |s| s.base.registration = table::Registration::Init);
        // SAFETY: the caller guarantees `init` is the `enshwick_mod_init` of a
        // `declare_mod!`, which takes the static fake table and a handle.
        let status = unsafe { init(self.api(), self.handle) };
        // The loader arms once every mod is initialised; what only init may
        // ask for comes too late from here on.
        table::with((), |s| s.base.registration = table::Registration::Closed);
        self.off_unless_ok(status)
    }

    /// Tells the mod its hooks are armed, and returns what its ready answers.
    /// An answer other than [`Status::OK`] turns the mod off, as the loader
    /// does: none of its callbacks runs from then on.
    ///
    /// # Safety
    ///
    /// - `ready` is the `enshwick_mod_ready` that
    ///   [`declare_mod!`](enshwick_core::declare_mod) exported.
    /// - Its `enshwick_mod_init` returned [`Status::OK`] on this fake.
    ///
    /// # Examples
    ///
    /// See the [module](crate::test).
    pub unsafe fn ready(&self, ready: ReadyFn) -> Status {
        table::with((), |s| {
            s.world.base_limit.armed = true;
            s.world.edits.readying = true;
        });
        // SAFETY: the caller guarantees `ready` is the `enshwick_mod_ready` of
        // a `declare_mod!` whose init succeeded on this fake.
        let status = unsafe { ready(self.api(), self.handle) };
        table::with((), |s| s.world.edits.readying = false);
        self.off_unless_ok(status)
    }

    /// Turns the mod off when `status` is not [`Status::OK`]; `status`.
    fn off_unless_ok(&self, status: Status) -> Status {
        if !status.is_ok() {
            table::base::registered::turn_off(self.handle);
        }
        status
    }

    /// Whether the mod is off: its init or ready failed or it faulted, as
    /// the loader turns a mod off. Its callbacks no longer run.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(!loader.is_off());
    /// ```
    #[must_use]
    pub fn is_off(&self) -> bool {
        table::base::registered::is_off(self.handle)
    }

    /// Reads the mod's manifest, initialises the mod unless the loader would
    /// refuse it first, and, when init agrees, tells it its hooks are armed;
    /// returns the answer of `init` when it fails, else that of `ready`, or
    /// [`Status::REFUSED`] for a refused mod.
    ///
    /// A mod with `developer: true` is refused before its init while
    /// [`FakeHost::developer_tools`] is off, and a mod whose side is not
    /// [`Side::SERVER`](enshwick_core::abi::Side::SERVER) or
    /// [`Side::BOTH`](enshwick_core::abi::Side::BOTH) always, as the loader refuses
    /// them.
    ///
    /// # Safety
    ///
    /// - `query`, `init` and `ready` are the `enshwick_mod_query`,
    ///   `enshwick_mod_init` and `enshwick_mod_ready` exports of one mod,
    ///   such as one [`declare_mod!`](enshwick_core::declare_mod) writes.
    ///
    /// # Examples
    ///
    /// See the [module](crate::test).
    pub unsafe fn start(&self, query: QueryFn, init: InitFn, ready: ReadyFn) -> Status {
        // SAFETY: `query` returns null or a manifest valid for the process.
        if let Some(manifest) = unsafe { query().as_ref() }
            && (table::base::mods::refuses_side(self.api(), manifest)
                || table::base::mods::refuses_developer(self.api(), manifest))
        {
            return Status::REFUSED;
        }
        // SAFETY: the caller guarantees `init` is the `enshwick_mod_init` of
        // the mod whose manifest `query` returned.
        let status = unsafe { self.init(init) };
        if !status.is_ok() {
            return status;
        }
        // SAFETY: the caller guarantees `ready` is the matching
        // `enshwick_mod_ready`, and `status` says its init returned `OK`.
        unsafe { self.ready(ready) }
    }

    /// The lines the mod logged, at the levels the loader kept.
    ///
    /// # Examples
    ///
    /// See the [module](crate::test).
    #[must_use]
    pub fn logs(&self) -> Vec<(Level, String)> {
        table::with(Vec::new(), |s| s.base.logs.clone())
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
        table::with(Vec::new(), |s| s.base.faults.clone())
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
        table::with(Vec::new(), |s| s.base.sent.clone())
    }
}
