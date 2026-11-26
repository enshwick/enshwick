//! Transactions of the server's world for mods: terrain changes written as
//! one at quiet moments, with an undo the loader keeps.
//!
//! A mod commits its changes from any thread; the post-step link writes
//! them on the game thread, a budget of 8³-voxel bricks a step. Before a
//! change writes, the loader reads every brick it can touch through the
//! game's box read, which also tells whether the bricks are loaded: an edit
//! over a brick that is not starts from air and writes the result back, so
//! a transaction checks that its whole region is loaded before it writes
//! anything. An undo writes the kept bricks back through the editor's own
//! undo write, which replicates and saves like any edit, newest first, and
//! only while every brick still holds what the transaction left there.

mod block;
pub(crate) mod cover;
mod error;
#[cfg(windows)]
mod game;
mod ledger;
mod look;
mod paste;
pub(crate) mod props;
mod record;
mod settled;

use core::time::Duration;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

#[cfg(test)]
use enshwick_core::abi::TransactionInfo;

use crate::engine::fuse::Fuse;
use crate::engine::mod_index::ModIndex;
use crate::engine::pinned;
use enshwick_base::Relock;
pub(crate) use error::TransactionError;
#[cfg(windows)]
pub(crate) use game::{prop_placement_info, resolve, resolve_prop, saving};
#[cfg(any(windows, test, feature = "test"))]
pub(crate) use ledger::Change;
pub(crate) use ledger::Standing;
use ledger::{Ledger, Limits};
use look::{LedgerGuard, Look};
#[cfg(windows)]
pub(crate) use paste::{Erase, Paste};
pub(crate) use settled::End;
use settled::{Wait, Waits};

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::building::BLOCK_BOX.id(),
    pinned::building::BOX_EXTENT.id(),
    pinned::building::BOX_MIN.id(),
    pinned::building::BOX_SIZE.id(),
    pinned::building::BLUEPRINT_LOOKUP.id(),
    pinned::building::MATERIAL_FEEDBACK.id(),
    pinned::server::BLUEPRINT_REGISTRY.id(),
    pinned::server::PLAYER_INFOS.id(),
    pinned::voxel::WRITE_BLOCK.id(),
    pinned::terrain::ADD.id(),
    pinned::terrain::REGISTRY.id(),
    pinned::terrain::DIG.id(),
    pinned::terrain::FLATTEN.id(),
    pinned::voxel::READ_BOX.id(),
    pinned::voxel::UNDO_APPLY.id(),
    pinned::voxel::UNDO_EXTENT.id(),
];

/// The bricks a step aims to write, before-images and after-reads
/// included: a target, not a cap, since a step holds at least one whole
/// change, however large.
const BRICKS_PER_STEP: usize = 64;

/// What the loader keeps: 1,024 changes in a transaction, 64 MiB of undo
/// data (65,536 bricks of 1 KiB), the newest 4,096 transactions; an undo
/// waits 3,600 steps in a row for a column to load before it is refused.
pub(crate) const LIMITS: Limits = Limits {
    ops: 1024,
    bricks: 65_536,
    records: 4096,
    waits: 3600,
};

/// The transactions of the running world.
struct Transactions {
    /// Whether the ledger holds work, so a step without any takes no lock;
    /// written only under the ledger's lock.
    busy: AtomicBool,
    ledger: Mutex<Ledger>,
    /// Off after a pass panicked: no more passes, commits or undos.
    fuse: Fuse,
    /// The props its removals and undos keep players from.
    holds: crate::world::prop_holds::PropHolds,
    /// Whether mods wait for ends, so a step without waits takes no lock;
    /// written only under the waits' lock.
    waiting: AtomicBool,
    /// The mods' waits for their transactions' ends. Its lock is taken
    /// before the ledger's, never after.
    waits: Mutex<Waits>,
    /// What the last look at the waits saw, so a step looks again only
    /// once something is new.
    look: Look,
    /// Whether the interaction filter that keeps players from held props is
    /// off, named by the interactions when they seal.
    filter_off: std::sync::OnceLock<fn() -> bool>,
}

