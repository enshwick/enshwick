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

use super::block::Block;
use super::cover::{self, BRICK_VOXELS, Brick, Bricks};
use super::paste::{Erase, Paste};
use super::props::Prop;
use crate::mod_index::ModIndex;
use crate::voxels::WorldExtent;

/// The voxels of a brick, x fastest, then y, then z.
pub(crate) type Image = Box<[u16; BRICK_VOXELS]>;

/// What the ledger needs of the world.
pub(crate) trait Voxels {
    /// Whether the server lets `actor` edit the box from `min` to `max` in
    /// metres: 0, or the game's reason it refuses.
    fn rights(&mut self, actor: u32, min: [f64; 3], max: [f64; 3]) -> Result<u32, Status>;
    /// Places `block` for `actor` (0 for none) and announces it; an error
    /// means nothing was written.
    fn place(&mut self, block: &Block, actor: u32) -> Result<(), Status>;
    /// Spawns `prop` for `actor` (0 for none) and announces it; the new
    /// entity, in the world from the next step.
    fn spawn_prop(&mut self, prop: &Prop, actor: u32) -> Result<u32, Status>;
    /// The prop `entity` is, and the kinds of state it holds that a
    /// respawn would lose, as bits; [`Status::MISSING`] when it is no prop.
    fn read_prop(&mut self, entity: u32) -> Result<(Prop, u32), Status>;
    /// Holds the prop `entity` against players until its destroy: no
    /// opening, moving items in or out, Quick Stash or dismantling; the
    /// game's systems of the next step run with it held.
    fn hold_prop(&mut self, entity: u32) -> Result<(), Status>;
    /// `Ok` while the prop `entity` is still held and the filter that
    /// keeps players from it is on; [`Status::MISSING`] otherwise, when a
    /// destroy would go unguarded.
    fn still_held(&mut self, entity: u32) -> Result<(), Status>;
    /// Lets players reach the prop `entity` again, as before
    /// [`Voxels::hold_prop`].
    fn release_prop(&mut self, entity: u32);
    /// The start state of the prop `entity` as bytes to compare: what a
    /// fresh prop of its kind holds and a player or time may change, a
    /// logic trigger's settings and a plant's level; `None` for a prop with
    /// none, [`Status::MISSING`] while it is not in the world.
    fn start_state(&mut self, entity: u32) -> Result<Option<Vec<u8>>, Status>;
    /// Whether a prop like `prop` stands where it stood.
    fn prop_at(&mut self, prop: &Prop) -> Result<bool, Status>;
    /// Removes the prop `entity` for `actor` (0 for none), as the game tears
    /// one down, and announces it.
    fn remove_prop(&mut self, entity: u32, actor: u32) -> Result<(), Status>;
    /// Announces that `erase` tore down building voxels for `actor` (0 for
    /// none); called only when it cleared some and wrote every brick.
    fn erased(&mut self, erase: &Erase, actor: u32);
    /// The world's extent.
    fn extent(&mut self) -> Result<WorldExtent, Status>;
    /// `Ok` when the column of bricks `brick` stands in is loaded;
    /// [`Status::NOT_LOADED`] when not.
    fn loaded(&mut self, brick: Brick) -> Result<(), Status>;
    /// The voxels of `brick`; [`Status::NOT_LOADED`] when not loaded.
    fn read(&mut self, brick: Brick) -> Result<Image, Status>;
    /// Writes `edit`; an error means nothing was written.
    fn edit(&mut self, edit: &TerrainEdit) -> Result<(), Status>;
    /// Writes `image` into `brick`, as an edit that clients receive and the
    /// save keeps.
    fn write(&mut self, brick: Brick, image: &[u16; BRICK_VOXELS]) -> Result<(), Status>;
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

/// A change of the world a transaction makes.
#[derive(Clone, Debug)]
pub(crate) enum Change {
    /// A terrain add, dig or flatten.
    Terrain(TerrainEdit),
    /// Voxels written over a box.
    Voxels(Paste),
    /// The building voxels of a box cleared to air.
    Erase(Erase),
    /// A building block placed.
    Block(Box<Block>),
    /// A prop placed.
    PlaceProp(Prop),
    /// A prop removed; refused when it holds state, unless `discard_state`.
    RemoveProp { entity: u32, discard_state: bool },
}

impl Change {
    /// The bricks it can write, or where a prop goes; `None` for a removal,
    /// whose place the step reads. [`Status::BAD_ARGS`] for a change the
    /// loader refuses.
    fn bricks(&self) -> Result<Option<Bricks>, Status> {
        Ok(Some(match self {
            Self::Terrain(edit) => cover::check(edit)?,
            Self::Voxels(paste) => paste.bricks(),
            Self::Erase(erase) => erase.bricks(),
            Self::Block(block) => block.region.bricks(),
            Self::PlaceProp(prop) => prop.bricks(),
            Self::RemoveProp { .. } => return Ok(None),
        }))
    }

