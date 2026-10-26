//! The transactions the loader holds: their queue, their undo data and where each stands.
//!
//! A transaction's changes are written in order, each whole within one
//! step, as many a step as the brick budget allows (one at least). Before a
//! change writes, every brick it can touch is read and kept; after, a hash
//! of each is kept, so an undo can tell whether a brick still holds what
//! the transaction left. An undo puts the kept bricks back newest first, a
//! budget of bricks a step, and checks each brick against what it should
//! hold right before writing it back.
//!
//! A step takes one transaction's work out of the ledger, runs it against
//! the world with the ledger free for other calls, and puts it back.

use std::collections::VecDeque;
use std::hash::{Hash, Hasher};

use enshwick::abi::{Header, Status, TerrainEdit, TransactionInfo};
use rustc_hash::{FxHashMap, FxHashSet, FxHasher};

use super::cover::{self, BRICK_VOXELS, Brick, Bricks};
use crate::mod_index::ModIndex;
use crate::voxels::WorldExtent;

/// The voxels of a brick, x fastest, then y, then z.
pub(crate) type Image = Box<[u16; BRICK_VOXELS]>;

/// What the ledger needs of the world.
pub(crate) trait Voxels {
    /// The world's extent.
    fn extent(&mut self) -> Result<WorldExtent, Status>;
    /// `Ok` when the column of bricks `brick` stands in is loaded;
    /// [`Status::NOT_LOADED`] when not.
    fn loaded(&mut self, brick: Brick) -> Result<(), Status>;
    /// The voxels of `brick`; [`Status::NOT_LOADED`] when not loaded.
    fn read(&mut self, brick: Brick) -> Result<Image, Status>;
    /// Writes `edit`; an error means nothing was written.
    fn edit(&mut self, edit: &TerrainEdit) -> Result<(), Status>;
    /// Writes `image` back into `brick`, as an edit that clients receive and
    /// the save keeps.
    fn restore(&mut self, brick: Brick, image: &[u16; BRICK_VOXELS]) -> Result<(), Status>;
}

/// How much the ledger holds.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Limits {
    /// Changes in one transaction.
    pub(crate) ops: usize,
    /// Bricks of undo data kept over all transactions, 1 KiB each; a
    /// transaction that can touch more is refused.
    pub(crate) bricks: usize,
    /// Transactions remembered; the oldest settled one is forgotten first.
    pub(crate) records: usize,
    /// Steps in a row an undo waits for a column to load before it is
    /// refused.
    pub(crate) waits: u32,
}

/// One change and the bricks it can touch.
#[derive(Debug)]
struct Op {
    edit: TerrainEdit,
    bricks: Bricks,
}

/// What a step works on: a transaction's changes and undo data.
#[derive(Debug, Default)]
struct Work {
    ops: Vec<Op>,
    /// The earlier voxels of the bricks of each change written, in order.
    written: Vec<Vec<(Brick, Image)>>,
    /// A hash of what each brick holds now, as far as the transaction
    /// knows: what it left there, or what its undo wrote back.
    now: FxHashMap<Brick, u64>,
    /// Whether its first step checked its region.
    started: bool,
    /// Whether its undo checked every brick before writing one.
    undo_checked: bool,
    /// Steps in a row its undo has waited for a column.
    waits: u32,
}

impl Work {
    fn kept(&self) -> usize {
        self.written.iter().map(Vec::len).sum()
    }
}

#[derive(Debug)]
struct Record {
    id: u64,
    owner: ModIndex,
    /// Its changes.
    ops: usize,
    /// The bricks its changes can touch, counted per change.
    bound: usize,
    /// One of the [`TransactionInfo`] states.
    state: u32,
    reason: Status,
    /// Its work; `None` while a step has it in hand.
    work: Option<Work>,
    /// Changes written and not undone, as of the last return of its work.
    applied: usize,
    /// Bricks of undo data held, as of the last return of its work.
    kept: usize,
    /// An undo asked for while its work was in hand.
    undo_asked: bool,
}