impl Transactions {
    const fn new() -> Self {
        Self {
            busy: AtomicBool::new(false),
            ledger: Mutex::new(Ledger::new(LIMITS)),
            fuse: Fuse::new("world transactions"),
            holds: crate::world::prop_holds::PropHolds::new(),
            waiting: AtomicBool::new(false),
            waits: Mutex::new(Waits::new()),
            look: Look::new(),
            filter_off: std::sync::OnceLock::new(),
        }
    }

    /// The ledger, its mutable uses counted for the waits' look.
    fn ledger(&self) -> LedgerGuard<'_> {
        LedgerGuard {
            guard: self.ledger.relock(),
            look: &self.look,
            changed: false,
        }
    }

    /// [`Transactions::ledger`] without waiting: `None` while another
    /// thread holds it.
    fn try_ledger(&self) -> Option<LedgerGuard<'_>> {
        Some(LedgerGuard {
            guard: self.ledger.try_relock()?,
            look: &self.look,
            changed: false,
        })
    }

    /// Keeps `wait` for the steps to come; [`TransactionError::Unavailable`]
    /// once the feature is off.
    fn add_wait(&self, wait: Wait) -> Result<(), TransactionError> {
        let mut waits = self.waits.relock();
        // Under the waits' lock, so a fuse turning off now runs this one too.
        if self.fuse.is_off() {
            return Err(TransactionError::Unavailable);
        }
        waits.add(wait);
        self.waiting.store(true, Ordering::Release);
        self.look.ask();
        Ok(())
    }

    /// Runs the waits that end at `now`, in the order asked, outside the
    /// locks so a callback may ask again or read the ledger. A step looks
    /// only once the ledger changed, a mod turned off or a wait's time came
    /// since the last look; one that finds either lock taken leaves them to
    /// the next, as the pass does, so the game thread never waits on a
    /// mod's thread.
    fn run_due(&self, now: Instant) {
        if !self.waiting.load(Ordering::Acquire) {
            return;
        }
        let offs = offs();
        let Some(changes) = self.look.due(now, offs) else {
            return;
        };
        let Some(mut waits) = self.waits.try_relock() else {
            return;
        };
        let Some(ledger) = self.ledger.try_relock() else {
            return;
        };
        let due = waits.due(now, |owner, id| ledger.info(owner, id), is_off);
        self.look.looked(changes, offs, waits.earliest());
        self.waiting.store(!waits.is_empty(), Ordering::Release);
        drop(ledger);
        drop(waits);
        for (wait, standing) in due {
            run(wait, standing.as_ref());
        }
    }

    /// Runs every wait as its transaction stands, as the feature turns off.
    fn run_all(&self) {
        let all = {
            let mut waits = self.waits.relock();
            self.waiting.store(false, Ordering::Release);
            waits.take_all()
        };
        for wait in all {
            let standing = self.ledger().info(wait.owner, wait.id);
            run(wait, standing.as_ref());
        }
    }
}

#[cfg(windows)]
use crate::engine::loader::is_off;

/// Off Windows no loader turns a mod off.
#[cfg(not(windows))]
const fn is_off(_: ModIndex) -> bool {
    false
}

/// How many mods the running loader turned off so far.
#[cfg(windows)]
fn offs() -> u64 {
    crate::engine::loader::loader().map_or(0, crate::engine::loader::Loader::offs)
}

/// Off Windows no loader turns a mod off.
#[cfg(not(windows))]
const fn offs() -> u64 {
    0
}

/// Runs `wait`'s callback as its mod's, unless the loader turned the mod
/// off: its callback then never runs, and what it holds in the mod stays.
fn run(wait: Wait, standing: Option<&Standing>) {
    if is_off(wait.owner) {
        return;
    }
    let owner = wait.owner;
    crate::engine::profile::call(owner, crate::engine::profile::Feature::Events, || {
        wait.run(standing);
    });
}

static STATE: Transactions = Transactions::new();

/// The props the running world's transactions keep players from.
pub(crate) fn holds() -> &'static crate::world::prop_holds::PropHolds {
    &STATE.holds
}

/// Names `off`, which tells whether the interaction filter that keeps
/// players from held props is off; the first one named stays.
pub(crate) fn on_filter_off(off: fn() -> bool) {
    let _first = STATE.filter_off.set(off);
}

