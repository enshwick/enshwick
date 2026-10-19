//! The Enshwick modding API for Enshrouded: what a mod is written against.
//!
//! A mod is a DLL in the game's `mods/` folder. It implements [`Mod`],
//! exports itself with [`declare_mod!`], and during [`Mod::init`] links hooks
//! with [`detour!`] and [`Load::hook`], reads its settings and finds game
//! functions by name. The loader arms every hook once all mods are
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
//! enshwick::detour! {
//!     static UPGRADE: unsafe extern "system" fn(ctx: *mut u64) = upgrade;
//! }
//!
//! fn upgrade(next: enshwick::Next<unsafe extern "system" fn(*mut u64)>, ctx: *mut u64) {
//!     // SAFETY: the original takes the same context.
//!     unsafe { next.get()(ctx) }
//! }
//!
//! impl enshwick::Mod for Hello {
//!     fn manifest() -> &'static enshwick::abi::Manifest {
//!         enshwick::manifest!(id: "hello", side: SERVER)
//!     }
//!
//!     fn init(load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
//!         let settings: Settings = load.settings("enabled = true\n")?;
//!         if settings.enabled {
//!             let target = load.system("upgrade_building_range")?;
//!             load.hook(&UPGRADE, target)?;
//!         }
//!         Ok(Self { settings })
//!     }
//! }
//!
//! enshwick::declare_mod!(Hello);
//! ```

pub mod abi;
mod declare;
mod hook;
mod host;
mod load;

pub use hook::{FnPtr, Hook, Next};
pub use host::{Host, host};
pub use load::{Load, Mod, Off};

#[doc(hidden)]
pub mod __private {
    use std::sync::OnceLock;

    use crate::abi::{HostApi, Level, ManifestFlags, ModHandle, Status};
    use crate::{Host, Load, Mod};

    #[must_use]
    pub const fn valid_id(id: &str) -> bool {
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

    #[must_use]
    pub const fn join_flag(join: &[bool]) -> ManifestFlags {
        match join {
            [true] => ManifestFlags::JOIN,
            _ => ManifestFlags::NONE,
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

    pub fn log(level: Level, message: core::fmt::Arguments<'_>) {
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
