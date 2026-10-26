//! The transactions the loader holds: their queue, their undo data and where each stands.
//!
//! A transaction's changes are written in order, each whole within one
//! step, as many a step as the brick budget allows (one at least). Before a
//! change writes, every brick it can touch is read and kept; after, a hash
//! of each is kept, so an undo can tell whether a brick still holds what
//! the transaction left. An undo puts the kept bricks back newest first, a
//! budget of bricks a step.

use std::collections::VecDeque;
use std::hash::{Hash, Hasher};

use enshwick::abi::{Header, Status, TerrainEdit, TransactionInfo};
use rustc_hash::{FxHashMap, FxHashSet, FxHasher};

use super::cover::{self, BRICK_VOXELS, Brick, Bricks};
use crate::mod_index::ModIndex;

/// The voxels of a brick, x fastest, then y, then z.
pub(crate) type Image = Box<[u16; BRICK_VOXELS]>;

/// What the ledger needs of the world.
pub(crate) trait Voxels {
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
}

/// One change and the bricks it can touch.
#[derive(Debug)]
struct Op {
    edit: TerrainEdit,
    bricks: Bricks,
}

#[derive(Debug)]
struct Record {
    id: u64,
    owner: ModIndex,
    ops: Vec<Op>,
    /// The bricks its changes can touch, counted per change.
    bound: usize,
    /// One of the [`TransactionInfo`] states.
    state: u32,
    reason: Status,
    /// The earlier voxels of the bricks of each change written, in order.
    written: Vec<Vec<(Brick, Image)>>,
    /// A hash of each brick as the transaction left it.
    after: FxHashMap<Brick, u64>,
    /// Whether its first step checked that its whole region is loaded.
    started: bool,
    /// Whether its undo checked its bricks.
    undo_checked: bool,
}

impl Record {
    fn kept(&self) -> usize {
        self.written.iter().map(Vec::len).sum()
    }

    /// Whether it waits for steps of the ledger.
    const fn working(&self) -> bool {
        matches!(
            self.state,
            TransactionInfo::PENDING | TransactionInfo::UNDOING
        )
    }

    /// The bricks of undo data it may still come to hold.
    fn claim(&self) -> usize {
        if self.state == TransactionInfo::PENDING {
            self.bound
        } else {
            self.kept()
        }
    }

    fn fail(&mut self, reason: Status) {
        self.state = TransactionInfo::FAILED;
        self.reason = reason;
    }
}

/// How a job of a step ended.
enum Job {
    /// It is settled.
    Done,
    /// The step's budget ran out; it goes on first in the next step.
    Again,
    /// It waits for a part of the world to load; it goes on after the rest.
    Later,
}

/// The transactions of the running world.
#[derive(Debug)]
pub(crate) struct Ledger {
    limits: Limits,
    next: u64,
    records: VecDeque<Record>,
    /// Ids with work, in the order it runs.
    work: VecDeque<u64>,
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

