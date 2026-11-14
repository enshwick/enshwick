//! The base handle a mod's `Load` holds while the loader initialises it,
//! and the helpers every domain registers through.

use core::ffi::c_void;

use crate::abi::{HostApi, Status, Str};
use crate::host::Host;
use crate::services::{ModEvent, Service};

#[doc(inline)]
pub use enshwick_values::Off;

/// Why `what` is off, from the `status` the loader answered with.
#[must_use]
pub fn off_status(what: &str, status: Status) -> Off {
    let why = match status {
        Status::MISSING => "missing on this game build",
        Status::AMBIGUOUS => "ambiguous on this game build",
        Status::REFUSED => "refused by the loader",
        Status::BAD_ARGS => "malformed",
        Status::TOO_LATE => "asked for too late",
        Status::FAULTED => "unavailable: the mod has faulted",
        Status::WRONG_THREAD => "called off the thread it serves",
        Status::NOT_LOADED => "not loaded yet",
        Status::OUTSIDE_WORLD => "outside the world",
        Status::STATIC_HALF => "template data, never written",
        Status::NOT_A_PROP => "not a prop",
        Status::NOT_KEPT => "not found again after a load",
        Status::UNAVAILABLE => "not available here or on this game build",
        Status::BUILDING => "being built further up this call",
        _ => "failed",
    };
    Off::new(format!("{what}: {why}"))
}

/// `From<E> for Off` with `E`'s message as the reason, called beside each
/// domain's error type.
#[doc(hidden)]
#[macro_export]
macro_rules! __off_from {
    ($($error:ty),* $(,)?) => {$(
        impl From<$error> for $crate::Off {
            fn from(error: $error) -> Self {
                Self::new(error.to_string())
            }
        }
    )*};
}
pub(crate) use __off_from as off_from;

/// The mod's loader during `init` as the API's functions outside `Load`
/// take it: pass the mod's `Load`.
///
/// Sealed: only `Load`, the base handle it holds and a `&mut` to either
/// implement it, so `&load` passes inside `init(load: &mut Load)`.
///
/// # Examples
///
/// ```
/// use enshwick::ecs::ComponentType;
///
/// fn used_item(load: &mut enshwick::Load<'_>) -> Result<u32, enshwick::Off> {
///     Ok(ComponentType::resolve(load, "keen::ecs::UsedItem")?.hash())
/// }
/// ```
pub trait Loading: sealed::Sealed {
    /// The base handle.
    #[doc(hidden)]
    fn base(&self) -> &BaseLoad;

    /// The base handle, to register through.
    #[doc(hidden)]
    fn base_mut(&mut self) -> &mut BaseLoad;
}

/// Keeps [`Loading`] to `Load` and [`BaseLoad`].
#[doc(hidden)]
pub mod sealed {
    /// Implemented by `Load`, [`BaseLoad`](super::BaseLoad) and a `&mut` to
    /// either only.
    pub trait Sealed {}

    impl<L: super::Loading + ?Sized> Sealed for &mut L {}
}

impl<L: Loading + ?Sized> Loading for &mut L {
    fn base(&self) -> &BaseLoad {
        (**self).base()
    }

    fn base_mut(&mut self) -> &mut BaseLoad {
        (**self).base_mut()
    }
}

/// What every domain shares of a mod's `Load`: its host, its join flag, its
/// embedded texts and the state each domain keeps while the mod
/// initialises. `Load` holds it and forwards to it.
#[doc(hidden)]
#[derive(Debug)]
pub struct BaseLoad {
    host: Host,
    /// Whether the mod runs on both sides, so it takes part in the join
    /// check, as its manifest says.
    joins: bool,
    /// The mod's `src/texts.toml`, as `declare_mod!` embedded it.
    embedded_texts: Option<&'static str>,
    /// What each domain keeps while the mod initialises.
    slots: slots::Slots,
}

impl sealed::Sealed for BaseLoad {}

impl Loading for BaseLoad {
    fn base(&self) -> &BaseLoad {
        self
    }

    fn base_mut(&mut self) -> &mut BaseLoad {
        self
    }
}

impl BaseLoad {
    /// A handle for a mod outside the join check, without embedded texts.
    #[must_use]
    pub fn new(host: Host) -> Self {
        Self::of(host, false, None)
    }

