use enshwick::abi::{Header, Status, TerrainEdit, TransactionInfo};
use rustc_hash::{FxHashMap, FxHashSet};

use super::{BRICK_VOXELS, Brick, Change, Image, Ledger, Limits, Voxels};
use crate::mod_index::ModIndex;
use crate::transactions::block::Block;
use crate::transactions::cover;
use crate::transactions::paste::{Erase, Paste, Region};
use crate::transactions::props::Prop;
use crate::voxels::WorldExtent;

mod holds;
mod props;
mod queue;
mod start_state;
mod terrain;
mod voxels;

const ME: ModIndex = ModIndex::new(0);
const LIMITS: Limits = Limits {
    ops: 8,
    bricks: 256,
    records: 16,
    waits: 3,
};

/// A world of bricks, air where never written; an edit stamps every brick
/// it can touch with its own number.
#[derive(Default)]
struct World {
    bricks: FxHashMap<Brick, Image>,
    unloaded: FxHashSet<[u32; 2]>,
    edits: u16,
    restores: usize,
    /// Pages of 32 by 32 bricks along x and z.
    pages: u32,
    /// A column the next edit unloads once it has written.
    unload_on_edit: Option<Brick>,
    /// What the next restore fails with.
    restore_fails: Option<Status>,
    /// The refusal each actor meets, by entity; none for the rest.
    refusals: FxHashMap<u32, u32>,
    /// The boxes asked about, in metres.
    asked: Vec<([f64; 3], [f64; 3])>,
    /// What was announced: placed or torn down, and by whom.
    announced: Vec<(&'static str, u32)>,
    /// The props in the world, by entity.
    props: FxHashMap<u32, Prop>,
    /// The state each prop holds that a respawn would lose, as kind bits.
    stateful: FxHashMap<u32, u32>,
    /// The last entity id handed out.
    last_entity: u32,
    /// The bytes of each prop's start state, by entity: a logic trigger's
    /// settings, a plant's level.
    triggers: FxHashMap<u32, Vec<u8>>,
    /// The kind of start state each item's props hold, as a `HOLDS_` bit:
    /// logic triggers, plants.
    trigger_items: FxHashMap<u32, u32>,
    /// The kind of start state each prop holds, by entity.
    kinds: FxHashMap<u32, u32>,
    /// Whether a spawned prop lands only at [`World::land`], as the
    /// game's appear the step after their spawn.
    spawns_wait: bool,
    /// Props spawned and not landed yet.
    landing: FxHashSet<u32>,
    /// Props held against players before their destroy.
    held: FxHashSet<u32>,
    /// Creatures placed from items: animals, pets, survivors.
    creatures: FxHashSet<u32>,
    /// Whether the facts the state reads need fail to resolve.
    facts_missing: bool,
    /// Whether the filter that keeps players from held props went off.
    filter_off: bool,
}

impl World {
    /// A world of 40 by 40 pages: 10,240 voxels, 5,120 metres a side.
    fn new() -> Self {
        Self {
            pages: 40,
            ..Self::default()
        }
    }

    /// The props spawned since the last call stand in the world.
    fn land(&mut self) {
        self.landing.clear();
    }

    fn brick(&self, brick: Brick) -> [u16; BRICK_VOXELS] {
        self.bricks
            .get(&brick)
            .map_or([0; BRICK_VOXELS], |image| **image)
    }

    /// Every brick written so far, without the air.
    fn snapshot(&self) -> Vec<(Brick, [u16; BRICK_VOXELS])> {
        let mut bricks: Vec<_> = self
            .bricks
            .iter()
            .filter(|(_, image)| image.iter().any(|&voxel| voxel != 0))
            .map(|(brick, image)| (*brick, **image))
            .collect();
        bricks.sort_by_key(|(brick, _)| *brick);
        bricks
    }

    fn unload(&mut self, brick: Brick) {
        let [x, _, z] = brick.0;
        self.unloaded.insert([x, z]);
    }
}

impl Voxels for World {
    fn spawn_prop(&mut self, prop: &Prop, actor: u32) -> Result<u32, Status> {
        self.last_entity += 1;
        self.props.insert(self.last_entity, *prop);
        if let Some(&kind) = self.trigger_items.get(&prop.item) {
            self.triggers.insert(self.last_entity, vec![1, 2, 3]);
            self.kinds.insert(self.last_entity, kind);
        }
        if self.spawns_wait {
            self.landing.insert(self.last_entity);
        }
        self.announced.push(("prop placed", actor));
        Ok(self.last_entity)
    }

    fn read_prop(&mut self, entity: u32) -> Result<(Prop, u32), Status> {
        if self.landing.contains(&entity) || self.facts_missing {
            return Err(Status::MISSING);
        }
        if self.creatures.contains(&entity) {
            return Err(Status::NOT_A_PROP);
        }
        let prop = self.props.get(&entity).copied().ok_or(Status::NOT_A_PROP)?;
        let trigger = self.kinds.get(&entity).copied().unwrap_or(0);
        Ok((
            prop,
            self.stateful.get(&entity).copied().unwrap_or(0) | trigger,
        ))
    }

