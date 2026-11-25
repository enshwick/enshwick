//! The crafting, item transfer, equipment and item requests client commands
//! write into the local player's input: one at a time across all mods, in
//! the builder's next step pass after the game's own, as the waypoint's.
//! A written request is applied; it is observed once the client's copy of
//! the player's inventory changes after it, which a refused request never
//! does, so it times out. While none is handed, the detour adds one atomic
//! load.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use enshwick_base::Relock;
use enshwick_core::abi::ClientCommandStatus as Status;

use crate::actions::client_commands::{Handoff, Progress};
use crate::actions::client_control::pass::STEP;
use crate::actions::client_control::{Act, ActLayout};
use crate::actions::{local_held, player_state};
use crate::engine::mod_index::ModIndex;

/// The stages of a handed request.
mod stage {
    /// It waits for the next step pass.
    pub(super) const WRITE: u8 = 1;
    /// It was written; its effect is awaited.
    pub(super) const WRITTEN: u8 = 2;
    /// The command's mod turned off.
    pub(super) const MOD_OFF: u8 = 3;
}

/// The handed request: the handoff's word, its act, its mod, and the slot
/// change count when it was written.
struct Request {
    handoff: Handoff,
    act: Mutex<Option<Act>>,
    owner: AtomicUsize,
    written_at: AtomicU64,
}

static REQUEST: Request = Request {
    handoff: Handoff::new(),
    act: Mutex::new(None),
    owner: AtomicUsize::new(0),
    written_at: AtomicU64::new(0),
};

/// Hands mod `owner`'s request `id` to the builder; whether it took it,
/// false while another request waits. Starts the reads its outcome needs.
pub(crate) fn hand(id: u64, owner: ModIndex, act: Act) -> bool {
    let _asked = local_held::sample();
    let _counted = player_state::slot_changes(local_held::now_ms());
    REQUEST.handoff.arm(id, stage::WRITE, || {
        *REQUEST.act.relock() = Some(act);
        REQUEST.owner.store(owner.get(), Ordering::Relaxed);
    })
}

/// Where the builder stands with request `id`; `None` once it is no longer
/// handed.
pub(crate) fn progress(id: u64) -> Option<Progress> {
    let handed = REQUEST.handoff.stage(id)?;
    Some(match handed {
        stage::WRITTEN => {
            let changes = player_state::slot_changes(local_held::now_ms());
            observed(REQUEST.written_at.load(Ordering::Relaxed), changes)
        }
        stage::MOD_OFF => Progress::Cancelled(Status::MOD_OFF),
        _ => Progress::Waiting,
    })
}

/// A written request's progress: done once the slots changed after it.
const fn observed(written_at: u64, changes: u64) -> Progress {
    if changes > written_at {
        Progress::Done
    } else {
        Progress::Taken
    }
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

/// Writes the handed request in a step pass.
///
/// # Safety
///
/// - `input` is the local player's live `ClientPlayerInput` the builder
///   just wrote, on the builder's thread right after the game's pass.
#[inline]
pub(super) unsafe fn drive(layout: &ActLayout, input: *mut u8, mode: u32) {
    let Some((id, stage::WRITE)) = REQUEST.handoff.current() else {
        return;
    };
    if mode != STEP {
        return;
    }
    let owner = ModIndex::new(REQUEST.owner.load(Ordering::Relaxed));
    if crate::engine::loader::is_off(owner) {
        let _ended = REQUEST.handoff.advance(id, stage::WRITE, stage::MOD_OFF);
        return;
    }
    // A craft by hand names the player, whom the held read found.
    let Some(player) = local_held::player() else {
        return;
    };
    let Some(act) = *REQUEST.act.relock() else {
        return;
    };
    let before = player_state::slot_changes(local_held::now_ms());
    REQUEST.written_at.store(before, Ordering::Relaxed);
    if REQUEST.handoff.advance(id, stage::WRITE, stage::WRITTEN) {
        // SAFETY: the caller's record, on the builder's thread.
        unsafe { layout.write(input, player.0, act) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_written_request_is_applied_until_the_slots_change_after_it() {
        assert_eq!(observed(4, 4), Progress::Taken);
        assert_eq!(observed(4, 5), Progress::Done);
    }

    #[test]
    fn one_request_waits_at_a_time_and_a_frame_pass_writes_nothing() {
        let layout = &crate::actions::client_control::ACT_TEST_LAYOUT;
        let mut input = vec![0u8; 0x600];
        let act = Act::Trash { slot: 0x43 };
        assert!(hand(70, ModIndex::LOADER, act));
        assert!(!hand(71, ModIndex::LOADER, act), "one at a time");
        // SAFETY: a record of the build's size, not shared.
        unsafe {
            drive(
                layout,
                input.as_mut_ptr(),
                crate::actions::client_control::pass::FRAME,
            );
        }
        assert_eq!(progress(70), Some(Progress::Waiting));
        release(70);
        assert_eq!(progress(70), None);
    }
}
