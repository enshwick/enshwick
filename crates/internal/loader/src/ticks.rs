//! Tick handlers on a period of wall time, and the time of the server's
//! steps: a step looks at each handler's due time first, so a step a
//! handler skips never enters its mod.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use arc_swap::ArcSwapOption;
use enshwick::abi::{EventFn, Status, TickData, TickHandle};

use crate::locks::Relock;
use crate::mod_index::ModIndex;

/// The loader's periodic tick handlers and step times.
pub static TICKS: Ticks = Ticks::new();

/// The step time average's weight: a new step counts for 1/16 of it.
const AVERAGE_SHIFT: u32 = 4;

/// Whether a handler last called at `last` (`None` before its first call)
/// is due at `now` on a period of `every`, all in nanoseconds; a period of
/// 0 is due on every step.
///
/// # Examples
///
/// ```
/// use enshwick_loader::ticks::due;
///
/// assert!(due(5, None, 100));
/// assert!(!due(150, Some(100), 100));
/// assert!(due(200, Some(100), 100));
/// ```
#[must_use]
#[inline]
pub const fn due(now: u64, last: Option<u64>, every: u64) -> bool {
    match last {
        None => true,
        Some(last) => now >= last.saturating_add(every),
    }
}

/// One periodic tick handler, leaked once and never freed, alone on its
/// cache lines: the game thread writes its last call while any thread may
/// set its period.
#[derive(Debug)]
#[repr(align(128))]
pub struct Tick {
    owner: ModIndex,
    /// The owner's reload generation when it subscribed; a handler of an
    /// older one never runs.
    generation: u32,
    callback: EventFn,
    /// The mod's pointer for the handler, as an address.
    user: usize,
    every: AtomicU64,
    /// The step of the last call; 0 before the first, since steps count
    /// from 1.
    last_step: AtomicU64,
    /// The time of the last call, in the loader's step nanoseconds.
    last_nanos: AtomicU64,
    /// The owner's timed event calls and their ticks at the last call, so
    /// the next call's cost covers the calls in between.
    seen_calls: AtomicU64,
    seen_ticks: AtomicU64,
}

impl Tick {
    /// The mod that subscribed it.
    #[must_use]
    pub const fn owner(&self) -> ModIndex {
        self.owner
    }

    /// The owner's reload generation when it subscribed.
    #[must_use]
    pub const fn generation(&self) -> u32 {
        self.generation
    }

    /// The handler, inside the owner's DLL.
    #[must_use]
    pub const fn callback(&self) -> EventFn {
        self.callback
    }

    /// The pointer the owner subscribed the handler with.
    #[must_use]
    pub fn user(&self) -> *mut core::ffi::c_void {
        core::ptr::with_exposed_provenance_mut(self.user)
    }

    /// The owner's event calls and their ticks since the last time this
    /// was asked, from its running totals `calls` and `ticks`.
    pub fn since_seen(&self, calls: u64, ticks: u64) -> (u64, u64) {
        let seen_calls = self.seen_calls.swap(calls, Ordering::Relaxed);
        let seen_ticks = self.seen_ticks.swap(ticks, Ordering::Relaxed);
        (
            calls.saturating_sub(seen_calls),
            ticks.saturating_sub(seen_ticks),
        )
    }
}

/// How long the last step took and how long steps take lately, in
/// nanoseconds; 0 before a step was measured.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StepTime {
    /// Since the step before.
    pub elapsed: u64,
    /// The average over about the last 16 steps.
    pub average: u64,
}

/// The periodic tick handlers of every mod, and the time of the steps they
/// run after.
#[derive(Debug)]
pub struct Ticks {
    /// Whether a handler is listed, so a step without one pays one load.
    listed: AtomicBool,
    list: ArcSwapOption<Vec<&'static Tick>>,
    /// Takes additions in turn, so each gets its own handle.
    adding: Mutex<()>,
    epoch: OnceLock<Instant>,
    /// The time of the last step plus one; 0 before the first.
    previous: AtomicU64,
    average: AtomicU64,
}

impl Default for Ticks {
    fn default() -> Self {
        Self::new()
    }
}