    /// The box it can write, in metres, lowest and highest corner; a
    /// removal's is its prop's.
    fn reach(&self, removed: Option<&Prop>) -> Option<([f64; 3], [f64; 3])> {
        Some(match self {
            Self::Terrain(edit) => cover::reach(edit),
            Self::Voxels(paste) => paste.region().metres(),
            Self::Erase(erase) => erase.region().metres(),
            Self::Block(block) => block.region.metres(),
            Self::PlaceProp(prop) => prop.reach(),
            Self::RemoveProp { .. } => removed?.reach(),
        })
    }
}

/// One change and the bricks it can touch.
#[derive(Debug)]
struct Op {
    change: Change,
    bricks: Option<Bricks>,
}

impl Op {
    /// What it costs a step's budget: its bricks, or 1 for a prop.
    fn cost(&self) -> usize {
        self.bricks.map_or(1, |bricks| match self.change {
            Change::PlaceProp(_) => 1,
            _ => bricks.count(),
        })
    }
}

/// What a written change left for its undo.
#[derive(Debug)]
enum Done {
    /// The earlier voxels of each brick it wrote.
    Bricks(Vec<(Brick, Image)>),
    /// A prop it placed, as `entity`, and its start state when it first
    /// stood in the world.
    Placed {
        entity: u32,
        prop: Prop,
        settings: Settings,
    },
    /// A prop it removed, and the kinds of state the prop held then.
    Removed { prop: Prop, holds: u32 },
}

/// The start state of a prop a transaction placed, a logic trigger's
/// settings and a plant's level, as it was the first step the prop stood
/// in the world, before a player or time could change it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Settings {
    /// Not read: the prop did not stand in the world within the wait limit,
    /// or the read failed; an undo cannot tell what changed.
    Unread,
    /// Read: the bytes, `None` for a prop with no start state.
    Read(Option<Vec<u8>>),
}

impl Done {
    fn bricks(&self) -> &[(Brick, Image)] {
        match self {
            Self::Bricks(bricks) => bricks,
            _ => &[],
        }
    }
}

/// What a step works on: a transaction's changes and undo data.
#[derive(Debug, Default)]
struct Work {
    ops: Vec<Op>,
    /// What each change written left for its undo, in order.
    done: Vec<Done>,
    /// A hash of what each brick holds now, as far as the transaction
    /// knows: what it left there, or what its undo wrote back.
    now: FxHashMap<Brick, u64>,
    /// Whether its first step checked its region.
    started: bool,
    /// Whether its undo checked every brick before writing one.
    undo_checked: bool,
    /// Steps in a row its undo has waited for a column.
    waits: u32,
    /// The player it acts for, whose rights it checks; `None` for the mod's
    /// own change.
    actor: Option<u32>,
    /// The game's reason the server refused its actor, 0 for none.
    refusal: u32,
    /// The kinds of state a prop held that stopped it, as bits; 0 for none.
    holds: u32,
    /// The props it holds against players until it destroys them.
    held: Vec<u32>,
}

impl Work {
    /// The entities of the props it placed and still holds.
    fn created(&self) -> Vec<u32> {
        self.done
            .iter()
            .filter_map(|done| match done {
                Done::Placed { entity, .. } => Some(*entity),
                _ => None,
            })
            .collect()
    }

    fn kept(&self) -> usize {
        self.done.iter().map(|done| done.bricks().len()).sum()
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
    /// The game's reason the server refused its actor, 0 for none.
    refusal: u32,
    /// The kinds of state a prop held that stopped it, as bits.
    holds: u32,
    /// The entities of the props it placed, as of the last return of its
    /// work.
    created: Vec<u32>,
    /// Whether it keeps what its undo needs, as of the last return of its
    /// work.
    undoable: bool,
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
            work.done.clear();
            work.now.clear();
        }
        self.kept = 0;
        self.created.clear();
        self.undoable = false;
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

/// Writes each brick of `images` back with `change` laid over it; the first
/// write that fails, after which the bricks before it stay written.
fn write_over(
    world: &mut impl Voxels,
    images: &[(Brick, Image)],
    mut change: impl FnMut(Brick, &mut [u16; BRICK_VOXELS]),
) -> Option<Status> {
    images
        .iter()
        .try_for_each(|(brick, before)| {
            let mut image = **before;
            change(*brick, &mut image);
            world.write(*brick, &image)
        })
        .err()
}

impl Task {
    /// Runs the task against the world for what is left of the pass.
    pub(crate) fn run(&mut self, world: &mut impl Voxels, pass: &mut Pass) -> Outcome {
        let outcome = match self.mode {
            Mode::Apply => self.apply(world, pass),
            Mode::Undo => self.take_back(world, pass),
        };
        if matches!(outcome, Outcome::Done(..)) {
            for entity in self.work.held.drain(..) {
                world.release_prop(entity);
            }
        }
        outcome
    }

