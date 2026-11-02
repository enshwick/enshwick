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

use enshwick::abi::{Status, TerrainEdit, TransactionInfo};
use rustc_hash::{FxHashMap, FxHasher};

use super::block::Block;
use super::cover::{self, BRICK_VOXELS, Brick, Bricks};
use super::paste::{Erase, Paste};
use super::props::Prop;
use crate::mod_index::ModIndex;
use crate::voxels::WorldExtent;

mod checks;
mod queue;
mod run;

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

#[cfg(test)]
mod tests;
