//! One mod's VM: the sandbox, the budgets, the calls in and out, and what
//! turns the mod off.

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant};

use enshwick::abi::{Event, Level};
use enshwick::script::{self, EventKind, Signature, Value};
use enshwick::{Host, Load, Off};
use mlua::{Function, Lua, LuaOptions, MultiValue, StdLib, Table, Value as LuaValue, VmState};

use crate::bridge::{arguments, from_lua, to_lua};
use crate::files::{MAIN, Requirer, Scripts};

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

/// Takes a lock under the API's poison policy: a lock a panicking thread
/// held is taken again with its state kept and its poison cleared.
trait Relock<T> {
    fn relock(&self) -> MutexGuard<'_, T>;
}

impl<T> Relock<T> for Mutex<T> {
    fn relock(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|poisoned| {
            self.clear_poison();
            poisoned.into_inner()
        })
    }
}

/// How many of Luau's interrupts pass between two reads of the clock.
const TICKS_PER_CHECK: u32 = 64;

pub(crate) struct Inner {
    id: String,
    host: Host,
    lua: Lua,
    limits: Limits,
    off: AtomicBool,
    starting: AtomicBool,
    handlers: Mutex<Vec<(Event, Function)>>,
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
///     let runtime = enshwick_luau::start(load, "you.hello", Scripts::new().with("main.luau", ""), Limits::default())?;
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
        let memory = self.0.lua.used_memory();
        Usage {
            memory,
            peak: self.0.peak.load(Ordering::Relaxed).max(memory),
            calls: self.0.calls.load(Ordering::Relaxed),
            overruns: self.0.overruns.load(Ordering::Relaxed),
        }
    }

    /// Whether a fault turned the mod off.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_luau::{Limits, Scripts};
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let runtime = enshwick_luau::start(load, "you.hello", Scripts::new().with("main.luau", ""), Limits::default())?;
    ///     assert!(!runtime.is_off());
    ///     Ok(())
    /// }
    /// ```
    #[must_use]
    pub fn is_off(&self) -> bool {
        self.0.off.load(Ordering::Acquire)
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
    limits: Limits,
) -> Result<Runtime, Off> {
    let main = scripts
        .get(MAIN)
        .cloned()
        .ok_or_else(|| Off::new(format!("{MAIN} is missing")))?;
    let lua = sandbox(limits).map_err(|e| Off::new(format!("the Luau sandbox: {e}")))?;
    let overruns = Arc::new(AtomicU64::new(0));
    lua.set_interrupt({
        let overruns = Arc::clone(&overruns);
        move |_| interrupt(&overruns)
    });
    let inner = Arc::new(Inner {
        id: id.to_owned(),
        host: load.host(),
        lua,
        limits,
        off: AtomicBool::new(false),
        starting: AtomicBool::new(true),
        handlers: Mutex::new(Vec::new()),
        calls: AtomicU64::new(0),
        overruns,
        peak: AtomicUsize::new(0),
    });
    let api = globals(&inner, load, scripts).map_err(|e| Off::new(format!("the Luau API: {e}")))?;
    inner
        .lua
        .sandbox(true)
        .map_err(|e| Off::new(format!("the Luau sandbox: {e}")))?;
    let started = inner.enter(limits.start, || {
        inner
            .lua
            .load(&main[..])
            .set_name(format!("@{MAIN}"))
            .exec()
    });
    inner.starting.store(false, Ordering::Release);
    match started {
        Ok(Ok(())) => {}
        Ok(Err(error)) => return Err(Off::new(describe(&error))),
        Err(()) => return Err(Off::new(format!("{MAIN} panicked in a call it made"))),
    }
    api.set_readonly(true);
    subscribe(&inner, load)?;
    Ok(Runtime(inner))
}

