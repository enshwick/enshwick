//! What a mod does while the loader initialises it.

use core::ffi::c_void;
use std::borrow::Cow;

use settings::check_text;

use crate::abi::{HostApi, Status, Str};
use crate::config::{Config, ConfigError, Stage};
use crate::host::Host;
use crate::locks::Relock;
use crate::services::{ModEvent, Service};

/// Why a mod turns itself off during initialisation; the loader logs the
/// reason and keeps the game vanilla where the mod would have acted.
///
/// Every error type of this crate turns into an `Off` with `?`, its message
/// the reason.
///
/// # Examples
///
/// ```
/// let off = enshwick::Off::new("this game build is not supported");
/// assert_eq!(off.to_string(), "this game build is not supported");
///
/// fn calls(load: &enshwick::Load<'_>) -> Result<usize, enshwick::Off> {
///     let pattern = enshwick::code::Pattern::parse("E8 ?? ?? ?? ??")?;
///     Ok(pattern.find_all(load.code()?).count())
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{reason}")]
pub struct Off {
    reason: Cow<'static, str>,
}

impl Off {
    /// An `Off` with a reason for the log.
    ///
    /// # Examples
    ///
    /// See [`Off`].
    #[must_use]
    pub fn new(reason: impl Into<Cow<'static, str>>) -> Self {
        Self {
            reason: reason.into(),
        }
    }

    pub(crate) fn status(what: &str, status: Status) -> Self {
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
            Status::NOT_A_PROP => "not a prop",
            _ => "failed",
        };
        Self::new(format!("{what}: {why}"))
    }
}

/// `From<E> for Off` with `E`'s message as the reason.
macro_rules! off_from {
    ($($error:ty),* $(,)?) => {$(
        impl From<$error> for Off {
            fn from(error: $error) -> Self {
                Self::new(error.to_string())
            }
        }
    )*};
}

off_from!(
    crate::affixes::RollError,
    crate::altars::ResizeError,
    crate::code::FindError,
    crate::code::PatternError,
    crate::console::RunError,
    ConfigError,
    crate::content::vanilla::VanillaError,
    crate::inventory::BatchError,
    crate::item_data::SetError,
    crate::migrations::MigrationError,
    crate::net::SendError,
    crate::packs::PackError,
    crate::packs::ReadPackError,
    crate::rates::RateError,
    crate::services::LayoutError,
    crate::stats::ModifyError,
    crate::ui::UiError,
    crate::world::WorldError,
);

