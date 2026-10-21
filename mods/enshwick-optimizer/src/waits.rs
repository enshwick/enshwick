//! The two waits the mod changes, both in the engine's event loop, found
//! through [`enshwick_loader_api::pacing`].
//!
//! **Exact waits.** With less than a millisecond before its deadline the
//! event loop waits 0 ms on its handles, again and again, until the deadline
//! passes: the app's main loop for about 0.6 ms of every frame, the
//! session's loop for about 0.9 ms of every 5 ms, and the server loop for
//! 0.1 ms after a slow pass. Each of those waits is a system call on
//! Windows and a call into Wine's synchronisation under Wine. The mod
//! sleeps the time left instead, then lets the game's own zero wait run
//! once: it still sees every event, and the mod waits on no handle of the
//! game's and consumes none. Under Wine the sleep is a plain Unix sleep; on
//! Windows it is a high-resolution timer, which wakes a fraction of a
//! millisecond late whatever the timer resolution.
//!
//! **Server wait cap.** With players online the server's game loop waits a
//! fixed 1 ms after each pass, about 900 passes a second for 60 simulation
//! steps. The cap lengthens that wait up to a limit, never past the next
//! step: the wait wakes up to one timer tick late, so it ends a tick and a
//! margin before the step is due. The idle 40 ms wait and the short wait
//! after a slow pass stay.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::Instant;

use enshwick_loader_api::pacing::{self, LoopWait};
use enshwick_loader_api::{Load, Off};

use crate::sys;

/// One millisecond, the shortest wait the helper can ask for.
pub(crate) const MILLISECOND_NS: u64 = 1_000_000;
/// One simulation step at 60 a second.
pub(crate) const STEP_NS: u64 = 16_666_667;
/// What the server wait cap leaves before the next step.
pub(crate) const STEP_MARGIN_NS: u64 = 500_000;
/// The server loop's wait with players online.
pub(crate) const VANILLA_SERVER_WAIT_NS: u64 = pacing::SERVER_ONLINE_WAIT_NS;

/// Whether the exact waits run.
static EXACT: AtomicBool = AtomicBool::new(false);
/// Whether they sleep on a high-resolution timer, on Windows.
static PRECISE: AtomicBool = AtomicBool::new(false);
/// What a sleep leaves to the clock's granularity, in ns: the timer
/// resolution on Windows, 0 under Wine.
static SLACK_NS: AtomicU64 = AtomicU64::new(0);
/// How late a timed wait of the event loop may wake, in ns: the timer
/// resolution on Windows, a millisecond under Wine.
static WAKE_NS: AtomicU64 = AtomicU64::new(MILLISECOND_NS);
/// The longest server wait with players online, in ns.
static CAP_NS: AtomicU64 = AtomicU64::new(VANILLA_SERVER_WAIT_NS);
/// The clock the hooks read.
static EPOCH: OnceLock<Instant> = OnceLock::new();
/// When the last simulation step ran, ns after [`EPOCH`]; 0 before one.
static LAST_STEP_NS: AtomicU64 = AtomicU64::new(0);

/// Exact waits slept.
pub(crate) static SLEEPS: AtomicU64 = AtomicU64::new(0);
/// Nanoseconds the exact waits slept.
pub(crate) static SLEPT_NS: AtomicU64 = AtomicU64::new(0);
/// Nanoseconds the exact waits woke past the game's deadline.
pub(crate) static LATE_NS: AtomicU64 = AtomicU64::new(0);
/// Passes of the server's game loop.
pub(crate) static PASSES: AtomicU64 = AtomicU64::new(0);
/// Passes whose wait the cap lengthened.
pub(crate) static CAPPED: AtomicU64 = AtomicU64::new(0);
/// Simulation steps.
pub(crate) static STEPS: AtomicU64 = AtomicU64::new(0);
/// The thread of the server's game loop, 0 before its first pass; it follows
/// the loop to a new thread, as when a client hosts another game.
pub(crate) static SERVER_THREAD: AtomicU32 = AtomicU32::new(0);

