//! What a mod does while the loader initialises it.

use core::ffi::c_void;
use std::borrow::Cow;

use crate::abi::{Event, HostApi, Status, Str};
use crate::config::Config;
use crate::hook::{FnPtr, Hook};
use crate::host::Host;

/// Why a mod turns itself off during initialisation; the loader logs the
/// reason and keeps the game vanilla where the mod would have acted.
///
/// # Examples
///
/// ```
/// let off = enshwick_api::Off::new("this game build is not supported");
/// assert_eq!(off.to_string(), "this game build is not supported");
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

    fn status(what: &str, status: Status) -> Self {
        let why = match status {
            Status::MISSING => "missing on this game build",
            Status::AMBIGUOUS => "ambiguous on this game build",
            Status::REFUSED => "refused by the loader",
            Status::BAD_ARGS => "malformed",
            Status::TOO_LATE => "asked for too late",
            Status::FAULTED => "unavailable: the mod has faulted",
            _ => "failed",
        };
        Self::new(format!("{what}: {why}"))
    }
}

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
/// impl enshwick_api::Mod for Quiet {
///     fn manifest() -> &'static enshwick_api::abi::Manifest {
///         enshwick_api::manifest!(id: "quiet", side: SERVER)
///     }
///
///     fn init(_load: &mut enshwick_api::Load<'_>) -> Result<Self, enshwick_api::Off> {
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
    _init: core::marker::PhantomData<&'a ()>,
}