    fn apply(&mut self, world: &mut impl Voxels, pass: &mut Pass) -> Outcome {
        let work = &mut self.work;
        let failed = |reason| Outcome::Done(TransactionInfo::FAILED, reason);
        if !work.started {
            work.started = true;
            if let Some(outcome) = work.first_check(world, pass) {
                return outcome;
            }
            match work.hold_removals(world) {
                Ok(false) => {}
                Ok(true) => return Outcome::Later,
                Err(reason) => return failed(reason),
            }
        }
        while let Some(op) = work.ops.get(work.done.len()) {
            let cost = op.cost();
            if pass.spent > 0 && pass.spent + cost > pass.budget {
                return Outcome::Again;
            }
            let actor = work.actor.unwrap_or(0);
            match &op.change {
                Change::PlaceProp(prop) => match world.spawn_prop(prop, actor) {
                    Ok(entity) => work.done.push(Done::Placed {
                        entity,
                        prop: *prop,
                        settings: Settings::Unread,
                    }),
                    Err(reason) => return failed(reason),
                },
                &Change::RemoveProp {
                    entity,
                    discard_state,
                } => {
                    if let Err(reason) = work.remove(world, entity, discard_state, actor) {
                        return failed(reason);
                    }
                }
                change => {
                    let Some(bricks) = op.bricks else {
                        return failed(Status::MISSING);
                    };
                    let images = bricks
                        .iter()
                        .map(|brick| world.read(brick).map(|image| (brick, image)))
                        .collect::<Result<Vec<_>, _>>();
                    let images = match images {
                        Ok(images) => images,
                        Err(reason) => return failed(reason),
                    };
                    let mut lost = match change {
                        Change::Terrain(edit) => match world.edit(edit) {
                            Ok(()) => None,
                            Err(reason) => return failed(reason),
                        },
                        Change::Block(block) => match world.place(block, actor) {
                            Ok(()) => None,
                            Err(reason) => return failed(reason),
                        },
                        Change::Voxels(paste) => write_over(world, &images, |brick, image| {
                            paste.overlay(brick, image);
                        }),
                        Change::Erase(erase) => {
                            let mut cleared = 0;
                            let lost = write_over(world, &images, |brick, image| {
                                cleared += erase.overlay(brick, image);
                            });
                            if lost.is_none() && cleared > 0 {
                                world.erased(erase, actor);
                            }
                            lost
                        }
                        Change::PlaceProp(_) | Change::RemoveProp { .. } => None,
                    };
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
                    work.done.push(Done::Bricks(images));
                    if let Some(reason) = lost {
                        pass.spent += cost;
                        return failed(reason);
                    }
                }
            }
            pass.spent += cost;
        }
        work.read_settings(world, pass)
    }

