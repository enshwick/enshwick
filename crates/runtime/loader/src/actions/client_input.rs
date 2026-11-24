//! The look, the short move and the presses client commands ask for,
//! written into the local player's input right after the game's own input
//! builder, so the player's input reads first and wins.
//!
//! The builder runs per game step (mode 0, the stick and the buttons) and
//! per frame (mode 1, the look), on the interface's thread (client
//! 26430625, seen live). While a loader window shows over the world, the
//! player's input is held here first
//! ([`crate::engine::screens::holds_world`]), as the game holds it while it
//! blocks the input. Until a mod hands one over and with no window, its
//! detour adds four atomic loads.

mod presses;

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};

use enshwick_core::abi::{ClientCommandStatus as Status, HookFlags};

use crate::actions::client_commands::{Handoff, Progress};
use crate::actions::client_control::pass::{FRAME, STEP};
use crate::actions::client_control::{GROUPS, InputLayout, Look};
use crate::actions::local_held;
use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, Target};
use crate::engine::mod_index::ModIndex;
use crate::engine::pinned;

/// The feature's name in the log and on the Mods screen.
const FEATURE: &str = "the client's look, move and press commands";
/// `cameraRotationInput`'s mouse slot, which takes radians per frame with
/// no time step (client 26430625).
const MOUSE: usize = 2;

/// The stages of a handed look or move.
mod stage {
    /// A look waits for the next frame pass.
    pub(super) const LOOK: u8 = 1;
    /// A move waits for the next step pass.
    pub(super) const MOVE: u8 = 2;
    /// A move holds its direction until its deadline.
    pub(super) const MOVING: u8 = 3;
    /// The look was written whole, or the move's hold ran out.
    pub(super) const DONE: u8 = 4;
    /// The player's own stick or look input was not zero.
    pub(super) const PLAYER: u8 = 5;
    /// The game blocked the player's input.
    pub(super) const BLOCKED: u8 = 6;
    /// The command's mod turned off.
    pub(super) const MOD_OFF: u8 = 7;
}

/// What the builder is handed, beside the handoff's word.
struct Drive {
    handoff: Handoff,
    /// A look's yaw and pitch or a move's direction, as `f32` bits.
    x: AtomicU32,
    y: AtomicU32,
    hold_ms: AtomicU64,
    /// When a move's hold ends, as [`local_held::now_ms`] counts.
    until: AtomicU64,
    /// The command's mod, as its load-order index.
    owner: AtomicUsize,
    linked: AtomicBool,
}

static DRIVE: Drive = Drive {
    handoff: Handoff::new(),
    x: AtomicU32::new(0),
    y: AtomicU32::new(0),
    hold_ms: AtomicU64::new(0),
    until: AtomicU64::new(0),
    owner: AtomicUsize::new(0),
    linked: AtomicBool::new(false),
};
static FUSE: Fuse = Fuse::new(FEATURE);

/// Links the builder's detour on a client whose build has the input group.
pub(crate) fn link(loader: &Loader) {
    if GROUPS.input.get().is_none() {
        return;
    }
    let target = Target::Pinned(pinned::input::CLIENT_BUILD);
    let linked = loader.link_feature(&FUSE, target, &NEXT, client_build, HookFlags::NONE);
    DRIVE.linked.store(linked, Ordering::Release);
}

/// Whether the build runs looks and moves: the input group and the
/// builder's detour are linked and on.
pub(crate) fn runs() -> bool {
    GROUPS.input.get().is_some() && DRIVE.linked.load(Ordering::Acquire) && !FUSE.is_off()
}

/// Whether the build runs presses: looks and moves run, and the buttons
/// group is linked and on.
pub(crate) fn presses_run() -> bool {
    runs() && GROUPS.buttons.get().is_some()
}

pub(crate) use presses::{hand as press, progress as press_progress, release as unpress, written};

/// What a command hands the builder.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Ask {
    /// Turns by yaw and pitch radians once.
    Look([f32; 2]),
    /// Holds the direction for the milliseconds.
    Move([f32; 2], u32),
}

/// Hands mod `owner`'s command `id` to the builder; whether it took it,
/// false while another look or move runs.
pub(crate) fn hand(id: u64, owner: ModIndex, ask: Ask) -> bool {
    let (stage, [x, y], hold) = match ask {
        Ask::Look(look) => (stage::LOOK, look, 0),
        Ask::Move(direction, hold) => (stage::MOVE, direction, hold),
    };
    DRIVE.handoff.arm(id, stage, || {
        DRIVE.x.store(x.to_bits(), Ordering::Relaxed);
        DRIVE.y.store(y.to_bits(), Ordering::Relaxed);
        DRIVE.hold_ms.store(u64::from(hold), Ordering::Relaxed);
        DRIVE.owner.store(owner.get(), Ordering::Relaxed);
    })
}

