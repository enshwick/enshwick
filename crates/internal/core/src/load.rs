//! What a mod does while the loader initialises it.

use core::ffi::c_void;

use crate::abi::{HostApi, Status, Str};
use crate::host::Host;
use crate::services::{ModEvent, Service};

#[doc(inline)]
pub use enshwick_values::Off;

/// Why `what` is off, from the `status` the loader answered with.
pub(crate) fn off_status(what: &str, status: Status) -> Off {
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
macro_rules! off_from {
    ($($error:ty),* $(,)?) => {$(
        impl From<$error> for $crate::Off {
            fn from(error: $error) -> Self {
                Self::new(error.to_string())
            }
        }
    )*};
}
pub(crate) use off_from;

/// A mod: what the loader initialises when it loads the DLL.
///
/// Implement it on the mod's state type and export it with
/// [`declare_mod!`](crate::declare_mod), which reads what the mod is from its
/// `Cargo.toml`.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_core as enshwick;
/// struct Quiet;
///
/// impl enshwick::Mod for Quiet {
///     fn init(_load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
///         Ok(Self)
///     }
/// }
/// ```
pub trait Mod: Sized + Send + Sync + 'static {
    /// Registers hooks, services and settings. Nothing takes effect until every
    /// mod is initialised and the loader arms the hooks.
    ///
    /// # Errors
    ///
    /// [`Off`] when the mod cannot run on this game or with these settings.
    fn init(load: &mut Load<'_>) -> Result<Self, Off>;

    /// Runs once the hooks are armed; start background work here.
    fn ready(&'static self, _host: Host) {}
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
    /// Whether the mod runs on both sides, so it takes part in the join
    /// check, as its manifest says.
    joins: bool,
    /// The mod's `src/texts.toml`, as `declare_mod!` embedded it.
    embedded_texts: Option<&'static str>,
    /// What each domain keeps while the mod initialises.
    #[cfg_attr(
        not(any(feature = "unstable", feature = "typed")),
        expect(dead_code, reason = "only unstable and typed domains keep state")
    )]
    slots: slots::Slots,
    _init: core::marker::PhantomData<&'a ()>,
}

#[crate::script(dirs)]
impl Load<'_> {
    pub(crate) fn new(host: Host) -> Self {
        Self::of(host, false, None)
    }

    /// A `Load` for a mod that takes part in the join check when `joins`,
    /// with the texts `declare_mod!` embedded.
    pub(crate) fn of(host: Host, joins: bool, embedded_texts: Option<&'static str>) -> Self {
        Self {
            host,
            joins,
            embedded_texts,
            slots: slots::Slots::default(),
            _init: core::marker::PhantomData,
        }
    }

    /// The mod's `src/texts.toml`, as `declare_mod!` embedded it.
    pub(crate) fn embedded_texts(&self) -> Option<&'static str> {
        self.embedded_texts
    }

    /// The state of type `T` a domain keeps while the mod initialises, empty
    /// until the domain fills it.
    #[cfg_attr(
        not(any(feature = "unstable", feature = "typed")),
        expect(dead_code, reason = "only unstable and typed domains keep state")
    )]
    pub(crate) fn slot<T: slots::Slot>(&mut self) -> &mut Option<T> {
        self.slots.of()
    }

    /// Whether the mod runs on both sides, so it takes part in the join
    /// check, as its manifest says.
    pub(crate) fn joins(&self) -> bool {
        self.joins
    }

    /// The loader's services, also available later through
    /// [`host()`](crate::host()).
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
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

    /// The mod's [`Scheduler`](crate::schedule::Scheduler), which runs work on
    /// the server's game thread on a tick of the default period: the dedicated
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
            && let Err(off) =
                self.on::<crate::events::Ticked>(move |tick| scheduler.run_due(tick.game))
        {
            scheduler.unsubscribed();
            return Err(off);
        }
        Ok(scheduler)
    }

    /// The mod's data folder in the world that runs, `<stem>-mod-data/<id>`
    /// beside the world's save; `None` while no world runs, as during init
    /// before a hosted game or a dedicated server's world loads. A world's
    /// state belongs there; [`WorldStore`](crate::state::WorldStore) keeps
    /// it for you, and `World::data_dir`
    /// answers the same once the world runs.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     if let Some(dir) = load.data_dir() {
    ///         enshwick::info!("this world's data: {}", dir.display());
    ///     }
    ///     Ok(())
    /// }
    /// ```
    #[must_use]
    #[script]
    pub fn data_dir(&self) -> Option<std::path::PathBuf> {
        crate::state::data_dir(self.host)
    }

    /// The mod's folder in `mods/`, which holds its installation, such as an
    /// installation [`Hold`](crate::state::Hold).
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader names no folder or a path that is not UTF-8.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let hold = enshwick::state::Hold::new(&load.mod_dir()?);
    ///     let _ = hold.reason();
    ///     Ok(())
    /// }
    /// ```
    #[script]
    pub fn mod_dir(&self) -> Result<std::path::PathBuf, Off> {
        let mut out = Str::EMPTY;
        let status = (self.api().mod_dir)(self.host.handle(), &raw mut out);
        ok(status, (), || "mod folder".to_owned())?;
        // SAFETY: the loader keeps the path alive for the process.
        let path = unsafe { out.as_str() }.ok_or_else(|| Off::new("mod folder: not UTF-8"))?;
        Ok(std::path::PathBuf::from(path))
    }

    /// The folder `export/<id>/` in the game folder, named by the mod id
    /// whatever the mod's folder in `mods/` is called, where a mod writes
    /// files for tools outside the game, such as a wiki or a map; a mod may
    /// write to any other path too.
    ///
    /// # Errors
    ///
    /// As [`Load::mod_dir`], for a mod folder outside a `mods/` folder, and
    /// when the loader lists no mod in that folder.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let items = load.export_dir()?.join("items.json");
    ///     enshwick::files::write_atomically(&items, "[]").map_err(|e| enshwick::Off::new(e.to_string()))
    /// }
    /// ```
    #[script]
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

    /// A value of type `T` kept per world in the mod's data folder of that
    /// world, opened when the world loads and written when it saves; see
    /// [`WorldStore`](crate::state::WorldStore). A world file that does not
    /// read keeps the store shut, with the reason in the status summary and
    /// on the Mods screen, as the feature `world store <name>` off.
    ///
    /// # Errors
    ///
    /// [`Off`] when `name` is not 1 to 64 of `[a-z0-9_-]` or the world
    /// events are refused.
    ///
    /// # Examples
    ///
    /// See [`WorldStore`](crate::state::WorldStore).
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

std::thread_local! {
    /// The reason a mod's check refused a settings text, kept on the thread
    /// that asked until its next check there: the game thread and the UI
    /// thread may check at once, and each reads its own after the call.
    pub(crate) static CHECK_WHY: core::cell::RefCell<String> = const { core::cell::RefCell::new(String::new()) };
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
        Err(off_status(&what(), status))
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
#[cfg_attr(
    not(any(test, feature = "unstable", feature = "typed")),
    expect(dead_code, reason = "only unstable and typed domains keep state")
)]
mod slots;
#[cfg(test)]
mod tests;

#[cfg(feature = "typed")]
pub(crate) use events::game_events_of;