impl Ticks {
    /// No handler and no step yet.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            listed: AtomicBool::new(false),
            list: ArcSwapOption::const_empty(),
            adding: Mutex::new(()),
            epoch: OnceLock::new(),
            previous: AtomicU64::new(0),
            average: AtomicU64::new(0),
        }
    }

    /// Whether any mod subscribed a periodic handler.
    #[must_use]
    #[inline]
    pub fn listed(&self) -> bool {
        self.listed.load(Ordering::Relaxed)
    }

    /// `at` in nanoseconds from the first time asked.
    #[must_use]
    pub fn nanos(&self, at: Instant) -> u64 {
        let epoch = *self.epoch.get_or_init(|| at);
        u64::try_from(at.saturating_duration_since(epoch).as_nanos()).unwrap_or(u64::MAX - 1)
    }

    /// Counts a step that ran at `now` and says how long it took, and how
    /// long steps take lately; on the game thread after each step.
    pub fn advance(&self, now: u64) -> StepTime {
        let previous = self.previous.swap(now.saturating_add(1), Ordering::Relaxed);
        if previous == 0 {
            return StepTime::default();
        }
        let elapsed = now.saturating_sub(previous - 1);
        let average = match self.average.load(Ordering::Relaxed) {
            0 => elapsed,
            average => average - (average >> AVERAGE_SHIFT) + (elapsed >> AVERAGE_SHIFT),
        };
        self.average.store(average, Ordering::Relaxed);
        StepTime { elapsed, average }
    }

    /// Lists a handler of mod `owner` at its reload `generation`, due on
    /// the next step and then every `every` nanoseconds; `None` past
    /// `u32::MAX` handlers.
    pub fn add(
        &self,
        owner: ModIndex,
        generation: u32,
        every: u64,
        callback: EventFn,
        user: *mut core::ffi::c_void,
    ) -> Option<TickHandle> {
        let _turn = self.adding.relock();
        let mut list = self
            .list
            .load_full()
            .map_or_else(Vec::new, |l| Vec::clone(&l));
        let handle = TickHandle(u32::try_from(list.len()).ok()?);
        list.push(Box::leak(Box::new(Tick {
            owner,
            generation,
            callback,
            user: user.expose_provenance(),
            every: AtomicU64::new(every),
            last_step: AtomicU64::new(0),
            last_nanos: AtomicU64::new(0),
            seen_calls: AtomicU64::new(0),
            seen_ticks: AtomicU64::new(0),
        })));
        self.list.store(Some(list.into()));
        self.listed.store(true, Ordering::Release);
        Some(handle)
    }

    /// Sets the period of handler `handle` for mod `owner` at its reload
    /// `generation`: [`Status::REFUSED`] for another mod's handler,
    /// [`Status::MISSING`] for no handler or one of an older generation.
    pub fn set_every(
        &self,
        handle: TickHandle,
        owner: ModIndex,
        generation: u32,
        every: u64,
    ) -> Status {
        let list = self.list.load();
        let Some(&tick) = list.as_deref().and_then(|l| l.get(handle.0 as usize)) else {
            return Status::MISSING;
        };
        if tick.owner != owner {
            return Status::REFUSED;
        }
        if tick.generation != generation {
            return Status::MISSING;
        }
        tick.every.store(every, Ordering::Relaxed);
        Status::OK
    }

    /// Calls each due handler for step `step` at `now` through `call`,
    /// with the steps and time since its last call, once `live` says its
    /// mod runs; returns how many ran. A handler not due costs its loads
    /// and its comparison only.
    pub fn run(
        &self,
        step: u64,
        now: u64,
        time: StepTime,
        live: impl Fn(&Tick) -> bool,
        mut call: impl FnMut(&Tick, TickData),
    ) -> usize {
        if !self.listed() {
            return 0;
        }
        let handlers = self.list.load();
        let Some(handlers) = handlers.as_deref() else {
            return 0;
        };
        let mut ran = 0;
        for &tick in handlers {
            let last_step = tick.last_step.load(Ordering::Relaxed);
            let last = (last_step != 0).then(|| tick.last_nanos.load(Ordering::Relaxed));
            if !due(now, last, tick.every.load(Ordering::Relaxed)) {
                continue;
            }
            if !live(tick) {
                // A mod that is off or reloaded never takes this handler
                // back, so it stops being due instead of being checked on
                // every step.
                tick.every.store(u64::MAX, Ordering::Relaxed);
                tick.last_step.store(step.max(1), Ordering::Relaxed);
                tick.last_nanos.store(now, Ordering::Relaxed);
                continue;
            }
            tick.last_step.store(step, Ordering::Relaxed);
            tick.last_nanos.store(now, Ordering::Relaxed);
            let data = TickData {
                elapsed_steps: if last_step == 0 { 1 } else { step - last_step },
                elapsed_nanos: last.map_or(time.elapsed, |last| now.saturating_sub(last)),
                step_nanos: time.average,
                ..TickData::new(step)
            };
            call(tick, data);
            ran += 1;
        }
        ran
    }
}

#[cfg(test)]
mod tests {
    use core::ffi::c_void;

    use enshwick::abi::Event;

    use super::*;

    unsafe extern "system" fn nothing(_: Event, _: *const c_void, _: *mut c_void) {}

    const MS: u64 = 1_000_000;

    fn listed(every: &[u64]) -> Ticks {
        let ticks = Ticks::new();
        for (owner, &every) in every.iter().enumerate() {
            let added = ticks.add(
                ModIndex::new(owner),
                0,
                every,
                nothing,
                core::ptr::null_mut(),
            );
            assert_eq!(added, Some(TickHandle(u32::try_from(owner).unwrap())));
        }
        ticks
    }

