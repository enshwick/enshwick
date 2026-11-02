//! One mod's VM: the sandbox, the budgets, the calls in and out, and what
//! turns the mod off.

mod api;
mod handlers;
mod sandbox;

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use arc_swap::{ArcSwap, ArcSwapOption};
use enshwick::Relock as _;
use enshwick::abi::{Event, Level, Relation};
use enshwick::events::Ticked;
use enshwick::script::{self, Opened, Signature, Value};
use enshwick::{Host, Load, Off};
use mlua::{Function, Lua, MultiValue, Table, Value as LuaValue};

use crate::bridge::{from_lua, to_lua};
use enshwick::packs::SCRIPT_MAIN as MAIN;

use crate::calls::CallDesc;
use crate::files::Scripts;
use crate::points::{EventDesc, PointDesc, Read, decode};
use crate::subscribe::{RawReader, Subscription, raw_table};
use enshwick::points::Order;
use enshwick::services::{DescribedCall, DescribedCaller, DescribedEmitter};
use sandbox::describe;
use std::sync::OnceLock;

/// How much a mod's scripts may take.
///
/// # Examples
///
/// ```
/// let limits = enshwick_luau::Limits::default();
/// assert_eq!(limits.memory, 64 << 20);
/// ```
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct Limits {
    /// Bytes the VM may hold; an allocation past it fails the call.
    pub memory: usize,
    /// How long one call into the scripts may run: an event handler, a task.
    /// Past it the call stops and the mod turns off.
    pub call: Duration,
    /// How long the calls into the scripts may run together in any one
    /// second; past it the mod turns off.
    pub per_second: Duration,
    /// How long `main.luau` may run at start.
    pub start: Duration,
}

impl Default for Limits {
    /// 10 ms per call, a fifth of a server step at 20 steps a second, and
    /// 100 ms per second, a tenth of the server's time.
    fn default() -> Self {
        Self {
            memory: 64 << 20,
            call: Duration::from_millis(10),
            per_second: Duration::from_millis(100),
            start: Duration::from_secs(2),
        }
    }
}

impl Limits {
    /// The same with `call` as the budget of each call.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    ///
    /// let limits = enshwick_luau::Limits::default().with_call(Duration::from_millis(5));
    /// assert_eq!(limits.call, Duration::from_millis(5));
    /// ```
    #[must_use]
    pub const fn with_call(mut self, call: Duration) -> Self {
        self.call = call;
        self
    }

    /// The same with `per_second` as the budget of all calls in one second.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    ///
    /// let limits = enshwick_luau::Limits::default().with_per_second(Duration::from_millis(50));
    /// assert_eq!(limits.per_second, Duration::from_millis(50));
    /// ```
    #[must_use]
    pub const fn with_per_second(mut self, per_second: Duration) -> Self {
        self.per_second = per_second;
        self
    }

    /// The same with `memory` bytes for the VM.
    ///
    /// # Examples
    ///
    /// ```
    /// let limits = enshwick_luau::Limits::default().with_memory(1 << 20);
    /// assert_eq!(limits.memory, 1 << 20);
    /// ```
    #[must_use]
    pub const fn with_memory(mut self, memory: usize) -> Self {
        self.memory = memory;
        self
    }
}

/// What a mod's VM has cost so far, for the diagnostics.
///
/// # Examples
///
/// See [`Runtime::usage`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Usage {
    /// Bytes the VM holds now.
    pub memory: usize,
    /// The most bytes it held after a call.
    pub peak: usize,
    /// Calls into the scripts: the start, handlers, tasks.
    pub calls: u64,
    /// Calls stopped for running past their budget.
    pub overruns: u64,
}

/// One VM of a mod: the Luau state, the scripts it runs and the handlers
/// its `main.luau` registered.
pub(crate) struct Vm {
    lua: Lua,
    scripts: Scripts,
    /// Whether `main.luau` is still running its top level.
    starting: AtomicBool,
    handlers: Mutex<Vec<(Subscription, Function)>>,
    /// The descriptions of the points the scripts extend.
    points: Mutex<Vec<PointDesc>>,
    /// The descriptions of the mod events the scripts subscribe to.
    events: Mutex<Vec<EventDesc>>,
    /// `enshwick.refuse`, which a point's handler returns to refuse.
    refuse: OnceLock<Table>,
}

