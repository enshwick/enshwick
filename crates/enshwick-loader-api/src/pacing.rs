//! How the engine paces its loops and sizes its thread pools: the places a
//! mod changes to make the game use less CPU.
//!
//! - [`on_loop_wait`]: the engine's event loop, about to wait on its handles.
//!   It turns the time left before its deadline into whole milliseconds,
//!   truncating, so with less than a millisecond left it waits 0 ms, again
//!   and again, until the deadline passes.
//! - [`on_server_wait`]: the server's game loop picking how long it waits
//!   between its passes.
//! - [`cap_task_workers`]: the most worker threads each task group starts
//!   with, which the engine sizes from the logical CPUs it sees.
//!
//! Every site is found by its bytes, on the server and on the client, and a
//! site this build lacks is reported, never guessed.
//!
//! # Examples
//!
//! ```
//! use enshwick_loader_api::pacing;
//!
//! fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
//!     let found = pacing::on_loop_wait(load, |wait| {
//!         if wait.ms == 0 && wait.left_ns > 0 {
//!             // Sleep `left_ns` here instead of letting the game poll.
//!         }
//!     })?;
//!     if !found.handles {
//!         enshwick_loader_api::warn!("the event loop's wait was not found");
//!     }
//!     Ok(())
//! }
//! ```

use crate::{Load, Off, Registers};

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
const DIVIDE_SPAN: usize = 0x80;

/// The server loop picking its wait: 40 ms with nobody online, else 1 ms, or
/// 0.1 ms after a pass longer than 14 ms, into `r9`; then the call into the
/// event loop.
const SERVER_WAIT: &str = "44 39 BB 3C 05 00 00 75 08 41 B9 00 5A 62 02 EB 12 48 81 7D ?? 80 9F D5 00 \
                           41 B9 40 42 0F 00 4C 0F 4F CF 4D 8B C4 48 8D 55 ?? 48 8D 4D ?? E8";
/// Where the hook goes in [`SERVER_WAIT`]: `r9` holds the final wait.
const SERVER_WAIT_AT: u64 = 35;

/// The task system reading its per-group worker caps from its config
/// (`r15`): `movzx eax, byte [r15+9]` for the default group, then
/// `movzx eax, byte [r15+0xa]` for the background group, each followed by a
/// `min` with the CPUs counted.
const WORKERS: &str = "41 0F B6 47 09 8B CF 40 3A C7 0F 42 C8 84 DB 89 8D ?? ?? ?? ?? 8B CF 0F B6 C3 \
                       0F 45 C8 41 0F B6 47 0A 44 0F B6 E9";
/// Where `eax` holds the default group's cap in [`WORKERS`].
const DEFAULT_WORKERS_AT: u64 = 5;
/// Where `eax` holds the background group's cap in [`WORKERS`].
const BACKGROUND_WORKERS_AT: u64 = 34;

/// The server loop's wait with nobody online.
pub const SERVER_IDLE_WAIT_NS: u64 = 40_000_000;
/// The server loop's wait with players online.
pub const SERVER_ONLINE_WAIT_NS: u64 = 1_000_000;
/// The server loop's wait after a pass longer than 14 ms.
pub const SERVER_SLOW_WAIT_NS: u64 = 100_000;

/// A wait of the engine's event loop on its handles, about to happen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct LoopWait {
    /// What the game is about to wait, in milliseconds: the time left,
    /// truncated.
    pub ms: u32,
    /// The time left before the loop's deadline, in nanoseconds.
    pub left_ns: i64,
}

/// Which of the event loop's waits [`on_loop_wait`] found and hooked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct LoopWaits {
    /// The wait on handles, on the server and the client.
    pub handles: bool,
    /// The client's wait on handles and window messages.
    pub messages: bool,
}