    /// A handle for a mod that takes part in the join check when `joins`,
    /// with the texts `declare_mod!` embedded.
    #[must_use]
    pub fn of(host: Host, joins: bool, embedded_texts: Option<&'static str>) -> Self {
        Self {
            host,
            joins,
            embedded_texts,
            slots: slots::Slots::default(),
        }
    }

    /// The mod's `src/texts.toml`, as `declare_mod!` embedded it.
    #[must_use]
    pub fn embedded_texts(&self) -> Option<&'static str> {
        self.embedded_texts
    }

    /// The state of type `T` a domain keeps while the mod initialises, empty
    /// until the domain fills it.
    pub fn slot<T: Slot>(&mut self) -> &mut Option<T> {
        self.slots.of()
    }

    /// Whether the mod runs on both sides, so it takes part in the join
    /// check, as its manifest says.
    #[must_use]
    pub fn joins(&self) -> bool {
        self.joins
    }

    /// The loader's table.
    #[must_use]
    pub fn api(&self) -> &'static HostApi {
        self.host.api()
    }
}

impl BaseLoad {
    /// As `Load::host`.
    #[doc(hidden)]
    #[must_use]
    pub fn host(&self) -> Host {
        self.host
    }

    /// As `Load::scheduler`.
    #[doc(hidden)]
    pub fn scheduler(&mut self) -> Result<crate::schedule::Scheduler, Off> {
        let (scheduler, subscribe) = crate::schedule::Scheduler::get(self.host.handle());
        if subscribe
            && let Err(off) =
                self.on::<crate::events::Ticked>(move |tick| scheduler.run_due(tick.game))
        {
            scheduler.unsubscribed();
            return Err(off);
        }
        Ok(scheduler)
    }

    /// As `Load::data_dir`.
    #[doc(hidden)]
    #[must_use]
    pub fn data_dir(&self) -> Option<std::path::PathBuf> {
        crate::state::data_dir(self.host)
    }

    /// As `Load::mod_dir`.
    #[doc(hidden)]
    pub fn mod_dir(&self) -> Result<std::path::PathBuf, Off> {
        let mut out = Str::EMPTY;
        let status = (self.api().mod_dir)(self.host.handle(), &raw mut out);
        ok(status, (), || "mod folder".to_owned())?;
        // SAFETY: the loader keeps the path alive for the process.
        let path = unsafe { out.as_str() }.ok_or_else(|| Off::new("mod folder: not UTF-8"))?;
        Ok(std::path::PathBuf::from(path))
    }

    /// As `Load::export_dir`.
    #[doc(hidden)]
    pub fn export_dir(&self) -> Result<std::path::PathBuf, Off> {
        let dir = self.mod_dir()?;
        let game = dir
            .parent()
            .filter(|mods| mods.file_name().is_some_and(|name| name == "mods"))
            .and_then(std::path::Path::parent);
        let (Some(game), Some(folder)) = (game, dir.file_name().and_then(|name| name.to_str()))
        else {
            return Err(Off::new(format!(
                "export folder: {} is no folder in mods/",
                dir.display()
            )));
        };
        let id = self
            .host
            .listing(self.api().mods, false)
            .into_iter()
            .find(|entry| crate::mods::folder(&entry.file) == Some(folder))
            .map(|entry| entry.id)
            .ok_or_else(|| {
                Off::new(format!(
                    "export folder: the loader lists no mod in mods/{folder}"
                ))
            })?;
        Ok(game.join("export").join(id))
    }

