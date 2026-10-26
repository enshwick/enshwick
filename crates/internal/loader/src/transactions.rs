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
#[cfg(windows)]
mod game;
mod ledger;
mod paste;
mod record;

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use enshwick::abi::{Status, TransactionInfo};

use crate::fuse::Fuse;
use crate::locks::Relock;
use crate::mod_index::ModIndex;
use crate::pinned;
#[cfg(windows)]
pub(crate) use game::resolve;
#[cfg(any(windows, test))]
pub(crate) use ledger::Change;
use ledger::{Ledger, Limits};
#[cfg(windows)]
pub(crate) use paste::{Erase, Paste};

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::building::BLOCK_BOX.id(),
    pinned::building::BLUEPRINT_LOOKUP.id(),
    pinned::server::BLUEPRINT_REGISTRY.id(),
    pinned::server::PLAYER_INFOS.id(),
    pinned::voxel::WRITE_BLOCK.id(),
    pinned::terrain::ADD.id(),
    pinned::terrain::DIG.id(),
    pinned::terrain::FLATTEN.id(),
    pinned::voxel::READ_BOX.id(),
    pinned::voxel::UNDO_APPLY.id(),
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
}

impl Transactions {
    const fn new() -> Self {
        Self {
            busy: AtomicBool::new(false),
            ledger: Mutex::new(Ledger::new(LIMITS)),
            fuse: Fuse::new("world transactions"),
        }
    }
}

static STATE: Transactions = Transactions::new();

/// Runs a step's `work` behind the feature's fuse. A panic turns the
/// feature off and settles every transaction held, since its work in hand
/// is lost: those in work fail, the rest expire.
fn guarded(state: &Transactions, work: impl FnOnce()) {
    if state.fuse.run("the post-step link", work).is_none() && state.fuse.is_off() {
        let mut ledger = state.ledger.relock();
        if ledger.busy() || ledger.kept() > 0 {
            ledger.forget();
        }
        state.busy.store(false, Ordering::Release);
    }
}

/// Queues `changes` as one transaction of `owner`. [`Status::MISSING`] while
/// no world runs or on a build without the edits, the read or the undo
/// write; otherwise as [`Ledger::commit`].
#[cfg(windows)]
pub(crate) fn commit(
    loader: &crate::host::Loader,
    owner: ModIndex,
    actor: Option<u32>,
    changes: Vec<Change>,
) -> Result<u64, Status> {
    if STATE.fuse.is_off() || !game::ready(loader) {
        return Err(Status::MISSING);
    }
    if let Some(actor) = actor
        && crate::world::server().is_ok()
    {
        use ledger::Voxels;

        let mut world = game::Game::new(loader)?;
        for change in &changes {
            let (min, max) = change.reach();
            if world.rights(actor, min, max)? != 0 {
                return Err(Status::REFUSED);
            }
        }
    }
    let mut ledger = STATE.ledger.relock();
    let id = ledger.commit(owner, actor, changes)?;
    STATE.busy.store(true, Ordering::Release);
    Ok(id)
}

/// Queues the undo of transaction `id` of `owner`, as [`Ledger::undo`];
/// [`Status::MISSING`] once the feature is off.
pub(crate) fn undo(owner: ModIndex, id: u64) -> Result<(), Status> {
    undo_in(&STATE, owner, id)
}

fn undo_in(state: &Transactions, owner: ModIndex, id: u64) -> Result<(), Status> {
    if state.fuse.is_off() {
        return Err(Status::MISSING);
    }
    let mut ledger = state.ledger.relock();
    ledger.undo(owner, id)?;
    state.busy.store(ledger.busy(), Ordering::Release);
    Ok(())
}

/// Where transaction `id` of `owner` stands.
pub(crate) fn info(owner: ModIndex, id: u64) -> Option<TransactionInfo> {
    STATE.ledger.relock().info(owner, id)
}

/// The ledger without waiting: `None` while another thread holds it.
#[cfg(windows)]
fn try_ledger() -> Option<std::sync::MutexGuard<'static, Ledger>> {
    STATE.ledger.try_relock()
}

/// Writes the queued work of a step, on the game thread after the step,
/// behind the feature's fuse.
#[cfg(windows)]
pub(crate) fn after_step(loader: &crate::host::Loader) {
    if !STATE.busy.load(Ordering::Acquire) {
        return;
    }
    guarded(&STATE, || pass(loader));
}

/// The step's pass over the queue. The game's edits and undo writes run
/// with the ledger free, so other threads and the game's own callbacks may
/// commit, undo and ask meanwhile; a step that finds the ledger taken skips
/// its work until the next.
#[cfg(windows)]
fn pass(loader: &crate::host::Loader) {
    use ledger::Voxels;

    let Ok(mut world) = game::Game::new(loader) else {
        return;
    };
    let started = std::time::Instant::now();
    let extent = world.extent().ok();
    let Some(mut pass) = try_ledger().map(|mut ledger| ledger.begin(extent, BRICKS_PER_STEP))
    else {
        return;
    };
    while let Some(mut task) = try_ledger().and_then(|mut ledger| ledger.take(&mut pass)) {
        let outcome = task.run(&mut world, &mut pass);
        let mut ledger = STATE.ledger.relock();
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

/// Drops what the loader holds for a world that ended.
pub(crate) fn forget() {
    let mut ledger = STATE.ledger.relock();
    ledger.forget();
    STATE.busy.store(false, Ordering::Release);
}

#[cfg(test)]
mod tests {
    use enshwick::abi::{Header, TerrainEdit};

    use super::*;

    const ME: ModIndex = ModIndex::new(0);

    fn dig() -> TerrainEdit {
        TerrainEdit {
            header: Header::of::<TerrainEdit>(),
            kind: TerrainEdit::DIG,
            shape: TerrainEdit::ROUND,
            flags: 0,
            material: 0,
            position: [20.0; 3],
            radius: 4.0,
            height: 0.0,
            protect: [0; 16],
        }
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
        let info = state.ledger.relock().info(ME, id).unwrap();
        assert_eq!(
            (info.state, info.reason),
            (TransactionInfo::FAILED, Status::MISSING)
        );
        assert_eq!(undo_in(&state, ME, id), Err(Status::MISSING));

        let mut ran = false;
        guarded(&state, || ran = true);
        assert!(!ran, "a turned-off feature runs no more passes");
    }
}