/// Which waits were found and hooked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Linked {
    /// The wait helper's handle wait.
    pub(crate) exact: bool,
    /// The client's message wait.
    pub(crate) message: bool,
    /// The server loop's wait.
    pub(crate) server: bool,
}

/// How long the exact wait sleeps, in 100 ns units, when the game would wait
/// `ms` with `left_ns` before its deadline: only for a zero wait with less
/// than a millisecond left, and never past the deadline less `slack_ns`.
#[must_use]
pub(crate) fn sleep_ticks(ms: u64, left_ns: i64, slack_ns: u64) -> Option<u64> {
    let left = u64::try_from(left_ns).ok()?;
    if ms != 0 || left >= MILLISECOND_NS {
        return None;
    }
    let ticks = left.saturating_sub(slack_ns) / 100;
    (ticks > 0).then_some(ticks)
}

/// The server loop's wait, in ns, at `now_ns` after a step at `last_step_ns`
/// (0 for none yet), for a wait that wakes up to `wake_ns` late: the vanilla
/// `wait_ns` unless it is the 1 ms wait with players online, which grows up
/// to `cap_ns` but, woken late, still ends [`STEP_MARGIN_NS`] before the
/// next step is due. Where that leaves less than vanilla, vanilla.
#[must_use]
pub(crate) fn server_wait(
    wait_ns: u64,
    now_ns: u64,
    last_step_ns: u64,
    cap_ns: u64,
    wake_ns: u64,
) -> u64 {
    if wait_ns != VANILLA_SERVER_WAIT_NS || cap_ns <= wait_ns || last_step_ns == 0 {
        return wait_ns;
    }
    let left = (last_step_ns + STEP_NS)
        .saturating_sub(now_ns)
        .saturating_sub(STEP_MARGIN_NS)
        .saturating_sub(wake_ns);
    left.clamp(wait_ns, cap_ns)
}

/// Nanoseconds since the mod started.
fn now_ns() -> Option<u64> {
    let epoch = EPOCH.get()?;
    Some(
        u64::try_from(epoch.elapsed().as_nanos())
            .unwrap_or(u64::MAX)
            .max(1),
    )
}

/// Records a simulation step; on the game thread, after each step.
pub(crate) fn stepped() {
    STEPS.fetch_add(1, Ordering::Relaxed);
    if let Some(now) = now_ns() {
        LAST_STEP_NS.store(now, Ordering::Relaxed);
    }
}

/// The exact wait before a wait of the event loop.
fn exact(wait: LoopWait) {
    if !EXACT.load(Ordering::Relaxed) {
        return;
    }
    let Some(ticks) = sleep_ticks(
        u64::from(wait.ms),
        wait.left_ns,
        SLACK_NS.load(Ordering::Relaxed),
    ) else {
        return;
    };
    let left_ns = u64::try_from(wait.left_ns).unwrap_or(0);
    let started = Instant::now();
    if !(PRECISE.load(Ordering::Relaxed) && sys::delay_precise(ticks)) {
        sys::delay(ticks);
    }
    let slept = u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);
    SLEEPS.fetch_add(1, Ordering::Relaxed);
    SLEPT_NS.fetch_add(slept, Ordering::Relaxed);
    LATE_NS.fetch_add(slept.saturating_sub(left_ns), Ordering::Relaxed);
}

