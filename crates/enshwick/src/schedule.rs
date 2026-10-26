//! Work for the server's game thread: on the next step, after a delay, or
//! repeatedly. Tasks run after a simulation step, where the game's own server
//! code runs, so they may call into the game.
//!
//! Delays and periods are wall-clock time: after a stall, a task whose time
//! passed runs on the next step. A wait that should stand still with the
//! server counts on the [`StepClock`](crate::time::StepClock) instead. A
//! panic in a task turns the mod off, as in any event handler.

use std::ops::ControlFlow;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::abi::ModHandle;
use crate::locks::Relock;

/// A task: when it is due, its period when it repeats, and what it runs.
struct Task {
    due: Instant,
    period: Option<Duration>,
    run: Box<dyn FnMut() -> ControlFlow<()> + Send>,
}

struct Queue {
    /// Whether a tick handler runs the queue.
    ticking: AtomicBool,
    /// When the earliest task is due, in nanoseconds from `epoch`, or
    /// [`IDLE`]; written under the `tasks` lock, so a step with nothing due
    /// takes no lock.
    next_due: AtomicU64,
    epoch: Instant,
    tasks: Mutex<Vec<Task>>,
    /// The buffer due tasks run from, kept between runs so a run does not
    /// allocate.
    spare: Mutex<Vec<Task>>,
}

const IDLE: u64 = u64::MAX;

impl Queue {
    fn new() -> Self {
        Self {
            ticking: AtomicBool::new(false),
            next_due: AtomicU64::new(IDLE),
            epoch: Instant::now(),
            tasks: Mutex::new(Vec::new()),
            spare: Mutex::new(Vec::new()),
        }
    }

    fn nanos(&self, at: Instant) -> u64 {
        u64::try_from(at.saturating_duration_since(self.epoch).as_nanos()).unwrap_or(IDLE - 1)
    }

    /// Lowers `next_due` to `due`; called under the `tasks` lock.
    fn due_by(&self, due: Instant) {
        self.next_due.fetch_min(self.nanos(due), Ordering::AcqRel);
    }
}

/// Each mod's queue by its handle: a mod DLL holds one, the loader's own copy
/// of this crate one per data pack or script it runs. Queues are kept for
/// the process, so a task still running in one never loses it.
static QUEUES: crate::fresh::Fresh<Mutex<Vec<(ModHandle, &'static Queue)>>> =
    crate::fresh::Fresh::new();

/// Puts work on the server's game thread; get it from
/// [`Load::scheduler`](crate::Load::scheduler). Copy it anywhere, including
/// other threads.
///
/// # Examples
///
/// ```
/// use std::ops::ControlFlow;
/// use std::time::Duration;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let scheduler = load.scheduler()?;
///     scheduler.after(Duration::from_secs(5), || enshwick::info!("five seconds in"));
///     scheduler.every(Duration::from_secs(60), || {
///         enshwick::info!("a minute passed");
///         ControlFlow::Continue(())
///     });
///     Ok(())
/// }
/// ```
#[derive(Clone, Copy)]
pub struct Scheduler {
    queue: &'static Queue,
}

impl crate::script::Bound for Scheduler {
    const NAME: &'static str = "scheduler";
    const DOC: &'static str =
        "Work for the server's game thread: on the next step, after a delay, or repeatedly.";
    const PARTS: &'static [&'static [crate::script::Function<Self>]] = &[Self::SCRIPT_TASKS];

    fn resolve(load: &mut crate::Load<'_>) -> Result<Self, crate::Off> {
        load.scheduler()
    }
}

impl core::fmt::Debug for Scheduler {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Scheduler")
            .field(
                "pending",
                &(self.queue.next_due.load(Ordering::Relaxed) != IDLE),
            )
            .finish()
    }
}

#[crate::script(tasks)]
impl Scheduler {
    /// The scheduler of the mod `me`, and whether the caller is to
    /// subscribe the tick that runs it: true once, and again after
    /// [`Scheduler::unsubscribed`].
    pub(crate) fn get(me: ModHandle) -> (Self, bool) {
        let queue = {
            let mut queues = QUEUES.get_or_init(|| Mutex::new(Vec::new())).relock();
            if let Some(&(_, queue)) = queues.iter().find(|(handle, _)| *handle == me) {
                queue
            } else {
                let queue: &'static Queue = Box::leak(Box::new(Queue::new()));
                queues.push((me, queue));
                queue
            }
        };
        let subscribe = !queue.ticking.swap(true, Ordering::AcqRel);
        (Self { queue }, subscribe)
    }

    /// The tick could not be subscribed; the next [`Scheduler::get`] asks
    /// for it again.
    pub(crate) fn unsubscribed(self) {
        self.queue.ticking.store(false, Ordering::Release);
    }

    /// Runs `task` after the next simulation step.
    ///
    /// # Examples
    ///
    /// See [`Scheduler`].
    #[script]
    pub fn next_tick(self, task: impl FnOnce() + Send + 'static) {
        self.after(Duration::ZERO, task);
    }

    /// Runs `task` after the first step at least `delay` from now.
    ///
    /// # Examples
    ///
    /// See [`Scheduler`].
    #[script]
    pub fn after(self, delay: Duration, task: impl FnOnce() + Send + 'static) {
        let mut task = Some(task);
        self.push(delay, None, move || {
            if let Some(task) = task.take() {
                task();
            }
            ControlFlow::Break(())
        });
    }

