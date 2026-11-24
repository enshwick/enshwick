//! The waypoint and ping requests client commands write into the local
//! player's input, as the map's own events have the builder write them: one
//! at a time across all mods, in the builder's next step pass, after the
//! game's own pass. The builder rewrites the request only when the map
//! raises an event, so it stays as written and the server takes it once.
//! While none is handed, the detour adds one atomic load.

use std::sync::atomic::{AtomicU8, AtomicU64, AtomicUsize, Ordering};

use enshwick_core::abi::ClientCommandStatus as Status;

use crate::actions::client_commands::{Handoff, Progress};
use crate::actions::client_control::pass::STEP;
use crate::actions::client_control::{Mark, WaypointLayout};
use crate::engine::mod_index::ModIndex;

/// The stages of a handed request.
mod stage {
    /// It waits for the next step pass.
    pub(super) const MARK: u8 = 1;
    /// It was written into the input.
    pub(super) const DONE: u8 = 2;
    /// The command's mod turned off.
    pub(super) const MOD_OFF: u8 = 3;
}

/// What a [`Mark`] carries, as plain words for the handoff.
mod what {
    pub(super) const SET: u8 = 1;
    pub(super) const CLEAR: u8 = 2;
    pub(super) const PING: u8 = 3;
}

/// The handed request: the handoff's word and its payload.
struct Request {
    handoff: Handoff,
    what: AtomicU8,
    /// The position's metres, as `f64` bits.
    at: [AtomicU64; 3],
    /// The command's mod, as its load-order index.
    owner: AtomicUsize,
}

static REQUEST: Request = Request {
    handoff: Handoff::new(),
    what: AtomicU8::new(0),
    at: [const { AtomicU64::new(0) }; 3],
    owner: AtomicUsize::new(0),
};

/// Hands mod `owner`'s request `id` to the builder; whether it took it,
/// false while another request waits.
pub(crate) fn hand(id: u64, owner: ModIndex, mark: Mark) -> bool {
    let (what, at) = match mark {
        Mark::Set(at) => (what::SET, at),
        Mark::Clear => (what::CLEAR, [0.0; 3]),
        Mark::Ping(at) => (what::PING, at),
    };
    REQUEST.handoff.arm(id, stage::MARK, || {
        REQUEST.what.store(what, Ordering::Relaxed);
        for (slot, metres) in REQUEST.at.iter().zip(at) {
            slot.store(metres.to_bits(), Ordering::Relaxed);
        }
        REQUEST.owner.store(owner.get(), Ordering::Relaxed);
    })
}

/// Where the builder stands with request `id`; `None` once it is no longer
/// handed.
pub(crate) fn progress(id: u64) -> Option<Progress> {
    REQUEST.handoff.stage(id).map(|handed| match handed {
        stage::DONE => Progress::Done,
        stage::MOD_OFF => Progress::Cancelled(Status::MOD_OFF),
        _ => Progress::Waiting,
    })
}

/// Takes request `id` back, so the next can be handed.
pub(crate) fn release(id: u64) {
    let _had = REQUEST.handoff.release(id);
}

/// Whether a request is handed, the detour's one load while none is.
#[inline]
pub(super) fn pending() -> bool {
    REQUEST.handoff.current().is_some()
}

/// The handed request's mark.
fn mark() -> Mark {
    let at = REQUEST
        .at
        .each_ref()
        .map(|slot| f64::from_bits(slot.load(Ordering::Relaxed)));
    match REQUEST.what.load(Ordering::Relaxed) {
        what::SET => Mark::Set(at),
        what::PING => Mark::Ping(at),
        _ => Mark::Clear,
    }
}

/// Writes the handed request in a step pass.
///
/// # Safety
///
/// - `input` is the local player's live `ClientPlayerInput` the builder
///   just wrote, on the builder's thread right after the game's pass.
#[inline]
pub(super) unsafe fn drive(layout: &WaypointLayout, input: *mut u8, mode: u32) {
    let Some((id, stage::MARK)) = REQUEST.handoff.current() else {
        return;
    };
    if mode != STEP {
        return;
    }
    let owner = ModIndex::new(REQUEST.owner.load(Ordering::Relaxed));
    if crate::engine::loader::is_off(owner) {
        let _ended = REQUEST.handoff.advance(id, stage::MARK, stage::MOD_OFF);
        return;
    }
    let mark = mark();
    if REQUEST.handoff.advance(id, stage::MARK, stage::DONE) {
        // SAFETY: the caller's record, on the builder's thread.
        unsafe { layout.write(input, mark) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::client_control::pass::FRAME;

    #[test]
    fn a_request_is_written_once_in_a_step_pass_and_then_done() {
        let layout = &crate::actions::client_control::WAYPOINT_TEST_LAYOUT;
        let mut input = vec![0u8; 0x600];
        assert!(hand(90, ModIndex::LOADER, Mark::Ping([8.0, 1.0, 9.0])));
        assert!(!hand(91, ModIndex::LOADER, Mark::Clear), "one at a time");
        // SAFETY: a record of the build's size, not shared.
        unsafe { drive(layout, input.as_mut_ptr(), FRAME) };
        assert_eq!(progress(90), Some(Progress::Waiting), "not in a frame pass");
        // SAFETY: the same record.
        unsafe { drive(layout, input.as_mut_ptr(), STEP) };
        assert_eq!(progress(90), Some(Progress::Done));
        assert_eq!((input[0x250], input[0x251]), (1, 1), "a ping");
        let version = input[0x254];
        // SAFETY: the same record.
        unsafe { drive(layout, input.as_mut_ptr(), STEP) };
        assert_eq!(input[0x254], version, "written once");
        release(90);
        assert_eq!(progress(90), None);
        assert!(hand(91, ModIndex::LOADER, Mark::Clear));
        release(91);
    }
}