    fn take_back(&mut self, world: &mut impl Voxels, pass: &mut Pass) -> Outcome {
        let work = &mut self.work;
        if !work.undo_checked {
            if let Err((reason, holds)) = work.undo_check(world) {
                return work.refuse_undo(reason, holds);
            }
            work.undo_checked = true;
            let placed: Vec<u32> = work
                .done
                .iter()
                .filter_map(|done| match done {
                    Done::Placed { entity, .. } => Some(*entity),
                    _ => None,
                })
                .collect();
            for entity in placed {
                if let Err(reason) = world.hold_prop(entity) {
                    return work.refuse_undo(reason, 0);
                }
                work.held.push(entity);
            }
            if !work.held.is_empty() {
                return Outcome::Later;
            }
        }
        while let Some(done) = work.done.last_mut() {
            if pass.spent >= pass.budget {
                return Outcome::Again;
            }
            match done {
                Done::Bricks(bricks) => {
                    let Some((brick, image)) = bricks.last() else {
                        work.done.pop();
                        continue;
                    };
                    let brick = *brick;
                    let expected = work.now.get(&brick).copied();
                    let result = world.read(brick).and_then(|now| {
                        if Some(hash(&now)) == expected {
                            world.write(brick, image)
                        } else {
                            Err(Status::REFUSED)
                        }
                    });
                    match result {
                        Ok(()) => {
                            work.now.insert(brick, hash(image));
                            bricks.pop();
                            pass.spent += 1;
                            work.waits = 0;
                        }
                        Err(Status::NOT_LOADED) => {
                            work.waits += 1;
                            if work.waits >= pass.waits {
                                return work.refuse_undo(Status::NOT_LOADED, 0);
                            }
                            return Outcome::Later;
                        }
                        Err(reason) => return work.refuse_undo(reason, 0),
                    }
                }
                Done::Placed {
                    entity,
                    prop,
                    settings,
                } => {
                    let entity = *entity;
                    if let Err((reason, holds)) = still_placed(world, entity, prop, settings) {
                        return work.refuse_undo(reason, holds);
                    }
                    if let Err(reason) = world.still_held(entity) {
                        return work.refuse_undo(reason, 0);
                    }
                    if let Err(reason) = world.remove_prop(entity, work.actor.unwrap_or(0)) {
                        return work.refuse_undo(reason, 0);
                    }
                    work.held.retain(|&held| held != entity);
                    work.done.pop();
                    pass.spent += 1;
                }
                &mut Done::Removed { prop, holds } => {
                    if let Err((reason, holds)) = still_removed(world, &prop, holds) {
                        return work.refuse_undo(reason, holds);
                    }
                    if let Err(reason) = world.spawn_prop(&prop, work.actor.unwrap_or(0)) {
                        return work.refuse_undo(reason, 0);
                    }
                    work.done.pop();
                    pass.spent += 1;
                }
            }
        }
        work.now.clear();
        work.holds = 0;
        Outcome::Done(TransactionInfo::UNDONE, Status::OK)
    }
}

/// `Ok` while the prop the transaction placed as `entity` is still there
/// as it placed it and holds no state, its start state as it first was;
/// the reason and the kinds of state otherwise.
fn still_placed(
    world: &mut impl Voxels,
    entity: u32,
    prop: &Prop,
    settings: &Settings,
) -> Result<(), (Status, u32)> {
    const START: u32 = TransactionInfo::HOLDS_LOGIC_GRAPH_TRIGGER | TransactionInfo::HOLDS_GROWTH;
    match world.read_prop(entity) {
        Ok((now, holds)) if now.same_place(prop) => {
            let holds = if holds & START != 0 && settings_kept(world, entity, settings) {
                holds & !START
            } else {
                holds
            };
            if holds == 0 {
                Ok(())
            } else {
                Err((Status::REFUSED, holds))
            }
        }
        Ok(_) | Err(Status::NOT_A_PROP) => Err((Status::REFUSED, 0)),
        Err(reason) => Err((reason, 0)),
    }
}

/// Whether the start state of `entity` is still the one read the first
/// step it stood in the world.
fn settings_kept(world: &mut impl Voxels, entity: u32, settings: &Settings) -> bool {
    match settings {
        Settings::Read(Some(first)) => {
            matches!(world.start_state(entity), Ok(Some(now)) if now == *first)
        }
        Settings::Read(None) | Settings::Unread => false,
    }
}

/// `Ok` while the prop the transaction removed can come back as it was:
/// it held no state a respawn would lose, and its place is free.
fn still_removed(world: &mut impl Voxels, prop: &Prop, holds: u32) -> Result<(), (Status, u32)> {
    if holds != 0 {
        return Err((Status::REFUSED, holds));
    }
    match world.prop_at(prop) {
        Ok(false) => Ok(()),
        Ok(true) => Err((Status::REFUSED, 0)),
        Err(reason) => Err((reason, 0)),
    }
}

impl Work {
    /// The checks of a transaction's first step, before it writes anything:
    /// inside the world, the props it removes, its actor's rights and its
    /// columns loaded; the outcome that stops it, or `None`.
    fn first_check(&mut self, world: &mut impl Voxels, pass: &mut Pass) -> Option<Outcome> {
        let failed = |reason| Some(Outcome::Done(TransactionInfo::FAILED, reason));
        let inside = pass.extent.map(|extent| {
            self.ops
                .iter()
                .all(|op| op.bricks.is_none_or(|bricks| bricks.inside(extent)))
        });
        if inside != Some(true) {
            return failed(inside.map_or(Status::MISSING, |_| Status::OUTSIDE_WORLD));
        }
        let mut removed = Vec::with_capacity(self.ops.len());
        for op in &self.ops {
            let prop = match op.change {
                Change::RemoveProp {
                    entity,
                    discard_state,
                } => match world.read_prop(entity) {
                    Ok((prop, holds)) if holds == 0 || discard_state => Some(prop),
                    Ok((_, holds)) => {
                        self.holds = holds;
                        return failed(Status::REFUSED);
                    }
                    Err(reason) => return failed(reason),
                },
                _ => None,
            };
            removed.push(prop);
        }
        if let Some(actor) = self.actor {
            for (op, prop) in self.ops.iter().zip(&removed) {
                let Some((min, max)) = op.change.reach(prop.as_ref()) else {
                    return failed(Status::MISSING);
                };
                match world.rights(actor, min, max) {
                    Ok(0) => {}
                    Ok(refusal) => {
                        self.refusal = refusal;
                        return failed(Status::REFUSED);
                    }
                    Err(reason) => return failed(reason),
                }
            }
        }
        let mut columns = FxHashSet::default();
        let bricks = self.ops.iter().filter_map(|op| op.bricks);
        for brick in bricks.flat_map(Bricks::columns) {
            let [x, _, z] = brick.0;
            if columns.insert([x, z]) {
                pass.probes += 1;
                if let Err(reason) = world.loaded(brick) {
                    return failed(reason);
                }
            }
        }
        None
    }