pub(crate) struct Inner {
    /// This mod's own pointer, for the functions and callbacks it hands out.
    me: Weak<Inner>,
    id: String,
    /// The mods `enshwick.deps` names, each an alias and a mod id.
    deps: Vec<(String, String)>,
    host: Host,
    limits: Limits,
    /// Every namespace the game build offers, opened for this mod.
    namespaces: Vec<(&'static script::Namespace, Opened)>,
    /// The VM; gone once the mod is off, which frees its memory.
    vm: ArcSwapOption<Vm>,
    /// The subscriptions made at start, the only ones a reload can serve.
    subscribed: Mutex<Vec<Subscription>>,
    /// The mod events the scripts define, each with the emitter the loader
    /// gives once `main.luau` ran at start.
    defined: Mutex<Vec<Defined>>,
    /// The calls the scripts ask for, each with the caller the loader
    /// hands over once `main.luau` ran at start.
    asked: Mutex<Vec<Asked>>,
    /// Whether the start is over, so a new definition waits for the next.
    started: AtomicBool,
    /// The game event types of the build, with their sizes, read at start.
    event_types: Vec<(&'static str, u32)>,
    off: AtomicBool,
    /// The mod's settings in force, from its `pack.toml` schema.
    settings: ArcSwap<toml::Table>,
    calls: AtomicU64,
    overruns: Arc<AtomicU64>,
    peak: AtomicUsize,
    /// When the mod started, which the per-second window counts from.
    epoch: Instant,
    /// The script time of the current second.
    second: crate::budget::Window,
}

/// A mod event the scripts define: its description and the emitter the
/// loader hands back.
struct Defined {
    desc: EventDesc,
    emitter: Arc<OnceLock<DescribedEmitter>>,
}

/// A call a script asked for, and its caller once the start is over.
struct Asked {
    desc: CallDesc,
    caller: Arc<OnceLock<DescribedCaller>>,
}

/// A running Luau mod.
///
/// # Examples
///
/// ```
/// use enshwick_luau::{Limits, Scripts};
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let runtime = enshwick_luau::start(load, "you.hello", Scripts::new().with("main.luau", ""), None, Limits::default())?;
///     assert_eq!(runtime.usage().overruns, 0);
///     Ok(())
/// }
/// ```
#[derive(Clone)]
pub struct Runtime(Arc<Inner>);

impl core::fmt::Debug for Runtime {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Runtime")
            .field("id", &self.0.id)
            .field("off", &self.0.off.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl Runtime {
    /// What the VM has cost so far.
    ///
    /// # Examples
    ///
    /// See [`Runtime`].
    #[must_use]
    pub fn usage(&self) -> Usage {
        self.0.usage()
    }

    /// Whether a fault turned the mod off.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_luau::{Limits, Scripts};
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let runtime = enshwick_luau::start(load, "you.hello", Scripts::new().with("main.luau", ""), None, Limits::default())?;
    ///     assert!(!runtime.is_off());
    ///     Ok(())
    /// }
    /// ```
    #[must_use]
    pub fn is_off(&self) -> bool {
        self.0.off.load(Ordering::Acquire)
    }

    /// Whether the mod runs exactly `scripts` now.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_luau::{Limits, Scripts};
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let scripts = Scripts::new().with("main.luau", "");
    ///     let runtime = enshwick_luau::start(load, "you.hello", scripts.clone(), None, Limits::default())?;
    ///     assert!(runtime.runs(&scripts));
    ///     Ok(())
    /// }
    /// ```
    #[must_use]
    pub fn runs(&self, scripts: &Scripts) -> bool {
        self.0
            .vm
            .load()
            .as_ref()
            .is_some_and(|vm| vm.scripts == *scripts)
    }

    /// Runs `scripts` in place of the mod's scripts, for development: a new
    /// VM runs `main.luau`, takes over the handlers of the events subscribed
    /// at start, and the old VM's tasks stop at their next run.
    ///
    /// # Errors
    ///
    /// [`ReloadError`] when the mod is off, or with the file, line and
    /// message when the new `main.luau` fails at its start; the old scripts
    /// keep running.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_luau::{Limits, Scripts};
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let runtime = enshwick_luau::start(load, "you.hello", Scripts::new().with("main.luau", ""), None, Limits::default())?;
    ///     let newer = Scripts::new().with("main.luau", "print('again')");
    ///     runtime.reload(newer.clone()).map_err(|e| enshwick::Off::new(e.to_string()))?;
    ///     assert!(runtime.runs(&newer));
    ///     Ok(())
    /// }
    /// ```
    pub fn reload(&self, scripts: Scripts) -> Result<(), ReloadError> {
        let inner = &self.0;
        if self.is_off() {
            return Err(ReloadError::Off);
        }
        let vm = inner.run_main(scripts)?;
        let waiting: Vec<String> = {
            let subscribed = inner.subscribed.relock();
            vm.events()
                .into_iter()
                .filter(|sub| !subscribed.contains(sub))
                .map(|sub| sub.name())
                .collect()
        };
        if !waiting.is_empty() {
            inner.host.log(
                Level::WARN,
                &format!("handlers of {} run from the next start", waiting.join(", ")),
            );
        }
        inner.vm.store(Some(vm));
        if self.is_off() {
            inner.vm.store(None);
            return Err(ReloadError::Off);
        }
        Ok(())
    }
}

/// Why [`Runtime::reload`] left the old scripts running.
///
/// # Examples
///
/// ```
/// use enshwick_luau::{ReloadError, StartError};
///
/// let error = ReloadError::Start(StartError::Missing);
/// assert_eq!(error.to_string(), "main.luau is missing");
/// ```
#[derive(Clone, Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ReloadError {
    /// A fault turned the mod off.
    #[error("the mod is off")]
    Off,
    /// The new `main.luau` failed at its start: the file, line and message.
    #[error(transparent)]
    Start(#[from] StartError),
}

/// Why a mod's `main.luau` did not start.
///
/// # Examples
///
/// See [`ReloadError`].
#[derive(Clone, Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StartError {
    /// The scripts hold no `main.luau`.
    #[error("{MAIN} is missing")]
    Missing,
    /// The sandbox did not build.
    #[error("the Luau sandbox: {0}")]
    Sandbox(mlua::Error),
    /// The `enshwick` API did not build.
    #[error("the Luau API: {0}")]
    Api(mlua::Error),
    /// The script failed: the file, the line and the message.
    #[error("{}", describe(.0))]
    Script(mlua::Error),
    /// A call the script made panicked.
    #[error("{MAIN} panicked in a call it made")]
    Panicked,
}

/// The mod and the VM a function or a callback belongs to.
#[derive(Clone)]
pub(crate) struct Scope {
    inner: Weak<Inner>,
    vm: Weak<Vm>,
}

impl Scope {
    /// Calls the script's `function` from the API with `args`, as a
    /// function of `signature`; an error once its VM was replaced.
    pub(crate) fn call_function(
        &self,
        function: &Function,
        args: Vec<Value>,
        signature: &'static Signature,
    ) -> Result<Value, script::Error> {
        let stale = || script::Error::new("the scripts were reloaded or stopped");
        let inner = self.inner.upgrade().ok_or_else(stale)?;
        let vm = self.vm.upgrade().ok_or_else(stale)?;
        let current = inner
            .vm
            .load()
            .as_ref()
            .is_some_and(|now| Arc::ptr_eq(now, &vm));
        if !current {
            return Err(stale());
        }
        let answer = inner.call(&vm, &|| "a function passed to the API".to_owned(), |vm| {
            let lua_args = args
                .into_iter()
                .zip(signature.params())
                .map(|(value, param)| to_lua(&vm.lua, self, value, param.ty()))
                .collect::<mlua::Result<MultiValue>>()?;
            function.call::<LuaValue>(lua_args)
        });
        let answer = answer.ok_or_else(|| script::Error::new("the script failed"))?;
        from_lua(self, answer, signature.ret())
    }
}

/// Starts the Luau mod `id` for the mod `load` initialises: builds its
/// sandbox, runs `main.luau`, and subscribes the events it asked for.
///
/// # Errors
///
/// [`Off`] with the script's file, line and message when `main.luau` is
/// missing, does not compile, fails, or runs past its budget at start.
///
/// # Examples
///
/// See the [crate](crate).
pub fn start(
    load: &mut Load<'_>,
    id: &str,
    scripts: Scripts,
    settings: Option<enshwick::config::Schema>,
    limits: Limits,
) -> Result<Runtime, Off> {
    begin(load, id, scripts, settings, Vec::new(), limits)
}

/// Starts the scripts of the pack `manifest` describes, as [`start`] does,
/// with the settings it declares and its dependencies of `requires`,
/// `after` and `recommends` in `enshwick.deps`, by alias.
///
/// # Errors
///
/// As [`start`].
///
/// # Examples
///
/// ```
/// use enshwick::packs::PackManifest;
/// use enshwick_luau::{Limits, Scripts};
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let manifest = PackManifest::parse("id = \"you.hello\"\nversion = \"1.0.0\"\n[after]\nlockers = \"you.lockers\"\n")
///         .map_err(|e| enshwick::Off::new(e.to_string()))?;
///     let main = "assert(enshwick.deps.lockers.id == 'you.lockers')";
///     enshwick_luau::start_pack(load, &manifest, Scripts::new().with("main.luau", main), Limits::default())?;
///     Ok(())
/// }
/// ```
pub fn start_pack(
    load: &mut Load<'_>,
    manifest: &enshwick::packs::PackManifest,
    scripts: Scripts,
    limits: Limits,
) -> Result<Runtime, Off> {
    let deps = manifest
        .dependencies()
        .filter(|(relation, _, _)| *relation != Relation::BREAKS)
        .map(|(_, alias, dependency)| (alias.to_owned(), dependency.id.clone()))
        .collect();
    begin(
        load,
        &manifest.id,
        scripts,
        manifest.settings_schema(),
        deps,
        limits,
    )
}

/// What [`start`] and [`start_pack`] do, with the dependencies `deps`, each
/// an alias and a mod id.
fn begin(
    load: &mut Load<'_>,
    id: &str,
    scripts: Scripts,
    settings: Option<enshwick::config::Schema>,
    deps: Vec<(String, String)>,
    limits: Limits,
) -> Result<Runtime, Off> {
    let host = load.host();
    let mut namespaces = Vec::new();
    for namespace in script::NAMESPACES {
        match namespace.open(load) {
            Ok(opened) => namespaces.push((namespace, opened)),
            Err(off) => host.log(
                Level::INFO,
                &format!(
                    "enshwick.{} is not on this game build: {off}",
                    namespace.name()
                ),
            ),
        }
    }
    let event_types = load
        .game_event_types()
        .map(|types| types.iter().map(|t| (t.name(), t.size())).collect())
        .unwrap_or_default();
    let inner = Arc::new_cyclic(|me| Inner {
        me: Weak::clone(me),
        id: id.to_owned(),
        deps,
        host,
        limits,
        namespaces,
        vm: ArcSwapOption::empty(),
        subscribed: Mutex::new(Vec::new()),
        defined: Mutex::new(Vec::new()),
        asked: Mutex::new(Vec::new()),
        started: AtomicBool::new(false),
        event_types,
        off: AtomicBool::new(false),
        calls: AtomicU64::new(0),
        overruns: Arc::new(AtomicU64::new(0)),
        peak: AtomicUsize::new(0),
        epoch: Instant::now(),
        second: crate::budget::Window::default(),
        settings: ArcSwap::from_pointee(toml::Table::new()),
    });
    if let Some(schema) = settings {
        let (settings, values) = load.table_config(schema)?;
        inner.settings.store(Arc::new(values));
        let weak = Arc::downgrade(&inner);
        // Subscribed before the script's own handlers, so a handler of
        // `SettingsChanged` reads the new values.
        settings.on_change(load, move |values| {
            if let Some(inner) = weak.upgrade() {
                inner.settings.store(Arc::new(values));
            }
        })?;
    }
    let vm = inner
        .run_main(scripts)
        .map_err(|error| Off::new(error.to_string()))?;
    let events = vm.events();
    inner.vm.store(Some(vm));
    inner.subscribed.relock().clone_from(&events);
    subscribe(&inner, load, events)?;
    let defined: Vec<(
        enshwick::services::DescribedEvent,
        Arc<OnceLock<DescribedEmitter>>,
    )> = inner
        .defined
        .relock()
        .iter()
        .map(|d| (d.desc.event.clone(), Arc::clone(&d.emitter)))
        .collect();
    for (event, emitter) in defined {
        // SAFETY: `EventDesc::parse_definition` accepts only primitive
        // fields whose hashes it worked out from their reads and offsets,
        // and `encode` writes each primitive's valid bytes, a bool as 0 or
        // 1; every send goes through the table `define_event` made for this
        // description, which a reload cannot change.
        let _first = emitter.set(unsafe { load.define_described_event(&event) }?);
    }
    let asked: Vec<(DescribedCall, Arc<OnceLock<DescribedCaller>>)> = inner
        .asked
        .relock()
        .iter()
        .map(|a| (a.desc.call.clone(), Arc::clone(&a.caller)))
        .collect();
    for (call, caller) in asked {
        // SAFETY: `CallDesc::parse` accepts only primitive input fields, a
        // number, bool, entity or GUID, and works out each field's hash
        // itself from its name, primitive and offset, so the loader's match
        // pairs a field only with a server field of that same primitive.
        // `encode` writes each primitive's valid bytes, a bool as 0 or 1, and
        // every other primitive is valid for any bytes; the input buffer
        // starts zeroed and the match on fields and size with `#[call]`'s
        // padding check leaves no other byte. Every call goes through the
        // function `caller` made for this description, which a reload cannot
        // change.
        let _first = caller.set(unsafe { load.described_caller(&call) }?);
    }
    inner.started.store(true, Ordering::Release);

    let usage = Arc::downgrade(&inner);
    load.add_status(move || {
        usage.upgrade().map_or_else(String::new, |inner| {
            let usage = inner.usage();
            format!(
                "Luau: {} KiB, peak {} KiB, {} calls, {} past their budget",
                usage.memory >> 10,
                usage.peak >> 10,
                usage.calls,
                usage.overruns
            )
        })
    })?;
    Ok(Runtime(inner))
}

/// Subscribes what the scripts of `inner` asked for at start: loader
/// events, typed and raw game events, and point links.
fn subscribe(
    inner: &Arc<Inner>,
    load: &mut Load<'_>,
    events: Vec<Subscription>,
) -> Result<(), Off> {
    for sub in events {
        let handled = Arc::clone(inner);
        match &sub {
            Subscription::Loader(event) => {
                let kind = Subscription::loader_kind(*event)?;
                if *event == Event::TICKED {
                    load.on_tick(Ticked::DEFAULT_EVERY, move |tick| {
                        handled.dispatch(kind, (&raw const *tick).cast());
                    })?;
                } else {
                    load.on_raw_event(*event, move |raw| handled.dispatch(kind, raw))?;
                }
            }
            Subscription::Game(name) => {
                let kind = Subscription::game_kind(name)?;
                let key = sub.clone();
                kind.subscribe(load, move |value| {
                    handled.deliver(&key, |lua, scope| to_lua(lua, scope, value(), kind.data()));
                })?;
            }
            Subscription::Point(name) => {
                let desc = inner
                    .vm
                    .load()
                    .as_ref()
                    .and_then(|vm| {
                        vm.points
                            .relock()
                            .iter()
                            .find(|d| d.point.name == *name)
                            .cloned()
                    })
                    .ok_or_else(|| Off::new(format!("point {name} has no description")))?;
                let key = sub.clone();
                let point = desc.point.clone();
                // SAFETY: in a mode that changes the answer, `PointDesc::parse`
                // accepts only a primitive answer whose layout hash, computed
                // here from that primitive, equals the module's hash, which
                // the loader matched to the owner's; `encode` writes that
                // primitive's valid bytes, a bool as 0 or 1.
                unsafe {
                    load.extend_described_point(&point, &Order::new(), move |input, answer| {
                        handled.run_point(&key, &desc, input, answer)
                    })
                }?;
            }
            Subscription::ModEvent(name) => {
                let desc = inner
                    .vm
                    .load()
                    .as_ref()
                    .and_then(|vm| {
                        vm.events
                            .relock()
                            .iter()
                            .find(|d| d.event.name == *name)
                            .cloned()
                    })
                    .ok_or_else(|| Off::new(format!("mod event {name} has no description")))?;
                let key = sub.clone();
                let read = Read::Struct(desc.fields.clone());
                load.on_described_mod_event(&desc.event, move |data| {
                    handled.deliver(&key, |lua, _| decode(lua, data, 0, &read));
                })?;
            }
            Subscription::Raw(spec) => {
                let (ty, reader) = RawReader::resolve(load, spec, &inner.event_types)?;
                let key = sub.clone();
                load.on_raw_game_event(ty, move |event| {
                    handled.deliver(&key, |lua, _| raw_table(lua, &reader.read(event)));
                })?;
            }
        }
    }
    Ok(())
}

impl Inner {
    fn usage(&self) -> Usage {
        let memory = self.vm.load().as_ref().map_or(0, |vm| vm.lua.used_memory());
        Usage {
            memory,
            peak: self.peak.load(Ordering::Relaxed).max(memory),
            calls: self.calls.load(Ordering::Relaxed),
            overruns: self.overruns.load(Ordering::Relaxed),
        }
    }
}
