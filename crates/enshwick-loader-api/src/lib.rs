//! The Enshwick modding API for Enshrouded: what a mod is written against.
//!
//! A mod is a DLL in the game's `mods/` folder. It implements [`Mod`],
//! exports itself with [`declare_mod!`], and during [`Mod::init`] links hooks
//! declared with [`detour!`] or [`#[hook]`](macro@hook) through
//! [`Load::hook`], reads its settings and finds game functions by name. The loader arms every hook once all mods are
//! initialised; a mod that fails or panics is turned off and the game runs
//! vanilla where it acted.
//!
//! [`abi`] is the binary contract with the loader; the rest of the crate is
//! the safe layer over it. With the `test` feature, `test` is a fake loader
//! for a mod's unit tests:
//!
//! ```toml
//! [dev-dependencies]
//! enshwick-loader-api = { git = "https://github.com/themaiby/enshwick", tag = "enshwick-loader-api-v<version>", features = ["test"] }
//! ```
//!
//! The derive and attribute macros come from `enshwick-loader-macros`, an
//! internal crate this one re-exports: depend on this crate only.
//!
//! # Examples
//!
//! ```
//! #[derive(serde::Deserialize)]
//! struct Settings {
//!     enabled: bool,
//! }
//!
//! struct Hello {
//!     settings: Settings,
//! }
//!
//! enshwick_loader_api::detour! {
//!     static UPGRADE: unsafe extern "system" fn(ctx: *mut u64) = upgrade;
//! }
//!
//! fn upgrade(next: enshwick_loader_api::Next<unsafe extern "system" fn(*mut u64)>, ctx: *mut u64) {
//!     // SAFETY: the original takes the same context.
//!     unsafe { next.get()(ctx) }
//! }
//!
//! impl enshwick_loader_api::Mod for Hello {
//!     fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
//!         enshwick_loader_api::manifest!(id: "hello", side: SERVER)
//!     }
//!
//!     fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<Self, enshwick_loader_api::Off> {
//!         let settings: Settings = load.settings("enabled = true\n")?;
//!         if settings.enabled {
//!             let target = load.system("upgrade_building_range")?;
//!             load.hook(&UPGRADE, target)?;
//!         }
//!         Ok(Self { settings })
//!     }
//! }
//!
//! enshwick_loader_api::declare_mod!(Hello);
//! ```

/// The version of this crate, which a mod's generated `Cargo.toml` pins.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// The derive names this crate by its path, as mods see it.
#[cfg(test)]
extern crate self as enshwick_loader_api;

pub mod abi;
pub mod actions;
pub mod affixes;
pub mod altars;
#[cfg(doctest)]
mod book;
pub mod buffs;
pub mod code;
pub mod commands;
pub mod config;
pub mod content;
pub mod damage;
pub mod daytime;
mod declare;
pub mod ecs;
pub mod entity_data;
pub mod events;
pub mod game_events;
pub mod game_settings;
mod hook;
mod host;
pub mod i18n;
pub mod interactions;
pub mod inventory;
pub mod item_data;
pub mod knowledge;
mod load;
pub mod loot;
pub mod migrations;
pub mod mods;
pub mod net;
pub mod pacing;
pub mod packs;
pub mod progress;
pub mod raw;
pub mod schedule;
pub mod services;
pub mod state;
pub mod stats;
pub mod status;
pub mod survivors;
pub mod terrain;
#[cfg(feature = "test")]
pub mod test;
pub mod ui;
pub mod updates;
pub mod vetoes;
pub mod weather;
pub mod world;