    fn hold_prop(&mut self, entity: u32) -> Result<(), Status> {
        self.held.insert(entity);
        Ok(())
    }

    fn still_held(&mut self, entity: u32) -> Result<(), Status> {
        if self.filter_off || !self.held.contains(&entity) {
            return Err(Status::MISSING);
        }
        Ok(())
    }

    fn release_prop(&mut self, entity: u32) {
        self.held.remove(&entity);
    }

    fn start_state(&mut self, entity: u32) -> Result<Option<Vec<u8>>, Status> {
        if self.landing.contains(&entity) || !self.props.contains_key(&entity) {
            return Err(Status::MISSING);
        }
        Ok(self.triggers.get(&entity).cloned())
    }

    fn prop_at(&mut self, prop: &Prop) -> Result<bool, Status> {
        Ok(self.props.values().any(|there| there == prop))
    }

    fn remove_prop(&mut self, entity: u32, actor: u32) -> Result<(), Status> {
        self.props.remove(&entity).ok_or(Status::MISSING)?;
        self.held.remove(&entity);
        self.announced.push(("prop removed", actor));
        Ok(())
    }

    fn rights(&mut self, actor: u32, min: [f64; 3], max: [f64; 3]) -> Result<u32, Status> {
        self.asked.push((min, max));
        Ok(self.refusals.get(&actor).copied().unwrap_or(0))
    }

    fn place(&mut self, block: &Block, actor: u32) -> Result<(), Status> {
        let word = u16::from(block.material) | 0xff00;
        for brick in block.region.bricks().iter() {
            let mut image = self.brick(brick);
            Paste::new(
                block.region.bounds().0,
                block.region.bounds().1,
                vec![word; block.region.len()],
            )?
            .overlay(brick, &mut image);
            self.bricks.insert(brick, Box::new(image));
        }
        self.announced.push(("placed", actor));
        Ok(())
    }

    fn erased(&mut self, _erase: &Erase, actor: u32) {
        self.announced.push(("torn down", actor));
    }

    fn extent(&mut self) -> Result<WorldExtent, Status> {
        WorldExtent::from_grid(32 << 3 | 32 << 35, self.pages, self.pages).ok_or(Status::MISSING)
    }

    fn loaded(&mut self, brick: Brick) -> Result<(), Status> {
        let [x, _, z] = brick.0;
        if self.unloaded.contains(&[x, z]) {
            Err(Status::NOT_LOADED)
        } else {
            Ok(())
        }
    }

    fn read(&mut self, brick: Brick) -> Result<Image, Status> {
        self.loaded(brick)?;
        Ok(Box::new(self.brick(brick)))
    }

    fn edit(&mut self, edit: &TerrainEdit) -> Result<(), Status> {
        self.edits += 1;
        for brick in cover::check(edit)?.iter() {
            self.bricks
                .insert(brick, Box::new([self.edits; BRICK_VOXELS]));
        }
        if let Some(brick) = self.unload_on_edit.take() {
            self.unload(brick);
        }
        Ok(())
    }

    fn write(&mut self, brick: Brick, image: &[u16; BRICK_VOXELS]) -> Result<(), Status> {
        if let Some(reason) = self.restore_fails.take() {
            return Err(reason);
        }
        self.restores += 1;
        self.bricks.insert(brick, Box::new(*image));
        Ok(())
    }
}

/// A round dig of radius 4 at `at` metres on every axis: 64 bricks.
fn dig(at: f32) -> Change {
    Change::Terrain(terrain(at))
}

/// The edit of [`dig`].
fn terrain(at: f32) -> TerrainEdit {
    TerrainEdit {
        header: Header::of::<TerrainEdit>(),
        kind: TerrainEdit::DIG,
        shape: TerrainEdit::ROUND,
        flags: 0,
        material: 0,
        position: [at; 3],
        radius: 4.0,
        height: 0.0,
        protect: [0; 16],
    }
}

fn state(ledger: &Ledger, id: u64) -> (u32, Status) {
    let info = ledger.info(ME, id).unwrap();
    (info.state, info.reason)
}

fn applied(ledger: &Ledger, id: u64) -> u32 {
    ledger.info(ME, id).unwrap().applied
}

/// A prop of item 77 at `at` metres on every axis.
fn prop(at: f64) -> Prop {
    Prop::for_tests(77, [at; 3])
}

/// Steps until transaction `id` stops working, at most ten steps.
fn settle(ledger: &mut Ledger, world: &mut World, id: u64) {
    for _ in 0..10 {
        let (state, _) = state(ledger, id);
        if state != TransactionInfo::PENDING && state != TransactionInfo::UNDOING {
            return;
        }
        ledger.step(world, 1000);
    }
}

/// Steps until no work is left, at most ten steps.
fn run_out(ledger: &mut Ledger, world: &mut World) {
    for _ in 0..10 {
        ledger.step(world, 1000);
    }
}