    /// Runs `task` about every `period`, the first time a period from now,
    /// until it returns [`ControlFlow::Break`], which a script's task does by
    /// returning `false`.
    ///
    /// # Examples
    ///
    /// See [`Scheduler`].
    #[script]
    pub fn every(self, period: Duration, task: impl FnMut() -> ControlFlow<()> + Send + 'static) {
        self.push(period, Some(period), task);
    }

    fn push(
        self,
        delay: Duration,
        period: Option<Duration>,
        run: impl FnMut() -> ControlFlow<()> + Send + 'static,
    ) {
        // A delay past what `Instant` holds is never due.
        let Some(due) = Instant::now().checked_add(delay) else {
            return;
        };
        let task = Task {
            due,
            period,
            run: Box::new(run),
        };
        let mut tasks = self.queue.tasks.relock();
        self.queue.due_by(task.due);
        tasks.push(task);
    }

    /// Runs the tasks that are due; called after each step.
    #[inline]
    pub(crate) fn run_due(self) {
        let next_due = self.queue.next_due.load(Ordering::Acquire);
        if next_due == IDLE {
            return;
        }
        let now = Instant::now();
        if self.queue.nanos(now) >= next_due {
            self.run_due_at(now);
        }
    }

    #[cold]
    fn run_due_at(self, now: Instant) {
        let mut due = core::mem::take(&mut *self.queue.spare.relock());
        {
            let mut tasks = self.queue.tasks.relock();
            due.extend(tasks.extract_if(.., |task| task.due <= now));
            let next = tasks.iter().map(|task| task.due).min();
            self.queue.next_due.store(
                next.map_or(IDLE, |at| self.queue.nanos(at)),
                Ordering::Release,
            );
        }
        // Outside the lock: a task may schedule more.
        due.retain_mut(|task| match ((task.run)(), task.period) {
            (ControlFlow::Continue(()), Some(period)) => match now.checked_add(period) {
                Some(at) => {
                    task.due = at;
                    true
                }
                None => false,
            },
            _ => false,
        });
        {
            let mut tasks = self.queue.tasks.relock();
            for task in &due {
                self.queue.due_by(task.due);
            }
            tasks.append(&mut due);
        }
        *self.queue.spare.relock() = due;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::AtomicU32;

    use super::*;

    #[test]
    fn tasks_run_when_due_and_repeat_until_they_stop() {
        let (scheduler, _) = Scheduler::get(ModHandle(1));
        let once = Arc::new(AtomicU32::new(0));
        let repeated = Arc::new(AtomicU32::new(0));
        let later = Arc::new(AtomicU32::new(0));
        let count = Arc::clone(&once);
        scheduler.next_tick(move || {
            count.fetch_add(1, Ordering::Relaxed);
        });
        let count = Arc::clone(&repeated);
        scheduler.every(Duration::ZERO, move || {
            if count.fetch_add(1, Ordering::Relaxed) + 1 < 3 {
                ControlFlow::Continue(())
            } else {
                ControlFlow::Break(())
            }
        });
        let count = Arc::clone(&later);
        scheduler.after(Duration::from_secs(3600), move || {
            count.fetch_add(1, Ordering::Relaxed);
        });
        for _ in 0..5 {
            scheduler.run_due();
        }
        assert_eq!(once.load(Ordering::Relaxed), 1);
        assert_eq!(repeated.load(Ordering::Relaxed), 3);
        assert_eq!(later.load(Ordering::Relaxed), 0);
        assert_ne!(scheduler.queue.next_due.load(Ordering::Relaxed), IDLE);
    }

    #[test]
    fn runs_keep_their_buffers_between_steps() {
        let (scheduler, _) = Scheduler::get(ModHandle(1));
        scheduler.every(Duration::ZERO, || ControlFlow::Continue(()));
        scheduler.run_due();
        let lock = |m: &'static Mutex<Vec<Task>>| m.relock();
        let spare = lock(&scheduler.queue.spare).capacity();
        let tasks = lock(&scheduler.queue.tasks).capacity();
        assert!(spare >= 1);
        scheduler.run_due();
        assert_eq!(lock(&scheduler.queue.spare).capacity(), spare);
        assert_eq!(lock(&scheduler.queue.tasks).capacity(), tasks);
        assert_eq!(lock(&scheduler.queue.tasks).len(), 1);
    }

    #[test]
    fn each_mod_has_a_queue_of_its_own() {
        let (first, subscribe_first) = Scheduler::get(ModHandle(7));
        let (second, subscribe_second) = Scheduler::get(ModHandle(8));
        assert!(subscribe_first && subscribe_second);
        assert!(!core::ptr::eq(first.queue, second.queue));
        let (again, subscribe_again) = Scheduler::get(ModHandle(7));
        assert!(core::ptr::eq(first.queue, again.queue));
        assert!(!subscribe_again);
    }

    #[test]
    fn a_delay_past_what_time_holds_never_runs() {
        let (scheduler, _) = Scheduler::get(ModHandle(1));
        scheduler.after(Duration::MAX, || unreachable!());
        scheduler.every(Duration::MAX, || unreachable!());
        scheduler.run_due();
    }
}
