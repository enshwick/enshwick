//! The game actions client commands press: each press holds its bit in the
//! local player's digital input, set after the game's own pass into the
//! mask the builder wrote, so several presses hold at once.
//!
//! The builder clears and rewrites every bit of the mask in its step pass
//! (client 26430625, read in the decompiler and seen live), so a press sets
//! its bit again on every step it holds, and the mask is the game's own
//! again in the first pass after it ends. A press takes one of [`SLOTS`]
//! slots, each its own [`Handoff`]; while none is armed and the last pass
//! wrote nothing, the detour adds two atomic loads.

use std::sync::atomic::{AtomicU8, AtomicU32, AtomicU64, AtomicUsize, Ordering};

use enshwick_core::abi::ClientCommandStatus as Status;

use crate::actions::client_commands::{Handoff, Progress};
use crate::actions::client_control::pass::STEP;
use crate::actions::client_control::{Buttons, InputLayout};
use crate::actions::local_held;
use crate::engine::mod_index::ModIndex;

/// How many presses hold at once, every mod's together.
pub(crate) const SLOTS: usize = 8;
const _: () = assert!(
    enshwick_core::__private::rules::PRESSES_PER_MOD < SLOTS,
    "one mod's presses leave slots for the others"
);

/// The stages of a handed press.
mod stage {
    /// It waits for the next step pass.
    pub(super) const PRESS: u8 = 1;
    /// It holds its bit until its deadline.
    pub(super) const PRESSING: u8 = 2;
    /// Its hold ran out.
    pub(super) const DONE: u8 = 3;
    /// The player pressed a button not held when the press started.
    pub(super) const PLAYER: u8 = 4;
    /// The game blocked the player's input.
    pub(super) const BLOCKED: u8 = 5;
    /// The command's mod turned off.
    pub(super) const MOD_OFF: u8 = 6;
}

/// One press: the handoff's word and what it carries.
struct Slot {
    handoff: Handoff,
    /// The action's bit in the digital mask.
    bit: AtomicU8,
    hold_ms: AtomicU32,
    /// When the hold ends, as [`local_held::now_ms`] counts.
    until: AtomicU64,
    /// The player's buttons at the step the press started: only a button
    /// beyond these is the player's new input.
    baseline: AtomicU64,
    /// The command's mod, as its load-order index.
    owner: AtomicUsize,
}

impl Slot {
    const fn new() -> Self {
        Self {
            handoff: Handoff::new(),
            bit: AtomicU8::new(0),
            hold_ms: AtomicU32::new(0),
            until: AtomicU64::new(0),
            baseline: AtomicU64::new(0),
            owner: AtomicUsize::new(0),
        }
    }
}

/// The presses of this process.
struct Presses {
    slots: [Slot; SLOTS],
    /// A bit per slot the interface frame armed and has not released.
    armed: AtomicU32,
    /// The bits the last pass set in the mask, 0 for none.
    written: AtomicU64,
}

static PRESSES: Presses = Presses {
    slots: [const { Slot::new() }; SLOTS],
    armed: AtomicU32::new(0),
    written: AtomicU64::new(0),
};

/// Hands mod `owner`'s press `id` of `bit` for `hold_ms` to a free slot;
/// whether one took it, false while every slot holds a press.
pub(crate) fn hand(id: u64, owner: ModIndex, bit: u8, hold_ms: u32) -> bool {
    for (at, slot) in PRESSES.slots.iter().enumerate() {
        let armed = slot.handoff.arm(id, stage::PRESS, || {
            slot.bit.store(bit, Ordering::Relaxed);
            slot.hold_ms.store(hold_ms, Ordering::Relaxed);
            slot.owner.store(owner.get(), Ordering::Relaxed);
        });
        if armed {
            PRESSES.armed.fetch_or(1 << at, Ordering::Release);
            return true;
        }
    }
    false
}

