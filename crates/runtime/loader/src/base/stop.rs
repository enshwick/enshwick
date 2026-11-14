//! A mod's request to stop a dedicated server the way the game stops it.
//!
//! Engine facts from server 26430253, read in the decompiler:
//! the game hands one console control handler to `SetConsoleCtrlHandler` as
//! it starts, `server.console_handler`. Called with a control event, it
//! writes the exit reason into the application, wakes the main loop and
//! waits for the main thread before it returns. The main loop then asks the
//! gameflow to shut down: the server's instances go from Run to Shutdown,
//! where the loader lets the save holds go and raises `ShutdownStarted`,
//! the final save runs, and the process exits with 0. Ctrl+C on a server
//! copy of the same build took that path, final save included.
//!
//! The handler spins for ever when the application (`server.application`)
//! is not made yet or its exit reason (`server.exit_reason`) is set already,
//! as after a person's Ctrl+C, before the loader sees the shutdown. So the
//! loader reads both first and calls the handler only for a running
//! application with no exit reason, once, as Windows does: with
//! `CTRL_C_EVENT` on a thread of its own, since the handler waits on the
//! main thread, which may be the caller's.

use std::sync::atomic::{AtomicBool, Ordering};

use enshwick_core::abi::{Side, Status};

use crate::engine::loader::Loader;
use crate::engine::pinned;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::server::APPLICATION.id(),
    pinned::server::CONSOLE_HANDLER.id(),
    pinned::server::EXIT_REASON.id(),
];

/// `CTRL_C_EVENT`, the event the handler takes for Ctrl+C.
const CTRL_C_EVENT: u32 = 0;

type HandlerFn = unsafe extern "system" fn(event: u32) -> i32;

/// The stop's one state: whether a mod asked for it.
struct Stop {
    asked: AtomicBool,
}

static STOP: Stop = Stop {
    asked: AtomicBool::new(false),
};

/// The game's application as a stop finds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Application {
    /// Not made yet or unreadable: the handler would spin for ever.
    Absent,
    /// Its exit reason is set, a stop under way: the handler would spin for
    /// ever.
    Exiting,
    /// Running with no exit reason: the handler stops it.
    Running,
}

impl Application {
    /// The application from its pointer `app`, `None` when unreadable, and
    /// `reason`, which reads the exit reason of a non-null one.
    fn from_reads(app: Option<u64>, reason: impl FnOnce(u64) -> Option<u8>) -> Self {
        match app {
            None | Some(0) => Self::Absent,
            Some(app) => match reason(app) {
                Some(0) => Self::Running,
                Some(_) => Self::Exiting,
                None => Self::Absent,
            },
        }
    }

    /// Reads the application at the global `global`, its exit reason
    /// `reason` bytes into it, through the fault-safe reader.
    fn read(global: usize, reason: usize) -> Self {
        let app = crate::engine::pages::read_up_to(global, 8)
            .and_then(|bytes| Some(u64::from_le_bytes(bytes.get(..8)?.try_into().ok()?)));
        Self::from_reads(app, |app| {
            let at = usize::try_from(app).ok()?.checked_add(reason)?;
            crate::engine::pages::read_up_to(at, 1)?.first().copied()
        })
    }
}

/// Asks the dedicated server to stop through the game's own path, for
/// `HostApi::server_stop`: [`Status::OK`] when the stop starts,
/// [`Status::REFUSED`] when it is under way already, from a mod or the
/// game, or the game has not started yet, [`Status::UNAVAILABLE`] off a
/// dedicated server and [`Status::MISSING`] when the build has no handler
/// the loader found; `who` names the asking mod for the log.
pub(crate) fn request(loader: &Loader, who: &str) -> Status {
    if loader.side != Side::SERVER {
        return Status::UNAVAILABLE;
    }
    let found = (
        loader
            .address(pinned::server::CONSOLE_HANDLER)
            .and_then(|address| usize::try_from(address).ok()),
        loader
            .address(pinned::server::APPLICATION)
            .and_then(|address| usize::try_from(address).ok()),
        loader.offset(pinned::server::EXIT_REASON),
    );
    let (Some(handler), Some(global), Some(reason)) = found else {
        return Status::MISSING;
    };
    let application = Application::read(global, reason);
    if application != Application::Running
        || crate::base::saves::shutting_down()
        || STOP.asked.swap(true, Ordering::AcqRel)
    {
        crate::log!(
            INFO,
            "{who} asked the server to stop, refused: the game is {}",
            if application == Application::Absent {
                "not started yet"
            } else {
                "stopping already"
            }
        );
        return Status::REFUSED;
    }
    // SAFETY: the table's `server.console_handler` is the routine this
    // build hands `SetConsoleCtrlHandler` (route `server.console_handler`,
    // server 26430253), whose signature `HandlerFn` states.
    let handler = unsafe { core::mem::transmute::<usize, HandlerFn>(handler) };
    let started = crate::engine::off_thread::start("server stop", move || {
        // SAFETY: as Windows calls a console control handler: with a control
        // event, on a thread of its own, once, and only for a running
        // application with no exit reason, read just now.
        let _handled = unsafe { handler(CTRL_C_EVENT) };
    });
    if started {
        crate::log!(INFO, "{who} asked the server to stop; the game shuts down");
        Status::OK
    } else {
        STOP.asked.store(false, Ordering::Release);
        Status::REFUSED
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_handler_runs_only_for_a_running_application_with_no_exit_reason() {
        let never = |_| panic!("no exit reason is read without an application");
        assert_eq!(Application::from_reads(None, never), Application::Absent);
        assert_eq!(Application::from_reads(Some(0), never), Application::Absent);
        assert_eq!(
            Application::from_reads(Some(0x1000), |_| None),
            Application::Absent
        );
        assert_eq!(
            Application::from_reads(Some(0x1000), |_| Some(1)),
            Application::Exiting
        );
        assert_eq!(
            Application::from_reads(Some(0x1000), |app| (app == 0x1000).then_some(0)),
            Application::Running
        );
    }

    #[test]
    fn an_unreadable_application_reads_as_absent() {
        assert_eq!(Application::read(0, 0xaa0), Application::Absent);
    }
}