/// The server loop's wait, in ns.
fn server(wait_ns: &mut u64) {
    PASSES.fetch_add(1, Ordering::Relaxed);
    let me = sys::current_thread();
    if SERVER_THREAD.load(Ordering::Relaxed) != me {
        SERVER_THREAD.store(me, Ordering::Relaxed);
    }
    let cap = CAP_NS.load(Ordering::Relaxed);
    if cap <= VANILLA_SERVER_WAIT_NS || *wait_ns != VANILLA_SERVER_WAIT_NS {
        return;
    }
    let Some(now) = now_ns() else {
        return;
    };
    let wait = server_wait(
        *wait_ns,
        now,
        LAST_STEP_NS.load(Ordering::Relaxed),
        cap,
        WAKE_NS.load(Ordering::Relaxed),
    );
    if wait != *wait_ns {
        *wait_ns = wait;
        CAPPED.fetch_add(1, Ordering::Relaxed);
    }
}

/// Turns the exact waits on or off.
pub(crate) fn set_exact(on: bool) {
    EXACT.store(on, Ordering::Relaxed);
}

/// Whether the exact waits run.
pub(crate) fn exact_on() -> bool {
    EXACT.load(Ordering::Relaxed)
}

/// What a sleep on a high-resolution timer leaves before the deadline. Such
/// a timer fires about 0.2 ms late; a longer slack only hands that time back
/// to polling.
pub(crate) const PRECISE_SLACK_NS: u64 = 50_000;

/// Sets the slack for Windows: what a high-resolution timer needs when the
/// system has them, else the timer resolution the game set.
pub(crate) fn tune_for_windows() {
    let precise = sys::precise_timers();
    set_precise(precise);
    let resolution = sys::timer_resolution_ns().unwrap_or(MILLISECOND_NS);
    WAKE_NS.store(resolution, Ordering::Relaxed);
    let slack = if precise {
        PRECISE_SLACK_NS
    } else {
        resolution
    };
    set_slack(slack);
}

/// Makes the exact waits sleep on a high-resolution timer.
pub(crate) fn set_precise(on: bool) {
    PRECISE.store(on, Ordering::Relaxed);
}

/// Sets how late a timed wait may wake.
#[cfg(test)]
pub(crate) fn set_wake(ns: u64) {
    WAKE_NS.store(ns, Ordering::Relaxed);
}

/// Sets what each exact sleep leaves to the clock's granularity.
pub(crate) fn set_slack(ns: u64) {
    SLACK_NS.store(ns, Ordering::Relaxed);
}

/// Sets the server wait cap in milliseconds; 1 is vanilla.
pub(crate) fn set_cap(ms: u32) {
    CAP_NS.store(u64::from(ms.max(1)) * MILLISECOND_NS, Ordering::Relaxed);
}

