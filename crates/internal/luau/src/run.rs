//! One mod's VM: the sandbox, the budgets, the calls in and out, and what
//! turns the mod off.

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use arc_swap::{ArcSwap, ArcSwapOption};
use enshwick::__private::Relock as _;
use enshwick::abi::Level;
use enshwick::script::{self, EventKind, Opened, Signature, Value};
use enshwick::{Host, Load, Off};
use mlua::chunk::ChunkMode;
use mlua::{Function, Lua, LuaOptions, MultiValue, StdLib, Table, Value as LuaValue, VmState};

use crate::bridge::{arguments, from_lua, to_lua};
use enshwick::packs::SCRIPT_MAIN as MAIN;

use crate::files::{Requirer, Scripts};
use crate::points::{PointDesc, Read, decode, encode};
use crate::subscribe::{RawReader, Subscription, raw_table};
use enshwick::points::{DescribedVerdict, Order};

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
    pub call: Duration,
    /// How long `main.luau` may run at start.
    pub start: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            memory: 64 << 20,
            call: Duration::from_millis(100),
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

thread_local! {
    /// When the outermost call into a script on this thread must end.
    static DEADLINE: Cell<Option<Instant>> = const { Cell::new(None) };
    /// Interrupts since the clock was last read on this thread.
    static TICKS: Cell<u32> = const { Cell::new(0) };
    /// Whether the call on this thread ran past its deadline: every
    /// interrupt from then on fails, so a `pcall` cannot swallow the stop.
    static EXPIRED: Cell<bool> = const { Cell::new(false) };
}

/// How many of Luau's interrupts pass between two reads of the clock.
const TICKS_PER_CHECK: u32 = 64;

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
    /// `enshwick.refuse`, which a point's handler returns to refuse.
    refuse: std::sync::OnceLock<Table>,
}