impl Record {
    /// Whether it waits for steps of the ledger.
    const fn working(&self) -> bool {
        matches!(
            self.state,
            TransactionInfo::PENDING | TransactionInfo::UNDOING
        )
    }

    /// The bricks of undo data it may still come to hold.
    const fn claim(&self) -> usize {
        if self.state == TransactionInfo::PENDING {
            self.bound
        } else {
            self.kept
        }
    }

    fn settle(&mut self, state: u32, reason: Status) {
        self.state = state;
        self.reason = reason;
    }

    /// Drops its undo data.
    fn drop_work(&mut self) {
        if let Some(work) = &mut self.work {
            work.written.clear();
            work.now.clear();
        }
        self.kept = 0;
    }
}

/// What a step does with a transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Apply,
    Undo,
}

/// A transaction's work, out of the ledger for a step.
#[derive(Debug)]
pub(crate) struct Task {
    id: u64,
    epoch: u64,
    mode: Mode,
    work: Work,
}

/// How a step's run of a task ended.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Outcome {
    /// It settled in this state, for this reason.
    Done(u32, Status),
    /// The step's budget ran out; it goes on first in the next step.
    Again,
    /// It waits for a part of the world to load; it goes on after the rest.
    Later,
}

/// One step's pass over the queue.
#[derive(Debug)]
pub(crate) struct Pass {
    budget: usize,
    waits: u32,
    extent: Option<WorldExtent>,
    /// Jobs the pass may still take.
    jobs: usize,
    /// Bricks written, read before and after included.
    pub(crate) spent: usize,
    /// Columns probed for being loaded.
    pub(crate) probes: usize,
}

/// The transactions of the running world.
#[derive(Debug)]
pub(crate) struct Ledger {
    limits: Limits,
    next: u64,
    records: VecDeque<Record>,
    /// Ids with work, in the order it runs.
    work: VecDeque<u64>,
    /// The world's extent, as its last step with work found it.
    extent: Option<WorldExtent>,
    /// Counts the worlds that ended, so work in hand from one is dropped.
    epoch: u64,
}

fn hash(image: &[u16; BRICK_VOXELS]) -> u64 {
    let mut hasher = FxHasher::default();
    image.hash(&mut hasher);
    hasher.finish()
}

impl Ledger {
    pub(crate) const fn new(limits: Limits) -> Self {
        Self {
            limits,
            next: 1,
            records: VecDeque::new(),
            work: VecDeque::new(),
            extent: None,
            epoch: 0,
        }
    }

    fn index(&self, id: u64) -> Option<usize> {
        self.records
            .binary_search_by_key(&id, |record| record.id)
            .ok()
    }

    fn find(&mut self, owner: ModIndex, id: u64) -> Option<&mut Record> {
        let index = self.index(id)?;
        self.records
            .get_mut(index)
            .filter(|record| record.owner == owner)
    }

    /// The bricks of undo data the transactions in work may come to hold.
    fn claimed(&self) -> usize {
        self.records
            .iter()
            .filter(|r| r.working())
            .map(Record::claim)
            .sum()
    }

