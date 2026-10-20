//! The two waits the mod changes, both in the engine's event loop.
//!
//! **Exact waits.** The loop's wait helper turns the time left before its
//! deadline into whole milliseconds, truncating, and waits on its handles
//! for that long. With less than a millisecond left it waits for 0 ms, again
//! and again, until the deadline passes: the app's main loop spins so for
//! about 0.6 ms of every frame, and the server loop for 0.1 ms after a slow
//! pass. Under Wine each of those zero waits is a call into the Wine server
//! or a system call. A hook just before the wait sleeps the time left with
//! `NtDelayExecution` when the game would wait 0 ms, then lets the game's own
//! zero wait run once: it still sees every event, the mod waits on no handle
//! and consumes none, and it never sleeps past the deadline.
//!
//! **Server wait cap.** With players online the server's game loop waits a
//! fixed 1 ms after each pass, about 900 passes a second for 60 simulation
//! steps. A hook where the loop picks that wait may lengthen it up to a cap,
//! never past the next step. The idle 40 ms wait and the short wait after a
//! slow pass stay as they are.
//!
//! Every site is found by its bytes, on the server and on the client, whose
//! loop has a second wait for window messages.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::Instant;

use enshwick_loader_api::{Load, Off, Registers};

use crate::sys;

/// The wait helper after it clamped the milliseconds into `r9d`: `cmp rdx,
/// max; mov r9d, max; cmovl r9d, edx`, then the arguments of
/// `WaitForMultipleObjectsEx(count, handles, FALSE, ms, TRUE)`.
const WAIT: &str = "48 3B ?? 44 8B ?? 44 0F 4C CA 48 8B 55 D8 45 33 C0 8B 4D E0 \
                    C7 44 24 20 01 00 00 00 FF 15";
/// The client's message wait, the milliseconds in `r8d`: the arguments of
/// `MsgWaitForMultipleObjectsEx(count, handles, ms, QS_ALLINPUT, alertable
/// | input available)`.
const MESSAGE_WAIT: &str = "48 3B ?? 44 8B ?? 44 0F 4C C2 48 8B 55 D8 41 B9 FF 1C 00 00 8B 4D E0 \
                            C7 44 24 20 06 00 00 00 FF D0";
/// Where the hook goes in [`WAIT`] and [`MESSAGE_WAIT`]: after the clamp.
const WAIT_AT: u64 = 10;
/// `mov rax, 0x431bde82d7b634db`: the start of a division by 1 000 000.
const DIVIDE: [u8; 10] = [0x48, 0xB8, 0xDB, 0x34, 0xB6, 0xD7, 0x82, 0xDE, 0x1B, 0x43];
/// `imul rbx; sar rdx, 18`: the dividend is `rbx`, the nanoseconds left.
const DIVIDE_RBX: [u8; 7] = [0x48, 0xF7, 0xEB, 0x48, 0xC1, 0xFA, 0x12];
/// How far before the hook the division may start.
const DIVIDE_SPAN: u64 = 0x80;

/// The server loop picking its wait: 40 ms with nobody online, else 1 ms, or
/// 0.1 ms after a pass longer than 14 ms, into `r9`; then the call into the
/// event loop.
const SERVER_WAIT: &str = "44 39 BB 3C 05 00 00 75 08 41 B9 00 5A 62 02 EB 12 48 81 7D ?? 80 9F D5 00 \
                           41 B9 40 42 0F 00 4C 0F 4F CF 4D 8B C4 48 8D 55 ?? 48 8D 4D ?? E8";
/// Where the hook goes in [`SERVER_WAIT`]: `r9` holds the final wait.
const SERVER_WAIT_AT: u64 = 35;

/// One millisecond, the shortest wait the helper can ask for.
pub(crate) const MILLISECOND_NS: u64 = 1_000_000;
/// One simulation step at 60 a second.
pub(crate) const STEP_NS: u64 = 16_666_667;
/// What the server wait cap leaves before the next step.
pub(crate) const STEP_MARGIN_NS: u64 = 500_000;
/// The server loop's wait with players online.
pub(crate) const VANILLA_SERVER_WAIT_NS: u64 = MILLISECOND_NS;