/// Whether `before`, the code ahead of a hook site, divides `rbx` by a
/// million, so `rbx` holds the nanoseconds left at the site.
fn divides_rbx(before: &[u8]) -> bool {
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

/// Whether the code before `site` divides `rbx` by a million.
fn checks(code: &[u8], site: u64) -> bool {
    let start = code.as_ptr().addr() as u64;
    let Some(end) = site
        .checked_sub(start)
        .and_then(|end| usize::try_from(end).ok())
    else {
        return false;
    };
    code.get(end.saturating_sub(DIVIDE_SPAN)..end)
        .is_some_and(divides_rbx)
}

/// The one match of `pattern` plus `at`; `None` when the pattern has no
/// match or several.
fn site(load: &Load<'_>, pattern: &str, at: u64) -> Option<u64> {
    load.find_code(pattern).ok().map(|found| found + at)
}

/// Runs `handler` before every wait of the engine's event loop on its
/// handles, on the thread that waits: the app's main loop, the server's
/// game loop and its session, and on the client its message wait too. A
/// handler that sleeps must never sleep past `left_ns`, and must leave the
/// waiting to the game, so it sees every event. Allowed during
/// [`Mod::init`](crate::Mod::init) only.
///
/// # Errors
///
/// [`Off`] when the loader refuses a hook. A wait this build lacks is
/// reported as not found in the result.
///
/// # Examples
///
/// ```
/// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
///     let found = enshwick_loader_api::pacing::on_loop_wait(load, |wait| {
///         let _ = (wait.ms, wait.left_ns);
///     })?;
///     let _ = found.messages;
///     Ok(())
/// }
/// ```
pub fn on_loop_wait<F>(load: &mut Load<'_>, handler: F) -> Result<LoopWaits, Off>
where
    F: Fn(LoopWait) + Clone + Send + Sync + 'static,
{
    let code = load.code()?;
    let mut found = LoopWaits::default();
    let wait = |ms: u64, left: u64| LoopWait {
        ms: u32::try_from(ms & 0xffff_ffff).unwrap_or(u32::MAX),
        left_ns: i64::from_ne_bytes(left.to_ne_bytes()),
    };
    if let Some(at) = site(load, WAIT, WAIT_AT).filter(|&at| checks(code, at)) {
        let handler = handler.clone();
        load.hook_at(at, move |registers: &mut Registers| {
            handler(wait(registers.r9, registers.rbx));
        })?;
        found.handles = true;
    }
    if let Some(at) = site(load, MESSAGE_WAIT, WAIT_AT).filter(|&at| checks(code, at)) {
        load.hook_at(at, move |registers: &mut Registers| {
            handler(wait(registers.r8, registers.rbx));
        })?;
        found.messages = true;
    }
    Ok(found)
}

/// Runs `handler` where the server's game loop has picked its wait between
/// two passes, in nanoseconds: [`SERVER_IDLE_WAIT_NS`],
/// [`SERVER_ONLINE_WAIT_NS`] or [`SERVER_SLOW_WAIT_NS`]. The handler may
/// change it; the loop waits on its handles for that long, so events still
/// end the wait early. On the server's game thread, once a pass; `false`
/// when this build lacks the site. Allowed during
/// [`Mod::init`](crate::Mod::init) only.
///
/// # Errors
///
/// [`Off`] when the loader refuses the hook.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::pacing;
///
/// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
///     let hooked = pacing::on_server_wait(load, |wait_ns| {
///         if *wait_ns == pacing::SERVER_ONLINE_WAIT_NS {
///             *wait_ns = 2 * pacing::SERVER_ONLINE_WAIT_NS;
///         }
///     })?;
///     let _ = hooked;
///     Ok(())
/// }
/// ```
pub fn on_server_wait<F>(load: &mut Load<'_>, handler: F) -> Result<bool, Off>
where
    F: Fn(&mut u64) + Send + Sync + 'static,
{
    let Some(at) = site(load, SERVER_WAIT, SERVER_WAIT_AT) else {
        return Ok(false);
    };
    load.hook_at(at, move |registers: &mut Registers| {
        handler(&mut registers.r9);
    })?;
    Ok(true)
}

/// The most worker threads the task groups start with; 0 leaves a group
/// as the game sizes it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct TaskWorkers {
    /// The default group, which also sizes the ECS task queue.
    pub default: u8,
    /// The background group: streaming, saves and other slow work.
    pub background: u8,
}

impl TaskWorkers {
    /// The same cap for both groups.
    ///
    /// # Examples
    ///
    /// ```
    /// let caps = enshwick_loader_api::pacing::TaskWorkers::at_most(4);
    /// assert_eq!((caps.default, caps.background), (4, 4));
    /// ```
    #[must_use]
    pub const fn at_most(workers: u8) -> Self {
        Self {
            default: workers,
            background: workers,
        }
    }
}

/// Caps the worker threads of the task groups as the task system starts.
/// The engine sizes them from the logical CPUs it sees, which under Wine
/// are the host's, whatever a container allows. Only the dedicated server
/// starts its task system after the loader arms hooks; on the client it has
/// started already and the result is `false`, as it is when this build lacks
/// the site. The game's log shows the counts it started with:
/// `[task] Group 'Default' Worker:…`. Allowed during
/// [`Mod::init`](crate::Mod::init) only.
///
/// # Errors
///
/// [`Off`] when the loader refuses a hook.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::pacing::{self, TaskWorkers};
///
/// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
///     if !pacing::cap_task_workers(load, TaskWorkers::at_most(4))? {
///         enshwick_loader_api::info!("task workers stay as the game sizes them");
///     }
///     Ok(())
/// }
/// ```
pub fn cap_task_workers(load: &mut Load<'_>, caps: TaskWorkers) -> Result<bool, Off> {
    if load.host().build().side != crate::abi::Side::SERVER {
        return Ok(false);
    }
    let Some(at) = site(load, WORKERS, 0) else {
        return Ok(false);
    };
    for (offset, cap) in [
        (DEFAULT_WORKERS_AT, caps.default),
        (BACKGROUND_WORKERS_AT, caps.background),
    ] {
        if cap > 0 {
            load.hook_at(at + offset, move |registers: &mut Registers| {
                registers.rax = registers.rax.min(u64::from(cap));
            })?;
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        for pattern in [WAIT, MESSAGE_WAIT, SERVER_WAIT, WORKERS] {
            assert!(crate::code::Pattern::parse(pattern).is_ok(), "{pattern}");
        }
    }
}