/// Where the builder stands with press `id`; `None` once it is no longer
/// handed.
pub(crate) fn progress(id: u64) -> Option<Progress> {
    PRESSES
        .slots
        .iter()
        .find_map(|slot| slot.handoff.stage(id))
        .map(|handed| match handed {
            stage::PRESSING => Progress::Taken,
            stage::DONE => Progress::Done,
            stage::PLAYER => Progress::Cancelled(Status::PLAYER_INPUT),
            stage::BLOCKED => Progress::Cancelled(Status::INPUT_BLOCKED),
            stage::MOD_OFF => Progress::Cancelled(Status::MOD_OFF),
            _ => Progress::Waiting,
        })
}

/// Takes press `id` back: from the builder's next pass on, its bit is the
/// game's own again. The commands' lock orders it with [`hand`].
pub(crate) fn release(id: u64) {
    for (at, slot) in PRESSES.slots.iter().enumerate() {
        if slot.handoff.release(id).is_some() {
            PRESSES.armed.fetch_and(!(1 << at), Ordering::Release);
        }
    }
}

/// The bits the builder's last pass set in the mask for presses.
pub(crate) fn written() -> u64 {
    PRESSES.written.load(Ordering::Relaxed)
}

/// Whether a pass has work: a press is armed, or the last pass wrote bits
/// that [`written`] must drop.
#[inline]
pub(crate) fn pending() -> bool {
    PRESSES.armed.load(Ordering::Acquire) != 0 || PRESSES.written.load(Ordering::Relaxed) != 0
}

/// What the game's own step pass gave, read before any write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Game {
    blocked: bool,
    /// The game's own buttons hold one the press's start did not: the
    /// player pressed a key.
    player: bool,
    owner_off: bool,
}

/// What a step pass does with a press.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pass {
    /// Nothing: the press ended already.
    Skip,
    /// Ends the press at the stage, writing nothing.
    End(u8),
    /// Starts the press: holds its bit until the deadline.
    Start(u64),
    /// Holds the bit of a running press.
    Hold,
}

/// The step pass for a press at `stage` of a hold of `hold` ms and the
/// deadline `until`, given what the game gave, at `now`.
fn pass(stage: u8, hold: u64, until: u64, game: Game, now: u64) -> Pass {
    if !matches!(stage, stage::PRESS | stage::PRESSING) {
        return Pass::Skip;
    }
    if game.owner_off {
        return Pass::End(stage::MOD_OFF);
    }
    if game.blocked {
        return Pass::End(stage::BLOCKED);
    }
    if game.player {
        return Pass::End(stage::PLAYER);
    }
    match stage {
        stage::PRESS => Pass::Start(now.saturating_add(hold)),
        _ if now >= until => Pass::End(stage::DONE),
        _ => Pass::Hold,
    }
}

/// The bit `bit` of a mask; none for a bit past it.
fn mask(bit: u8) -> u64 {
    1u64.checked_shl(u32::from(bit)).unwrap_or(0)
}