/// Where the builder stands with command `id`; `None` once it is no
/// longer handed.
pub(crate) fn progress(id: u64) -> Option<Progress> {
    DRIVE.handoff.stage(id).map(|handed| match handed {
        stage::MOVING => Progress::Taken,
        stage::DONE => Progress::Done,
        stage::PLAYER => Progress::Cancelled(Status::PLAYER_INPUT),
        stage::BLOCKED => Progress::Cancelled(Status::INPUT_BLOCKED),
        stage::MOD_OFF => Progress::Cancelled(Status::MOD_OFF),
        _ => Progress::Waiting,
    })
}

/// Takes command `id` back from the builder: from its next pass on, the
/// stick and the look are the game's own again.
pub(crate) fn release(id: u64) {
    let _had = DRIVE.handoff.release(id);
}

/// What the builder's pass does with a handed command.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Pass {
    /// Nothing in this mode.
    Skip,
    /// Ends the command at the stage, writing nothing.
    End(u8),
    /// Writes the look and ends the command done.
    Look([f32; 2]),
    /// Starts the move: holds until the deadline, writing the stick.
    Start([f32; 2], u64),
    /// Writes the stick of a running move.
    Hold([f32; 2]),
}

/// What the game's own pass gave, read before any write.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Game {
    mode: u32,
    blocked: bool,
    /// The game's own stick (step) or look (frame) was not zero.
    player: bool,
    owner_off: bool,
}

