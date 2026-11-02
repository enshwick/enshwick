//! The sandbox a mod's scripts run in and the budgets of each call into
//! them: time, memory, and the fault that turns the mod off.

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use enshwick::packs::SCRIPT_MAIN as MAIN;
use mlua::chunk::ChunkMode;
use mlua::{Lua, LuaOptions, StdLib, VmState};

use super::{Inner, Limits, StartError, Vm};
use crate::files::Scripts;

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
pub(super) fn describe(error: &mlua::Error) -> String {
    match error {
        mlua::Error::CallbackError { traceback, cause } => format!("{cause}\n{traceback}"),
        mlua::Error::MemoryError(_) => "the script ran out of its memory budget".to_owned(),
        other => other.to_string(),
    }
}

impl Inner {
    /// A new VM with the API and `scripts`, after its `main.luau` ran;
    /// the error names the file, the line and the message.
    pub(super) fn run_main(&self, scripts: Scripts) -> Result<Arc<Vm>, StartError> {
        let main = scripts.get(MAIN).cloned().ok_or(StartError::Missing)?;
        let lua = sandbox(self.limits, &self.overruns).map_err(StartError::Sandbox)?;
        let vm = Arc::new(Vm {
            lua,
            scripts: scripts.clone(),
            starting: AtomicBool::new(true),
            handlers: Mutex::new(Vec::new()),
            points: Mutex::new(Vec::new()),
            events: Mutex::new(Vec::new()),
            refuse: OnceLock::new(),
        });
        let api = self.globals(&vm, scripts).map_err(StartError::Api)?;
        vm.lua.sandbox(true).map_err(StartError::Sandbox)?;
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
            Ok(Err(error)) => Err(StartError::Script(error)),
            Err(()) => Err(StartError::Panicked),
        }
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
    pub(super) fn call<R>(
        &self,
        vm: &Vm,
        what: &dyn Fn() -> String,
        body: impl FnOnce(&Vm) -> mlua::Result<R>,
    ) -> Option<R> {
        if self.off.load(Ordering::Acquire) {
            return None;
        }
        // A call inside another call counts in the outer one's time.
        let outermost = DEADLINE.get().is_none();
        let began = Instant::now();
        match self.enter(&vm.lua, self.limits.call, || body(vm)) {
            Ok(Ok(value)) => {
                if outermost {
                    self.charge(began, what);
                }
                Some(value)
            }
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

    /// Adds the call that began at `began` to the second's script time, and
    /// turns the mod off when the second's time passes its budget.
    fn charge(&self, began: Instant, what: &dyn Fn() -> String) {
        let now = Instant::now();
        let spent = self.second.charge(
            now.saturating_duration_since(self.epoch),
            now.saturating_duration_since(began),
        );
        if spent > self.limits.per_second {
            self.fail(&format!(
                "{}: the scripts ran {} ms within one second, past their budget of {} ms per second",
                what(),
                spent.as_millis(),
                self.limits.per_second.as_millis()
            ));
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
}