/// Whether the interaction filter that keeps players from held props is
/// off; on until the interactions name it.
pub(crate) fn filter_off() -> bool {
    STATE.filter_off.get().is_some_and(|off| off())
}

/// Runs a step's `work` behind the feature's fuse. A panic turns the
/// feature off and settles every transaction held, since its work in hand
/// is lost: those in work fail, the rest expire; then every wait runs.
fn guarded(state: &Transactions, work: impl FnOnce()) {
    if state.fuse.run("the post-step link", work).is_none() && state.fuse.is_off() {
        {
            let mut ledger = state.ledger();
            if ledger.busy() || ledger.kept() > 0 {
                ledger.forget();
            }
            state.busy.store(false, Ordering::Release);
        }
        let deposits = state.holds.release_all();
        #[cfg(windows)]
        game::restore_deposits(&deposits);
        #[cfg(not(windows))]
        drop(deposits);
        state.run_all();
    }
}

/// Runs `end` once for transaction `id` of `owner`, after the first step
/// that ends with it no longer pending, or after `within`;
/// [`TransactionError::Unavailable`] once the feature is off.
pub(crate) fn when_settled(
    owner: ModIndex,
    id: u64,
    within: Duration,
    end: End,
) -> Result<(), TransactionError> {
    let until = Instant::now().checked_add(within);
    STATE.add_wait(Wait::new(owner, id, until, end))
}

/// For the benchmark of what a step pays for its waits: a step's look at
/// `pending` waits, each on a transaction still pending, none due.
#[cfg(feature = "test")]
#[doc(hidden)]
pub fn bench_settle_step(pending: usize) -> impl Fn() {
    let state: &'static Transactions = Box::leak(Box::new(Transactions::new()));
    let dig = enshwick_abi::headed!(enshwick_core::abi::TerrainEdit {
        kind: enshwick_core::abi::TerrainEdit::DIG,
        shape: enshwick_core::abi::TerrainEdit::ROUND,
        flags: 0,
        material: 0,
        position: [20.0; 3],
        radius: 1.0,
        height: 0.0,
        protect: [0; 16],
    });
    for _ in 0..pending {
        let committed =
            state
                .ledger
                .relock()
                .commit(ModIndex::LOADER, None, vec![Change::Terrain(dig)]);
        if let Ok(id) = committed {
            let _kept = state.add_wait(Wait::new(ModIndex::LOADER, id, None, Box::new(|_| {})));
        }
    }
    // The step hands over its own time, so the look reads no clock.
    let now = Instant::now();
    move || state.run_due(now)
}

/// Queues `changes` as one transaction of `owner`. [`TransactionError::Unavailable`] while
/// no world runs or on a build without the edits, the read or the undo
/// write; otherwise as [`Ledger::commit`].
#[cfg(windows)]
pub(crate) fn commit(
    loader: &crate::engine::loader::Loader,
    owner: ModIndex,
    actor: Option<u32>,
    changes: Vec<Change>,
) -> Result<u64, TransactionError> {
    if STATE.fuse.is_off() || !game::ready(loader) {
        return Err(TransactionError::Unavailable);
    }
    let mut ledger = STATE.ledger();
    let id = ledger.commit(owner, actor, changes)?;
    STATE.busy.store(true, Ordering::Release);
    Ok(id)
}

/// Queues the undo of transaction `id` of `owner`, as [`Ledger::undo`];
/// [`TransactionError::Unavailable`] once the feature is off.
pub(crate) fn undo(owner: ModIndex, id: u64) -> Result<(), TransactionError> {
    undo_in(&STATE, owner, id)
}

fn undo_in(state: &Transactions, owner: ModIndex, id: u64) -> Result<(), TransactionError> {
    if state.fuse.is_off() {
        return Err(TransactionError::Unavailable);
    }
    let mut ledger = state.ledger();
    ledger.undo(owner, id)?;
    state.busy.store(ledger.busy(), Ordering::Release);
    Ok(())
}

/// The entities of the props transaction `id` of `owner` placed and still
/// holds, as [`Ledger::entities`].
pub(crate) fn entities(owner: ModIndex, id: u64) -> Option<Vec<u32>> {
    STATE.ledger().entities(owner, id)
}