pub use enshwick_loader_macros::Config;
/// Declares a mod event other mods subscribe to: the struct becomes
/// `repr(C)` and implements [`services::ModEvent`] with a layout hash per
/// field.
///
/// `#[event(name = "<defining mod id>.<what>")]`; the struct is `Copy`
/// plain data and grows by appending fields, as a
/// [`#[service]`](macro@service) table does.
///
/// # Examples
///
/// ```
/// #[enshwick_loader_api::event(name = "trading-post.sold")]
/// #[derive(Clone, Copy, Debug)]
/// pub struct Sold {
///     pub item: u32,
///     pub price: u32,
/// }
///
/// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
///     load.on_mod_event(|sold: &Sold| enshwick_loader_api::info!("sold {} for {}", sold.item, sold.price))
/// }
/// ```
pub use enshwick_loader_macros::event;
/// Declares a hook from its body, as [`detour!`] does: the function stays,
/// and a static [`Hook`] named after it in upper case joins it.
///
/// The first argument is `next: Next<F>`, where `F` is the target's
/// `extern "system" fn` type or an alias of it; the other arguments are the
/// target's, by plain names. `#[hook(priority = <i32>, flags = <HookFlags>)]` sets the link's
/// order and flags. The static takes the function's visibility and doc
/// comment. A panic in the body falls through as with [`detour!`]; work after
/// `next` goes in [`Hook::after_next`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::HookFlags;
/// use enshwick_loader_api::{Next, hook};
///
/// /// Counts upgrades.
/// #[hook(priority = 5, flags = HookFlags::CALLER_FRAME)]
/// fn upgrade(next: Next<unsafe extern "system" fn(*mut u64)>, ctx: *mut u64) {
///     // SAFETY: the original takes the same context.
///     unsafe { next.get()(ctx) }
/// }
///
/// assert_eq!(UPGRADE.name(), "UPGRADE");
/// ```
pub use enshwick_loader_macros::hook;
/// Declares a service table other mods use: the struct becomes `repr(C)`
/// and implements [`services::Service`] with a layout hash per field, which
/// the loader checks when a mod asks for the service.
///
/// `#[service(name = "<publisher id>.<what>", version = <major>)]`, the
/// version 1 when left out. Fields are `extern "system" fn` pointers or
/// plain data. Append fields to grow the table: users built against the
/// shorter table keep working. Change a published field only with a new
/// major. Types count as written, so keep one declaration both mods compile.
///
/// # Examples
///
/// ```
/// #[enshwick_loader_api::service(name = "trading-post.prices", version = 1)]
/// pub struct Prices {
///     pub price: extern "system" fn(item: u32) -> u32,
/// }
///
/// extern "system" fn price(_item: u32) -> u32 {
///     3
/// }
///
/// static PRICES: Prices = Prices { price };
///
/// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
///     load.publish(&PRICES)
/// }
/// ```
pub use enshwick_loader_macros::service;
pub use hook::{FnPtr, Hook, Next};
pub use host::{Host, host};
pub use load::{Load, Mod, Off};
pub use mods::mods;

#[doc(hidden)]
pub mod __private {
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    use crate::abi::{HostApi, Level, ManifestFlags, ModHandle, Status};
    use crate::{Host, Load, Mod};

    #[must_use]
    pub const fn flags(join: &[bool], reload: &[bool]) -> ManifestFlags {
        let join = match join {
            [true] => ManifestFlags::JOIN.0,
            _ => 0,
        };
        let reload = match reload {
            [true] => ManifestFlags::RELOAD.0,
            _ => 0,
        };
        ManifestFlags(join | reload)
    }

    #[must_use]
    pub const fn first_or_null<T>(given: &'static [T]) -> *const T {
        if given.is_empty() {
            core::ptr::null()
        } else {
            given.as_ptr()
        }
    }