    /// As `Load::world_store`.
    #[doc(hidden)]
    pub fn world_store<T>(&mut self, name: &str) -> Result<crate::state::WorldStore<T>, Off>
    where
        T: serde::de::DeserializeOwned + serde::Serialize + Default + Send + 'static,
    {
        let store = crate::state::WorldStore::new(name)?;
        let host = self.host;
        let opened = store.clone();
        let feature = format!("world store {name}");
        let world = move || {
            Some((
                crate::state::world_key(host)?,
                crate::state::data_dir(host)?,
            ))
        };
        self.on::<crate::events::WorldLoaded>(move |()| match world() {
            Some((key, dir)) => {
                let shut = opened.open(&key, &dir).err().map(|error| error.to_string());
                if let Some(error) = &shut {
                    crate::error!("world store kept shut: {error}");
                }
                let reason = shut.as_deref().unwrap_or_default();
                let _shown =
                    (host.api().feature)(host.handle(), Str::new(&feature), Str::new(reason));
            }
            None => crate::warn!("world store kept shut: the loader names no world"),
        })?;
        let saved = store.clone();
        self.on::<crate::events::WorldSaved>(move |()| saved.flush_in_background())?;
        let shown = store.clone();
        let named = name.to_owned();
        self.add_status(move || {
            shown
                .shut_reason()
                .map(|why| format!("world store {named} kept shut: {why}"))
                .unwrap_or_default()
        })?;
        let closed = store.clone();
        self.on::<crate::events::ShutdownStarted>(move |()| {
            if let Err(error) = closed.close() {
                crate::error!("{error}");
            }
        })?;
        Ok(store)
    }
}

/// The ABI description of the service `T`, with `table` or null.
pub(crate) fn service_spec<T: Service>(table: *const c_void) -> crate::abi::ServiceSpec {
    crate::abi::ServiceSpec {
        header: crate::abi::Header::of::<crate::abi::ServiceSpec>(),
        name: crate::services::name(T::NAME),
        major: T::MAJOR,
        layout: crate::services::layout(T::LAYOUT, size_of::<T>()),
        table,
    }
}

/// The ABI description of the mod event `E`.
pub(crate) fn event_spec<E: ModEvent>() -> crate::abi::EventSpec {
    crate::abi::EventSpec {
        header: crate::abi::Header::of::<crate::abi::EventSpec>(),
        name: crate::services::name(E::NAME),
        layout: crate::services::layout(E::LAYOUT, size_of::<E>()),
    }
}

std::thread_local! {
    /// The reason a mod's check refused a settings text, kept on the thread
    /// that asked until its next check there: the game thread and the UI
    /// thread may check at once, and each reads its own after the call.
    pub static CHECK_WHY: core::cell::RefCell<String> = const { core::cell::RefCell::new(String::new()) };
}

/// A mid-function hook's handler and where it runs, for the fault message.
struct MidHook<F> {
    address: u64,
    handler: F,
}

/// `value` when `status` is OK, otherwise an [`Off`] naming `what` and why.
///
/// # Errors
///
/// That [`Off`].
pub fn ok<T>(status: Status, value: T, what: impl FnOnce() -> String) -> Result<T, Off> {
    if status.is_ok() {
        Ok(value)
    } else {
        Err(off_status(&what(), status))
    }
}

/// A domain's error for a status the loader answered: the statuses the
/// domain tells apart, and its error for every other.
#[doc(hidden)]
pub trait StatusError: Sized {
    /// The error for `status`, any but [`Status::OK`]: one the domain tells
    /// apart, or its error for the rest, a newer loader's included.
    fn of(status: Status) -> Self;
}

/// The outcome `status` stands for in a domain whose error is `E`: `Ok` for
/// [`Status::OK`], `E`'s error for a status it knows, and for any other,
/// one this mod's build may not know, `E`'s failure.
///
/// # Errors
///
/// `E`'s error for any status but [`Status::OK`].
#[doc(hidden)]
pub fn outcome<E: StatusError>(status: Status) -> Result<(), E> {
    if status.is_ok() {
        return Ok(());
    }
    Err(E::of(status))
}

/// Hands `value` to the loader through `register` as its user pointer, which
/// the loader keeps for the life of the process once it accepts it.
///
/// # Errors
///
/// [`Off`] naming `what` when the loader refuses it.
pub fn hand_over<T>(
    value: T,
    register: impl FnOnce(*mut c_void) -> Status,
    what: &str,
) -> Result<(), Off> {
    let user = Box::into_raw(Box::new(value));
    let status = register(user.cast());
    if !status.is_ok() {
        // SAFETY: refused, so the loader keeps no copy of the pointer.
        drop(unsafe { Box::from_raw(user) });
    }
    ok(status, (), || what.to_owned())
}

mod build;
mod code;
mod events;
mod services;
mod settings;
mod slots;
#[cfg(test)]
mod tests;

#[cfg(feature = "unstable")]
pub use events::client_game_events_of;
pub use events::game_events_of;
pub use slots::Slot;