/// A VM with Luau's safe libraries, `print` and `require` of the mod's own,
/// and the memory limit; the caller turns the sandbox on once the globals
/// are in place.
fn sandbox(limits: Limits) -> mlua::Result<Lua> {
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

/// Puts `enshwick`, `print` and `require` among the globals.
fn globals(inner: &Arc<Inner>, load: &mut Load<'_>, scripts: Scripts) -> mlua::Result<Table> {
    let lua = &inner.lua;
    let api = lua.create_table()?;
    for namespace in script::NAMESPACES {
        let opened = match namespace.open(load) {
            Ok(opened) => opened,
            Err(off) => {
                inner.host.log(
                    Level::INFO,
                    &format!(
                        "enshwick.{} is not on this game build: {off}",
                        namespace.name()
                    ),
                );
                continue;
            }
        };
        let table = lua.create_table()?;
        for (index, declared) in namespace.functions().into_iter().enumerate() {
            let weak = Arc::downgrade(inner);
            let opened = opened.clone();
            let what = format!("enshwick.{}.{}", namespace.name(), declared.name());
            let signature = declared.signature();
            table.raw_set(
                declared.name(),
                lua.create_function(move |lua, args: MultiValue| {
                    let values = arguments(lua, &weak, args, signature, &what)?;
                    let answer = opened
                        .call(index, values)
                        .map_err(|e| mlua::Error::runtime(format!("{what}: {e}")))?;
                    to_lua(lua, &weak, answer, signature.ret())
                })?,
            )?;
        }
        table.set_readonly(true);
        api.raw_set(namespace.name(), table)?;
    }
    let weak = Arc::downgrade(inner);
    api.raw_set(
        "on",
        lua.create_function(move |_, (name, handler): (String, Function)| {
            let inner = weak
                .upgrade()
                .ok_or_else(|| mlua::Error::runtime("the mod has stopped"))?;
            inner.on(&name, handler)
        })?,
    )?;
    let globals = lua.globals();
    globals.raw_set("enshwick", api.clone())?;
    let host = inner.host;
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

/// Subscribes each event a handler was registered for, once.
fn subscribe(inner: &Arc<Inner>, load: &mut Load<'_>) -> Result<(), Off> {
    let mut events: Vec<Event> = inner
        .handlers
        .relock()
        .iter()
        .map(|(event, _)| *event)
        .collect();
    events.sort_unstable_by_key(|e| e.0);
    events.dedup();
    for event in events {
        let kind = script::events()
            .find(|k| k.event() == event)
            .ok_or_else(|| Off::new(format!("event {} has no script entry", event.0)))?;
        let inner = Arc::clone(inner);
        load.on_raw_event(event, move |raw| inner.dispatch(kind, raw))?;
    }
    Ok(())
}

impl Inner {
    /// Runs `body` as a call into the scripts, under `budget` unless an
    /// outer call on this thread set an earlier end; `Err` when it panicked.
    fn enter<R>(&self, budget: Duration, body: impl FnOnce() -> R) -> Result<R, ()> {
        let outer = DEADLINE.get();
        let deadline = Instant::now().checked_add(budget);
        let mine = match (outer, deadline) {
            (Some(outer), Some(deadline)) => Some(outer.min(deadline)),
            (outer, deadline) => outer.or(deadline),
        };
        DEADLINE.set(mine);
        self.calls.fetch_add(1, Ordering::Relaxed);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(body));
        DEADLINE.set(outer);
        if outer.is_none() {
            EXPIRED.set(false);
        }
        self.peak
            .fetch_max(self.lua.used_memory(), Ordering::Relaxed);
        result.map_err(|_| ())
    }

    /// Runs `body` as a call into the scripts after start; on an error or a
    /// panic, logs why and turns the mod off.
    fn call<R>(&self, what: &str, body: impl FnOnce() -> mlua::Result<R>) -> Option<R> {
        if self.off.load(Ordering::Acquire) {
            return None;
        }
        match self.enter(self.limits.call, body) {
            Ok(Ok(value)) => Some(value),
            Ok(Err(error)) => {
                self.fail(&format!("{what}: {}", describe(&error)));
                None
            }
            Err(()) => {
                self.fail(&format!("{what}: a call it made panicked"));
                None
            }
        }
    }

    /// Turns the mod off for `reason`, once.
    #[cold]
    fn fail(&self, reason: &str) {
        if !self.off.swap(true, Ordering::AcqRel) {
            self.host.log(Level::ERROR, reason);
            self.host.fault(reason);
        }
    }

    fn on(&self, name: &str, handler: Function) -> mlua::Result<()> {
        if !self.starting.load(Ordering::Acquire) {
            return Err(mlua::Error::runtime(
                "enshwick.on: subscribe at the top level of main.luau, while the mod starts",
            ));
        }
        let kind = script::events().find(|k| k.name() == name).ok_or_else(|| {
            mlua::Error::runtime(format!("enshwick.on: no event is named {name:?}"))
        })?;
        self.handlers.relock().push((kind.event(), handler));
        Ok(())
    }

    fn dispatch(&self, kind: &'static EventKind, raw: *const core::ffi::c_void) {
        if self.off.load(Ordering::Acquire) {
            return;
        }
        // SAFETY: the loader passes `raw` with `kind`'s event, as the
        // subscription in `subscribe` asked for that event alone.
        let Some(data) = (unsafe { kind.read(raw) }) else {
            return;
        };
        let handlers: Vec<Function> = self
            .handlers
            .relock()
            .iter()
            .filter(|(event, _)| *event == kind.event())
            .map(|(_, handler)| handler.clone())
            .collect();
        let weak = Weak::new();
        for handler in handlers {
            let done = self.call(kind.name(), || {
                let arg = to_lua(&self.lua, &weak, data.clone(), kind.data())?;
                handler.call::<()>(arg)
            });
            if done.is_none() {
                return;
            }
        }
    }

    /// Calls the script's `function` from the API with `args`, as a
    /// function of `signature`.
    pub(crate) fn call_function(
        &self,
        function: &Function,
        args: Vec<Value>,
        signature: &'static Signature,
    ) -> Result<Value, script::Error> {
        let weak = Weak::new();
        let answer = self.call("a function passed to the API", || {
            let lua_args = args
                .into_iter()
                .zip(signature.params())
                .map(|(value, param)| to_lua(&self.lua, &weak, value, param.ty()))
                .collect::<mlua::Result<MultiValue>>()?;
            function.call::<LuaValue>(lua_args)
        });
        let answer = answer.ok_or_else(|| script::Error::new("the script failed"))?;
        from_lua(&weak, answer, signature.ret())
    }
}