/// Where transaction `id` of `owner` stands.
pub(crate) fn info(owner: ModIndex, id: u64) -> Option<Standing> {
    STATE.ledger().info(owner, id)
}

/// The ledger without waiting: `None` while another thread holds it.
#[cfg(windows)]
fn try_ledger() -> Option<LedgerGuard<'static>> {
    STATE.try_ledger()
}

/// The step's work under the feature's own fuse, the one that settles what
/// a panic leaves held, so the step adds no second fuse around it.
#[cfg(windows)]
pub(crate) const fn step() -> crate::engine::parts::Step {
    crate::engine::parts::Step {
        stage: crate::engine::parts::Stage::Output,
        work: crate::engine::parts::Fused {
            fuse: &STATE.fuse,
            run: after_step,
        },
        part: None,
    }
}

/// Writes the queued work of a step, then runs the waits it ended, on the
/// game thread after the step, behind the feature's fuse.
#[cfg(windows)]
fn after_step(stepping: &crate::engine::parts::Stepping) {
    if STATE.busy.load(Ordering::Acquire) {
        guarded(&STATE, || pass(stepping.loader));
    }
    // A pass that panicked already ran every wait as the feature turned off.
    if !STATE.fuse.is_off() {
        guarded(&STATE, || STATE.run_due(stepping.now));
    }
}

/// The step's pass over the queue. The game's edits and undo writes run
/// with the ledger free, so other threads and the game's own callbacks may
/// commit, undo and ask meanwhile; a step that finds the ledger taken skips
/// its work until the next.
#[cfg(windows)]
fn pass(loader: &crate::engine::loader::Loader) {
    use ledger::Voxels;

    let Ok(mut world) = game::Game::new(loader) else {
        return;
    };
    let started = Instant::now();
    let extent = world.extent().ok();
    let Some(mut pass) = try_ledger().map(|mut ledger| ledger.begin(extent, BRICKS_PER_STEP))
    else {
        return;
    };
    while let Some(mut task) = try_ledger().and_then(|mut ledger| ledger.take(&mut pass)) {
        let outcome = crate::world::voxels::changes::by(task.owner.handle(), || {
            task.run(&mut world, &mut pass)
        });
        let mut ledger = STATE.ledger();
        let go_on = ledger.put(task, outcome);
        STATE.busy.store(ledger.busy(), Ordering::Release);
        drop(ledger);
        if !go_on {
            break;
        }
    }
    if let Some(ledger) = try_ledger() {
        STATE.busy.store(ledger.busy(), Ordering::Release);
    }
    if pass.spent > 0 || pass.probes > 0 {
        crate::log!(
            DEBUG,
            "world transactions: {} bricks and {} column probes in {} µs",
            pass.spent,
            pass.probes,
            started.elapsed().as_micros()
        );
    }
}

/// Drops what the loader holds for a world that ended, then runs every
/// wait with its transaction as the world left it, so no wait reaches the
/// next game.
pub(crate) fn forget() {
    forget_in(&STATE);
}

fn forget_in(state: &Transactions) {
    {
        let mut ledger = state.ledger();
        ledger.forget();
        state.busy.store(false, Ordering::Release);
    }
    state.holds.forget();
    state.run_all();
}

#[cfg(test)]
mod tests {
    use enshwick_core::abi::TerrainEdit;

    use super::*;

    const ME: ModIndex = ModIndex::new(0);

    fn dig() -> TerrainEdit {
        enshwick_abi::headed!(TerrainEdit {
            kind: TerrainEdit::DIG,
            shape: TerrainEdit::ROUND,
            flags: 0,
            material: 0,
            position: [20.0; 3],
            radius: 4.0,
            height: 0.0,
            protect: [0; 16],
        })
    }

    #[test]
    fn a_pass_that_panics_turns_transactions_off_and_settles_them() {
        let state = Transactions::new();
        let id = state
            .ledger
            .relock()
            .commit(ME, None, vec![Change::Terrain(dig())])
            .unwrap();
        state.busy.store(true, Ordering::Release);

        guarded(&state, || panic!("in a pass"));
        assert!(state.fuse.is_off());
        assert!(!state.busy.load(Ordering::Acquire));
        let info = state.ledger().info(ME, id).unwrap();
        assert_eq!(
            (info.state, info.reason),
            (TransactionInfo::FAILED, Some(TransactionError::Unavailable))
        );
        assert_eq!(undo_in(&state, ME, id), Err(TransactionError::Unavailable));

        let mut ran = false;
        guarded(&state, || ran = true);
        assert!(!ran, "a turned-off feature runs no more passes");
    }