    /// Queues `edits` as one transaction of `owner`.
    /// [`Status::BAD_ARGS`] for none, an edit the loader refuses, more
    /// changes or bricks than the limits; [`Status::OUTSIDE_WORLD`] for an
    /// edit that leaves the world, once a step has found its extent;
    /// [`Status::REFUSED`] while the queued transactions and those
    /// remembered leave no room.
    pub(crate) fn commit(&mut self, owner: ModIndex, edits: &[TerrainEdit]) -> Result<u64, Status> {
        if edits.is_empty() || edits.len() > self.limits.ops {
            return Err(Status::BAD_ARGS);
        }
        let ops = edits
            .iter()
            .map(|edit| {
                cover::check(edit).map(|bricks| Op {
                    edit: *edit,
                    bricks,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        if let Some(extent) = self.extent
            && !ops.iter().all(|op| op.bricks.inside(extent))
        {
            return Err(Status::OUTSIDE_WORLD);
        }
        let bound = ops.iter().map(|op| op.bricks.count()).sum::<usize>();
        if bound > self.limits.bricks {
            return Err(Status::BAD_ARGS);
        }
        if self.claimed() + bound > self.limits.bricks {
            return Err(Status::REFUSED);
        }
        if self.records.len() >= self.limits.records {
            let settled = self
                .records
                .iter()
                .position(|record| !record.working())
                .ok_or(Status::REFUSED)?;
            self.records.remove(settled);
        }
        let id = self.next;
        self.next += 1;
        self.records.push_back(Record {
            id,
            owner,
            ops: ops.len(),
            bound,
            state: TransactionInfo::PENDING,
            reason: Status::OK,
            work: Some(Work {
                ops,
                ..Work::default()
            }),
            applied: 0,
            kept: 0,
            undo_asked: false,
        });
        self.work.push_back(id);
        Ok(id)
    }

    /// Queues the undo of transaction `id` of `owner`: one that wrote
    /// nothing yet is dropped. [`Status::REFUSED`] for an unknown id, one
    /// undone, one without undo data, or while the transactions in work
    /// already claim the cap, which this undo's data would push past.
    pub(crate) fn undo(&mut self, owner: ModIndex, id: u64) -> Result<(), Status> {
        let claimed = self.claimed();
        let cap = self.limits.bricks;
        let record = self.find(owner, id).ok_or(Status::REFUSED)?;
        let queue = match record.state {
            TransactionInfo::UNDOING => return Ok(()),
            TransactionInfo::PENDING if record.work.is_none() => {
                record.undo_asked = true;
                return Ok(());
            }
            TransactionInfo::PENDING if record.applied == 0 => {
                record.settle(TransactionInfo::UNDONE, Status::OK);
                self.work.retain(|&queued| queued != id);
                return Ok(());
            }
            TransactionInfo::PENDING => false,
            TransactionInfo::APPLIED | TransactionInfo::UNDO_REFUSED | TransactionInfo::FAILED
                if record.kept > 0 =>
            {
                if claimed + record.kept > cap {
                    return Err(Status::REFUSED);
                }
                true
            }
            _ => return Err(Status::REFUSED),
        };
        record.settle(TransactionInfo::UNDOING, Status::OK);
        if let Some(work) = &mut record.work {
            work.undo_checked = false;
            work.waits = 0;
        }
        if queue {
            self.work.push_back(id);
        }
        Ok(())
    }

    /// Where transaction `id` of `owner` stands; `None` for an id unknown
    /// or forgotten.
    pub(crate) fn info(&self, owner: ModIndex, id: u64) -> Option<TransactionInfo> {
        let record = self
            .records
            .get(self.index(id)?)
            .filter(|record| record.owner == owner)?;
        Some(TransactionInfo {
            header: Header::of::<TransactionInfo>(),
            state: record.state,
            reason: record.reason,
            ops: u32::try_from(record.ops).unwrap_or(u32::MAX),
            applied: u32::try_from(record.applied).unwrap_or(u32::MAX),
            bricks: record.kept as u64,
        })
    }

    /// Bricks of undo data held over all transactions.
    pub(crate) fn kept(&self) -> usize {
        self.records.iter().map(|record| record.kept).sum()
    }

    /// Whether any transaction or undo waits for a step.
    pub(crate) fn busy(&self) -> bool {
        !self.work.is_empty()
    }

    /// Drops what the ledger holds for a world that ended: transactions in
    /// work fail, kept undo data goes, and work in hand is dropped on its
    /// return.
    pub(crate) fn forget(&mut self) {
        for record in &mut self.records {
            match record.state {
                TransactionInfo::PENDING | TransactionInfo::UNDOING => {
                    record.settle(TransactionInfo::FAILED, Status::MISSING);
                }
                TransactionInfo::APPLIED | TransactionInfo::UNDO_REFUSED => {
                    record.state = TransactionInfo::EXPIRED;
                }
                _ => {}
            }
            record.drop_work();
            record.undo_asked = false;
        }
        self.work.clear();
        self.extent = None;
        self.epoch += 1;
    }

    /// Runs the queued work for one step of the world, writing at most
    /// `budget` bricks unless a single change takes more, which then runs
    /// alone. What the loader's step does with its lock released between
    /// [`Ledger::take`] and [`Ledger::put`].
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "the loader's step takes and puts work itself")
    )]
    pub(crate) fn step(&mut self, world: &mut impl Voxels, budget: usize) {
        let mut pass = self.begin(world.extent().ok(), budget);
        while let Some(mut task) = self.take(&mut pass) {
            let outcome = task.run(world, &mut pass);
            if !self.put(task, outcome) {
                break;
            }
        }
    }

    /// Starts a step's pass with the world's `extent`, when known.
    pub(crate) fn begin(&mut self, extent: Option<WorldExtent>, budget: usize) -> Pass {
        if extent.is_some() {
            self.extent = extent;
        }
        Pass {
            budget,
            waits: self.limits.waits,
            extent: self.extent,
            jobs: self.work.len(),
            spent: 0,
            probes: 0,
        }
    }

    /// The next transaction's work for `pass`, out of the ledger until
    /// [`Ledger::put`] returns it; room for its undo data is made first.
    pub(crate) fn take(&mut self, pass: &mut Pass) -> Option<Task> {
        while pass.jobs > 0 {
            pass.jobs -= 1;
            let id = self.work.pop_front()?;
            let Some(index) = self.index(id) else {
                continue;
            };
            let mode = match self.records[index].state {
                TransactionInfo::PENDING => Mode::Apply,
                TransactionInfo::UNDOING => Mode::Undo,
                _ => continue,
            };
            if mode == Mode::Apply {
                let record = &self.records[index];
                let to_come = record.bound.saturating_sub(record.kept);
                self.make_room(to_come, index);
            }
            let Some(work) = self.records[index].work.take() else {
                continue;
            };
            return Some(Task {
                id,
                epoch: self.epoch,
                mode,
                work,
            });
        }
        None
    }

    /// Returns a task's work after a run that ended in `outcome`; whether
    /// the pass may go on.
    pub(crate) fn put(&mut self, task: Task, outcome: Outcome) -> bool {
        if task.epoch != self.epoch {
            return false;
        }
        let Some(index) = self.index(task.id) else {
            return false;
        };
        let record = &mut self.records[index];
        record.applied = task.work.written.len();
        record.kept = task.work.kept();
        record.work = Some(task.work);
        let asked = core::mem::take(&mut record.undo_asked);
        let go_on = match outcome {
            Outcome::Done(state, reason) => {
                record.settle(state, reason);
                true
            }
            Outcome::Again => {
                self.work.push_front(task.id);
                false
            }
            Outcome::Later => {
                self.work.push_back(task.id);
                true
            }
        };
        if asked && task.mode == Mode::Apply {
            self.work.retain(|&queued| queued != task.id);
            let record = &mut self.records[index];
            if record.applied == 0 {
                record.settle(TransactionInfo::UNDONE, Status::OK);
            } else {
                record.settle(TransactionInfo::UNDOING, Status::OK);
                self.work.push_front(task.id);
            }
        }
        go_on
    }

    /// Drops the undo data of the oldest settled transactions, other than
    /// the one at `keep`, until `bricks` more fit the limit.
    fn make_room(&mut self, bricks: usize, keep: usize) {
        let limit = self.limits.bricks;
        let mut kept = self.kept();
        for (index, record) in self.records.iter_mut().enumerate() {
            if kept + bricks <= limit {
                return;
            }
            if index == keep || record.working() || record.kept == 0 {
                continue;
            }
            kept -= record.kept;
            record.drop_work();
            if record.state != TransactionInfo::FAILED {
                record.state = TransactionInfo::EXPIRED;
            }
        }
    }
}

impl Task {
    /// Runs the task against the world for what is left of the pass.
    pub(crate) fn run(&mut self, world: &mut impl Voxels, pass: &mut Pass) -> Outcome {
        match self.mode {
            Mode::Apply => self.apply(world, pass),
            Mode::Undo => self.take_back(world, pass),
        }
    }