/// Hooks the waits: the exact waits when `exact_waits` asks for them, the
/// server wait when `server_loop` does. A site this build lacks is logged
/// and left alone.
///
/// # Errors
///
/// [`Off`] when the loader refuses a hook it was asked for.
pub(crate) fn link(
    load: &mut Load<'_>,
    exact_waits: bool,
    server_loop: bool,
) -> Result<Linked, Off> {
    let _epoch = EPOCH.get_or_init(Instant::now);
    let mut linked = Linked::default();
    if exact_waits {
        if sys::init() {
            let found = pacing::on_loop_wait(load, exact)?;
            linked.exact = found.handles;
            linked.message = found.messages;
            if !found.handles {
                enshwick_loader_api::warn!("exact waits: the event loop's wait was not found");
            }
        } else {
            enshwick_loader_api::warn!("exact waits: ntdll has no NtDelayExecution");
        }
    }
    if server_loop {
        linked.server = pacing::on_server_wait(load, server)?;
        if !linked.server {
            enshwick_loader_api::warn!("the server loop's wait was not found");
        }
    }
    Ok(linked)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_zero_wait_under_a_millisecond_sleeps() {
        assert_eq!(sleep_ticks(0, 600_000, 0), Some(6_000));
        assert_eq!(sleep_ticks(0, 999_999, 0), Some(9_999));
        assert_eq!(
            sleep_ticks(0, 1_000_000, 0),
            None,
            "the game waits 1 ms itself"
        );
        assert_eq!(sleep_ticks(1, 600_000, 0), None, "a timed wait stays");
        assert_eq!(sleep_ticks(0, 0, 0), None, "the deadline passed");
        assert_eq!(sleep_ticks(0, -5, 0), None);
        assert_eq!(sleep_ticks(0, 99, 0), None, "less than one tick");
    }

    #[test]
    fn a_sleep_never_reaches_past_the_deadline() {
        for left in [1, 99, 100, 101, 12_345, 500_000, 999_999] {
            for slack in [0, 500_000] {
                if let Some(ticks) = sleep_ticks(0, left, slack) {
                    assert!(ticks * 100 + slack <= u64::try_from(left).unwrap());
                }
            }
        }
        assert_eq!(sleep_ticks(0, 600_000, 500_000), Some(1_000));
        assert_eq!(sleep_ticks(0, 400_000, 500_000), None);
    }

    #[test]
    fn the_server_wait_grows_only_with_players_and_stops_before_the_step() {
        let step = 10 * STEP_NS;
        // Right after a step, the cap holds.
        assert_eq!(
            server_wait(MILLISECOND_NS, step + 1_000, step, 4 * MILLISECOND_NS, 0),
            4 * MILLISECOND_NS
        );
        // Near the next step, the wait ends a margin before it.
        let now = step + STEP_NS - 3 * MILLISECOND_NS;
        assert_eq!(
            server_wait(MILLISECOND_NS, now, step, 4 * MILLISECOND_NS, 0),
            3 * MILLISECOND_NS - STEP_MARGIN_NS
        );
        // Late or close: never under vanilla.
        assert_eq!(
            server_wait(MILLISECOND_NS, step + STEP_NS, step, 4 * MILLISECOND_NS, 0),
            MILLISECOND_NS
        );
        assert_eq!(
            server_wait(
                MILLISECOND_NS,
                step + 5 * STEP_NS,
                step,
                4 * MILLISECOND_NS,
                0
            ),
            MILLISECOND_NS
        );
        // Vanilla cap, no step yet, the idle wait and the short wait stay.
        assert_eq!(
            server_wait(MILLISECOND_NS, step, step, MILLISECOND_NS, 0),
            MILLISECOND_NS
        );
        assert_eq!(
            server_wait(MILLISECOND_NS, step, 0, 4 * MILLISECOND_NS, 0),
            MILLISECOND_NS
        );
        assert_eq!(
            server_wait(40 * MILLISECOND_NS, step, step, 4 * MILLISECOND_NS, 0),
            40 * MILLISECOND_NS
        );
        assert_eq!(
            server_wait(100_000, step, step, 4 * MILLISECOND_NS, 0),
            100_000
        );
    }

    #[test]
    fn a_wait_that_wakes_late_still_ends_before_the_step() {
        let step = 10 * STEP_NS;
        let cap = 8 * MILLISECOND_NS;
        for wake in [0, 500_000, MILLISECOND_NS, 2 * MILLISECOND_NS] {
            for after in (0..STEP_NS).step_by(250_000) {
                let wait = server_wait(MILLISECOND_NS, step + after, step, cap, wake);
                if wait > MILLISECOND_NS {
                    assert!(
                        step + after + wait + wake + STEP_MARGIN_NS <= step + STEP_NS,
                        "wake {wake}, {after} ns after the step: {wait}"
                    );
                }
            }
        }
        // A coarse timer leaves the wait vanilla.
        assert_eq!(
            server_wait(MILLISECOND_NS, step, step, cap, 15_625_000),
            MILLISECOND_NS
        );
        // A tick late near the step: shorter by that tick.
        let now = step + STEP_NS - 6 * MILLISECOND_NS;
        assert_eq!(
            server_wait(MILLISECOND_NS, now, step, cap, MILLISECOND_NS),
            5 * MILLISECOND_NS - STEP_MARGIN_NS
        );
    }
}
