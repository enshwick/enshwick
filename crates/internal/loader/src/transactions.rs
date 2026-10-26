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

mod cover;
#[cfg(windows)]
mod game;
mod ledger;
mod record;

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use enshwick::abi::{Status, TransactionInfo};

use crate::locks::Relock;
use crate::mod_index::ModIndex;
use crate::pinned;
use ledger::{Ledger, Limits};

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::terrain::ADD.id(),
    pinned::terrain::DIG.id(),
    pinned::terrain::FLATTEN.id(),
    pinned::voxel::READ_BOX.id(),
    pinned::voxel::UNDO_APPLY.id(),
];

/// Bricks written a step, before-images and after-reads included: a step
/// holds at least one whole change, however large. A first guess, to be
/// tuned by the step time and the bytes sent to clients a live server
/// shows.
const BRICKS_PER_STEP: usize = 64;

/// What the loader keeps: 1,024 changes in a transaction, 64 MiB of undo
/// data (65,536 bricks of 1 KiB), the newest 4,096 transactions.
pub(crate) const LIMITS: Limits = Limits {
    ops: 1024,
    bricks: 65_536,
    records: 4096,
};

/// The transactions of the running world.
struct Transactions {
    /// Whether the ledger holds work, so a step without any takes no lock;
    /// written only under the ledger's lock.
    busy: AtomicBool,
    ledger: Mutex<Ledger>,
}

static STATE: Transactions = Transactions {
    busy: AtomicBool::new(false),
    ledger: Mutex::new(Ledger::new(LIMITS)),
};

/// Queues `edits` as one transaction of `owner`. [`Status::MISSING`] while
/// no world runs or on a build without the edits, the read or the undo
/// write; otherwise as [`Ledger::commit`].
#[cfg(windows)]
pub(crate) fn commit(
    loader: &crate::host::Loader,
    owner: ModIndex,
    edits: &[enshwick::abi::TerrainEdit],
) -> Result<u64, Status> {
    if !game::ready(loader) {
        return Err(Status::MISSING);
    }
    let mut ledger = STATE.ledger.relock();
    let id = ledger.commit(owner, edits)?;
    STATE.busy.store(true, Ordering::Release);
    Ok(id)
}

/// Queues the undo of transaction `id` of `owner`, as [`Ledger::undo`].
pub(crate) fn undo(owner: ModIndex, id: u64) -> Result<(), Status> {
    let mut ledger = STATE.ledger.relock();
    ledger.undo(owner, id)?;
    STATE.busy.store(ledger.busy(), Ordering::Release);
    Ok(())
}

/// Where transaction `id` of `owner` stands.
pub(crate) fn info(owner: ModIndex, id: u64) -> Option<TransactionInfo> {
    STATE.ledger.relock().info(owner, id)
}

/// Writes the queued work of a step, on the game thread after the step.
#[cfg(windows)]
pub(crate) fn after_step(loader: &crate::host::Loader) {
    if !STATE.busy.load(Ordering::Acquire) {
        return;
    }
    let mut ledger = STATE.ledger.relock();
    if let Ok(mut world) = game::Game::new(loader) {
        ledger.step(&mut world, BRICKS_PER_STEP);
    }
    STATE.busy.store(ledger.busy(), Ordering::Release);
}

/// Drops what the loader holds for a world that ended.
pub(crate) fn forget() {
    let mut ledger = STATE.ledger.relock();
    ledger.forget();
    STATE.busy.store(false, Ordering::Release);
}