    fn apply(&mut self, world: &mut impl Voxels, pass: &mut Pass) -> Outcome {
        let work = &mut self.work;
        let failed = |reason| Outcome::Done(TransactionInfo::FAILED, reason);
        if !work.started {
            work.started = true;
            let inside = pass
                .extent
                .map(|extent| work.ops.iter().all(|op| op.bricks.inside(extent)));
            if inside != Some(true) {
                return failed(inside.map_or(Status::MISSING, |_| Status::OUTSIDE_WORLD));
            }
            let mut columns = FxHashSet::default();
            for brick in work.ops.iter().flat_map(|op| op.bricks.columns()) {
                let [x, _, z] = brick.0;
                if columns.insert([x, z]) {
                    pass.probes += 1;
                    if let Err(reason) = world.loaded(brick) {
                        return failed(reason);
                    }
                }
            }
        }
        while let Some(op) = work.ops.get(work.written.len()) {
            let cost = op.bricks.count();
            if pass.spent > 0 && pass.spent + cost > pass.budget {
                return Outcome::Again;
            }
            let images = op
                .bricks
                .iter()
                .map(|brick| world.read(brick).map(|image| (brick, image)))
                .collect::<Result<Vec<_>, _>>()
                .and_then(|images| world.edit(&op.edit).map(|()| images));
            let images = match images {
                Ok(images) => images,
                Err(reason) => return failed(reason),
            };
            let mut lost = None;
            for (brick, _) in &images {
                match world.read(*brick) {
                    Ok(now) => {
                        work.now.insert(*brick, hash(&now));
                    }
                    Err(reason) => {
                        work.now.remove(brick);
                        lost.get_or_insert(reason);
                    }
                }
            }
            work.written.push(images);
            pass.spent += cost;
            if let Some(reason) = lost {
                return failed(reason);
            }
        }
        Outcome::Done(TransactionInfo::APPLIED, Status::OK)
    }