    /// The author keys `signed_by:` names; the build fails on a key that
    /// is not a minisign public key, or on more than two.
    #[must_use]
    pub const fn signed_by(keys: &[&'static str]) -> [crate::abi::Str; 2] {
        let mut out = [crate::abi::Str::EMPTY; 2];
        assert!(keys.len() <= out.len(), "signed_by names at most two keys");
        let mut i = 0;
        while i < keys.len() {
            assert!(
                crate::mods::is_author_key(keys[i]),
                "a signed_by key is a minisign public key, RW... (56 characters)"
            );
            out[i] = crate::abi::Str::new(keys[i]);
            i += 1;
        }
        out
    }

    /// The first non-empty text of `texts`, or empty.
    #[must_use]
    pub const fn first_text(texts: &[&'static str]) -> &'static str {
        let mut i = 0;
        while i < texts.len() {
            if !texts[i].is_empty() {
                return texts[i];
            }
            i += 1;
        }
        ""
    }

    /// The authors `authors:` names, else Cargo's `authors`, separated by
    /// `:`, trimmed and without empty names.
    const fn author_names<const N: usize>(
        given: &[&'static str],
        cargo: &'static str,
    ) -> ([crate::abi::Str; N], usize) {
        let mut out = [crate::abi::Str::EMPTY; N];
        let mut count = 0;
        if !given.is_empty() {
            while count < given.len() {
                assert!(!given[count].is_empty(), "an author's name is empty");
                if count < N {
                    out[count] = crate::abi::Str::new(given[count]);
                }
                count += 1;
            }
            return (out, count);
        }
        let mut rest = cargo.as_bytes();
        while !rest.is_empty() {
            let mut cut = 0;
            while cut < rest.len() && rest[cut] != b':' {
                cut += 1;
            }
            let (name, tail) = rest.split_at(cut);
            // Cut at an ASCII byte and trimmed of ASCII, so still UTF-8.
            if let Ok(name) = core::str::from_utf8(name.trim_ascii())
                && !name.is_empty()
            {
                if count < N {
                    out[count] = crate::abi::Str::new(name);
                }
                count += 1;
            }
            rest = match tail.split_first() {
                Some((_, after)) => after,
                None => tail,
            };
        }
        (out, count)
    }

    /// How many authors [`authors`] finds.
    #[must_use]
    pub const fn author_count(given: &[&'static str], cargo: &'static str) -> usize {
        author_names::<0>(given, cargo).1
    }

    /// The authors' names for the manifest.
    #[must_use]
    pub const fn authors<const N: usize>(
        given: &[&'static str],
        cargo: &'static str,
    ) -> [crate::abi::Str; N] {
        author_names::<N>(given, cargo).0
    }

    #[must_use]
    pub const fn or_default<T: Copy>(given: &[T], default: T) -> T {
        match given {
            [value] => *value,
            _ => default,
        }
    }

    pub fn fault_in(hook: &str) {
        if let Some(host) = crate::host() {
            host.fault(&format!("panic in hook {hook}"));
        }
    }

    /// The most detailed level the loader keeps, read once at init.
    static MAX_LEVEL: AtomicU8 = AtomicU8::new(Level::INFO.0);

    #[inline]
    #[must_use]
    pub fn enabled(level: Level) -> bool {
        level.0 <= MAX_LEVEL.load(Ordering::Relaxed)
    }

    unsafe extern "system" fn refresh_level(
        _event: crate::abi::Event,
        _data: *const core::ffi::c_void,
        _user: *mut core::ffi::c_void,
    ) {
        if let Some(host) = crate::host() {
            MAX_LEVEL.store(host.log_level().0, Ordering::Relaxed);
        }
    }

    /// A call site that logs its first time only.
    #[derive(Debug)]
    pub struct Once(AtomicBool);

    impl Once {
        #[must_use]
        pub const fn new() -> Self {
            Self(AtomicBool::new(false))
        }

        #[inline]
        #[must_use]
        pub fn first(&self) -> bool {
            !self.0.load(Ordering::Relaxed) && !self.0.swap(true, Ordering::Relaxed)
        }
    }

    impl Default for Once {
        fn default() -> Self {
            Self::new()
        }
    }

    /// A call site that logs at most once per period, counting what it skips.
    #[derive(Debug)]
    pub struct Every {
        next: AtomicU64,
        skipped: AtomicU64,
    }

    impl Every {
        #[must_use]
        pub const fn new() -> Self {
            Self {
                next: AtomicU64::new(0),
                skipped: AtomicU64::new(0),
            }
        }

        /// The lines skipped since the last one when a line is due now.
        #[must_use]
        pub fn due(&self, period: Duration) -> Option<u64> {
            static EPOCH: OnceLock<Instant> = OnceLock::new();
            let now = u64::try_from(EPOCH.get_or_init(Instant::now).elapsed().as_millis())
                .unwrap_or(u64::MAX);
            let next = self.next.load(Ordering::Relaxed);
            let period = u64::try_from(period.as_millis()).unwrap_or(u64::MAX);
            if now < next
                || self
                    .next
                    .compare_exchange(
                        next,
                        now.saturating_add(period),
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    )
                    .is_err()
            {
                self.skipped.fetch_add(1, Ordering::Relaxed);
                return None;
            }
            Some(self.skipped.swap(0, Ordering::Relaxed))
        }
    }

    impl Default for Every {
        fn default() -> Self {
            Self::new()
        }
    }

    pub fn log(level: Level, message: core::fmt::Arguments<'_>) {
        if !enabled(level) {
            return;
        }
        if let Some(host) = crate::host() {
            match message.as_str() {
                Some(text) => host.log(level, text),
                None => host.log(level, &message.to_string()),
            }
        }
    }

    /// # Safety
    ///
    /// `api` is the loader's process-wide table.
    pub unsafe fn init<M: Mod>(api: *const HostApi, me: ModHandle, slot: &OnceLock<M>) -> Status {
        // SAFETY: forwarded from the caller.
        let Some(host) = (unsafe { Host::new(api, me) }) else {
            return Status::BAD_ARGS;
        };
        let host = host.install();
        MAX_LEVEL.store(host.log_level().0, Ordering::Relaxed);
        let _watched = host.subscribe(
            crate::abi::Event::LOADER_SETTINGS_CHANGED,
            refresh_level,
            core::ptr::null_mut(),
        );
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            M::init(&mut Load::new(host))
        }));
        match outcome {
            Ok(Ok(state)) => {
                if slot.set(state).is_err() {
                    return Status::REFUSED;
                }
                Status::OK
            }
            Ok(Err(off)) => {
                host.log(Level::WARN, &format!("off: {off}"));
                Status::REFUSED
            }
            Err(_) => {
                host.log(Level::ERROR, "off: init panicked");
                Status::FAULTED
            }
        }
    }

    /// Runs `init` with a [`Load`] for the content pack `me`, as the loader
    /// does for a pack folder; the process's host stays as it is.
    ///
    /// # Safety
    ///
    /// `api` is the loader's process-wide table.
    pub unsafe fn init_pack(
        api: *const HostApi,
        me: ModHandle,
        init: impl FnOnce(&mut Load<'_>) -> Result<(), crate::Off>,
    ) -> Status {
        // SAFETY: forwarded from the caller.
        let Some(host) = (unsafe { Host::new(api, me) }) else {
            return Status::BAD_ARGS;
        };
        let outcome =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| init(&mut Load::new(host))));
        match outcome {
            Ok(Ok(())) => Status::OK,
            Ok(Err(off)) => {
                host.log(Level::WARN, &format!("off: {off}"));
                Status::REFUSED
            }
            Err(_) => {
                host.log(Level::ERROR, "off: init panicked");
                Status::FAULTED
            }
        }
    }

    /// Makes the table `api` with the handle `me` this process's host, for
    /// the loader's own copy of the API, which runs content packs.
    ///
    /// # Safety
    ///
    /// `api` is the loader's process-wide table.
    pub unsafe fn install_host(api: *const HostApi, me: ModHandle) {
        // SAFETY: forwarded from the caller.
        if let Some(host) = unsafe { Host::new(api, me) } {
            MAX_LEVEL.store(host.install().log_level().0, Ordering::Relaxed);
        }
    }

    pub fn shutdown<M: Mod>(slot: &'static OnceLock<M>) {
        let Some(state) = slot.get() else {
            return;
        };
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| state.shutdown())).is_err()
            && let Some(host) = crate::host()
        {
            host.fault("panic in shutdown");
        }
    }

    pub fn ready<M: Mod>(slot: &'static OnceLock<M>) {
        let (Some(state), Some(host)) = (slot.get(), crate::host()) else {
            return;
        };
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| state.ready(host))).is_err() {
            host.fault("panic in ready");
        }
    }
}
