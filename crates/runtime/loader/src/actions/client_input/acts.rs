//! The crafting, item transfer, equipment and item requests client commands
//! write into the local player's input: one at a time across all mods, in
//! the builder's next step pass after the game's own, as the waypoint's.
//! A request waits a step while the game wrote one of the player's own of
//! its kind, so the player's input wins. A written request is applied; it
//! is observed once the client's copy of an inventory it names changes
//! after it, which a refused request never does, so it times out. While
//! none is handed, the detour adds one atomic load.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

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

/// No version seen yet, which no `u32` version equals.
const UNSEEN: u64 = u64::MAX;

/// The handed request: the handoff's word, its act's words, its mod, the
/// version of its kind's request at the last step pass, and the watched
/// change count when it was written.
struct Request {
    handoff: Handoff,
    act: [AtomicU64; 3],
    owner: AtomicUsize,
    seen: AtomicU64,
    written_at: AtomicU64,
}

static REQUEST: Request = Request {
    handoff: Handoff::new(),
    act: [const { AtomicU64::new(0) }; 3],
    owner: AtomicUsize::new(0),
    seen: AtomicU64::new(UNSEEN),
    written_at: AtomicU64::new(0),
};

/// The inventories whose change shows `act`'s effect: the player's own for
/// a craft, which may take and put anywhere in it; the sub-inventories of
/// the slots it names; the two whole inventories of a move of every stack.
fn watched(act: Act) -> [u32; 2] {
    let link = |slot: u64| u32::try_from(slot & 0xffff_ffff).unwrap_or(0);
    let own = |entity: u32| {
        if entity == 0 {
            player_state::PLAYER
        } else {
            entity
        }
    };
    match act {
        Act::Craft { .. } => [player_state::PLAYER, 0],
        Act::Move { from, to, .. } => [link(from), link(to)],
        Act::MoveAll { from, to } => [own(from), own(to)],
        Act::Equip { slot, .. } | Act::Trash { slot } => [link(slot), 0],
    }
}

/// Hands mod `owner`'s request `id` to the builder; whether it took it,
/// false while another request waits. Starts the reads its outcome needs.
pub(crate) fn hand(id: u64, owner: ModIndex, act: Act) -> bool {
    let _asked = local_held::sample();
    REQUEST.handoff.arm(id, stage::WRITE, || {
        for (word, value) in REQUEST.act.iter().zip(act.words()) {
            word.store(value, Ordering::Relaxed);
        }
        REQUEST.owner.store(owner.get(), Ordering::Relaxed);
        REQUEST.seen.store(UNSEEN, Ordering::Relaxed);
        player_state::watch(watched(act), local_held::now_ms());
    })
}

/// Where the builder stands with request `id`; `None` once it is no longer
/// handed.
pub(crate) fn progress(id: u64) -> Option<Progress> {
    let handed = REQUEST.handoff.stage(id)?;
    Some(match handed {
        stage::WRITTEN => {
            let changes = player_state::watched_changes(local_held::now_ms());
            observed(REQUEST.written_at.load(Ordering::Relaxed), changes)
        }
        stage::MOD_OFF => Progress::Cancelled(Status::MOD_OFF),
        _ => Progress::Waiting,
    })
}

/// A written request's progress: done once its inventories changed after
/// it.
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

/// Whether the game's own request of `act`'s kind kept its version since
/// the last step pass, so writing now loses none of the player's; keeps the
/// version for the next pass.
fn players_quiet(version: u32) -> bool {
    REQUEST.seen.swap(u64::from(version), Ordering::Relaxed) == u64::from(version)
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
    let Some(act) = Act::from_words(REQUEST.act.each_ref().map(|w| w.load(Ordering::Relaxed)))
    else {
        return;
    };
    let now = local_held::now_ms();
    // The effect counts against a read of the watched inventories of the
    // last second: until one exists, the request waits, its hand started
    // the reads.
    if !player_state::watch_fresh(now) {
        return;
    }
    // SAFETY: the caller's record, on the builder's thread.
    if !players_quiet(unsafe { layout.version(input, act) }) {
        return;
    }
    let before = player_state::watched_changes(now);
    REQUEST.written_at.store(before, Ordering::Relaxed);
    if REQUEST.handoff.advance(id, stage::WRITE, stage::WRITTEN) {
        // SAFETY: the caller's record, on the builder's thread.
        unsafe { layout.write(input, player.get(), act) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_written_request_is_applied_until_its_inventories_change_after_it() {
        assert_eq!(observed(4, 4), Progress::Taken);
        assert_eq!(observed(4, 5), Progress::Done);
    }

    #[test]
    fn a_request_waits_while_the_game_writes_the_player_s_own_of_its_kind() {
        REQUEST.seen.store(UNSEEN, Ordering::Relaxed);
        assert!(!players_quiet(6), "the first pass only looks");
        assert!(players_quiet(6), "unchanged since the last pass");
        assert!(!players_quiet(7), "the player's own came this step");
        assert!(players_quiet(7));
    }

    #[test]
    fn each_act_watches_the_inventories_it_names() {
        let (from, to) = (0x41 | 2 << 32, 0x42 | 5 << 32);
        assert_eq!(
            watched(Act::Craft {
                station: 9,
                recipe: 1,
                count: 1
            }),
            [player_state::PLAYER, 0],
            "the player's whole inventory"
        );
        assert_eq!(watched(Act::Move { from, to, count: 0 }), [0x41, 0x42]);
        assert_eq!(
            watched(Act::MoveAll { from: 500, to: 0 }),
            [500, player_state::PLAYER]
        );
        assert_eq!(watched(Act::Trash { slot: to }), [0x42, 0]);
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