impl Load<'_> {
    pub(crate) fn new(host: Host) -> Self {
        Self {
            host,
            _init: core::marker::PhantomData,
        }
    }

    /// The loader's services, also available later through
    /// [`host`](crate::host).
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick_api::Load<'_>) {
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

    /// The absolute address of a symbolic id, such as `npc.lookup`, on the
    /// running build.
    ///
    /// # Errors
    ///
    /// [`Off`] when neither the mod nor the loader pins the id for this build.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick_api::Load<'_>) -> Result<(), enshwick_api::Off> {
    ///     let lookup = load.symbol("npc.lookup")?;
    ///     assert_ne!(lookup, 0);
    ///     Ok(())
    /// }
    /// ```
    pub fn symbol(&self, name: &str) -> Result<u64, Off> {
        let mut out = 0;
        let status = (self.api().symbol)(self.host.handle(), Str::new(name), &raw mut out);
        ok(status, out, || format!("symbol {name}"))
    }

    /// An engine constant of the running build, such as a field offset that
    /// reflection does not describe.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader does not pin the constant for this build.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick_api::Load<'_>) -> Result<(), enshwick_api::Off> {
    ///     let row = load.constant("ecs.context_row")?;
    ///     assert_eq!(row, 8);
    ///     Ok(())
    /// }
    /// ```
    pub fn constant(&self, name: &str) -> Result<u64, Off> {
        let mut out = 0;
        let status = (self.api().constant)(self.host.handle(), Str::new(name), &raw mut out);
        ok(status, out, || format!("constant {name}"))
    }

    /// The update function of an ECS system, found by its reflected name, so it
    /// survives game updates.
    ///
    /// # Errors
    ///
    /// [`Off`] when the system is missing or ambiguous on this build.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick_api::Load<'_>) -> Result<(), enshwick_api::Off> {
    ///     let system = load.system("add_buildzones_server")?;
    ///     assert_ne!(system, 0);
    ///     Ok(())
    /// }
    /// ```
    pub fn system(&self, name: &str) -> Result<u64, Off> {
        let mut out = 0;
        let status = (self.api().system)(Str::new(name), &raw mut out);
        ok(status, out, || format!("system {name}"))
    }

    /// The offset of a field in a reflected type.
    ///
    /// # Errors
    ///
    /// [`Off`] when the type or field is missing or ambiguous on this build.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick_api::Load<'_>) -> Result<(), enshwick_api::Off> {
    ///     let x = load.field_offset("keen::ecs::TriggerShape", "rangeX")?;
    ///     assert_eq!(x, 4);
    ///     Ok(())
    /// }
    /// ```
    pub fn field_offset(&self, type_name: &str, field: &str) -> Result<u64, Off> {
        let mut out = 0;
        let status = (self.api().field_offset)(Str::new(type_name), Str::new(field), &raw mut out);
        ok(status, out, || format!("field {type_name}.{field}"))
    }

    /// The size of a reflected type.
    ///
    /// # Errors
    ///
    /// [`Off`] when the type is missing or ambiguous on this build.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick_api::Load<'_>) -> Result<(), enshwick_api::Off> {
    ///     let size = load.type_size("keen::ecs::TriggerShape")?;
    ///     assert!(size >= 16);
    ///     Ok(())
    /// }
    /// ```
    pub fn type_size(&self, type_name: &str) -> Result<u32, Off> {
        let mut out = 0;
        let status = (self.api().type_size)(Str::new(type_name), &raw mut out);
        ok(status, out, || format!("type {type_name}"))
    }

    /// Links `hook` into the chain of the game function at `target`.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader refuses the link, for instance because another
    /// mod holds the target exclusively.
    ///
    /// # Examples
    ///
    /// ```
    /// enshwick_api::detour! {
    ///     static UPGRADE: unsafe extern "system" fn(ctx: *mut u64) = upgrade;
    /// }
    ///
    /// fn upgrade(next: enshwick_api::Next<unsafe extern "system" fn(*mut u64)>, ctx: *mut u64) {
    ///     // SAFETY: the original takes the same context.
    ///     unsafe { next.get()(ctx) }
    /// }
    ///
    /// fn init(load: &mut enshwick_api::Load<'_>) -> Result<(), enshwick_api::Off> {
    ///     let target = load.system("upgrade_building_range")?;
    ///     load.hook(&UPGRADE, target)
    /// }
    /// ```
    pub fn hook<F: FnPtr>(&mut self, hook: &'static Hook<F>, target: u64) -> Result<(), Off> {
        let spec = hook.spec(target);
        let status = (self.api().hook)(self.host.handle(), &raw const spec);
        ok(status, (), || format!("hook {}", hook.name()))
    }

    /// The mod's settings: its file in `mod-configs/`, created from `defaults` when
    /// missing, parsed into `T`.
    ///
    /// `defaults` is the commented TOML a player sees on first start; `T`
    /// should accept it as is.
    ///
    /// # Errors
    ///
    /// [`Off`] when the file cannot be read or does not parse into `T`.
    ///
    /// # Examples
    ///
    /// ```
    /// #[derive(serde::Deserialize)]
    /// struct Settings {
    ///     altars_per_flame_level: Vec<u8>,
    /// }
    ///
    /// const DEFAULTS: &str = "# Altars per Flame level.\naltars_per_flame_level = [2, 4, 8]\n";
    ///
    /// fn init(load: &mut enshwick_api::Load<'_>) -> Result<Settings, enshwick_api::Off> {
    ///     load.settings(DEFAULTS)
    /// }
    /// ```
    pub fn settings<T: serde::de::DeserializeOwned>(&mut self, defaults: &str) -> Result<T, Off> {
        let text = self.host.settings_text(defaults)?;
        toml::from_str(&text).map_err(|e| Off::new(format!("settings: {}", e.message())))
    }

    /// The mod's typed settings: `mod-configs/<id>.toml`, written from
    /// [`Config::template`](crate::config::Config::template) when missing, parsed
    /// and checked with [`Config::validate`](crate::config::Config::validate).
    ///
    /// # Errors
    ///
    /// [`Off`] when the file does not parse into `T` or a value is refused.
    ///
    /// # Examples
    ///
    /// ```
    /// /// Altar settings.
    /// #[derive(Default, serde::Deserialize, serde::Serialize, enshwick_api::Config)]
    /// struct Settings {
    ///     /// Altars allowed at each Flame level.
    ///     altars: Vec<u8>,
    /// }
    ///
    /// fn init(load: &mut enshwick_api::Load<'_>) -> Result<Settings, enshwick_api::Off> {
    ///     load.config()
    /// }
    /// ```
    pub fn config<T: Config>(&mut self) -> Result<T, Off> {
        read_config(self.host)
    }

    /// Calls `handler` with the mod's settings each time `mod-configs/<id>.toml`
    /// changes, on a loader thread. A file that no longer parses or validates
    /// is logged and skipped, and the running values stay.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader cannot watch settings.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::atomic::{AtomicU32, Ordering};
    ///
    /// /// Comfort settings.
    /// #[derive(Default, serde::Deserialize, serde::Serialize, enshwick_api::Config)]
    /// struct Settings {
    ///     /// Comfort per piece of furniture, times this factor.
    ///     multiplier: f32,
    /// }
    ///
    /// static MULTIPLIER: AtomicU32 = AtomicU32::new(0);
    ///
    /// fn init(load: &mut enshwick_api::Load<'_>) -> Result<(), enshwick_api::Off> {
    ///     let settings: Settings = load.config()?;
    ///     MULTIPLIER.store(settings.multiplier.to_bits(), Ordering::Relaxed);
    ///     load.on_config_change(|settings: Settings| {
    ///         MULTIPLIER.store(settings.multiplier.to_bits(), Ordering::Relaxed);
    ///     })
    /// }
    /// ```
    pub fn on_config_change<T, F>(&mut self, handler: F) -> Result<(), Off>
    where
        T: Config,
        F: Fn(T) + Send + Sync + 'static,
    {
        self.subscribe(Event::SETTINGS_CHANGED, move |_| {
            let Some(host) = crate::host() else {
                return;
            };
            match read_config::<T>(host) {
                Ok(settings) => {
                    handler(settings);
                    crate::info!("settings reloaded");
                }
                Err(off) => crate::warn!("settings change skipped, the running values stay: {off}"),
            }
        })
    }

    /// The mod's [`Scheduler`](crate::schedule::Scheduler), which runs work on
    /// the server's game thread after each simulation step.
    ///
    /// # Errors
    ///
    /// [`Off`] on a client, or when the loader raises no ticks.
    ///
    /// # Examples
    ///
    /// See [`Scheduler`](crate::schedule::Scheduler).
    pub fn scheduler(&mut self) -> Result<crate::schedule::Scheduler, Off> {
        if self.host.build().side != crate::abi::Side::SERVER {
            return Err(Off::new("the scheduler runs on the server's game thread"));
        }
        let (scheduler, created) = crate::schedule::Scheduler::get();
        if created {
            self.on::<crate::events::Tick>(move |_| scheduler.run_due())?;
        }
        Ok(scheduler)
    }

    /// Calls `handler` on every event of kind `E` with its typed data, on the
    /// thread that raises it. A panic in `handler` turns the mod off.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader does not know the event.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_api::events::{PlayerJoined, WorldSaved};
    ///
    /// fn init(load: &mut enshwick_api::Load<'_>) -> Result<(), enshwick_api::Off> {
    ///     load.on::<PlayerJoined>(|player| enshwick_api::info!("entity {} joined", player.entity))?;
    ///     load.on::<WorldSaved>(|()| enshwick_api::info!("saved"))
    /// }
    /// ```
    pub fn on<E: crate::events::Kind>(
        &mut self,
        handler: impl Fn(&E::Data) + Send + Sync + 'static,
    ) -> Result<(), Off> {
        self.subscribe(E::EVENT, move |raw| {
            // SAFETY: the loader passes this event's data, valid for the call.
            if let Some(data) = unsafe { E::data(raw) } {
                handler(data);
            }
        })
    }

    /// Calls `handler` on every `event` for this mod with the event's data,
    /// null for events without, on the thread that raises it. A panic in
    /// `handler` turns the mod off.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader does not know `event`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_api::abi::Event;
    ///
    /// fn init(load: &mut enshwick_api::Load<'_>) -> Result<(), enshwick_api::Off> {
    ///     load.subscribe(Event::SETTINGS_CHANGED, |_| enshwick_api::info!("settings edited"))
    /// }
    /// ```
    pub fn subscribe<F>(&mut self, event: Event, handler: F) -> Result<(), Off>
    where
        F: Fn(*const c_void) + Send + Sync + 'static,
    {
        let user = Box::into_raw(Box::new(handler));
        let status = self.host.subscribe(event, deliver::<F>, user.cast());
        if !status.is_ok() {
            // SAFETY: refused, so the loader keeps no copy of the pointer.
            drop(unsafe { Box::from_raw(user) });
        }
        ok(status, (), || format!("event {}", event.0))
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
    /// fn init(load: &mut enshwick_api::Load<'_>) -> Result<(), enshwick_api::Off> {
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

    /// Publishes a service other mods can find by `name` and `version`.
    ///
    /// `T` is a `repr(C)` table of `extern "system"` functions, the contract
    /// both mods share; a new incompatible table is a new `version`.
    ///
    /// # Errors
    ///
    /// [`Off`] when another mod already publishes the name and version.
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
    ///     0
    /// }
    ///
    /// static ZONES: Zones = Zones { count };
    ///
    /// fn init(load: &mut enshwick_api::Load<'_>) -> Result<(), enshwick_api::Off> {
    ///     load.publish("flame-altar-zones.zones", 1, &ZONES)
    /// }
    /// ```
    pub fn publish<T: Sync>(
        &mut self,
        name: &str,
        version: u32,
        table: &'static T,
    ) -> Result<(), Off> {
        let status = (self.api().publish)(
            self.host.handle(),
            Str::new(name),
            version,
            core::ptr::from_ref(table).cast(),
        );
        ok(status, (), || format!("service {name} v{version}"))
    }

    /// A service another mod published.
    ///
    /// # Safety
    ///
    /// `T` must be the exact table type the publisher declared for `name` and
    /// `version`.
    ///
    /// # Errors
    ///
    /// [`Off`] when no loaded mod publishes the service; declare the publisher
    /// as a dependency so it loads first.
    ///
    /// # Examples
    ///
    /// ```
    /// #[repr(C)]
    /// struct Zones {
    ///     count: extern "system" fn() -> u32,
    /// }
    ///
    /// fn init(load: &mut enshwick_api::Load<'_>) -> Result<(), enshwick_api::Off> {
    ///     // SAFETY: `Zones` is the table `flame-altar-zones` publishes as v1.
    ///     let zones: &'static Zones = unsafe { load.service("flame-altar-zones.zones", 1)? };
    ///     let _ = (zones.count)();
    ///     Ok(())
    /// }
    /// ```
    pub unsafe fn service<T>(&self, name: &str, version: u32) -> Result<&'static T, Off> {
        let mut out = core::ptr::null();
        let status =
            (self.api().service)(self.host.handle(), Str::new(name), version, &raw mut out);
        ok(status, (), || format!("service {name} v{version}"))?;
        // SAFETY: the publisher handed a `'static` table of type `T`, as the
        // caller guarantees.
        unsafe { out.cast::<T>().as_ref() }.ok_or_else(|| Off::new(format!("service {name}: null")))
    }
}

/// The mod's settings as `T`, parsed and validated.
fn read_config<T: Config>(host: Host) -> Result<T, Off> {
    let text = host.settings_text(&T::template())?;
    let settings: T =
        toml::from_str(&text).map_err(|e| Off::new(format!("settings: {}", e.message())))?;
    settings
        .validate()
        .map_err(|why| Off::new(format!("settings: {why}")))?;
    Ok(settings)
}

/// Runs a subscribed handler; `user` is the handler boxed by
/// [`Load::subscribe`], alive for the process.
unsafe extern "system" fn deliver<F: Fn(*const c_void)>(
    _event: Event,
    data: *const c_void,
    user: *mut c_void,
) {
    // SAFETY: `user` is the leaked `Box<F>` this function was subscribed with.
    let handler = unsafe { &*user.cast::<F>() };
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handler(data))).is_err()
        && let Some(host) = crate::host()
    {
        host.fault("panic in an event handler");
    }
}

fn ok<T>(status: Status, value: T, what: impl FnOnce() -> String) -> Result<T, Off> {
    if status.is_ok() {
        Ok(value)
    } else {
        Err(Off::status(&what(), status))
    }
}
