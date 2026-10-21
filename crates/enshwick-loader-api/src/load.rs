//! What a mod does while the loader initialises it.

use core::ffi::c_void;
use std::borrow::Cow;

use crate::abi::{Event, HookFlags, HostApi, MidHookSpec, Registers, Status, Str};
use crate::config::{Config, Stage};
use crate::hook::{FnPtr, Hook};
use crate::host::Host;
use crate::services::{Emitter, ModEvent, Service};

/// Why a mod turns itself off during initialisation; the loader logs the
/// reason and keeps the game vanilla where the mod would have acted.
///
/// # Examples
///
/// ```
/// let off = enshwick_loader_api::Off::new("this game build is not supported");
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

    pub(crate) fn status(what: &str, status: Status) -> Self {
        let why = match status {
            Status::MISSING => "missing on this game build",
            Status::AMBIGUOUS => "ambiguous on this game build",
            Status::REFUSED => "refused by the loader",
            Status::BAD_ARGS => "malformed",
            Status::TOO_LATE => "asked for too late",
            Status::FAULTED => "unavailable: the mod has faulted",
            Status::WRONG_THREAD => "called off the thread it serves",
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
/// impl enshwick_loader_api::Mod for Quiet {
///     fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
///         enshwick_loader_api::manifest!(id: "quiet", side: SERVER)
///     }
///
///     fn init(_load: &mut enshwick_loader_api::Load<'_>) -> Result<Self, enshwick_loader_api::Off> {
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
    /// with `reload = true` in its manifest): stop every thread and timer
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
    /// [`host()`](crate::host()).
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) {
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
    /// [`Off`] when neither the mod nor the loader pins the id for this build,
    /// or the address it names is null, so a mod that turns the address into
    /// a function pointer never makes a null one.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     let lookup = load.symbol("npc.lookup")?;
    ///     assert_ne!(lookup, 0);
    ///     Ok(())
    /// }
    /// ```
    pub fn symbol(&self, name: &str) -> Result<u64, Off> {
        let mut out = 0;
        let status = (self.api().symbol)(self.host.handle(), Str::new(name), &raw mut out);
        let address = ok(status, out, || format!("symbol {name}"))?;
        if address == 0 {
            return Err(Off::new(format!("symbol {name}: null on this game build")));
        }
        Ok(address)
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
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
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
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
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
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
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
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
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
    /// enshwick_loader_api::detour! {
    ///     static UPGRADE: unsafe extern "system" fn(ctx: *mut u64) = upgrade;
    /// }
    ///
    /// fn upgrade(next: enshwick_loader_api::Next<unsafe extern "system" fn(*mut u64)>, ctx: *mut u64) {
    ///     // SAFETY: the original takes the same context.
    ///     unsafe { next.get()(ctx) }
    /// }
    ///
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     let target = load.system("upgrade_building_range")?;
    ///     load.hook(&UPGRADE, target)
    /// }
    /// ```
    pub fn hook<F: FnPtr>(&mut self, hook: &'static Hook<F>, target: u64) -> Result<(), Off> {
        let spec = hook.spec(target);
        let status = (self.api().hook)(self.host.handle(), &raw const spec);
        ok(status, (), || format!("hook {}", hook.name()))
    }

    /// The game's code section (`.text`) as loaded; the slice starts at its
    /// absolute address and lives for the process.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader is too old to tell.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     let code = load.code()?;
    ///     enshwick_loader_api::info!("code at {:p}, {} bytes", code.as_ptr(), code.len());
    ///     Ok(())
    /// }
    /// ```
    pub fn code(&self) -> Result<&'static [u8], Off> {
        let api = self.api();
        offered(
            api,
            core::mem::offset_of!(HostApi, code_range),
            "code searches",
        )?;
        let (mut start, mut len) = (0u64, 0u64);
        let status = (api.code_range)(&raw mut start, &raw mut len);
        ok(status, (), || "code section".to_owned())?;
        let (Ok(start), Ok(len)) = (usize::try_from(start), usize::try_from(len)) else {
            return Err(Off::new("code section: out of range"));
        };
        if start == 0 {
            return Err(Off::new("code section: missing"));
        }
        // SAFETY: the loader names the game's code section, mapped readable
        // for the life of the process.
        Ok(unsafe { core::slice::from_raw_parts(core::ptr::with_exposed_provenance(start), len) })
    }

    /// The absolute address of the one place in the game's code that matches
    /// `pattern`: hex bytes separated by spaces, `??` for any byte (see
    /// [`Pattern`](crate::code::Pattern)). Unlike a pinned symbol it survives
    /// a game update that leaves those bytes alone.
    ///
    /// # Errors
    ///
    /// [`Off`] when the pattern is malformed, matches nothing or matches more
    /// than one place.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     let site = load.find_code("B9 00 02 00 00 66 3B C1 0F 84 ?? ?? ?? ??")?;
    ///     assert_ne!(site, 0);
    ///     Ok(())
    /// }
    /// ```
    pub fn find_code(&self, pattern: &str) -> Result<u64, Off> {
        let parsed = crate::code::Pattern::parse(pattern)
            .map_err(|e| Off::new(format!("code pattern {pattern:?}: {e}")))?;
        let code = self.code()?;
        let at = parsed
            .find_unique(code)
            .map_err(|e| Off::new(format!("code pattern {pattern:?}: {e}")))?;
        Ok(code.as_ptr().addr() as u64 + at as u64)
    }

    /// Replaces game code at `address`: the bytes there must read `expected`
    /// and become `replacement`, of the same length. The loader checks the
    /// bytes again when it arms, writes the patch together with every hook,
    /// and logs it. A patch stays for the process, even when the mod is
    /// turned off later.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader is too old, the lengths differ, the bytes are
    /// not in the game's code or another patch covers any of them. At arming,
    /// a patch whose bytes no longer read `expected` or that overlaps a hooked
    /// function's first bytes is skipped and the mod turned off. A mod's
    /// patches go in together: when one is skipped or one of its hooks fails,
    /// none is written.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     // `cmp eax, 8; jb` becomes `cmp eax, 16; jb`.
    ///     let site = load.find_code("83 F8 08 72 ??")?;
    ///     load.patch_code(site + 2, &[0x08], &[0x10])
    /// }
    /// ```
    pub fn patch_code(
        &mut self,
        address: u64,
        expected: &[u8],
        replacement: &[u8],
    ) -> Result<(), Off> {
        let api = self.api();
        offered(
            api,
            core::mem::offset_of!(HostApi, patch_code),
            "code patches",
        )?;
        if expected.len() != replacement.len() {
            return Err(Off::new(format!(
                "code patch at {address:#x}: {} bytes expected, {} given",
                expected.len(),
                replacement.len()
            )));
        }
        let status = (api.patch_code)(
            self.host.handle(),
            address,
            expected.as_ptr(),
            replacement.as_ptr(),
            expected.len(),
        );
        ok(status, (), || format!("code patch at {address:#x}"))
    }

    /// Runs `handler` before the instruction at `address`, inside a game
    /// function, with the registers the game code has there.
    ///
    /// Writes to any field but `rsp` take effect when the game code goes on.
    /// Every mod's handler on one address runs, in load order, each seeing the
    /// writes of the ones before. A panic in `handler` turns the mod off and
    /// leaves the registers as they were; the game goes on unchanged and the
    /// handler is skipped from then on.
    ///
    /// `address` must start an instruction, and the instructions in the 5
    /// bytes from it must not be jumped into from elsewhere: the loader
    /// replaces them with a jump.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader refuses the hook: a second one of this mod on
    /// `address`, another mod holding it exclusively, or a loader older than
    /// ABI minor 11.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     // An instruction inside a function, pinned per build in the mod's rows.
    ///     let site = load.symbol("building.range_check")?;
    ///     load.hook_at(site, |registers| registers.rax = registers.rax.min(64))
    /// }
    /// ```
    pub fn hook_at<F>(&mut self, address: u64, handler: F) -> Result<(), Off>
    where
        F: Fn(&mut Registers) + Send + Sync + 'static,
    {
        let api = self.api();
        offered(
            api,
            core::mem::offset_of!(HostApi, hook_at),
            "mid-function hooks",
        )?;
        let user = Box::into_raw(Box::new(MidHook { address, handler }));
        let spec = MidHookSpec {
            address,
            handler: run_mid_hook::<F>,
            user: user.cast(),
            priority: 0,
            flags: HookFlags::NONE,
        };
        let status = (api.hook_at)(self.host.handle(), &raw const spec);
        if !status.is_ok() {
            // SAFETY: refused, so the loader keeps no copy of the pointer.
            drop(unsafe { Box::from_raw(user) });
        }
        ok(status, (), || format!("hook at {address:#x}"))
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
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<Settings, enshwick_loader_api::Off> {
    ///     load.settings(DEFAULTS)
    /// }
    /// ```
    pub fn settings<T: serde::de::DeserializeOwned>(&mut self, defaults: &str) -> Result<T, Off> {
        let text = self.host.settings_text(defaults)?;
        toml::from_str(&text).map_err(|e| Off::new(format!("settings: {}", e.message())))
    }

    /// The mod's typed settings: `mod-configs/<id>.toml`, written from
    /// [`Config::template`] when missing, parsed and checked with
    /// [`Config::validate`].
    ///
    /// On a server, the fields marked `#[config(sync)]` are reported to the
    /// loader, which advertises them to players; see
    /// [`on_config_change`](Self::on_config_change) for the client's side.
    ///
    /// # Errors
    ///
    /// [`Off`] when the file does not parse into `T` or a value is refused.
    ///
    /// # Examples
    ///
    /// ```
    /// /// Altar settings.
    /// #[derive(Default, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
    /// struct Settings {
    ///     /// Altars allowed at each Flame level.
    ///     altars: Vec<u8>,
    /// }
    ///
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<Settings, enshwick_loader_api::Off> {
    ///     load.config()
    /// }
    /// ```
    pub fn config<T: Config>(&mut self) -> Result<T, Off> {
        self.host.config_schema(&T::schema().to_text());
        self.config_check::<T>();
        let settings: T = read_config(self.host)?;
        *STARTED
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Some(settings.values(Stage::Startup).unwrap_or_default());
        Ok(settings)
    }

    /// Calls `handler` with the mod's settings each time `mod-configs/<id>.toml`
    /// changes, after `/config` or the Mods screen sets one of them, and as a
    /// world loads or stops; the thread is not fixed. A file that no longer
    /// parses or validates is logged and skipped, and the running values stay.
    ///
    /// On a client, it also runs when the join values change: on joining a
    /// server, `handler` gets the settings with the server's
    /// [`synced`](Config::synced) fields over the file's (the file stays as
    /// it is), and on joining a server without them, the file's values again.
    /// A mod with join values applies them here; without a handler it runs
    /// with the file's values until the next start.
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
    /// #[derive(Default, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
    /// struct Settings {
    ///     /// Comfort per piece of furniture, times this factor.
    ///     multiplier: f32,
    /// }
    ///
    /// static MULTIPLIER: AtomicU32 = AtomicU32::new(0);
    ///
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
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
        let handler = std::sync::Arc::new(handler);
        let on_world = std::sync::Arc::clone(&handler);
        if self.api().header.abi_minor >= 46 {
            self.subscribe(Event::WORLD_SETTINGS, move |_| {
                let Some(host) = crate::host() else {
                    return;
                };
                match read_config::<T>(host) {
                    Ok(settings) => {
                        *STARTED
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner) =
                            Some(settings.values(Stage::Startup).unwrap_or_default());
                        on_world(settings);
                        crate::info!("the world's settings apply");
                    }
                    Err(off) => crate::warn!("the world's settings are skipped: {off}"),
                }
            })?;
        }
        self.subscribe(Event::SETTINGS_CHANGED, move |_| {
            let Some(host) = crate::host() else {
                return;
            };
            match read_config::<T>(host) {
                Ok(mut settings) => {
                    hold_startup(&mut settings);
                    handler(settings);
                    crate::info!("settings reloaded");
                }
                Err(off) => crate::warn!("settings change skipped, the running values stay: {off}"),
            }
        })
    }

    /// The server's [`World`](crate::world::World): reads, queries and
    /// requests; on a client, the built-in server's while it hosts a game.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader is too old or the game build lacks a type the
    /// reads need.
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
    /// [`Off`] under a loader older than ABI minor 12, or when this game
    /// build lacks a piece of them.
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
        let (scheduler, subscribe) = crate::schedule::Scheduler::get();
        if subscribe
            && let Err(off) = self.on::<crate::events::Ticked>(move |_| scheduler.run_due())
        {
            scheduler.unsubscribed();
            return Err(off);
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
    /// use enshwick_loader_api::events::{PlayerJoined, WorldSaved};
    ///
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     load.on::<PlayerJoined>(|player| enshwick_loader_api::info!("entity {} joined", player.entity))?;
    ///     load.on::<WorldSaved>(|()| enshwick_loader_api::info!("saved"))
    /// }
    /// ```
    pub fn on<E: crate::events::Kind>(
        &mut self,
        handler: impl Fn(&E::Data) + Send + Sync + 'static,
    ) -> Result<(), Off> {
        self.subscribe(E::EVENT, move |raw| {
            // SAFETY: the loader passes this event's data, valid for the call.
            if let Some(data) = unsafe { E::data(raw) } {
                handler(&data);
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
    /// use enshwick_loader_api::abi::Event;
    ///
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     load.subscribe(Event::SETTINGS_CHANGED, |_| enshwick_loader_api::info!("settings edited"))
    /// }
    /// ```
    pub fn subscribe<F>(&mut self, event: Event, handler: F) -> Result<(), Off>
    where
        F: Fn(*const c_void) + Send + Sync + 'static,
    {
        let host = self.host;
        hand_over(
            handler,
            |user| host.subscribe(event, deliver::<F>, user),
            &format!("event {}", event.0),
        )
    }

    /// The reflected game event type `name`, such as
    /// `keen::ecs::EntityDiedEvent`, for
    /// [`on_raw_game_event`](Self::on_raw_game_event).
    ///
    /// # Errors
    ///
    /// [`Off`] when the type is not reflected on this build.
    ///
    /// # Examples
    ///
    /// See [`EventType`](crate::game_events::EventType).
    pub fn event_type(&self, name: &'static str) -> Result<crate::game_events::EventType, Off> {
        crate::game_events::EventType::resolve(self, name)
    }

    /// Calls `handler` on every game event of type `ty`, on the server's
    /// game thread before the systems of the next step run; see
    /// [`game_events`](crate::game_events). A panic in `handler` turns the
    /// mod off.
    ///
    /// # Errors
    ///
    /// [`Off`] under a loader older than ABI minor 20, or after `init`.
    ///
    /// # Examples
    ///
    /// See [`EventType::field`](crate::game_events::EventType::field).
    pub fn on_raw_game_event<F>(
        &mut self,
        ty: crate::game_events::EventType,
        handler: F,
    ) -> Result<(), Off>
    where
        F: Fn(&crate::game_events::RawGameEvent<'_>) + Send + Sync + 'static,
    {
        self.game_events(ty.hash(), ty.name(), handler)
    }

    /// Calls `handler` on every game event of kind `E`, typed; see
    /// [`game_events`](crate::game_events). A panic in `handler` turns the
    /// mod off.
    ///
    /// # Errors
    ///
    /// [`Off`] under a loader older than ABI minor 20, after `init`, or when
    /// this build lacks a field of the event.
    ///
    /// # Examples
    ///
    /// See [`Died`](crate::game_events::Died).
    pub fn on_game<E: crate::game_events::Kind>(
        &mut self,
        handler: impl Fn(&E) + Send + Sync + 'static,
    ) -> Result<(), Off> {
        E::subscribe(self, handler)
    }

    /// Calls `handler` on every game event of every type, for finding out
    /// what the game raises. Each event is copied for the mod, so leave this
    /// to development builds.
    ///
    /// # Errors
    ///
    /// As for [`on_raw_game_event`](Self::on_raw_game_event).
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::atomic::{AtomicU64, Ordering};
    ///
    /// static SEEN: AtomicU64 = AtomicU64::new(0);
    ///
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     load.on_every_raw_game_event(|_event| {
    ///         SEEN.fetch_add(1, Ordering::Relaxed);
    ///     })
    /// }
    /// ```
    pub fn on_every_raw_game_event<F>(&mut self, handler: F) -> Result<(), Off>
    where
        F: Fn(&crate::game_events::RawGameEvent<'_>) + Send + Sync + 'static,
    {
        self.game_events(0, "every type", handler)
    }

    fn game_events<F>(&mut self, type_hash: u32, what: &str, handler: F) -> Result<(), Off>
    where
        F: Fn(&crate::game_events::RawGameEvent<'_>) + Send + Sync + 'static,
    {
        let api = self.api();
        offered(
            api,
            core::mem::offset_of!(HostApi, game_event),
            "game events",
        )?;
        let host = self.host.handle();
        hand_over(
            handler,
            |user| (api.game_event)(host, type_hash, deliver_game::<F>, user),
            &format!("game events of {what}"),
        )
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
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
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

    /// Publishes `table` as the service its type declares, for other mods.
    ///
    /// The type comes from [`#[service]`](macro@crate::service), and the
    /// loader keeps its layout to check every user against.
    ///
    /// # Errors
    ///
    /// [`Off`] when another mod publishes the same name and major.
    ///
    /// # Examples
    ///
    /// ```
    /// #[enshwick_loader_api::service(name = "trading-post.prices")]
    /// pub struct Prices {
    ///     pub price: extern "system" fn(item: u32) -> u32,
    /// }
    ///
    /// extern "system" fn price(_item: u32) -> u32 {
    ///     0
    /// }
    ///
    /// static PRICES: Prices = Prices { price };
    ///
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     load.publish(&PRICES)
    /// }
    /// ```
    pub fn publish<T: Service>(&mut self, table: &'static T) -> Result<(), Off> {
        services_offered(self.api())?;
        let spec = service_spec::<T>(core::ptr::from_ref(table).cast());
        let status = (self.api().publish)(self.host.handle(), &raw const spec);
        ok(status, (), || {
            format!(
                "service {} v{}: published by another mod",
                T::NAME,
                T::MAJOR
            )
        })
    }

    /// The service `T` declares, published by another mod, once the loader
    /// has checked that the published table starts with every field of
    /// `T`.
    ///
    /// A publisher built against a longer declaration serves this mod; one
    /// built against a shorter declaration does not, and the error names
    /// the first field it lacks.
    ///
    /// # Errors
    ///
    /// [`Off`] with the reason when no running mod publishes the service,
    /// or the published table lacks or changed a field of `T`. Declare the
    /// publisher in `requires` so it loads first.
    ///
    /// # Examples
    ///
    /// ```
    /// #[enshwick_loader_api::service(name = "trading-post.prices")]
    /// pub struct Prices {
    ///     pub price: extern "system" fn(item: u32) -> u32,
    /// }
    ///
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     let prices: &'static Prices = load.service()?;
    ///     let _ = (prices.price)(0);
    ///     Ok(())
    /// }
    /// ```
    pub fn service<T: Service>(&self) -> Result<&'static T, Off> {
        self.find_service::<T>()
            .map_err(|(_, reason)| Off::new(reason))
    }

    /// The service `T` declares when a running mod publishes a table that
    /// serves it, `None` otherwise: for a service the mod can do without,
    /// with the publisher in `after`. A table that does not serve `T` is
    /// logged as a warning.
    ///
    /// # Examples
    ///
    /// ```
    /// #[enshwick_loader_api::service(name = "trading-post.prices")]
    /// pub struct Prices {
    ///     pub price: extern "system" fn(item: u32) -> u32,
    /// }
    ///
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     if let Some(prices) = load.optional_service::<Prices>() {
    ///         let _ = (prices.price)(0);
    ///     }
    ///     Ok(())
    /// }
    /// ```
    #[must_use]
    pub fn optional_service<T: Service>(&self) -> Option<&'static T> {
        match self.find_service::<T>() {
            Ok(table) => Some(table),
            Err((status, reason)) => {
                let level = if status == Status::MISSING {
                    crate::abi::Level::DEBUG
                } else {
                    crate::abi::Level::WARN
                };
                self.host.log(level, &reason);
                None
            }
        }
    }

    fn find_service<T: Service>(&self) -> Result<&'static T, (Status, String)> {
        services_offered(self.api()).map_err(|off| (Status::MISSING, off.to_string()))?;
        let spec = service_spec::<T>(core::ptr::null());
        let mut found = crate::abi::ServiceFound::EMPTY;
        let status = (self.api().service)(self.host.handle(), &raw const spec, &raw mut found);
        let what = format!("service {} v{}", T::NAME, T::MAJOR);
        if !status.is_ok() {
            // SAFETY: the loader keeps the reason for the process.
            let reason = unsafe { found.reason.as_str() }.unwrap_or_default();
            return Err((
                status,
                if reason.is_empty() {
                    format!("{what}: not found")
                } else {
                    format!("{what}: {reason}")
                },
            ));
        }
        // SAFETY: the loader checked the published layout starts with
        // every field of `T`, and tables are `'static`.
        unsafe { found.table.cast::<T>().as_ref() }
            .ok_or_else(|| (Status::MISSING, format!("{what}: null table")))
    }

    /// Defines the mod event `E` for other mods to subscribe to, and returns
    /// the [`Emitter`] that sends it.
    ///
    /// `E` comes from [`#[event]`](macro@crate::event), named after this
    /// mod: `<id>.<what>`.
    ///
    /// # Errors
    ///
    /// [`Off`] when the name is outside this mod's id or defined twice.
    ///
    /// # Examples
    ///
    /// See [`Emitter`].
    pub fn define_event<E: ModEvent>(&mut self) -> Result<Emitter<E>, Off> {
        services_offered(self.api())?;
        let spec = event_spec::<E>();
        let mut id = 0;
        let status = (self.api().define_event)(self.host.handle(), &raw const spec, &raw mut id);
        ok(status, Emitter::new(self.host, id), || {
            format!(
                "event {}: named after another mod or defined twice",
                E::NAME
            )
        })
    }

    /// Calls `handler` on every mod event `E` another mod sends, on the
    /// sending thread. A panic in `handler` turns this mod off.
    ///
    /// The loader matches the subscription once every mod is initialised:
    /// with no running mod defining `E`, or a definition that lacks or
    /// changed a field of `E`, it logs why and the handler stays idle.
    ///
    /// # Errors
    ///
    /// [`Off`] when called after `init`.
    ///
    /// # Examples
    ///
    /// ```
    /// #[enshwick_loader_api::event(name = "trading-post.sold")]
    /// #[derive(Clone, Copy, Debug)]
    /// pub struct Sold {
    ///     pub item: u32,
    /// }
    ///
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     load.on_mod_event(|sold: &Sold| enshwick_loader_api::info!("sold {}", sold.item))
    /// }
    /// ```
    pub fn on_mod_event<E: ModEvent>(
        &mut self,
        handler: impl Fn(&E) + Send + Sync + 'static,
    ) -> Result<(), Off> {
        subscribe_mod_event(self.host, handler)
    }
}

/// Subscribes `handler` to the mod event `E`.
fn subscribe_mod_event<E: ModEvent, F: Fn(&E) + Send + Sync + 'static>(
    host: Host,
    handler: F,
) -> Result<(), Off> {
    services_offered(host.api())?;
    let spec = event_spec::<E>();
    hand_over(
        handler,
        |user| {
            (host.api().subscribe_event)(
                host.handle(),
                &raw const spec,
                deliver_mod_event::<E, F>,
                user,
            )
        },
        &format!("event {}", E::NAME),
    )
}

/// Whether the loader has typed services and mod events.
fn services_offered(api: &HostApi) -> Result<(), Off> {
    offered(
        api,
        core::mem::offset_of!(HostApi, send_event),
        "services and mod events",
    )
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

/// Runs a mod event handler; `user` is the handler boxed by
/// `Load::on_mod_event`, alive for the process.
unsafe extern "system" fn deliver_mod_event<E: ModEvent, F: Fn(&E)>(
    data: *const c_void,
    user: *mut c_void,
) {
    // SAFETY: `user` is the leaked `Box<F>` this function was subscribed with.
    let handler = unsafe { &*user.cast::<F>() };
    // SAFETY: the loader checked the sender's layout starts with every
    // field of `E`, and the data lives for the call.
    let Some(event) = (unsafe { data.cast::<E>().as_ref() }) else {
        return;
    };
    crate::host::guard("panic in a mod event handler", || (), || handler(event));
}

/// The startup values the mod read in [`Load::config`], as an inline table;
/// later reads keep them until a restart.
static STARTED: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Puts the startup values the mod started with back over `settings`,
/// noting the ones that wait for a restart.
fn hold_startup<T: Config>(settings: &mut T) {
    let started = STARTED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    let (Some(started), Some(now)) = (started, settings.values(Stage::Startup)) else {
        return;
    };
    if started == now {
        return;
    }
    match settings.merge_values(Stage::Startup, &started) {
        Ok(()) => crate::info!("settings that take effect when the world loads changed: {now}"),
        Err(why) => crate::warn!("startup settings not kept: {why}"),
    }
}

/// The mod's settings as `T`, parsed and validated, with the server's join
/// values over the file's while this client is joined to a server that has
/// them.
/// The reason a mod's check refused a settings text, kept until its next
/// check.
static CHECK_WHY: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

/// Parses and validates `text` as the mod's settings, for the loader's
/// check before it writes a change.
unsafe extern "system" fn check_text<T: Config>(
    text: Str,
    why: *mut Str,
    _user: *mut c_void,
) -> Status {
    crate::host::guard(
        "panic in the settings check",
        || Status::REFUSED,
        || {
            // SAFETY: the loader passes the text for the call.
            let text = unsafe { text.as_str() }.unwrap_or("");
            let checked = toml::from_str::<T>(text)
                .map_err(|e| e.message().to_owned())
                .and_then(|settings| settings.validate());
            let Err(reason) = checked else {
                return Status::OK;
            };
            let mut kept = CHECK_WHY
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *kept = reason;
            if !why.is_null() {
                // SAFETY: the loader's out pointer; the reason stays in
                // `CHECK_WHY` until the next check.
                unsafe { why.write(Str::new(&kept)) };
            }
            Status::REFUSED
        },
    )
}

impl Load<'_> {
    /// Hands the loader the mod's check of new settings texts, when it
    /// takes one.
    fn config_check<T: Config>(&self) {
        let api = self.api();
        if api
            .header
            .covers(core::mem::offset_of!(HostApi, config_check) + size_of::<usize>())
        {
            let _checked =
                (api.config_check)(self.host.handle(), check_text::<T>, core::ptr::null_mut());
        }
    }
}

fn read_config<T: Config>(host: Host) -> Result<T, Off> {
    let text = host.settings_text(&T::template())?;
    let settings: T =
        toml::from_str(&text).map_err(|e| Off::new(format!("settings: {}", e.message())))?;
    settings
        .validate()
        .map_err(|why| Off::new(format!("settings: {why}")))?;
    let Some(server) = settings.synced().and_then(|local| host.sync(&local)) else {
        return Ok(settings);
    };
    match with_server::<T>(&text, &server) {
        Ok(joined) => {
            crate::info!("the server's join values apply over the settings file: {server}");
            Ok(joined)
        }
        Err(why) => {
            crate::warn!("the server's join values are refused, the file's apply: {why}");
            Ok(settings)
        }
    }
}

/// The settings in `text` with the join values `server` over them, checked.
fn with_server<T: Config>(text: &str, server: &str) -> Result<T, String> {
    let mut joined: T = toml::from_str(text).map_err(|e| e.message().to_owned())?;
    joined.merge_synced(server)?;
    joined.validate()?;
    Ok(joined)
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
    crate::host::guard("panic in an event handler", || (), || handler(data));
}

/// Runs a game event handler; `user` is the handler boxed by
/// `Load::game_events`, alive for the process.
unsafe extern "system" fn deliver_game<F: Fn(&crate::game_events::RawGameEvent<'_>)>(
    event: *const crate::abi::GameEventData,
    user: *mut c_void,
) {
    // SAFETY: `user` is the leaked `Box<F>` this function was registered with.
    let handler = unsafe { &*user.cast::<F>() };
    // SAFETY: the loader passes its event, valid for the call.
    let Some(event) = (unsafe { crate::game_events::RawGameEvent::from_raw(event) }) else {
        return;
    };
    crate::host::guard("panic in a game event handler", || (), || handler(&event));
}

/// A mid-function hook's handler and where it runs, for the fault message.
struct MidHook<F> {
    address: u64,
    handler: F,
}

unsafe extern "system" fn run_mid_hook<F: Fn(&mut Registers)>(
    registers: *mut Registers,
    user: *mut c_void,
) {
    // SAFETY: `user` is the leaked `MidHook<F>` this function was registered with.
    let hook = unsafe { &*user.cast::<MidHook<F>>() };
    // SAFETY: the loader passes its saved registers, valid and unaliased for the call.
    let Some(registers) = (unsafe { registers.as_mut() }) else {
        return;
    };
    // The handler works on a copy, so a panic halfway leaves the game's
    // registers whole.
    let mut copy = *registers;
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (hook.handler)(&mut copy))) {
        Ok(()) => *registers = copy,
        Err(_) => {
            if let Some(host) = crate::host() {
                host.fault(&format!("panic in the hook at {:#x}", hook.address));
            }
        }
    }
}

/// `value` when `status` is OK, otherwise an [`Off`] naming `what` and why.
pub(crate) fn ok<T>(status: Status, value: T, what: impl FnOnce() -> String) -> Result<T, Off> {
    if status.is_ok() {
        Ok(value)
    } else {
        Err(Off::status(&what(), status))
    }
}

/// An [`Off`] unless the loader's table reaches the entry at `field`, an
/// offset into [`HostApi`].
pub(crate) fn offered(api: &HostApi, field: usize, what: &str) -> Result<(), Off> {
    if api.header.covers(field + size_of::<usize>()) {
        Ok(())
    } else {
        Err(Off::new(format!("{what}: needs a newer loader")))
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

#[cfg(test)]
mod tests {
    use super::with_server;

    /// Test settings.
    #[derive(Debug, Default, serde::Deserialize, serde::Serialize, crate::Config)]
    #[config(validate = Settings::check)]
    struct Settings {
        /// A join value.
        #[config(sync)]
        table: Vec<u8>,
        /// This side's own.
        local: u8,
    }

    impl Settings {
        fn check(&self) -> Result<(), String> {
            (self.table.len() < 4)
                .then_some(())
                .ok_or_else(|| "a long table".to_owned())
        }
    }

    #[test]
    fn a_failed_call_names_what_failed_and_why() {
        use crate::abi::Status;
        assert_eq!(super::ok(Status::OK, 3, || unreachable!()), Ok(3));
        let off = super::ok(Status::MISSING, (), || "component type keen::X".to_owned());
        assert_eq!(
            off.unwrap_err().to_string(),
            "component type keen::X: missing on this game build"
        );
    }

    #[test]
    fn the_server_values_apply_over_the_file_and_are_checked() {
        let file = "table = [1]\nlocal = 5\n";
        let joined: Settings = with_server(file, "{ table = [2, 4] }").unwrap();
        assert_eq!((joined.table.as_slice(), joined.local), (&[2, 4][..], 5));
        assert!(with_server::<Settings>(file, "{ table = [1, 2, 3, 4] }").is_err());
        assert!(with_server::<Settings>(file, "{ table = 'x' }").is_err());
    }
}