/// Runs the builder's step pass on every armed press and sets the bits of
/// those that hold into the mask the step rewrote. A frame pass writes only
/// the camera's look and leaves the mask, and so the bits [`written`]
/// names, as the step left them (client 26430625), so it does nothing.
///
/// # Safety
///
/// - `input` is the local player's live `ClientPlayerInput` the builder
///   just wrote, and `mapping` the input mapping runtime it took, on the
///   builder's thread right after the game's pass.
#[inline(never)]
pub(crate) unsafe fn drive(
    layout: &InputLayout,
    buttons: &Buttons,
    input: *mut u8,
    mapping: *const u8,
    mode: u32,
) {
    if mode != STEP {
        return;
    }
    // SAFETY: the caller's record, just written by the game's pass.
    let own = unsafe { buttons.digital_input(input) };
    // SAFETY: the caller's contract: the mapping the builder took.
    let blocked = unsafe { layout.blocked(mapping) };
    let now = local_held::now_ms();
    let armed = PRESSES.armed.load(Ordering::Acquire);
    let mut ours = 0u64;
    for (at, slot) in PRESSES.slots.iter().enumerate() {
        if armed & 1 << at == 0 {
            continue;
        }
        let Some((id, stage)) = slot.handoff.current() else {
            continue;
        };
        let bit = mask(slot.bit.load(Ordering::Relaxed));
        let game = Game {
            blocked,
            player: stage == stage::PRESSING && own & !slot.baseline.load(Ordering::Relaxed) != 0,
            owner_off: crate::engine::loader::is_off(ModIndex::new(
                slot.owner.load(Ordering::Relaxed),
            )),
        };
        let hold = u64::from(slot.hold_ms.load(Ordering::Relaxed));
        let until = slot.until.load(Ordering::Relaxed);
        match pass(stage, hold, until, game, now) {
            Pass::Skip => {}
            Pass::End(to) => {
                let _ended = slot.handoff.advance(id, stage, to);
            }
            Pass::Start(until) => {
                slot.until.store(until, Ordering::Relaxed);
                slot.baseline.store(own, Ordering::Relaxed);
                if slot.handoff.advance(id, stage, stage::PRESSING) {
                    ours |= bit;
                }
            }
            Pass::Hold => ours |= bit,
        }
    }
    if ours != 0 {
        // SAFETY: the caller's record, on the builder's thread.
        unsafe { buttons.set_digital_input(input, own | ours) };
    }
    PRESSES.written.store(ours, Ordering::Relaxed);
}

#[cfg(test)]
pub(super) mod tests {
    use enshwick_base::Relock;

    use super::*;
    use crate::actions::client_control::pass::FRAME;

    /// Held by every test that arms the process's slots, so tests in one
    /// process never see each other's presses.
    pub(crate) static TEST_SLOTS: std::sync::Mutex<()> = std::sync::Mutex::new(());

    const IDLE: Game = Game {
        blocked: false,
        player: false,
        owner_off: false,
    };

    #[test]
    fn a_press_starts_in_its_first_step_holds_and_ends_at_its_deadline() {
        assert_eq!(pass(stage::PRESS, 16, 0, IDLE, 100), Pass::Start(116));
        assert_eq!(pass(stage::PRESSING, 16, 116, IDLE, 115), Pass::Hold);
        assert_eq!(
            pass(stage::PRESSING, 16, 116, IDLE, 116),
            Pass::End(stage::DONE),
            "the mask is the game's own in the deadline's step"
        );
    }

    #[test]
    fn a_one_millisecond_press_holds_for_exactly_one_step() {
        let Pass::Start(until) = pass(stage::PRESS, 1, 0, IDLE, 50) else {
            panic!("a press starts in its first step");
        };
        assert_eq!(
            pass(stage::PRESSING, 1, until, IDLE, 50 + 16),
            Pass::End(stage::DONE)
        );
    }

    #[test]
    fn the_player_s_keys_a_block_and_a_mod_turned_off_each_end_a_press() {
        for (game, ends) in [
            (
                Game {
                    player: true,
                    ..IDLE
                },
                stage::PLAYER,
            ),
            (
                Game {
                    blocked: true,
                    ..IDLE
                },
                stage::BLOCKED,
            ),
            (
                Game {
                    owner_off: true,
                    player: true,
                    ..IDLE
                },
                stage::MOD_OFF,
            ),
        ] {
            assert_eq!(
                pass(stage::PRESSING, 16, 500, game, 100),
                Pass::End(ends),
                "{game:?}"
            );
            assert_eq!(pass(stage::PRESS, 16, 0, game, 100), Pass::End(ends));
        }
        assert_eq!(
            pass(stage::DONE, 16, 500, IDLE, 100),
            Pass::Skip,
            "an ended press is left for the frame to read"
        );
    }

