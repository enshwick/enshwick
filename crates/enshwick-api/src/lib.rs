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
//! the safe layer over it.
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
//! enshwick_api::detour! {
//!     static UPGRADE: unsafe extern "system" fn(ctx: *mut u64) = upgrade;
//! }
//!
//! fn upgrade(next: enshwick_api::Next<unsafe extern "system" fn(*mut u64)>, ctx: *mut u64) {
//!     // SAFETY: the original takes the same context.
//!     unsafe { next.get()(ctx) }
//! }
//!
//! impl enshwick_api::Mod for Hello {
//!     fn manifest() -> &'static enshwick_api::abi::Manifest {
//!         enshwick_api::manifest!(id: "hello", side: SERVER)
//!     }
//!
//!     fn init(load: &mut enshwick_api::Load<'_>) -> Result<Self, enshwick_api::Off> {
//!         let settings: Settings = load.settings("enabled = true\n")?;
//!         if settings.enabled {
//!             let target = load.system("upgrade_building_range")?;
//!             load.hook(&UPGRADE, target)?;
//!         }
//!         Ok(Self { settings })
//!     }
//! }
//!
//! enshwick_api::declare_mod!(Hello);
//! ```

/// The version of this crate, which a mod's generated `Cargo.toml` pins.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// The derive names this crate by its path, as mods see it.
#[cfg(test)]
extern crate self as enshwick_api;

pub mod abi;
pub mod code;
pub mod commands;
pub mod config;
pub mod content;
mod declare;
pub mod ecs;
pub mod events;
mod hook;
mod host;
pub mod i18n;
pub mod inventory;
mod load;
pub mod mods;
pub mod schedule;
pub mod state;
pub mod world;

pub use abi::Registers;
pub use enshwick_macros::Config;
/// Declares a hook from its body, as [`detour!`] does: the function stays,
/// and a static [`Hook`] named after it in upper case joins it.
///
/// The first argument is `next: Next<F>`, where `F` is the target's
/// `extern "system" fn` type; the other arguments are the target's, by plain
/// names. `#[hook(priority = <i32>, flags = <HookFlags>)]` sets the link's
/// order and flags. The static takes the function's visibility and doc
/// comment.
///
/// # Examples
///
/// ```
/// use enshwick_api::abi::HookFlags;
/// use enshwick_api::{Next, hook};
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
pub use enshwick_macros::hook;
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
    pub const fn join_flag(join: &[bool]) -> ManifestFlags {
        match join {
            [true] => ManifestFlags::JOIN,
            _ => ManifestFlags::NONE,
        }
    }

    #[must_use]
    pub const fn first_or_null<T>(given: &'static [T]) -> *const T {
        if given.is_empty() {
            core::ptr::null()
        } else {
            given.as_ptr()
        }
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
        // An older loader has no events; the level then stays as read.
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

    pub fn ready<M: Mod>(slot: &'static OnceLock<M>) {
        let (Some(state), Some(host)) = (slot.get(), crate::host()) else {
            return;
        };
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| state.ready(host))).is_err() {
            host.fault("panic in ready");
        }
    }
}