/// The pass for a command at `stage` carrying `[x, y]`, a hold of `hold`
/// ms and the deadline `until`, given what the game gave, at `now`.
fn pass(stage: u8, [x, y]: [f32; 2], hold: u64, until: u64, game: Game, now: u64) -> Pass {
    if !matches!(stage, stage::LOOK | stage::MOVE | stage::MOVING) {
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
    match (stage, game.mode) {
        (stage::LOOK, FRAME) => Pass::Look([x, y]),
        (stage::MOVE, STEP) => Pass::Start([x, y], now.saturating_add(hold)),
        (stage::MOVING, STEP) if now >= until => Pass::End(stage::DONE),
        (stage::MOVING, STEP) => Pass::Hold([x, y]),
        _ => Pass::Skip,
    }
}

/// The look `game` written with a turn of `yaw` and `pitch` radians in its
/// mouse slot: a turn to the right is a negative mouse x (client 26430625,
/// seen live).
fn turned(mut game: Look, [yaw, pitch]: [f32; 2]) -> Look {
    game[MOUSE][0] -= yaw;
    game[MOUSE][1] += pitch;
    game
}

/// Runs the builder's pass in `mode` on handed command `id` at `stage`.
///
/// # Safety
///
/// - `input` is the local player's live `ClientPlayerInput` the builder
///   just wrote, and `mapping` the input mapping runtime it took, on the
///   builder's thread right after the game's pass.
#[inline(never)]
unsafe fn drive(
    layout: &InputLayout,
    input: *mut u8,
    mapping: *const u8,
    mode: u32,
    (id, stage): (u64, u8),
) {
    // SAFETY: the caller's contract: the mapping the builder took.
    let blocked = unsafe { layout.blocked(mapping) };
    // SAFETY: the caller's record, just written by the game's pass.
    let look = unsafe { layout.camera_rotation(input) };
    // SAFETY: the same record.
    let stick = unsafe { layout.movement_stick(input) };
    let player = match mode {
        STEP => stick.iter().any(|value| *value != 0.0),
        FRAME => look.iter().flatten().any(|value| *value != 0.0),
        _ => false,
    };
    let game = Game {
        mode,
        blocked,
        player,
        owner_off: crate::engine::loader::is_off(ModIndex::new(
            DRIVE.owner.load(Ordering::Relaxed),
        )),
    };
    let carried = [
        f32::from_bits(DRIVE.x.load(Ordering::Relaxed)),
        f32::from_bits(DRIVE.y.load(Ordering::Relaxed)),
    ];
    let now = local_held::now_ms();
    let hold = DRIVE.hold_ms.load(Ordering::Relaxed);
    let until = DRIVE.until.load(Ordering::Relaxed);
    match pass(stage, carried, hold, until, game, now) {
        Pass::Skip => {}
        Pass::End(to) => {
            let _ended = DRIVE.handoff.advance(id, stage, to);
        }
        Pass::Look(turn) => {
            if DRIVE.handoff.advance(id, stage, stage::DONE) {
                // SAFETY: the caller's record, on the builder's thread.
                unsafe { layout.set_camera_rotation(input, turned(look, turn)) };
            }
        }
        Pass::Start(direction, until) => {
            DRIVE.until.store(until, Ordering::Relaxed);
            if DRIVE.handoff.advance(id, stage, stage::MOVING) {
                // SAFETY: the caller's record, on the builder's thread.
                unsafe { layout.set_movement_stick(input, direction) };
            }
        }
        Pass::Hold(direction) => {
            if DRIVE.handoff.stage(id) == Some(stage::MOVING) {
                // SAFETY: the caller's record, on the builder's thread.
                unsafe { layout.set_movement_stick(input, direction) };
            }
        }
    }
}

crate::engine::chains::chain_detour! {
    /// The client's input builder `(input, consumer, consumer2, _, mapping,
    /// settings, camera, mode, ..)`, twelve arguments and no answer (client
    /// 26430625). The game's pass runs first, then the hold while a loader
    /// window shows ([`crate::engine::screens::holds_world`]); a handed look
    /// or move and the presses are written after both, so they still work
    /// under a window.
    fn client_build(
        input: *mut u8,
        consumer: usize,
        other: usize,
        a4: usize,
        mapping: *const u8,
        settings: usize,
        camera: usize,
        mode: u32,
        a9: usize,
        a10: usize,
        a11: u32,
        a12: usize,
    ) -> ();
    next NEXT;
    fuse &FUSE, "the client's look, move and presses";
    |onward| {
        onward.call();
        if crate::engine::screens::holds_world()
            && !input.is_null()
            && let Some(layout) = GROUPS.input.get()
        {
            // SAFETY: the builder's own non-null input record, right after
            // its pass on its thread, which nothing else touches meanwhile.
            unsafe { layout.hold(input, mode) };
            if let Some(buttons) = GROUPS.buttons.get() {
                // SAFETY: the builder's own non-null input record, right
                // after its pass on its thread, which nothing else touches.
                unsafe { buttons.hold(input) };
            }
        }
        if let Some(handed) = DRIVE.handoff.current()
            && let Some(layout) = GROUPS.input.get()
            && !input.is_null()
            && !mapping.is_null()
        {
            // SAFETY: the builder's own non-null input record and mapping,
            // right after its pass on its thread.
            unsafe { drive(layout, input, mapping, mode, handed) };
        }
        if presses::pending()
            && !input.is_null()
            && !mapping.is_null()
            && let Some(layout) = GROUPS.input.get()
            && let Some(buttons) = GROUPS.buttons.get()
        {
            // SAFETY: the builder's own non-null input record and mapping,
            // right after its pass on its thread.
            unsafe { presses::drive(layout, buttons, input, mapping, mode) };
        }
    }
}

/// Readies the builder's detour for its benchmark with a stand-in for the
/// game's pass: the closure runs one step pass and answers the stick's
/// forward bits and the buttons, with nothing handed, a move held or a
/// move and two presses held.
#[cfg(any(test, feature = "test"))]
#[doc(hidden)]
pub fn bench_client_build(moving: bool, pressing: bool) -> impl FnMut() -> (u32, u64) {
    /// Stands in for the game's step pass: rewrites the stick and the
    /// buttons as the player's, zero, as the game does each step, so the
    /// detour sees no input of the player's and its move and presses hold.
    unsafe extern "system" fn game(
        input: *mut u8,
        _: usize,
        _: usize,
        _: usize,
        _: *const u8,
        _: usize,
        _: usize,
        _: u32,
        _: usize,
        _: usize,
        _: u32,
        _: usize,
    ) {
        let stick = crate::actions::client_control::BENCH_INPUT.stick_offset();
        // SAFETY: the benchmark's own record, of the stand-in layout's size.
        unsafe { enshwick_core::raw::write_at(input, stick, [0.0_f32; 2]) };
        // SAFETY: the same record, which holds the stand-in's buttons.
        unsafe { crate::actions::client_control::BENCH_BUTTONS.set_digital_input(input, 0) };
    }
    type Build = unsafe extern "system" fn(
        *mut u8,
        usize,
        usize,
        usize,
        *const u8,
        usize,
        usize,
        u32,
        usize,
        usize,
        u32,
        usize,
    );
    let next: Build = game;
    #[expect(
        clippy::fn_to_numeric_cast_any,
        reason = "a slot holds its next function's address"
    )]
    NEXT.slot().store(next as usize, Ordering::Release);
    crate::actions::client_control::bench_input();
    if moving {
        let _armed = hand(1, ModIndex::LOADER, Ask::Move([0.0, 1.0], 250));
        DRIVE.until.store(u64::MAX, Ordering::Relaxed);
        let _moving = DRIVE.handoff.advance(1, stage::MOVE, stage::MOVING);
    }
    if pressing {
        // Jump and Sprint, held past the benchmark's run.
        let _jump = press(2, ModIndex::LOADER, 32, u32::MAX);
        let _sprint = press(3, ModIndex::LOADER, 53, u32::MAX);
    }
    let mut input = vec![0u8; 0x600];
    let mapping = vec![0u8; 0xb000];
    move || {
        let (record, map) = (input.as_mut_ptr(), mapping.as_ptr());
        // SAFETY: the detour with a stand-in pass, on a record of the
        // build's size and a mapping past its blocked byte.
        unsafe { client_build(record, 0, 0, 0, map, 0, 0, STEP, 0, 0, 0, 0) };
        let Some(layout) = GROUPS.input.get() else {
            return (0, 0);
        };
        // SAFETY: the same record, of the build's size.
        let [_, forward] = unsafe { layout.movement_stick(input.as_ptr()) };
        // SAFETY: the same record, which holds the stand-in's buttons.
        let buttons =
            unsafe { crate::actions::client_control::BENCH_BUTTONS.digital_input(input.as_ptr()) };
        (forward.to_bits(), buttons)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn game(mode: u32) -> Game {
        Game {
            mode,
            blocked: false,
            player: false,
            owner_off: false,
        }
    }

    #[test]
    fn the_benchmark_s_move_holds_its_stick_through_every_pass_it_times() {
        let _slots = enshwick_base::Relock::relock(&presses::tests::TEST_SLOTS);
        let mut moving = bench_client_build(true, true);
        for pass in 0..4 {
            assert_eq!(
                moving(),
                (1.0_f32.to_bits(), 1 << 32 | 1 << 53),
                "pass {pass}"
            );
        }
        unpress(2);
        unpress(3);
    }

    #[test]
    fn a_look_writes_once_in_the_frame_pass_and_waits_through_a_step() {
        let turn = [0.5, -0.25];
        assert_eq!(pass(stage::LOOK, turn, 0, 0, game(STEP), 10), Pass::Skip);
        assert_eq!(
            pass(stage::LOOK, turn, 0, 0, game(FRAME), 10),
            Pass::Look(turn)
        );
    }

    #[test]
    fn a_move_starts_in_the_step_pass_holds_and_ends_at_its_deadline() {
        let forward = [0.0, 1.0];
        assert_eq!(
            pass(stage::MOVE, forward, 250, 0, game(STEP), 100),
            Pass::Start(forward, 350)
        );
        assert_eq!(
            pass(stage::MOVING, forward, 250, 350, game(FRAME), 200),
            Pass::Skip
        );
        assert_eq!(
            pass(stage::MOVING, forward, 250, 350, game(STEP), 349),
            Pass::Hold(forward)
        );
        assert_eq!(
            pass(stage::MOVING, forward, 250, 350, game(STEP), 350),
            Pass::End(stage::DONE),
            "the stick is the game's own in the deadline's step"
        );
    }

    #[test]
    fn the_player_s_input_a_block_and_a_mod_turned_off_each_end_a_command() {
        let forward = [0.0, 1.0];
        for (changed, ends) in [
            (
                Game {
                    player: true,
                    ..game(STEP)
                },
                stage::PLAYER,
            ),
            (
                Game {
                    player: true,
                    ..game(FRAME)
                },
                stage::PLAYER,
            ),
            (
                Game {
                    blocked: true,
                    ..game(STEP)
                },
                stage::BLOCKED,
            ),
            (
                Game {
                    owner_off: true,
                    ..game(STEP)
                },
                stage::MOD_OFF,
            ),
        ] {
            assert_eq!(
                pass(stage::MOVING, forward, 250, 350, changed, 200),
                Pass::End(ends),
                "{changed:?}"
            );
        }
        assert_eq!(
            pass(
                stage::DONE,
                forward,
                250,
                350,
                Game {
                    player: true,
                    ..game(STEP)
                },
                200
            ),
            Pass::Skip,
            "an ended command is left for the frame to read"
        );
    }

    #[test]
    fn a_turn_to_the_right_is_a_negative_mouse_x_in_the_game_s_look() {
        let look = turned([[0.0; 2]; 5], [0.5, 0.25]);
        assert_eq!(look[MOUSE], [-0.5, 0.25]);
        assert_eq!(look[0], [0.0; 2], "the other devices stay");
    }

    #[test]
    fn a_handed_command_reports_its_stage_until_it_is_released() {
        assert!(hand(40, ModIndex::new(0), Ask::Look([0.1, 0.0])));
        assert_eq!(progress(40), Some(Progress::Waiting));
        assert!(!hand(41, ModIndex::new(0), Ask::Move([0.0, 1.0], 100)));
        assert!(DRIVE.handoff.advance(40, stage::LOOK, stage::DONE));
        assert_eq!(progress(40), Some(Progress::Done));
        release(40);
        assert_eq!(progress(40), None);
        assert!(hand(41, ModIndex::new(0), Ask::Move([0.0, 1.0], 100)));
        release(41);
    }
}