    /// Runs `count` steps of `step` nanoseconds, after `start` steps, and
    /// returns each call as `(owner, step, elapsed steps, elapsed time)`.
    fn steps(ticks: &Ticks, start: u64, count: u64, step: u64) -> Vec<(usize, u64, u64, u64)> {
        let mut calls = Vec::new();
        for n in start + 1..=start + count {
            let time = ticks.advance(n * step);
            let _ran = ticks.run(
                n,
                n * step,
                time,
                |_| true,
                |tick, data| {
                    calls.push((
                        tick.owner().get(),
                        data.step,
                        data.elapsed_steps,
                        data.elapsed_nanos,
                    ));
                },
            );
        }
        calls
    }

    #[test]
    fn a_handler_runs_on_its_first_step_then_once_its_period_passed() {
        let ticks = listed(&[100 * MS]);
        assert_eq!(
            steps(&ticks, 0, 7, 40 * MS),
            [(0, 1, 1, 0), (0, 4, 3, 120 * MS), (0, 7, 3, 120 * MS)]
        );
    }

    #[test]
    fn a_first_call_after_steps_ran_carries_the_step_before() {
        let ticks = Ticks::new();
        let _before = steps(&ticks, 0, 3, 40 * MS);
        let _added = ticks.add(ModIndex::new(0), 0, MS, nothing, core::ptr::null_mut());
        assert_eq!(steps(&ticks, 3, 1, 40 * MS), [(0, 4, 1, 40 * MS)]);
    }

    #[test]
    fn a_period_of_zero_runs_on_every_step() {
        let ticks = listed(&[0]);
        assert_eq!(
            steps(&ticks, 0, 3, 50 * MS),
            [(0, 1, 1, 0), (0, 2, 1, 50 * MS), (0, 3, 1, 50 * MS)]
        );
    }

    #[test]
    fn a_longer_period_counts_from_the_last_call() {
        let ticks = listed(&[100 * MS]);
        let _first = steps(&ticks, 0, 1, 50 * MS);
        assert_eq!(
            ticks.set_every(TickHandle(0), ModIndex::new(0), 0, 300 * MS),
            Status::OK
        );
        assert_eq!(steps(&ticks, 1, 6, 50 * MS), [(0, 7, 6, 300 * MS)]);
    }

    #[test]
    fn a_shorter_period_that_already_passed_runs_on_the_next_step() {
        let ticks = listed(&[1_000 * MS]);
        let _first = steps(&ticks, 0, 4, 50 * MS);
        let _set = ticks.set_every(TickHandle(0), ModIndex::new(0), 0, 100 * MS);
        assert_eq!(steps(&ticks, 4, 1, 50 * MS), [(0, 5, 4, 200 * MS)]);
    }

    #[test]
    fn a_handler_of_a_mod_not_running_is_skipped_and_never_due_again() {
        let ticks = listed(&[0]);
        let ran = ticks.run(1, 0, StepTime::default(), |_| false, |_, _| unreachable!());
        assert_eq!(ran, 0);
        let checked = core::cell::Cell::new(0);
        let ran = ticks.run(
            2,
            10,
            StepTime::default(),
            |_| {
                checked.set(checked.get() + 1);
                true
            },
            |_, _| unreachable!(),
        );
        assert_eq!(
            (ran, checked.get()),
            (0, 0),
            "a dead handler costs no check"
        );
    }

    #[test]
    fn another_mod_s_handler_is_refused_and_an_older_copy_s_missing() {
        let ticks = listed(&[0, 0]);
        let set = |handle, owner, generation| {
            ticks.set_every(TickHandle(handle), ModIndex::new(owner), generation, 5)
        };
        assert_eq!(set(1, 0, 0), Status::REFUSED);
        assert_eq!(set(0, 0, 1), Status::MISSING);
        assert_eq!(set(2, 0, 0), Status::MISSING);
        assert_eq!(set(1, 1, 0), Status::OK);
    }

    #[test]
    fn nothing_listed_runs_nothing() {
        let ticks = Ticks::new();
        assert!(!ticks.listed());
        assert_eq!(
            ticks.run(1, 0, StepTime::default(), |_| true, |_, _| unreachable!()),
            0
        );
    }

    #[test]
    fn the_step_time_averages_about_sixteen_steps() {
        let ticks = Ticks::new();
        assert_eq!(ticks.advance(0), StepTime::default(), "the first step");
        assert_eq!(
            ticks.advance(16 * MS).average,
            16 * MS,
            "seeded by the first"
        );
        let mut now = 16 * MS;
        let mut time = StepTime::default();
        for _ in 0..64 {
            now += 48 * MS;
            time = ticks.advance(now);
        }
        assert_eq!(time.elapsed, 48 * MS);
        assert!(time.average.abs_diff(48 * MS) < MS, "{}", time.average);
    }

    #[test]
    fn the_cost_window_covers_the_calls_since_the_last_look() {
        let ticks = listed(&[0]);
        let list = ticks.list.load();
        let tick = list.as_deref().unwrap()[0];
        assert_eq!(tick.since_seen(10, 1_000), (10, 1_000));
        assert_eq!(tick.since_seen(14, 1_400), (4, 400));
    }
}