/// Whether the exact waits run.
static EXACT: AtomicBool = AtomicBool::new(false);
/// What a sleep leaves to the clock's granularity, in ns: the timer
/// resolution on Windows, 0 under Wine.
static SLACK_NS: AtomicU64 = AtomicU64::new(0);
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
/// (0 for none yet): the vanilla `wait_ns` unless it is the 1 ms wait with
/// players online, which grows up to `cap_ns` but ends
/// [`STEP_MARGIN_NS`] before the next step is due.
#[must_use]
pub(crate) fn server_wait(wait_ns: u64, now_ns: u64, last_step_ns: u64, cap_ns: u64) -> u64 {
    if wait_ns != VANILLA_SERVER_WAIT_NS || cap_ns <= wait_ns || last_step_ns == 0 {
        return wait_ns;
    }
    let left = (last_step_ns + STEP_NS)
        .saturating_sub(now_ns)
        .saturating_sub(STEP_MARGIN_NS);
    left.clamp(wait_ns, cap_ns)
}

/// Whether `before`, the code ahead of a hook site, divides `rbx` by a
/// million, so `rbx` holds the nanoseconds left at the site.
#[must_use]
pub(crate) fn divides_rbx(before: &[u8]) -> bool {
    before
        .windows(DIVIDE.len())
        .enumerate()
        .any(|(at, window)| {
            window == DIVIDE
                && before[at + DIVIDE.len()..]
                    .windows(DIVIDE_RBX.len())
                    .take(9)
                    .any(|next| next == DIVIDE_RBX)
        })
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

/// The exact wait for a site whose milliseconds are in `ms`.
fn exact(ms: u64, left_ns: u64) {
    if !EXACT.load(Ordering::Relaxed) {
        return;
    }
    #[expect(clippy::cast_possible_wrap, reason = "rbx holds a signed count")]
    let left = left_ns as i64;
    let Some(ticks) = sleep_ticks(ms & 0xffff_ffff, left, SLACK_NS.load(Ordering::Relaxed)) else {
        return;
    };
    sys::delay(ticks);
    SLEEPS.fetch_add(1, Ordering::Relaxed);
    SLEPT_NS.fetch_add(ticks * 100, Ordering::Relaxed);
}

/// The server loop's wait, `r9` in ns.
fn server(registers: &mut Registers) {
    PASSES.fetch_add(1, Ordering::Relaxed);
    let me = sys::current_thread();
    if SERVER_THREAD.load(Ordering::Relaxed) != me {
        SERVER_THREAD.store(me, Ordering::Relaxed);
    }
    let cap = CAP_NS.load(Ordering::Relaxed);
    if cap <= VANILLA_SERVER_WAIT_NS || registers.r9 != VANILLA_SERVER_WAIT_NS {
        return;
    }
    let Some(now) = now_ns() else {
        return;
    };
    let wait = server_wait(registers.r9, now, LAST_STEP_NS.load(Ordering::Relaxed), cap);
    if wait != registers.r9 {
        registers.r9 = wait;
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

/// Sets what each exact sleep leaves to the clock's granularity.
pub(crate) fn set_slack(ns: u64) {
    SLACK_NS.store(ns, Ordering::Relaxed);
}

/// Sets the server wait cap in milliseconds; 1 is vanilla.
pub(crate) fn set_cap(ms: u32) {
    CAP_NS.store(u64::from(ms.max(1)) * MILLISECOND_NS, Ordering::Relaxed);
}

/// The address of the one match of `pattern`, `None` when it has none or
/// several.
fn find(load: &Load<'_>, pattern: &str) -> Option<u64> {
    load.find_code(pattern).ok()
}

/// Whether the code before `site` divides `rbx` by a million.
fn checks(code: &[u8], site: u64) -> bool {
    let start = code.as_ptr().addr() as u64;
    let Some(end) = site
        .checked_sub(start)
        .and_then(|end| usize::try_from(end).ok())
    else {
        return false;
    };
    let span = usize::try_from(DIVIDE_SPAN).unwrap_or(0);
    code.get(end.saturating_sub(span)..end)
        .is_some_and(divides_rbx)
}

/// Finds and hooks the waits: the exact waits when `exact` asks for them,
/// the server wait when `server` does. A site that is missing or reads
/// otherwise is logged and left alone.
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
    let code = load.code()?;
    if exact_waits {
        if !sys::init() {
            enshwick_loader_api::warn!("exact waits: ntdll has no NtDelayExecution");
        } else if let Some(site) = find(load, WAIT).map(|at| at + WAIT_AT) {
            if checks(code, site) {
                load.hook_at(site, |registers| exact(registers.r9, registers.rbx))?;
                linked.exact = true;
            } else {
                enshwick_loader_api::warn!(
                    "exact waits: the wait at {site:#x} does not read as expected"
                );
            }
        } else {
            enshwick_loader_api::warn!("exact waits: the event loop's wait was not found");
        }
        if linked.exact
            && let Some(site) = find(load, MESSAGE_WAIT).map(|at| at + WAIT_AT)
            && checks(code, site)
        {
            load.hook_at(site, |registers| exact(registers.r8, registers.rbx))?;
            linked.message = true;
        }
    }
    if server_loop {
        if let Some(site) = find(load, SERVER_WAIT).map(|at| at + SERVER_WAIT_AT) {
            load.hook_at(site, server)?;
            linked.server = true;
        } else {
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
            server_wait(MILLISECOND_NS, step + 1_000, step, 4 * MILLISECOND_NS),
            4 * MILLISECOND_NS
        );
        // Near the next step, the wait ends a margin before it.
        let now = step + STEP_NS - 3 * MILLISECOND_NS;
        assert_eq!(
            server_wait(MILLISECOND_NS, now, step, 4 * MILLISECOND_NS),
            3 * MILLISECOND_NS - STEP_MARGIN_NS
        );
        // Late or close: never under vanilla.
        assert_eq!(
            server_wait(MILLISECOND_NS, step + STEP_NS, step, 4 * MILLISECOND_NS),
            MILLISECOND_NS
        );
        assert_eq!(
            server_wait(MILLISECOND_NS, step + 5 * STEP_NS, step, 4 * MILLISECOND_NS),
            MILLISECOND_NS
        );
        // Vanilla cap, no step yet, the idle wait and the short wait stay.
        assert_eq!(
            server_wait(MILLISECOND_NS, step, step, MILLISECOND_NS),
            MILLISECOND_NS
        );
        assert_eq!(
            server_wait(MILLISECOND_NS, step, 0, 4 * MILLISECOND_NS),
            MILLISECOND_NS
        );
        assert_eq!(
            server_wait(40 * MILLISECOND_NS, step, step, 4 * MILLISECOND_NS),
            40 * MILLISECOND_NS
        );
        assert_eq!(
            server_wait(100_000, step, step, 4 * MILLISECOND_NS),
            100_000
        );
    }

    #[test]
    fn the_division_of_rbx_is_recognised() {
        // Server: the division, `mov edi, -1`, then `imul rbx; sar rdx, 18`.
        let mut server = DIVIDE.to_vec();
        server.extend([0xBF, 0xFF, 0xFF, 0xFF, 0xFF]);
        server.extend(DIVIDE_RBX);
        assert!(divides_rbx(&server));
        // Client: the multiply right after.
        let mut client = vec![0x90; 4];
        client.extend(DIVIDE);
        client.extend(DIVIDE_RBX);
        assert!(divides_rbx(&client));
        // Another dividend.
        let mut other = DIVIDE.to_vec();
        other.extend([0x48, 0xF7, 0xE9, 0x48, 0xC1, 0xFA, 0x12]);
        assert!(!divides_rbx(&other));
        assert!(!divides_rbx(&DIVIDE));
        assert!(!divides_rbx(&[]));
    }

    #[test]
    fn the_patterns_parse() {
        for pattern in [WAIT, MESSAGE_WAIT, SERVER_WAIT] {
            assert!(
                enshwick_loader_api::code::Pattern::parse(pattern).is_ok(),
                "{pattern}"
            );
        }
    }
}