    #[test]
    fn a_pass_that_panics_lets_every_held_prop_go() {
        let state = Transactions::new();
        state.holds.linked(0, true);
        state.holds.hold(40, false).unwrap();
        state
            .holds
            .note_deposit(40, crate::world::prop_holds::Stash([1, 1]));
        guarded(&state, || panic!("in a pass"));
        assert!(!state.holds.is_held(40));
        assert!(state.holds.quiet(), "players reach it again");
    }

    /// The states the ends of one test saw, in order.
    type Seen = std::sync::Arc<Mutex<Vec<Option<u32>>>>;

    /// An end that notes its transaction's state in `seen`.
    fn noting(seen: &Seen) -> End {
        let seen = std::sync::Arc::clone(seen);
        Box::new(move |standing| seen.relock().push(standing.map(|s| s.state)))
    }

    /// A transaction of the loader's own, which the loader never turns off,
    /// with a wait that notes its end in `seen`.
    fn waited(state: &Transactions, within: Duration, seen: &Seen) -> u64 {
        let id = state
            .ledger
            .relock()
            .commit(ModIndex::LOADER, None, vec![Change::Terrain(dig())])
            .unwrap();
        let until = Instant::now().checked_add(within);
        state
            .add_wait(Wait::new(ModIndex::LOADER, id, until, noting(seen)))
            .unwrap();
        id
    }

    #[test]
    fn a_wait_runs_once_after_the_step_its_transaction_stops_pending() {
        let state = Transactions::new();
        let seen = Seen::default();
        let id = waited(&state, Duration::from_secs(60), &seen);
        state.run_due(Instant::now());
        assert!(seen.relock().is_empty(), "still pending");
        state.ledger().undo(ModIndex::LOADER, id).unwrap();
        state.run_due(Instant::now());
        state.run_due(Instant::now());
        assert_eq!(*seen.relock(), [Some(TransactionInfo::UNDONE)]);
        assert!(!state.waiting.load(Ordering::Acquire), "no wait is left");
    }

    #[test]
    fn a_world_that_ends_runs_every_wait_and_keeps_none_for_the_next() {
        let state = Transactions::new();
        let seen = Seen::default();
        waited(&state, Duration::from_secs(60), &seen);
        forget_in(&state);
        assert_eq!(*seen.relock(), [Some(TransactionInfo::FAILED)]);
        assert!(!state.waiting.load(Ordering::Acquire));
        state.run_due(Instant::now());
        assert_eq!(seen.relock().len(), 1, "the next game runs it no more");
    }

    #[test]
    fn a_step_that_finds_the_ledger_taken_leaves_its_waits_to_the_next() {
        let state = Transactions::new();
        let seen = Seen::default();
        waited(&state, Duration::ZERO, &seen);
        let held = state.ledger();
        state.run_due(Instant::now());
        assert!(seen.relock().is_empty(), "the game thread never waits");
        drop(held);
        state.run_due(Instant::now());
        assert_eq!(*seen.relock(), [Some(TransactionInfo::PENDING)]);
    }

    #[test]
    fn a_wait_past_its_time_runs_with_its_transaction_pending() {
        let state = Transactions::new();
        let seen = Seen::default();
        waited(&state, Duration::ZERO, &seen);
        state.run_due(Instant::now());
        assert_eq!(*seen.relock(), [Some(TransactionInfo::PENDING)]);
    }

    #[test]
    fn a_pass_that_panics_runs_every_wait_and_takes_no_more() {
        let state = Transactions::new();
        let seen = Seen::default();
        let id = waited(&state, Duration::from_secs(60), &seen);
        guarded(&state, || panic!("in a pass"));
        assert_eq!(*seen.relock(), [Some(TransactionInfo::FAILED)]);
        let late = Wait::new(ModIndex::LOADER, id, None, noting(&seen));
        assert_eq!(state.add_wait(late), Err(TransactionError::Unavailable));
    }
}