    fn take_back(&mut self, world: &mut impl Voxels, pass: &mut Pass) -> Outcome {
        let work = &mut self.work;
        let refused = |work: &mut Work, reason| {
            work.undo_checked = false;
            work.waits = 0;
            Outcome::Done(TransactionInfo::UNDO_REFUSED, reason)
        };
        if !work.undo_checked {
            let bricks: FxHashSet<Brick> = work.written.iter().flatten().map(|(b, _)| *b).collect();
            for brick in bricks {
                let reason = match (work.now.get(&brick), world.read(brick)) {
                    (Some(left), Ok(now)) if hash(&now) == *left => continue,
                    (_, Err(reason)) => reason,
                    _ => Status::REFUSED,
                };
                return refused(work, reason);
            }
            work.undo_checked = true;
        }
        while let Some(op) = work.written.last_mut() {
            let Some((brick, image)) = op.last() else {
                work.written.pop();
                continue;
            };
            if pass.spent >= pass.budget {
                return Outcome::Again;
            }
            let brick = *brick;
            let expected = work.now.get(&brick).copied();
            let result = world.read(brick).and_then(|now| {
                if Some(hash(&now)) == expected {
                    world.restore(brick, image)
                } else {
                    Err(Status::REFUSED)
                }
            });
            match result {
                Ok(()) => {
                    work.now.insert(brick, hash(image));
                    op.pop();
                    pass.spent += 1;
                    work.waits = 0;
                }
                Err(Status::NOT_LOADED) => {
                    work.waits += 1;
                    if work.waits >= pass.waits {
                        return refused(work, Status::NOT_LOADED);
                    }
                    return Outcome::Later;
                }
                Err(reason) => return refused(work, reason),
            }
        }
        work.now.clear();
        Outcome::Done(TransactionInfo::UNDONE, Status::OK)
    }
}

#[cfg(test)]
mod tests;
