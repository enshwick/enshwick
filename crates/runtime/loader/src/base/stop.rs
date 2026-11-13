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
//! The loader calls the handler as Windows does, with `CTRL_C_EVENT` on a
//! thread of its own: the handler waits on the main thread, which may be the
//! caller's. It calls it once, and never once the shutdown began, since a
//! call while the exit reason is set spins for ever.

use std::sync::atomic::{AtomicBool, Ordering};

use enshwick_core::abi::{Side, Status};

use crate::engine::loader::Loader;
use crate::engine::pinned;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[pinned::server::CONSOLE_HANDLER.id()];

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

/// Asks the dedicated server to stop through the game's own path, for
/// `HostApi::server_stop`: [`Status::OK`] when the stop starts,
/// [`Status::REFUSED`] when it is under way already, from a mod or the
/// game, [`Status::UNAVAILABLE`] off a dedicated server and
/// [`Status::MISSING`] when the build has no handler the loader found;
/// `who` names the asking mod for the log.
pub(crate) fn request(loader: &Loader, who: &str) -> Status {
    if loader.side != Side::SERVER {
        return Status::UNAVAILABLE;
    }
    let Some(handler) = loader
        .address(pinned::server::CONSOLE_HANDLER)
        .and_then(|address| usize::try_from(address).ok())
    else {
        return Status::MISSING;
    };
    if crate::base::saves::shutting_down() || STOP.asked.swap(true, Ordering::AcqRel) {
        return Status::REFUSED;
    }
    // SAFETY: the table's `server.console_handler` is the routine this
    // build hands `SetConsoleCtrlHandler` (route `server.console_handler`,
    // server 26430253), whose signature `HandlerFn` states.
    let handler = unsafe { core::mem::transmute::<usize, HandlerFn>(handler) };
    let started = std::thread::Builder::new()
        .name("enshwick server stop".to_owned())
        // SAFETY: as Windows calls a console control handler: with a control
        // event, on a thread of its own, once.
        .spawn(move || unsafe { handler(CTRL_C_EVENT) });
    match started {
        Ok(_) => {
            crate::log!(INFO, "{who} asked the server to stop; the game shuts down");
            Status::OK
        }
        Err(error) => {
            STOP.asked.store(false, Ordering::Release);
            crate::log!(WARN, "the server stop did not start: {error}");
            Status::REFUSED
        }
    }
}