    #[test]
    fn a_bit_past_the_mask_writes_nothing() {
        assert_eq!(mask(32), 1 << 32);
        assert_eq!(mask(64), 0);
    }

    /// Runs one pass in `mode` over a record whose game mask is `game`, at
    /// the stand-in layout; the mask after it.
    fn run(mode: u32, game: u64) -> u64 {
        use crate::actions::client_control::{BENCH_BUTTONS, BENCH_INPUT};
        let mut input = vec![0u8; 0x600];
        let mapping = vec![0u8; 0xb000];
        // SAFETY: a record of the build's size, not shared.
        unsafe { BENCH_BUTTONS.set_digital_input(input.as_mut_ptr(), game) };
        // SAFETY: the same record and a mapping past its blocked byte.
        unsafe {
            drive(
                &BENCH_INPUT,
                &BENCH_BUTTONS,
                input.as_mut_ptr(),
                mapping.as_ptr(),
                mode,
            );
        };
        // SAFETY: the same record.
        unsafe { BENCH_BUTTONS.digital_input(input.as_ptr()) }
    }

    #[test]
    fn two_presses_hold_together_beside_the_game_s_bits_and_leave_nothing_after_release() {
        let _slots = TEST_SLOTS.relock();
        let (jump, sprint) = (32, 53);
        assert!(hand(70, ModIndex::LOADER, jump, 1000));
        assert!(hand(71, ModIndex::LOADER, sprint, 1000));
        assert_eq!(progress(70), Some(Progress::Waiting));
        let both = mask(jump) | mask(sprint);
        assert_eq!(run(STEP, 0), both, "both start in the step pass");
        assert_eq!(progress(71), Some(Progress::Taken));
        assert_eq!(written(), both, "the held read can drop them");
        release(70);
        assert_eq!(run(STEP, 0), mask(sprint));
        assert_eq!(progress(70), None);
        release(71);
        assert!(pending(), "the last pass wrote bits to drop");
        assert_eq!(run(STEP, 0), 0, "the mask is the game's own again");
        assert_eq!(written(), 0);
        assert!(!pending());
    }

    #[test]
    fn a_frame_pass_after_a_release_leaves_the_last_step_s_bits_as_the_loader_s() {
        let _slots = TEST_SLOTS.relock();
        let jump = 32;
        assert!(hand(90, ModIndex::LOADER, jump, 1000));
        assert_eq!(run(STEP, 0), mask(jump));
        release(90);
        assert_eq!(
            run(FRAME, mask(jump)),
            mask(jump),
            "the frame pass leaves the mask the step wrote"
        );
        assert_eq!(
            written(),
            mask(jump),
            "so the held read still drops the bit as the loader's"
        );
        assert_eq!(run(STEP, 0), 0);
        assert_eq!(written(), 0);
    }

    #[test]
    fn a_player_s_new_key_ends_a_press_and_its_bit_is_not_written() {
        let _slots = TEST_SLOTS.relock();
        let quick_use = 1 << 14;
        assert!(hand(80, ModIndex::LOADER, 5, 1000));
        assert_eq!(run(STEP, 0), mask(5));
        assert_eq!(
            run(STEP, quick_use),
            quick_use,
            "the player's QuickUse0 only"
        );
        assert_eq!(
            progress(80),
            Some(Progress::Cancelled(Status::PLAYER_INPUT))
        );
        release(80);
        assert_eq!(run(STEP, 0), 0);
    }

    #[test]
    fn a_key_the_player_held_when_the_press_started_does_not_end_it() {
        let _slots = TEST_SLOTS.relock();
        let block = 1 << 30;
        assert!(hand(81, ModIndex::LOADER, 32, 1000));
        assert_eq!(run(STEP, block), block | mask(32), "it starts beside it");
        assert_eq!(run(STEP, block), block | mask(32), "and holds");
        assert_eq!(progress(81), Some(Progress::Taken));
        release(81);
        assert_eq!(run(STEP, 0), 0);
    }
}
