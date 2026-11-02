//! The ledger's records and queue: commits, undo requests, and work taken out for a step.

use std::collections::VecDeque;

use enshwick::abi::{Header, Status, TransactionInfo};

use super::{Bricks, Change, Ledger, Limits, Mode, Op, Outcome, Pass, Record, Task, Voxels, Work};
use crate::mod_index::ModIndex;
use crate::voxels::WorldExtent;

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
    pub(crate) fn commit(
        &mut self,
        owner: ModIndex,
        actor: Option<u32>,
        changes: Vec<Change>,
    ) -> Result<u64, Status> {
        if changes.is_empty() || changes.len() > self.limits.ops {
            return Err(Status::BAD_ARGS);
        }
        let ops = changes
            .into_iter()
            .map(|change| change.bricks().map(|bricks| Op { change, bricks }))
            .collect::<Result<Vec<_>, _>>()?;
        if let Some(extent) = self.extent
            && !ops
                .iter()
                .all(|op| op.bricks.is_none_or(|bricks| bricks.inside(extent)))
        {
            return Err(Status::OUTSIDE_WORLD);
        }
        let bound = ops
            .iter()
            .filter(|op| !matches!(op.change, Change::PlaceProp(_) | Change::RemoveProp { .. }))
            .filter_map(|op| op.bricks)
            .map(Bricks::count)
            .sum::<usize>();
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
                actor,
                ..Work::default()
            }),
            applied: 0,
            kept: 0,
            undo_asked: false,
            refusal: 0,
            holds: 0,
            created: Vec::new(),
            undoable: false,
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
                record.holds = 0;
                record.settle(TransactionInfo::UNDONE, Status::OK);
                self.work.retain(|&queued| queued != id);
                return Ok(());
            }
            TransactionInfo::PENDING => false,
            TransactionInfo::APPLIED | TransactionInfo::UNDO_REFUSED | TransactionInfo::FAILED
                if record.undoable =>
            {
                if claimed + record.kept > cap {
                    return Err(Status::REFUSED);
                }
                true
            }
            _ => return Err(Status::REFUSED),
        };
        record.settle(TransactionInfo::UNDOING, Status::OK);
        record.holds = 0;
        if let Some(work) = &mut record.work {
            work.undo_checked = false;
            work.waits = 0;
            work.holds = 0;
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
            refusal: record.refusal,
            reserved: 0,
            holds: record.holds,
            created: u32::try_from(record.created.len()).unwrap_or(u32::MAX),
        })
    }

    /// The entities of the props transaction `id` of `owner` placed and
    /// still holds, oldest first; `None` for an id unknown or forgotten.
    pub(crate) fn entities(&self, owner: ModIndex, id: u64) -> Option<Vec<u32>> {
        let record = self
            .records
            .get(self.index(id)?)
            .filter(|record| record.owner == owner)?;
        Some(record.created.clone())
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
        record.applied = task.work.done.len();
        record.kept = task.work.kept();
        record.refusal = task.work.refusal;
        record.holds = task.work.holds;
        record.created = task.work.created();
        record.undoable = !task.work.done.is_empty();
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
            if index == keep || record.working() || !record.undoable {
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