pub(crate) struct Inner {
    /// This mod's own pointer, for the functions and callbacks it hands out.
    me: Weak<Inner>,
    id: String,
    host: Host,
    limits: Limits,
    /// Every namespace the game build offers, opened for this mod.
    namespaces: Vec<(&'static script::Namespace, Opened)>,
    /// The VM; gone once the mod is off, which frees its memory.
    vm: ArcSwapOption<Vm>,
    /// The subscriptions made at start, the only ones a reload can serve.
    subscribed: Mutex<Vec<Subscription>>,
    /// The game event types of the build, with their sizes, read at start.
    event_types: Vec<(&'static str, u32)>,
    off: AtomicBool,
    /// The mod's settings in force, from its `pack.toml` schema.
    settings: ArcSwap<toml::Table>,
    calls: AtomicU64,
    overruns: Arc<AtomicU64>,
    peak: AtomicUsize,
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
        let vm = inner.run_main(scripts).map_err(ReloadError::Start)?;
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
/// let error = enshwick_luau::ReloadError::Start("main.luau:1: boom".to_owned());
/// assert_eq!(error.to_string(), "main.luau:1: boom");
/// ```
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ReloadError {
    /// A fault turned the mod off.
    #[error("the mod is off")]
    Off,
    /// The new `main.luau` failed at its start: the file, line and message.
    #[error("{0}")]
    Start(String),
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
        .event_types()
        .map(|types| types.iter().map(|t| (t.name(), t.size())).collect())
        .unwrap_or_default();
    let inner = Arc::new_cyclic(|me| Inner {
        me: Weak::clone(me),
        id: id.to_owned(),
        host,
        limits,
        namespaces,
        vm: ArcSwapOption::empty(),
        subscribed: Mutex::new(Vec::new()),
        event_types,
        off: AtomicBool::new(false),
        calls: AtomicU64::new(0),
        overruns: Arc::new(AtomicU64::new(0)),
        peak: AtomicUsize::new(0),
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
    let vm = inner.run_main(scripts).map_err(Off::new)?;
    let events = vm.events();
    inner.vm.store(Some(vm));
    inner.subscribed.relock().clone_from(&events);
    subscribe(&inner, load, events)?;

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
                load.on_raw_event(*event, move |raw| handled.dispatch(kind, raw))?;
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
                load.extend_described_point(&point, &Order::new(), move |input, answer| {
                    handled.run_point(&key, &desc, input, answer)
                })?;
            }
            Subscription::Raw(spec) => {
                let (ty, reader) = RawReader::resolve(load, spec, &inner.event_types)?;
                let key = sub.clone();
                load.on_raw_game_event(ty, move |event| {
                    let values = reader.read(event);
                    handled.deliver(&key, |lua, _| raw_table(lua, &values));
                })?;
            }
        }
    }
    Ok(())
}

/// A VM with Luau's safe libraries and the memory limit; the caller turns
/// the sandbox on once the globals are in place.
fn sandbox(limits: Limits, overruns: &Arc<AtomicU64>) -> mlua::Result<Lua> {
    let libs = StdLib::COROUTINE
        | StdLib::TABLE
        | StdLib::OS
        | StdLib::STRING
        | StdLib::UTF8
        | StdLib::BIT
        | StdLib::MATH
        | StdLib::BUFFER
        | StdLib::VECTOR;
    let lua = Lua::new_with(libs, LuaOptions::new().catch_rust_panics(false))?;
    lua.set_memory_limit(limits.memory)?;
    let overruns = Arc::clone(overruns);
    lua.set_interrupt(move |_| interrupt(&overruns));
    Ok(lua)
}

fn interrupt(overruns: &AtomicU64) -> mlua::Result<VmState> {
    if EXPIRED.get() {
        return Err(over_budget());
    }
    let ticks = TICKS.get().wrapping_add(1);
    TICKS.set(ticks);
    if !ticks.is_multiple_of(TICKS_PER_CHECK) {
        return Ok(VmState::Continue);
    }
    match DEADLINE.get() {
        Some(deadline) if Instant::now() >= deadline => {
            EXPIRED.set(true);
            overruns.fetch_add(1, Ordering::Relaxed);
            Err(over_budget())
        }
        _ => Ok(VmState::Continue),
    }
}

fn over_budget() -> mlua::Error {
    mlua::Error::runtime("the script ran past its time budget")
}

/// An error as the log shows it: the script's message and traceback.
fn describe(error: &mlua::Error) -> String {
    match error {
        mlua::Error::CallbackError { traceback, cause } => format!("{cause}\n{traceback}"),
        mlua::Error::MemoryError(_) => "the script ran out of its memory budget".to_owned(),
        other => other.to_string(),
    }
}

impl Vm {
    /// Every subscription a handler was registered for, once each, in the
    /// order of the first.
    fn events(&self) -> Vec<Subscription> {
        let mut events: Vec<Subscription> = Vec::new();
        for (sub, _) in self.handlers.relock().iter() {
            if !events.contains(sub) {
                events.push(sub.clone());
            }
        }
        events
    }

    fn subscribe(&self, what: &str, sub: Subscription, handler: Function) -> mlua::Result<()> {
        if !self.starting.load(Ordering::Acquire) {
            return Err(mlua::Error::runtime(format!(
                "{what}: subscribe at the top level of main.luau, while the mod starts"
            )));
        }
        self.handlers.relock().push((sub, handler));
        Ok(())
    }

    fn on(&self, name: &str, handler: Function) -> mlua::Result<()> {
        let sub = Subscription::named(name).ok_or_else(|| {
            mlua::Error::runtime(format!("enshwick.on: no event is named {name:?}"))
        })?;
        self.subscribe("enshwick.on", sub, handler)
    }
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

    /// A new VM with the API and `scripts`, after its `main.luau` ran;
    /// the error names the file, the line and the message.
    fn run_main(&self, scripts: Scripts) -> Result<Arc<Vm>, String> {
        let main = scripts
            .get(MAIN)
            .cloned()
            .ok_or_else(|| format!("{MAIN} is missing"))?;
        let lua =
            sandbox(self.limits, &self.overruns).map_err(|e| format!("the Luau sandbox: {e}"))?;
        let vm = Arc::new(Vm {
            lua,
            scripts: scripts.clone(),
            starting: AtomicBool::new(true),
            handlers: Mutex::new(Vec::new()),
            points: Mutex::new(Vec::new()),
            refuse: std::sync::OnceLock::new(),
        });
        let api = self
            .globals(&vm, scripts)
            .map_err(|e| format!("the Luau API: {e}"))?;
        vm.lua
            .sandbox(true)
            .map_err(|e| format!("the Luau sandbox: {e}"))?;
        let started = self.enter(&vm.lua, self.limits.start, || {
            vm.lua
                .load(&main[..])
                .set_name(format!("@{MAIN}"))
                .set_mode(ChunkMode::Text)
                .exec()
        });
        vm.starting.store(false, Ordering::Release);
        match started {
            Ok(Ok(())) => {
                api.set_readonly(true);
                Ok(vm)
            }
            Ok(Err(error)) => Err(describe(&error)),
            Err(()) => Err(format!("{MAIN} panicked in a call it made")),
        }
    }

    /// Puts `extend`, `refuse`, `on_game_type`, `game_event_types` and `on`
    /// into `api`, the subscriptions `vm`'s scripts make at start.
    fn subscription_functions(&self, vm: &Arc<Vm>, api: &Table) -> mlua::Result<()> {
        let lua = &vm.lua;
        let refuse = lua.create_table()?;
        refuse.set_readonly(true);
        let _first = vm.refuse.set(refuse.clone());
        api.raw_set("refuse", refuse)?;
        let weak = Arc::downgrade(vm);
        api.raw_set(
            "extend",
            lua.create_function(move |_, (point, handler): (Table, Function)| {
                let vm = weak
                    .upgrade()
                    .ok_or_else(|| mlua::Error::runtime("the mod has stopped"))?;
                let desc = PointDesc::parse(&point)?;
                let sub = Subscription::Point(desc.point.name.clone());
                {
                    let mut points = vm.points.relock();
                    if !points.iter().any(|d| d.point.name == desc.point.name) {
                        points.push(desc);
                    }
                }
                vm.subscribe("enshwick.extend", sub, handler)
            })?,
        )?;
        let weak = Arc::downgrade(vm);
        api.raw_set(
            "on_game_type",
            lua.create_function(
                move |_, (name, fields, handler): (String, Table, Function)| {
                    let vm = weak
                        .upgrade()
                        .ok_or_else(|| mlua::Error::runtime("the mod has stopped"))?;
                    let sub = Subscription::raw(name, &fields)?;
                    vm.subscribe("enshwick.on_game_type", sub, handler)
                },
            )?,
        )?;
        let types = self.event_types.clone();
        api.raw_set(
            "game_event_types",
            lua.create_function(move |lua, ()| {
                let list = lua.create_table_with_capacity(types.len(), 0)?;
                for (name, size) in &types {
                    let entry = lua.create_table_with_capacity(0, 2)?;
                    entry.raw_set("name", *name)?;
                    entry.raw_set("size", *size)?;
                    list.raw_push(entry)?;
                }
                Ok(list)
            })?,
        )?;
        let weak = Arc::downgrade(vm);
        api.raw_set(
            "on",
            lua.create_function(move |_, (name, handler): (String, Function)| {
                let vm = weak
                    .upgrade()
                    .ok_or_else(|| mlua::Error::runtime("the mod has stopped"))?;
                vm.on(&name, handler)
            })?,
        )?;
        Ok(())
    }

    /// Puts `enshwick`, `print` and `require` among the globals of `vm`.
    fn globals(&self, vm: &Arc<Vm>, scripts: Scripts) -> mlua::Result<Table> {
        let lua = &vm.lua;
        let scope = Scope {
            inner: Weak::clone(&self.me),
            vm: Arc::downgrade(vm),
        };
        let api = lua.create_table()?;
        for (namespace, opened) in &self.namespaces {
            let table = lua.create_table()?;
            for declared in namespace.functions() {
                let scope = scope.clone();
                let opened = opened.clone();
                let what = format!("enshwick.{}.{}", namespace.name(), declared.name());
                let signature = declared.signature();
                table.raw_set(
                    declared.name(),
                    lua.create_function(move |lua, args: MultiValue| {
                        let values = arguments(lua, &scope, args, signature, &what)?;
                        let answer = opened
                            .call(&declared, values)
                            .map_err(|e| mlua::Error::runtime(format!("{what}: {e}")))?;
                        to_lua(lua, &scope, answer, signature.ret())
                    })?,
                )?;
            }
            table.set_readonly(true);
            api.raw_set(namespace.name(), table)?;
        }
        let settings = scope.clone();
        api.raw_set(
            "settings",
            lua.create_function(move |lua, ()| {
                let inner = settings
                    .inner
                    .upgrade()
                    .ok_or_else(|| mlua::Error::runtime("the mod has stopped"))?;
                let values = inner.settings.load();
                crate::bridge::toml_to_lua(lua, &toml::Value::Table((**values).clone()))
            })?,
        )?;
        self.subscription_functions(vm, &api)?;
        let globals = lua.globals();
        globals.raw_set("enshwick", api.clone())?;
        let host = self.host;
        globals.raw_set(
            "print",
            lua.create_function(move |_, args: MultiValue| {
                let line = args
                    .iter()
                    .map(|v| v.to_string().unwrap_or_else(|_| "?".to_owned()))
                    .collect::<Vec<_>>()
                    .join("\t");
                host.log(Level::INFO, &line);
                Ok(())
            })?,
        )?;
        globals.raw_set(
            "require",
            lua.create_require_function(Requirer::new(scripts))?,
        )?;
        Ok(api)
    }

    /// Runs `body` as a call into the scripts of `lua`, under `budget`
    /// unless an outer call on this thread set an earlier end; `Err` when it
    /// panicked.
    fn enter<R>(&self, lua: &Lua, budget: Duration, body: impl FnOnce() -> R) -> Result<R, ()> {
        let outer = DEADLINE.get();
        let outer_expired = EXPIRED.get();
        let deadline = Instant::now().checked_add(budget);
        let mine = match (outer, deadline) {
            (Some(outer), Some(deadline)) => Some(outer.min(deadline)),
            (outer, deadline) => outer.or(deadline),
        };
        DEADLINE.set(mine);
        self.calls.fetch_add(1, Ordering::Relaxed);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(body));
        DEADLINE.set(outer);
        EXPIRED.set(outer_expired);
        self.peak.fetch_max(lua.used_memory(), Ordering::Relaxed);
        result.map_err(|_| ())
    }

    /// Runs `body` on `vm` as a call into the scripts after start; on an
    /// error or a panic, turns the mod off with why.
    fn call<R>(
        &self,
        vm: &Vm,
        what: &dyn Fn() -> String,
        body: impl FnOnce(&Vm) -> mlua::Result<R>,
    ) -> Option<R> {
        if self.off.load(Ordering::Acquire) {
            return None;
        }
        match self.enter(&vm.lua, self.limits.call, || body(vm)) {
            Ok(Ok(value)) => Some(value),
            Ok(Err(error)) => {
                self.fail(&format!("{}: {}", what(), describe(&error)));
                None
            }
            Err(()) => {
                self.fail(&format!("{}: a call it made panicked", what()));
                None
            }
        }
    }

    /// Turns the mod off for `reason`, once, and drops its VM; the loader's
    /// fault logs the reason.
    #[cold]
    fn fail(&self, reason: &str) {
        if !self.off.swap(true, Ordering::AcqRel) {
            self.vm.store(None);
            self.host.fault(reason);
        }
    }

    fn dispatch(&self, kind: &'static EventKind, raw: *const core::ffi::c_void) {
        if self.off.load(Ordering::Acquire) {
            return;
        }
        // SAFETY: `start` subscribed this closure with `Load::on_raw_event`
        // for `kind`'s event alone, and the loader delivers that event's data
        // for the call with no other thread writing it, as `read` asks.
        let Some(data) = (unsafe { kind.read(raw) }) else {
            return;
        };
        self.deliver(&Subscription::Loader(kind.event()), |lua, scope| {
            to_lua(lua, scope, data.clone(), kind.data())
        });
    }

    /// Runs the handlers of the point `sub` on its `input`, the answer so
    /// far `answer`, each seeing the answer the one before left; an error
    /// turns the mod off and leaves the answer as it was.
    fn run_point(
        &self,
        sub: &Subscription,
        desc: &PointDesc,
        input: &[u8],
        answer: &[u8],
    ) -> DescribedVerdict {
        if self.off.load(Ordering::Acquire) {
            return DescribedVerdict::Pass;
        }
        let Some(vm) = self.vm.load_full() else {
            return DescribedVerdict::Pass;
        };
        let handlers: Vec<Function> = vm
            .handlers
            .relock()
            .iter()
            .filter(|(key, _)| key == sub)
            .map(|(_, handler)| handler.clone())
            .collect();
        let input_read = Read::Struct(desc.input.clone());
        let mut value = answer.to_vec();
        let mut set = false;
        for handler in handlers {
            let outcome = self.call(&vm, &|| sub.name(), |vm| {
                let lua_input = decode(&vm.lua, input, 0, &input_read)?;
                let lua_answer = decode(&vm.lua, &value, 0, &desc.output)?;
                let result: LuaValue = handler.call((lua_input, lua_answer))?;
                if let (LuaValue::Table(t), Some(refuse)) = (&result, vm.refuse.get())
                    && t == refuse
                {
                    return Ok(None);
                }
                if result.is_nil() {
                    return Ok(Some(false));
                }
                let mut next = value.clone();
                encode(&result, &desc.output, &mut next, 0)?;
                value = next;
                Ok(Some(true))
            });
            match outcome {
                None => return DescribedVerdict::Pass,
                Some(None) => return DescribedVerdict::Refuse,
                Some(Some(changed)) => set |= changed,
            }
        }
        if set {
            DescribedVerdict::Set(value)
        } else {
            DescribedVerdict::Pass
        }
    }

    /// Calls every handler of `sub` in the current VM with the argument
    /// `arg` makes; stops at the first that fails, which turns the mod off.
    fn deliver(&self, sub: &Subscription, arg: impl Fn(&Lua, &Scope) -> mlua::Result<LuaValue>) {
        if self.off.load(Ordering::Acquire) {
            return;
        }
        let Some(vm) = self.vm.load_full() else {
            return;
        };
        let handlers: Vec<Function> = vm
            .handlers
            .relock()
            .iter()
            .filter(|(key, _)| key == sub)
            .map(|(_, handler)| handler.clone())
            .collect();
        let scope = Scope {
            inner: Weak::clone(&self.me),
            vm: Arc::downgrade(&vm),
        };
        for handler in handlers {
            let done = self.call(&vm, &|| sub.name(), |vm| {
                handler.call::<()>(arg(&vm.lua, &scope)?)
            });
            if done.is_none() {
                return;
            }
        }
    }
}