/// A mod: what the loader initialises when it loads the DLL.
///
/// Implement it on the mod's state type and export it with
/// [`declare_mod!`](crate::declare_mod).
///
/// # Examples
///
/// ```
/// struct Quiet;
///
/// impl enshwick::Mod for Quiet {
///     fn manifest() -> &'static enshwick::abi::Manifest {
///         enshwick::manifest!(id: "you.quiet", side: SERVER)
///     }
///
///     fn init(_load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
///         Ok(Self)
///     }
/// }
/// ```
pub trait Mod: Sized + Send + Sync + 'static {
    /// What the mod is; static data built with [`manifest!`](crate::manifest).
    fn manifest() -> &'static crate::abi::Manifest;

    /// Registers hooks, services and settings. Nothing takes effect until every
    /// mod is initialised and the loader arms the hooks.
    ///
    /// # Errors
    ///
    /// [`Off`] when the mod cannot run on this game or with these settings.
    fn init(load: &mut Load<'_>) -> Result<Self, Off>;

    /// Runs once the hooks are armed; start background work here.
    fn ready(&'static self, _host: Host) {}

    /// Runs before a development reload unloads the mod (only for a mod
    /// with `reload: true` in its manifest): stop every thread and timer
    /// the mod started and write what the next copy reads back. The loader
    /// has unlinked the mod's hooks and events already.
    fn shutdown(&'static self) {}
}

/// The loader during a mod's initialisation: lookups, hooks, settings and
/// services.
///
/// # Examples
///
/// See [`Mod`].
#[derive(Debug)]
pub struct Load<'a> {
    host: Host,
    /// Whether the mod takes part in the join check, as its manifest says.
    joins: bool,
    _init: core::marker::PhantomData<&'a ()>,
}

impl Load<'_> {
    pub(crate) fn new(host: Host) -> Self {
        Self::of(host, false)
    }

    /// A `Load` for a mod that takes part in the join check when `joins`.
    pub(crate) fn of(host: Host, joins: bool) -> Self {
        Self {
            host,
            joins,
            _init: core::marker::PhantomData,
        }
    }

    /// Whether the mod takes part in the join check, as its manifest says.
    pub(crate) fn joins(&self) -> bool {
        self.joins
    }

    /// The loader's services, also available later through
    /// [`host()`](crate::host()).
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) {
    ///     let side = load.host().build().side;
    ///     let _ = side;
    /// }
    /// ```
    #[must_use]
    pub fn host(&self) -> Host {
        self.host
    }

    fn api(&self) -> &'static HostApi {
        self.host.api()
    }

    /// The server's [`World`](crate::world::World): reads, queries and
    /// requests; on a client, the built-in server's while it hosts a game.
    ///
    /// # Errors
    ///
    /// [`Off`] when the game build lacks a type the reads need.
    ///
    /// # Examples
    ///
    /// See [`World`](crate::world::World).
    pub fn world(&mut self) -> Result<crate::world::World, Off> {
        crate::world::World::resolve(self)
    }

    /// Inventory reads and transactions in the game server, dedicated or
    /// hosted by the client; a mod asks for them during `init`.
    ///
    /// # Errors
    ///
    /// [`Off`] when this game build lacks a piece of them.
    ///
    /// # Examples
    ///
    /// See [`Inventories`](crate::inventory::Inventories).
    pub fn inventories(&mut self) -> Result<crate::inventory::Inventories, Off> {
        crate::inventory::Inventories::resolve(self)
    }

    /// The mod's [`Scheduler`](crate::schedule::Scheduler), which runs work on
    /// the server's game thread after each simulation step: the dedicated
    /// server's, or the built-in server's while a client hosts a game.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader raises no ticks.
    ///
    /// # Examples
    ///
    /// See [`Scheduler`](crate::schedule::Scheduler).
    pub fn scheduler(&mut self) -> Result<crate::schedule::Scheduler, Off> {
        let (scheduler, subscribe) = crate::schedule::Scheduler::get(self.host.handle());
        if subscribe
            && let Err(off) = self.on::<crate::events::Ticked>(move |_| scheduler.run_due())
        {
            scheduler.unsubscribed();
            return Err(off);
        }
        Ok(scheduler)
    }

    /// The mod's own data folder, `mod-data/<id>` beside the game, for state
    /// that outlives a session.
    ///
    /// # Errors
    ///
    /// [`Off`] when the folder cannot be created.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let dir = load.data_dir()?;
    ///     assert!(dir.ends_with("my-mod"));
    ///     Ok(())
    /// }
    /// ```
    pub fn data_dir(&self) -> Result<std::path::PathBuf, Off> {
        let mut out = Str::EMPTY;
        let status = (self.api().data_dir)(self.host.handle(), &raw mut out);
        ok(status, (), || "data folder".to_owned())?;
        // SAFETY: the loader keeps the path alive for the process.
        let path = unsafe { out.as_str() }.ok_or_else(|| Off::new("data folder: not UTF-8"))?;
        Ok(std::path::PathBuf::from(path))
    }

    /// A value of type `T` kept per world under the mod's data folder,
    /// opened when the world loads and written when it saves; see
    /// [`WorldStore`](crate::state::WorldStore).
    ///
    /// # Errors
    ///
    /// [`Off`] when `name` is not 1 to 64 of `[a-z0-9_-]`, the data folder
    /// cannot be created or the world events are refused.
    ///
    /// # Examples
    ///
    /// See [`WorldStore`](crate::state::WorldStore).
    pub fn world_store<T>(&mut self, name: &str) -> Result<crate::state::WorldStore<T>, Off>
    where
        T: serde::de::DeserializeOwned + serde::Serialize + Default + Send + 'static,
    {
        let store = crate::state::WorldStore::new(&self.data_dir()?, name)?;
        let host = self.host;
        let opened = store.clone();
        self.on::<crate::events::WorldLoaded>(move |()| match crate::world::world_key(host) {
            Some(key) => {
                if let Err(error) = opened.open(&key) {
                    crate::error!("world store kept shut: {error}");
                }
            }
            None => crate::warn!("world store kept shut: the loader names no world"),
        })?;
        let saved = store.clone();
        self.on::<crate::events::WorldSaved>(move |()| saved.flush_in_background())?;
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
fn service_spec<T: Service>(table: *const c_void) -> crate::abi::ServiceSpec {
    crate::abi::ServiceSpec {
        header: crate::abi::Header::of::<crate::abi::ServiceSpec>(),
        name: crate::services::name(T::NAME),
        major: T::MAJOR,
        layout: crate::services::layout(T::LAYOUT, size_of::<T>()),
        table,
    }
}

/// The ABI description of the mod event `E`.
fn event_spec<E: ModEvent>() -> crate::abi::EventSpec {
    crate::abi::EventSpec {
        header: crate::abi::Header::of::<crate::abi::EventSpec>(),
        name: crate::services::name(E::NAME),
        layout: crate::services::layout(E::LAYOUT, size_of::<E>()),
    }
}

/// The `startup` values in force, as an inline table: read in
/// [`Load::config`] and again as each world loads; a change between waits
/// for the next load.
static STARTED: Kept = Kept::new();

/// The `restart` values the mod read in [`Load::config`], as an inline
/// table; later reads keep them until the game starts again.
static RESTART: Kept = Kept::new();

/// Settings values kept for one mod life.
type Kept = crate::fresh::Fresh<std::sync::Mutex<Option<String>>>;

/// Notes `settings`' values at `stage` as the ones in force.
fn keep<T: Config>(kept: &Kept, settings: &T, stage: Stage) {
    *kept.get_or_init(std::sync::Mutex::default).relock() =
        Some(settings.values(stage).unwrap_or_default());
}

std::thread_local! {
    /// The reason a mod's check refused a settings text, kept on the thread
    /// that asked until its next check there: the game thread and the UI
    /// thread may check at once, and each reads its own after the call.
    static CHECK_WHY: core::cell::RefCell<String> = const { core::cell::RefCell::new(String::new()) };
}

impl Load<'_> {
    /// Hands the loader the mod's check of new settings texts, when it
    /// takes one.
    fn config_check<T: Config>(&self) {
        let api = self.api();
        let _checked =
            (api.config_check)(self.host.handle(), check_text::<T>, core::ptr::null_mut());
    }
}

/// Puts the join values `server` over `settings` and checks them; an error
/// leaves `settings` as they were.
fn with_server<T: Config>(settings: &mut T, server: &str) -> Result<(), ConfigError> {
    let own = settings.synced().unwrap_or_default();
    settings.merge_synced(server)?;
    settings.validate().inspect_err(|_| {
        let _back = settings.merge_synced(&own);
    })
}

/// A mid-function hook's handler and where it runs, for the fault message.
struct MidHook<F> {
    address: u64,
    handler: F,
}

/// `value` when `status` is OK, otherwise an [`Off`] naming `what` and why.
pub(crate) fn ok<T>(status: Status, value: T, what: impl FnOnce() -> String) -> Result<T, Off> {
    if status.is_ok() {
        Ok(value)
    } else {
        Err(Off::status(&what(), status))
    }
}

/// Hands `value` to the loader through `register` as its user pointer, which
/// the loader keeps for the life of the process once it accepts it.
pub(crate) fn hand_over<T>(
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
#[cfg(test)]
mod tests;