    /// Queues `edits` as one transaction of `owner`.
    /// [`Status::BAD_ARGS`] for none, an edit the loader refuses, more
    /// changes or bricks than the limits; [`Status::REFUSED`] while the
    /// queued transactions and those remembered leave no room.
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
        let bound = ops.iter().map(|op| op.bricks.count()).sum::<usize>();
        if bound > self.limits.bricks {
            return Err(Status::BAD_ARGS);
        }
        let claimed: usize = self
            .records
            .iter()
            .filter(|r| r.working())
            .map(Record::claim)
            .sum();
        if claimed + bound > self.limits.bricks {
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
            ops,
            bound,
            state: TransactionInfo::PENDING,
            reason: Status::OK,
            written: Vec::new(),
            after: FxHashMap::default(),
            started: false,
            undo_checked: false,
        });
        self.work.push_back(id);
        Ok(id)
    }

    /// Queues the undo of transaction `id` of `owner`: one that wrote
    /// nothing yet is dropped. [`Status::REFUSED`] for an unknown id, one
    /// undone, or one without undo data.
    pub(crate) fn undo(&mut self, owner: ModIndex, id: u64) -> Result<(), Status> {
        let record = self.find(owner, id).ok_or(Status::REFUSED)?;
        let queue = match record.state {
            TransactionInfo::UNDOING => return Ok(()),
            TransactionInfo::PENDING if record.written.is_empty() => {
                record.state = TransactionInfo::UNDONE;
                self.work.retain(|&queued| queued != id);
                return Ok(());
            }
            TransactionInfo::PENDING => false,
            TransactionInfo::APPLIED | TransactionInfo::UNDO_REFUSED => true,
            TransactionInfo::FAILED if !record.written.is_empty() => true,
            _ => return Err(Status::REFUSED),
        };
        record.state = TransactionInfo::UNDOING;
        record.reason = Status::OK;
        record.undo_checked = false;
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
            ops: u32::try_from(record.ops.len()).unwrap_or(u32::MAX),
            applied: u32::try_from(record.written.len()).unwrap_or(u32::MAX),
            bricks: record.kept() as u64,
        })
    }

    /// Bricks of undo data held over all transactions.
    pub(crate) fn kept(&self) -> usize {
        self.records.iter().map(Record::kept).sum()
    }

    /// Whether any transaction or undo waits for a step.
    pub(crate) fn busy(&self) -> bool {
        !self.work.is_empty()
    }

    /// Drops what the ledger holds for a world that ended: transactions
    /// and undos in work fail, kept undo data goes.
    pub(crate) fn forget(&mut self) {
        for record in &mut self.records {
            match record.state {
                TransactionInfo::PENDING | TransactionInfo::UNDOING => record.fail(Status::MISSING),
                TransactionInfo::APPLIED | TransactionInfo::UNDO_REFUSED => {
                    record.state = TransactionInfo::EXPIRED;
                }
                _ => {}
            }
            record.written.clear();
            record.after.clear();
        }
        self.work.clear();
    }

    /// Runs the queued work for one step of the world, writing at most
    /// `budget` bricks unless a single change takes more, which then runs
    /// alone.
    pub(crate) fn step(&mut self, world: &mut impl Voxels, budget: usize) {
        let mut spent = 0;
        for _ in 0..self.work.len() {
            let Some(id) = self.work.pop_front() else {
                break;
            };
            let Some(index) = self.index(id) else {
                continue;
            };
            let job = match self.records[index].state {
                TransactionInfo::PENDING => self.apply(index, world, budget, &mut spent),
                TransactionInfo::UNDOING => self.take_back(index, world, budget, &mut spent),
                _ => Job::Done,
            };
            match job {
                Job::Done => {}
                Job::Again => {
                    self.work.push_front(id);
                    break;
                }
                Job::Later => self.work.push_back(id),
            }
        }
    }

    fn apply(
        &mut self,
        index: usize,
        world: &mut impl Voxels,
        budget: usize,
        spent: &mut usize,
    ) -> Job {
        let record = &mut self.records[index];
        if !record.started {
            record.started = true;
            let mut columns = FxHashSet::default();
            let bricks = record.ops.iter().flat_map(|op| op.bricks.iter());
            for brick in bricks {
                let [x, _, z] = brick.0;
                if columns.insert([x, z])
                    && let Err(reason) = world.loaded(brick)
                {
                    record.fail(reason);
                    return Job::Done;
                }
            }
        }
        while let Some(op) = self.records[index]
            .ops
            .get(self.records[index].written.len())
        {
            let cost = op.bricks.count();
            if *spent > 0 && *spent + cost > budget {
                return Job::Again;
            }
            self.make_room(cost, index);
            let record = &mut self.records[index];
            let op = &record.ops[record.written.len()];
            let images = op
                .bricks
                .iter()
                .map(|brick| world.read(brick).map(|image| (brick, image)))
                .collect::<Result<Vec<_>, _>>()
                .and_then(|images| world.edit(&op.edit).map(|()| images));
            let images = match images {
                Ok(images) => images,
                Err(reason) => {
                    record.fail(reason);
                    return Job::Done;
                }
            };
            for (brick, _) in &images {
                match world.read(*brick) {
                    Ok(now) => record.after.insert(*brick, hash(&now)),
                    Err(_) => record.after.remove(brick),
                };
            }
            record.written.push(images);
            *spent += cost;
        }
        self.records[index].state = TransactionInfo::APPLIED;
        Job::Done
    }

    fn take_back(
        &mut self,
        index: usize,
        world: &mut impl Voxels,
        budget: usize,
        spent: &mut usize,
    ) -> Job {
        let record = &mut self.records[index];
        if !record.undo_checked {
            for (brick, left) in &record.after {
                let refused = match world.read(*brick) {
                    Ok(now) if hash(&now) == *left => continue,
                    Ok(_) => Status::REFUSED,
                    Err(reason) => reason,
                };
                record.state = TransactionInfo::UNDO_REFUSED;
                record.reason = refused;
                return Job::Done;
            }
            record.undo_checked = true;
        }
        while let Some(op) = record.written.last_mut() {
            let Some((brick, image)) = op.last() else {
                record.written.pop();
                continue;
            };
            if *spent >= budget {
                return Job::Again;
            }
            let brick = *brick;
            match world
                .loaded(brick)
                .and_then(|()| world.restore(brick, image))
            {
                Ok(()) => {
                    op.pop();
                    *spent += 1;
                }
                Err(Status::NOT_LOADED) => return Job::Later,
                Err(reason) => {
                    record.fail(reason);
                    return Job::Done;
                }
            }
        }
        record.after.clear();
        record.state = TransactionInfo::UNDONE;
        Job::Done
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
            if index == keep || record.working() || record.written.is_empty() {
                continue;
            }
            kept -= record.kept();
            record.written.clear();
            record.after.clear();
            if record.state != TransactionInfo::FAILED {
                record.state = TransactionInfo::EXPIRED;
            }
        }
    }
}

#[cfg(test)]
mod tests;