    /// The checks of an undo before it writes anything: every brick holds
    /// what the transaction left, every prop it placed is there without
    /// state, every prop it removed can come back.
    fn undo_check(&self, world: &mut impl Voxels) -> Result<(), (Status, u32)> {
        let bricks: FxHashSet<Brick> = self
            .done
            .iter()
            .flat_map(|done| done.bricks().iter().map(|(brick, _)| *brick))
            .collect();
        for brick in bricks {
            match (self.now.get(&brick), world.read(brick)) {
                (Some(left), Ok(now)) if hash(&now) == *left => {}
                (_, Err(reason)) => return Err((reason, 0)),
                _ => return Err((Status::REFUSED, 0)),
            }
        }
        for done in &self.done {
            match done {
                Done::Bricks(_) => {}
                Done::Placed {
                    entity,
                    prop,
                    settings,
                } => still_placed(world, *entity, prop, settings)?,
                Done::Removed { prop, holds } => still_removed(world, prop, *holds)?,
            }
        }
        Ok(())
    }

    /// Holds every prop it removes against players, for the step before
    /// it removes them; whether it holds any.
    fn hold_removals(&mut self, world: &mut impl Voxels) -> Result<bool, Status> {
        let removals = self.ops.iter().filter_map(|op| match op.change {
            Change::RemoveProp { entity, .. } => Some(entity),
            _ => None,
        });
        for entity in removals {
            world.hold_prop(entity)?;
            self.held.push(entity);
        }
        Ok(!self.held.is_empty())
    }

    /// Removes the prop `entity` held since the step before, checking
    /// again that it holds no state unless `discard_state`.
    fn remove(
        &mut self,
        world: &mut impl Voxels,
        entity: u32,
        discard_state: bool,
        actor: u32,
    ) -> Result<(), Status> {
        let (prop, holds) = world.read_prop(entity)?;
        if holds != 0 && !discard_state {
            self.holds = holds;
            return Err(Status::REFUSED);
        }
        world.still_held(entity)?;
        world.remove_prop(entity, actor)?;
        self.held.retain(|&held| held != entity);
        self.done.push(Done::Removed { prop, holds });
        Ok(())
    }

    /// Reads the start state of every prop it placed once the prop
    /// stands in the world, waiting a step at a time up to the wait limit;
    /// applied once read, or at the limit, with those unread kept unread.
    fn read_settings(&mut self, world: &mut impl Voxels, pass: &Pass) -> Outcome {
        let mut waiting = false;
        for done in &mut self.done {
            if let Done::Placed {
                entity,
                settings: settings @ Settings::Unread,
                ..
            } = done
            {
                match world.start_state(*entity) {
                    Ok(bytes) => *settings = Settings::Read(bytes),
                    Err(Status::MISSING) => waiting = true,
                    Err(_) => {}
                }
            }
        }
        if waiting {
            self.waits += 1;
            if self.waits < pass.waits {
                return Outcome::Later;
            }
        }
        self.waits = 0;
        Outcome::Done(TransactionInfo::APPLIED, Status::OK)
    }

    /// Stops an undo for `reason`, with the kinds of state `holds` that
    /// stopped it; asking again checks afresh.
    fn refuse_undo(&mut self, reason: Status, holds: u32) -> Outcome {
        self.undo_checked = false;
        self.waits = 0;
        self.holds = holds;
        Outcome::Done(TransactionInfo::UNDO_REFUSED, reason)
    }
}

#[cfg(test)]
mod tests;
