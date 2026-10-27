use enshwick::abi::{Header, Status, TerrainEdit, TransactionInfo};
use rustc_hash::{FxHashMap, FxHashSet};

use super::{BRICK_VOXELS, Brick, Change, Image, Ledger, Limits, Voxels};
use crate::mod_index::ModIndex;
use crate::transactions::block::Block;
use crate::transactions::cover;
use crate::transactions::paste::{Erase, Paste, Region};
use crate::transactions::props::Prop;
use crate::voxels::WorldExtent;

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

#[test]
fn a_transaction_writes_its_changes_in_order_and_its_undo_puts_every_brick_back() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    world.edit(&terrain(30.0)).unwrap();
    let before = world.snapshot();

    let id = ledger.commit(ME, None, vec![dig(20.0), dig(22.0)]).unwrap();
    assert_eq!(state(&ledger, id).0, TransactionInfo::PENDING);
    ledger.step(&mut world, 1000);
    assert_eq!(state(&ledger, id), (TransactionInfo::APPLIED, Status::OK));
    assert_eq!(world.edits, 3);
    assert_eq!(
        world.brick(Brick([5, 5, 5])),
        [3; BRICK_VOXELS],
        "the second change wrote over the first"
    );
    assert!(ledger.info(ME, id).unwrap().bricks >= 64);

    ledger.undo(ME, id).unwrap();
    assert_eq!(state(&ledger, id).0, TransactionInfo::UNDOING);
    ledger.step(&mut world, 1000);
    assert_eq!(state(&ledger, id).0, TransactionInfo::UNDONE);
    assert_eq!(world.snapshot(), before);
    assert_eq!(ledger.kept(), 0);
    assert!(!ledger.busy());
}

#[test]
fn the_budget_spreads_changes_over_steps_but_never_splits_one() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    let id = ledger
        .commit(ME, None, vec![dig(20.0), dig(40.0), dig(60.0)])
        .unwrap();

    ledger.step(&mut world, 100);
    assert_eq!(applied(&ledger, id), 1, "64 bricks fit, 128 do not");
    assert_eq!(state(&ledger, id).0, TransactionInfo::PENDING);
    ledger.step(&mut world, 10);
    assert_eq!(applied(&ledger, id), 2, "a step writes one change at least");
    ledger.step(&mut world, 10);
    assert_eq!(state(&ledger, id).0, TransactionInfo::APPLIED);

    ledger.undo(ME, id).unwrap();
    ledger.step(&mut world, 100);
    assert_eq!(world.restores, 100);
    assert_eq!(state(&ledger, id).0, TransactionInfo::UNDOING);
    ledger.step(&mut world, 100);
    ledger.step(&mut world, 100);
    assert_eq!(state(&ledger, id).0, TransactionInfo::UNDONE);
    assert_eq!(world.snapshot(), Vec::new());
}

#[test]
fn a_transaction_over_an_unloaded_column_fails_before_writing_anything() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    world.unload(Brick([8, 0, 8]));
    let id = ledger.commit(ME, None, vec![dig(20.0), dig(40.0)]).unwrap();

    ledger.step(&mut world, 1000);
    assert_eq!(
        state(&ledger, id),
        (TransactionInfo::FAILED, Status::NOT_LOADED)
    );
    assert_eq!(world.edits, 0);
    assert_eq!(applied(&ledger, id), 0);
    assert_eq!(ledger.undo(ME, id), Err(Status::REFUSED), "nothing to undo");
}

#[test]
fn a_column_unloaded_between_steps_stops_the_transaction_where_it_is() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    let id = ledger.commit(ME, None, vec![dig(20.0), dig(40.0)]).unwrap();
    ledger.step(&mut world, 64);
    world.unload(Brick([8, 0, 8]));
    ledger.step(&mut world, 64);
    assert_eq!(
        state(&ledger, id),
        (TransactionInfo::FAILED, Status::NOT_LOADED)
    );
    assert_eq!((world.edits, applied(&ledger, id)), (1, 1));

    ledger.undo(ME, id).unwrap();
    ledger.step(&mut world, 1000);
    assert_eq!(state(&ledger, id).0, TransactionInfo::UNDONE);
    assert_eq!(world.snapshot(), Vec::new());
}

#[test]
fn an_undo_while_writing_takes_back_what_was_written_and_stops() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    let id = ledger
        .commit(ME, None, vec![dig(20.0), dig(40.0), dig(60.0)])
        .unwrap();
    ledger.step(&mut world, 64);
    ledger.undo(ME, id).unwrap();
    ledger.step(&mut world, 1000);
    assert_eq!(state(&ledger, id).0, TransactionInfo::UNDONE);
    assert_eq!(world.edits, 1);
    assert_eq!(world.snapshot(), Vec::new());
}

#[test]
fn an_undo_of_a_queued_transaction_drops_it() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    let id = ledger.commit(ME, None, vec![dig(20.0)]).unwrap();
    ledger.undo(ME, id).unwrap();
    assert_eq!(state(&ledger, id).0, TransactionInfo::UNDONE);
    ledger.step(&mut world, 1000);
    assert_eq!(world.edits, 0);
    assert_eq!(ledger.undo(ME, id), Err(Status::REFUSED));
}

#[test]
fn an_undo_is_refused_while_a_later_change_holds_its_bricks() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    let first = ledger.commit(ME, None, vec![dig(20.0)]).unwrap();
    let second = ledger.commit(ME, None, vec![dig(22.0)]).unwrap();
    ledger.step(&mut world, 1000);
    let both = world.snapshot();

    ledger.undo(ME, first).unwrap();
    ledger.step(&mut world, 1000);
    assert_eq!(
        state(&ledger, first),
        (TransactionInfo::UNDO_REFUSED, Status::REFUSED)
    );
    assert_eq!(world.snapshot(), both, "a refused undo writes nothing");

    ledger.undo(ME, second).unwrap();
    ledger.undo(ME, first).unwrap();
    ledger.step(&mut world, 1000);
    assert_eq!(state(&ledger, second).0, TransactionInfo::UNDONE);
    assert_eq!(state(&ledger, first).0, TransactionInfo::UNDONE);
    assert_eq!(world.snapshot(), Vec::new());
}

#[test]
fn an_undo_over_an_unloaded_column_is_refused_and_may_be_asked_again() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    let id = ledger.commit(ME, None, vec![dig(20.0)]).unwrap();
    ledger.step(&mut world, 1000);
    world.unload(Brick([3, 0, 3]));
    ledger.undo(ME, id).unwrap();
    ledger.step(&mut world, 1000);
    assert_eq!(
        state(&ledger, id),
        (TransactionInfo::UNDO_REFUSED, Status::NOT_LOADED)
    );

    world.unloaded.clear();
    ledger.undo(ME, id).unwrap();
    ledger.step(&mut world, 1000);
    assert_eq!(state(&ledger, id).0, TransactionInfo::UNDONE);
}

#[test]
fn an_undo_begun_waits_for_a_column_that_unloads_and_then_finishes() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    let id = ledger.commit(ME, None, vec![dig(20.0)]).unwrap();
    ledger.step(&mut world, 1000);
    ledger.undo(ME, id).unwrap();
    ledger.step(&mut world, 8);
    world.unload(Brick([3, 0, 3]));
    ledger.step(&mut world, 1000);
    assert_eq!(state(&ledger, id).0, TransactionInfo::UNDOING);
    world.unloaded.clear();
    ledger.step(&mut world, 1000);
    assert_eq!(state(&ledger, id).0, TransactionInfo::UNDONE);
    assert_eq!(world.snapshot(), Vec::new());
}

#[test]
fn the_oldest_undo_data_goes_first_once_the_cap_is_reached() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    let ids: Vec<u64> = [20.0, 40.0, 60.0, 80.0, 100.0]
        .into_iter()
        .map(|at| {
            let id = ledger.commit(ME, None, vec![dig(at)]).unwrap();
            ledger.step(&mut world, 1000);
            id
        })
        .collect();
    assert_eq!(ledger.kept(), 256);
    assert_eq!(state(&ledger, ids[0]).0, TransactionInfo::EXPIRED);
    assert_eq!(ledger.info(ME, ids[0]).unwrap().bricks, 0);
    assert_eq!(ledger.undo(ME, ids[0]), Err(Status::REFUSED));
    for &id in &ids[1..] {
        assert_eq!(state(&ledger, id).0, TransactionInfo::APPLIED);
    }
}

#[test]
fn a_transaction_past_the_caps_is_refused_at_commit() {
    let mut ledger = Ledger::new(LIMITS);
    assert_eq!(ledger.commit(ME, None, vec![]), Err(Status::BAD_ARGS));
    assert_eq!(
        ledger.commit(ME, None, vec![dig(20.0); 9]),
        Err(Status::BAD_ARGS)
    );
    assert_eq!(
        ledger.commit(ME, None, vec![dig(20.0); 5]),
        Err(Status::BAD_ARGS),
        "320 bricks"
    );
    let dry = TerrainEdit {
        flags: TerrainEdit::DRY_RUN,
        ..terrain(20.0)
    };
    assert_eq!(
        ledger.commit(ME, None, vec![Change::Terrain(dry)]),
        Err(Status::BAD_ARGS)
    );

    ledger.commit(ME, None, vec![dig(20.0); 3]).unwrap();
    assert_eq!(
        ledger.commit(ME, None, vec![dig(20.0); 2]),
        Err(Status::REFUSED),
        "the queue claims 192 of 256 bricks"
    );
    ledger.commit(ME, None, vec![dig(20.0)]).unwrap();
}

#[test]
fn the_oldest_settled_transaction_is_forgotten_past_the_record_cap() {
    let mut world = World::new();
    let mut ledger = Ledger::new(Limits {
        records: 2,
        ..LIMITS
    });
    let first = ledger.commit(ME, None, vec![dig(20.0)]).unwrap();
    let second = ledger.commit(ME, None, vec![dig(40.0)]).unwrap();
    assert_eq!(
        ledger.commit(ME, None, vec![dig(60.0)]),
        Err(Status::REFUSED),
        "both work"
    );
    ledger.step(&mut world, 1000);
    let third = ledger.commit(ME, None, vec![dig(60.0)]).unwrap();
    assert_eq!(ledger.info(ME, first), None);
    assert!(ledger.info(ME, second).is_some());
    assert!(ledger.info(ME, third).is_some());
    assert_eq!(ledger.undo(ME, first), Err(Status::REFUSED));
}

#[test]
fn a_mod_sees_and_undoes_only_its_own_transactions() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    let id = ledger.commit(ME, None, vec![dig(20.0)]).unwrap();
    ledger.step(&mut world, 1000);
    let other = ModIndex::new(1);
    assert_eq!(ledger.info(other, id), None);
    assert_eq!(ledger.undo(other, id), Err(Status::REFUSED));
    assert_eq!(state(&ledger, id).0, TransactionInfo::APPLIED);
}

#[test]
fn a_world_that_ends_fails_the_work_and_drops_the_undo_data() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    let done = ledger.commit(ME, None, vec![dig(20.0)]).unwrap();
    ledger.step(&mut world, 1000);
    let queued = ledger.commit(ME, None, vec![dig(40.0)]).unwrap();
    ledger.forget();
    assert_eq!(state(&ledger, done).0, TransactionInfo::EXPIRED);
    assert_eq!(
        state(&ledger, queued),
        (TransactionInfo::FAILED, Status::MISSING)
    );
    assert_eq!(ledger.kept(), 0);
    assert!(!ledger.busy());
}

/// A dig of radius 4 whose box ends past the far edge of the test world.
fn past_the_edge() -> Change {
    Change::Terrain(TerrainEdit {
        position: [5119.0, 20.0, 20.0],
        ..terrain(20.0)
    })
}

#[test]
fn a_transaction_leaving_the_world_fails_before_writing_anything() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    let id = ledger
        .commit(ME, None, vec![dig(20.0), past_the_edge()])
        .unwrap();
    ledger.step(&mut world, 1000);
    assert_eq!(
        state(&ledger, id),
        (TransactionInfo::FAILED, Status::OUTSIDE_WORLD)
    );
    assert_eq!(world.edits, 0);
}

#[test]
fn once_the_extent_is_known_a_commit_leaving_the_world_is_refused() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    let first = ledger.commit(ME, None, vec![dig(20.0)]).unwrap();
    ledger.step(&mut world, 1000);
    assert_eq!(state(&ledger, first).0, TransactionInfo::APPLIED);
    assert_eq!(
        ledger.commit(ME, None, vec![dig(20.0), past_the_edge()]),
        Err(Status::OUTSIDE_WORLD)
    );
    let high = TerrainEdit {
        position: [20.0, 1023.0, 20.0],
        ..terrain(20.0)
    };
    assert_eq!(
        ledger.commit(ME, None, vec![Change::Terrain(high)]),
        Err(Status::OUTSIDE_WORLD)
    );
    ledger.forget();
    assert!(
        ledger.commit(ME, None, vec![past_the_edge()]).is_ok(),
        "a new world's extent is not known until its first step"
    );
}

#[test]
fn a_brick_changed_between_undo_steps_stops_the_undo_before_it() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    let id = ledger.commit(ME, None, vec![dig(20.0)]).unwrap();
    ledger.step(&mut world, 1000);
    ledger.undo(ME, id).unwrap();
    ledger.step(&mut world, 8);
    assert_eq!(world.restores, 8);

    let player = Brick([3, 3, 3]);
    world.bricks.insert(player, Box::new([99; BRICK_VOXELS]));
    ledger.step(&mut world, 1000);
    assert_eq!(
        state(&ledger, id),
        (TransactionInfo::UNDO_REFUSED, Status::REFUSED)
    );
    assert_eq!(
        world.brick(player),
        [99; BRICK_VOXELS],
        "the player's change stays"
    );
    assert!(world.restores < 64);
    assert_eq!(
        ledger.info(ME, id).unwrap().bricks,
        64 - world.restores as u64,
        "what was put back is no longer held"
    );

    world.bricks.insert(player, Box::new([1; BRICK_VOXELS]));
    ledger.undo(ME, id).unwrap();
    ledger.step(&mut world, 1000);
    assert_eq!(
        state(&ledger, id).0,
        TransactionInfo::UNDONE,
        "once the brick holds what the transaction left, the undo finishes"
    );
    assert_eq!(world.snapshot(), Vec::new());
}

#[test]
fn a_brick_that_cannot_be_read_after_its_edit_fails_the_transaction() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    world.unload_on_edit = Some(Brick([3, 0, 3]));
    let id = ledger.commit(ME, None, vec![dig(20.0), dig(40.0)]).unwrap();
    ledger.step(&mut world, 1000);
    assert_eq!(
        state(&ledger, id),
        (TransactionInfo::FAILED, Status::NOT_LOADED)
    );
    assert_eq!((world.edits, applied(&ledger, id)), (1, 1));

    world.unloaded.clear();
    ledger.undo(ME, id).unwrap();
    ledger.step(&mut world, 1000);
    assert_eq!(
        state(&ledger, id),
        (TransactionInfo::UNDO_REFUSED, Status::REFUSED),
        "a brick whose state after the edit is unknown is never written over"
    );
}

#[test]
fn an_undo_is_refused_while_queued_work_holds_the_cap() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    let first = ledger
        .commit(ME, None, vec![dig(20.0), dig(40.0), dig(60.0), dig(80.0)])
        .unwrap();
    ledger.step(&mut world, 1000);
    let second = ledger
        .commit(
            ME,
            None,
            vec![dig(100.0), dig(120.0), dig(140.0), dig(160.0)],
        )
        .unwrap();
    assert_eq!(ledger.undo(ME, first), Err(Status::REFUSED));
    ledger.step(&mut world, 1000);
    assert_eq!(state(&ledger, second).0, TransactionInfo::APPLIED);
    assert!(ledger.kept() <= LIMITS.bricks);
    assert_eq!(state(&ledger, first).0, TransactionInfo::EXPIRED);
}

#[test]
fn a_failed_restore_refuses_the_undo_and_a_new_one_finishes_it() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    let id = ledger.commit(ME, None, vec![dig(20.0)]).unwrap();
    ledger.step(&mut world, 1000);
    ledger.undo(ME, id).unwrap();
    ledger.step(&mut world, 8);
    world.restore_fails = Some(Status::MISSING);
    ledger.step(&mut world, 1000);
    assert_eq!(
        state(&ledger, id),
        (TransactionInfo::UNDO_REFUSED, Status::MISSING)
    );
    assert_eq!(ledger.info(ME, id).unwrap().bricks, 56);

    ledger.undo(ME, id).unwrap();
    ledger.step(&mut world, 1000);
    assert_eq!(state(&ledger, id).0, TransactionInfo::UNDONE);
    assert_eq!(world.snapshot(), Vec::new());
}

#[test]
fn an_undo_waits_a_limited_number_of_steps_for_a_column() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    let id = ledger.commit(ME, None, vec![dig(20.0)]).unwrap();
    ledger.step(&mut world, 1000);
    ledger.undo(ME, id).unwrap();
    ledger.step(&mut world, 8);
    world.unload(Brick([3, 0, 3]));
    for _ in 0..LIMITS.waits - 1 {
        ledger.step(&mut world, 1000);
        assert_eq!(state(&ledger, id).0, TransactionInfo::UNDOING);
    }
    ledger.step(&mut world, 1000);
    assert_eq!(
        state(&ledger, id),
        (TransactionInfo::UNDO_REFUSED, Status::NOT_LOADED)
    );
    assert!(!ledger.busy(), "it no longer holds the queue");

    world.unloaded.clear();
    ledger.undo(ME, id).unwrap();
    ledger.step(&mut world, 1000);
    assert_eq!(state(&ledger, id).0, TransactionInfo::UNDONE);
}

#[test]
fn an_undo_asked_while_a_change_is_in_hand_runs_once_it_is_back() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    let id = ledger.commit(ME, None, vec![dig(20.0), dig(40.0)]).unwrap();
    let mut pass = ledger.begin(world.extent().ok(), 64);
    let mut task = ledger.take(&mut pass).unwrap();
    assert_eq!(
        ledger.undo(ME, id),
        Ok(()),
        "the ledger answers while the change is out"
    );
    let outcome = task.run(&mut world, &mut pass);
    assert!(!ledger.put(task, outcome));
    assert_eq!(state(&ledger, id).0, TransactionInfo::UNDOING);
    ledger.step(&mut world, 1000);
    assert_eq!(state(&ledger, id).0, TransactionInfo::UNDONE);
    assert_eq!(world.edits, 1);
    assert_eq!(world.snapshot(), Vec::new());
}

#[test]
fn a_change_in_hand_when_the_world_ends_is_dropped_on_return() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    let id = ledger.commit(ME, None, vec![dig(20.0)]).unwrap();
    let mut pass = ledger.begin(world.extent().ok(), 64);
    let mut task = ledger.take(&mut pass).unwrap();
    ledger.forget();
    let outcome = task.run(&mut world, &mut pass);
    assert!(!ledger.put(task, outcome));
    assert_eq!(
        state(&ledger, id),
        (TransactionInfo::FAILED, Status::MISSING)
    );
    assert_eq!(ledger.kept(), 0);
}

/// A paste of `size` from `min` whose every voxel is `word`.
fn paste(min: [u32; 3], size: [u32; 3], word: u16) -> Change {
    let count = size.iter().product::<u32>() as usize;
    Change::Voxels(Paste::new(min, size, vec![word; count]).unwrap())
}

#[test]
fn a_paste_writes_its_voxels_over_the_bricks_and_its_undo_takes_them_back() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    world.edit(&terrain(20.0)).unwrap();
    let before = world.snapshot();

    let id = ledger
        .commit(ME, None, vec![paste([30, 30, 30], [4, 4, 4], 0xff83)])
        .unwrap();
    ledger.step(&mut world, 1000);
    assert_eq!(state(&ledger, id), (TransactionInfo::APPLIED, Status::OK));
    let brick = world.brick(Brick([3, 3, 3]));
    assert_eq!(brick[6 + 8 * 6 + 64 * 6], 0xff83, "inside the paste");
    assert_eq!(brick[0], 1, "the rest of the brick keeps what the dig left");
    assert_eq!(
        ledger.info(ME, id).unwrap().bricks,
        8,
        "a 4³ box across 2³ bricks"
    );

    ledger.undo(ME, id).unwrap();
    ledger.step(&mut world, 1000);
    assert_eq!(state(&ledger, id).0, TransactionInfo::UNDONE);
    assert_eq!(world.snapshot(), before);
}

#[test]
fn a_paste_over_an_unloaded_column_fails_before_writing_anything() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    world.unload(Brick([4, 0, 4]));
    let id = ledger
        .commit(ME, None, vec![paste([30, 30, 30], [4, 4, 4], 0xff83)])
        .unwrap();
    ledger.step(&mut world, 1000);
    assert_eq!(
        state(&ledger, id),
        (TransactionInfo::FAILED, Status::NOT_LOADED)
    );
    assert_eq!(world.snapshot(), Vec::new());
}

#[test]
fn an_erase_clears_the_building_voxels_of_its_box_and_its_undo_brings_them_back() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    let built = ledger
        .commit(ME, None, vec![paste([30, 30, 30], [4, 4, 4], 0xff83)])
        .unwrap();
    ledger.step(&mut world, 1000);
    assert_eq!(state(&ledger, built).0, TransactionInfo::APPLIED);
    let before = world.snapshot();

    let erase = Erase::new([31, 31, 31], [2, 2, 2]).unwrap();
    let id = ledger.commit(ME, None, vec![Change::Erase(erase)]).unwrap();
    ledger.step(&mut world, 1000);
    assert_eq!(state(&ledger, id).0, TransactionInfo::APPLIED);
    let brick = world.brick(Brick([3, 3, 3]));
    assert_eq!(brick[7 + 8 * 7 + 64 * 7], 0, "inside the erase");
    assert_eq!(brick[6 + 8 * 6 + 64 * 6], 0xff83, "outside it");

    ledger.undo(ME, id).unwrap();
    ledger.step(&mut world, 1000);
    assert_eq!(world.snapshot(), before);
}

/// A block of building material 0x83 filling the box of `size` from `min`.
fn block(min: [u32; 3], size: [u32; 3]) -> Change {
    Change::Block(Box::new(Block::for_tests(
        Region::new(min, size).unwrap(),
        0x83,
    )))
}

#[test]
fn a_block_is_placed_announced_and_taken_back_by_its_undo() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    let id = ledger
        .commit(ME, Some(7), vec![block([30, 30, 30], [4, 4, 4])])
        .unwrap();
    ledger.step(&mut world, 1000);
    assert_eq!(state(&ledger, id).0, TransactionInfo::APPLIED);
    assert_eq!(world.brick(Brick([3, 3, 3]))[6 + 8 * 6 + 64 * 6], 0xff83);
    assert_eq!(world.announced, [("placed", 7)]);

    ledger.undo(ME, id).unwrap();
    ledger.step(&mut world, 1000);
    assert_eq!(state(&ledger, id).0, TransactionInfo::UNDONE);
    assert_eq!(world.snapshot(), Vec::new());
}

#[test]
fn an_erase_is_announced_as_a_tear_down_by_its_actor_or_by_none() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    ledger
        .commit(ME, None, vec![paste([30, 30, 30], [2, 2, 2], 0xff83)])
        .unwrap();
    ledger.step(&mut world, 1000);
    let erase = || Change::Erase(Erase::new([30, 30, 30], [2, 2, 2]).unwrap());
    ledger.commit(ME, Some(7), vec![erase()]).unwrap();
    ledger
        .commit(ME, None, vec![paste([30, 30, 30], [2, 2, 2], 0xff83)])
        .unwrap();
    ledger.commit(ME, None, vec![erase()]).unwrap();
    ledger.step(&mut world, 1000);
    assert_eq!(world.announced, [("torn down", 7), ("torn down", 0)]);
}

#[test]
fn an_actor_the_server_refuses_writes_nothing_and_the_status_says_why() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    world.refusals.insert(7, 2);
    let id = ledger
        .commit(
            ME,
            Some(7),
            vec![dig(20.0), paste([30, 30, 30], [2, 2, 2], 0xff83)],
        )
        .unwrap();
    ledger.step(&mut world, 1000);
    let info = ledger.info(ME, id).unwrap();
    assert_eq!(
        (info.state, info.reason, info.refusal),
        (TransactionInfo::FAILED, Status::REFUSED, 2)
    );
    assert_eq!(world.edits, 0);
    assert_eq!(world.snapshot(), Vec::new());
    assert_eq!(
        world.asked.first(),
        Some(&([16.0; 3], [24.0; 3])),
        "the dig's own reach, not its bricks"
    );
}

#[test]
fn a_transaction_without_an_actor_asks_no_rights() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    world.refusals.insert(0, 1);
    let id = ledger.commit(ME, None, vec![dig(20.0)]).unwrap();
    ledger.step(&mut world, 1000);
    assert_eq!(state(&ledger, id).0, TransactionInfo::APPLIED);
    assert!(world.asked.is_empty());
}

#[test]
fn an_allowed_actor_is_asked_about_every_change_before_the_first_write() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    let id = ledger
        .commit(
            ME,
            Some(7),
            vec![dig(20.0), paste([30, 30, 30], [2, 2, 2], 0xff83)],
        )
        .unwrap();
    ledger.step(&mut world, 1000);
    assert_eq!(state(&ledger, id).0, TransactionInfo::APPLIED);
    assert_eq!(
        world.asked,
        [([16.0; 3], [24.0; 3]), ([15.0; 3], [16.0; 3])]
    );
}

#[test]
fn an_erase_that_clears_nothing_raises_no_event() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    let erase = Change::Erase(Erase::new([30, 30, 30], [2, 2, 2]).unwrap());
    ledger.commit(ME, None, vec![erase]).unwrap();
    ledger.step(&mut world, 1000);
    assert!(world.announced.is_empty(), "only air was there");
}

#[test]
fn an_erase_whose_write_fails_raises_no_event() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    ledger
        .commit(ME, None, vec![paste([30, 30, 30], [2, 2, 2], 0xff83)])
        .unwrap();
    ledger.step(&mut world, 1000);
    world.restore_fails = Some(Status::MISSING);
    let erase = Change::Erase(Erase::new([30, 30, 30], [2, 2, 2]).unwrap());
    let id = ledger.commit(ME, None, vec![erase]).unwrap();
    ledger.step(&mut world, 1000);
    assert_eq!(
        state(&ledger, id),
        (TransactionInfo::FAILED, Status::MISSING)
    );
    assert!(world.announced.is_empty());
}

/// A prop of item 77 at `at` metres on every axis.
fn prop(at: f64) -> Prop {
    Prop::for_tests(77, [at; 3])
}

#[test]
fn a_placed_prop_is_listed_and_its_undo_removes_it() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    let id = ledger
        .commit(ME, Some(7), vec![Change::PlaceProp(prop(20.0))])
        .unwrap();
    run_out(&mut ledger, &mut world);
    assert_eq!(state(&ledger, id).0, TransactionInfo::APPLIED);
    assert_eq!(ledger.entities(ME, id), Some(vec![1]));
    assert_eq!(world.props.get(&1), Some(&prop(20.0)));
    assert_eq!(world.announced, [("prop placed", 7)]);

    ledger.undo(ME, id).unwrap();
    run_out(&mut ledger, &mut world);
    assert_eq!(state(&ledger, id).0, TransactionInfo::UNDONE);
    assert!(world.props.is_empty());
}

#[test]
fn an_undo_of_a_placed_prop_that_is_gone_is_refused() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    let id = ledger
        .commit(ME, None, vec![Change::PlaceProp(prop(20.0))])
        .unwrap();
    ledger.step(&mut world, 1000);
    world.props.clear();
    ledger.undo(ME, id).unwrap();
    ledger.step(&mut world, 1000);
    assert_eq!(
        state(&ledger, id),
        (TransactionInfo::UNDO_REFUSED, Status::REFUSED)
    );
}

#[test]
fn a_removed_prop_comes_back_on_undo_as_it_was() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    world.props.insert(40, prop(20.0));
    world.last_entity = 40;
    let id = ledger
        .commit(
            ME,
            None,
            vec![Change::RemoveProp {
                entity: 40,
                discard_state: false,
            }],
        )
        .unwrap();
    run_out(&mut ledger, &mut world);
    assert_eq!(state(&ledger, id).0, TransactionInfo::APPLIED);
    assert!(world.props.is_empty());
    assert_eq!(
        ledger.entities(ME, id),
        Some(Vec::new()),
        "a removal creates nothing"
    );

    ledger.undo(ME, id).unwrap();
    run_out(&mut ledger, &mut world);
    assert_eq!(state(&ledger, id).0, TransactionInfo::UNDONE);
    assert_eq!(world.props.values().collect::<Vec<_>>(), [&prop(20.0)]);
}

#[test]
fn a_removed_prop_whose_place_is_taken_is_not_brought_back() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    world.props.insert(40, prop(20.0));
    world.last_entity = 40;
    let id = ledger
        .commit(
            ME,
            None,
            vec![Change::RemoveProp {
                entity: 40,
                discard_state: false,
            }],
        )
        .unwrap();
    run_out(&mut ledger, &mut world);
    world.props.insert(41, prop(20.0));
    ledger.undo(ME, id).unwrap();
    run_out(&mut ledger, &mut world);
    assert_eq!(
        state(&ledger, id),
        (TransactionInfo::UNDO_REFUSED, Status::REFUSED)
    );
    assert_eq!(world.props.len(), 1);
}

#[test]
fn a_prop_holding_state_is_not_removed() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    world.props.insert(40, prop(20.0));
    world.stateful.insert(40, 0b101);
    let id = ledger
        .commit(
            ME,
            None,
            vec![
                dig(20.0),
                Change::RemoveProp {
                    entity: 40,
                    discard_state: false,
                },
            ],
        )
        .unwrap();
    ledger.step(&mut world, 1000);
    let info = ledger.info(ME, id).unwrap();
    assert_eq!(
        (info.state, info.reason, info.holds),
        (TransactionInfo::FAILED, Status::REFUSED, 0b101)
    );
    assert_eq!(world.edits, 0, "refused before the first write");
    assert_eq!(world.props.len(), 1);
}

#[test]
fn a_removal_that_discards_state_goes_ahead_and_its_undo_is_refused() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    world.props.insert(40, prop(20.0));
    world.stateful.insert(40, 0b1);
    let id = ledger
        .commit(
            ME,
            None,
            vec![Change::RemoveProp {
                entity: 40,
                discard_state: true,
            }],
        )
        .unwrap();
    run_out(&mut ledger, &mut world);
    assert_eq!(state(&ledger, id).0, TransactionInfo::APPLIED);
    assert!(world.props.is_empty());

    ledger.undo(ME, id).unwrap();
    run_out(&mut ledger, &mut world);
    let info = ledger.info(ME, id).unwrap();
    assert_eq!(
        (info.state, info.reason, info.holds),
        (TransactionInfo::UNDO_REFUSED, Status::REFUSED, 0b1),
        "a respawn would come back without what it held"
    );
    assert!(world.props.is_empty());
}

#[test]
fn an_undo_never_destroys_a_placed_prop_that_has_gained_state() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    let id = ledger
        .commit(ME, None, vec![Change::PlaceProp(prop(20.0))])
        .unwrap();
    ledger.step(&mut world, 1000);
    world.stateful.insert(1, 0b10);
    ledger.undo(ME, id).unwrap();
    ledger.step(&mut world, 1000);
    let info = ledger.info(ME, id).unwrap();
    assert_eq!(
        (info.state, info.reason, info.holds),
        (TransactionInfo::UNDO_REFUSED, Status::REFUSED, 0b10)
    );
    assert_eq!(world.props.len(), 1);

    world.stateful.remove(&1);
    ledger.undo(ME, id).unwrap();
    run_out(&mut ledger, &mut world);
    assert_eq!(
        state(&ledger, id).0,
        TransactionInfo::UNDONE,
        "once emptied, it goes"
    );
}

#[test]
fn a_prop_and_bricks_in_one_transaction_undo_newest_first() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    let id = ledger
        .commit(
            ME,
            None,
            vec![dig(20.0), Change::PlaceProp(prop(20.0)), dig(22.0)],
        )
        .unwrap();
    run_out(&mut ledger, &mut world);
    assert_eq!(state(&ledger, id).0, TransactionInfo::APPLIED);
    assert_eq!(ledger.info(ME, id).unwrap().applied, 3);
    ledger.undo(ME, id).unwrap();
    run_out(&mut ledger, &mut world);
    assert_eq!(state(&ledger, id).0, TransactionInfo::UNDONE);
    assert!(world.props.is_empty());
    assert_eq!(world.snapshot(), Vec::new());
}

#[test]
fn an_actor_is_asked_about_a_prop_s_box() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    ledger
        .commit(ME, Some(7), vec![Change::PlaceProp(prop(20.0))])
        .unwrap();
    ledger.step(&mut world, 1000);
    let (min, max) = world.asked[0];
    assert!(min.iter().all(|&m| m < 20.0) && max.iter().all(|&m| m > 20.0));
}

#[test]
fn every_kind_of_state_stops_a_removal_and_an_undo() {
    for kind in [
        TransactionInfo::HOLDS_INVENTORY,
        TransactionInfo::HOLDS_GROWTH,
        TransactionInfo::HOLDS_FACTORY_STATION,
        TransactionInfo::HOLDS_NPC_BED,
        TransactionInfo::HOLDS_STORE_ASSOCIATED_NPC_IDS,
        TransactionInfo::HOLDS_CUSTOM_PLAYER_STRING,
        TransactionInfo::HOLDS_FUEL,
        TransactionInfo::HOLDS_LOGIC_GRAPH_NODE_CONNECTED,
        TransactionInfo::HOLDS_LOGIC_GRAPH_TRIGGER,
    ] {
        let mut world = World::new();
        let mut ledger = Ledger::new(LIMITS);
        world.props.insert(40, prop(20.0));
        world.stateful.insert(40, kind);
        let removal = ledger
            .commit(
                ME,
                None,
                vec![Change::RemoveProp {
                    entity: 40,
                    discard_state: false,
                }],
            )
            .unwrap();
        ledger.step(&mut world, 1000);
        let info = ledger.info(ME, removal).unwrap();
        assert_eq!((info.state, info.holds), (TransactionInfo::FAILED, kind));
        assert_eq!(world.props.len(), 1, "{kind:#x}: the prop stays");

        let placed = ledger
            .commit(ME, None, vec![Change::PlaceProp(prop(30.0))])
            .unwrap();
        ledger.step(&mut world, 1000);
        let entity = ledger.entities(ME, placed).unwrap()[0];
        world.stateful.insert(entity, kind);
        ledger.undo(ME, placed).unwrap();
        ledger.step(&mut world, 1000);
        let info = ledger.info(ME, placed).unwrap();
        assert_eq!(
            (info.state, info.holds),
            (TransactionInfo::UNDO_REFUSED, kind)
        );
    }
}

/// A world whose props of item 77 are logic triggers, landing a step
/// after their spawn.
fn trigger_world() -> World {
    let mut world = World::new();
    world
        .trigger_items
        .insert(77, TransactionInfo::HOLDS_LOGIC_GRAPH_TRIGGER);
    world.spawns_wait = true;
    world
}

#[test]
fn a_placed_trigger_left_as_it_was_is_taken_away_by_undo() {
    let mut world = trigger_world();
    let mut ledger = Ledger::new(LIMITS);
    let id = ledger
        .commit(ME, None, vec![Change::PlaceProp(prop(20.0))])
        .unwrap();
    ledger.step(&mut world, 1000);
    assert_eq!(
        state(&ledger, id).0,
        TransactionInfo::PENDING,
        "it waits for the prop to stand"
    );
    world.land();
    ledger.step(&mut world, 1000);
    assert_eq!(state(&ledger, id).0, TransactionInfo::APPLIED);

    ledger.undo(ME, id).unwrap();
    run_out(&mut ledger, &mut world);
    assert_eq!(state(&ledger, id).0, TransactionInfo::UNDONE);
    assert!(world.props.is_empty());
}

#[test]
fn a_placed_trigger_whose_settings_changed_stays() {
    let mut world = trigger_world();
    let mut ledger = Ledger::new(LIMITS);
    let id = ledger
        .commit(ME, None, vec![Change::PlaceProp(prop(20.0))])
        .unwrap();
    ledger.step(&mut world, 1000);
    world.land();
    ledger.step(&mut world, 1000);
    let entity = ledger.entities(ME, id).unwrap()[0];
    world.triggers.insert(entity, vec![9, 9, 9]);

    ledger.undo(ME, id).unwrap();
    ledger.step(&mut world, 1000);
    let info = ledger.info(ME, id).unwrap();
    assert_eq!(
        (info.state, info.holds),
        (
            TransactionInfo::UNDO_REFUSED,
            TransactionInfo::HOLDS_LOGIC_GRAPH_TRIGGER
        )
    );
    assert_eq!(world.props.len(), 1);
}

#[test]
fn a_placed_trigger_never_read_is_kept_by_undo() {
    let mut world = trigger_world();
    let mut ledger = Ledger::new(LIMITS);
    let id = ledger
        .commit(ME, None, vec![Change::PlaceProp(prop(20.0))])
        .unwrap();
    for _ in 0..LIMITS.waits {
        ledger.step(&mut world, 1000);
    }
    assert_eq!(
        state(&ledger, id).0,
        TransactionInfo::APPLIED,
        "it stops waiting at the limit"
    );
    world.land();
    ledger.undo(ME, id).unwrap();
    ledger.step(&mut world, 1000);
    let info = ledger.info(ME, id).unwrap();
    assert_eq!(
        (info.state, info.holds),
        (
            TransactionInfo::UNDO_REFUSED,
            TransactionInfo::HOLDS_LOGIC_GRAPH_TRIGGER
        ),
        "without its first settings, it cannot tell a player's"
    );
}

#[test]
fn a_trigger_the_transaction_did_not_place_is_not_removed() {
    let mut world = trigger_world();
    let mut ledger = Ledger::new(LIMITS);
    world.props.insert(40, prop(20.0));
    world.triggers.insert(40, vec![1, 2, 3]);
    world
        .kinds
        .insert(40, TransactionInfo::HOLDS_LOGIC_GRAPH_TRIGGER);
    let id = ledger
        .commit(
            ME,
            None,
            vec![Change::RemoveProp {
                entity: 40,
                discard_state: false,
            }],
        )
        .unwrap();
    ledger.step(&mut world, 1000);
    let info = ledger.info(ME, id).unwrap();
    assert_eq!(
        (info.state, info.holds),
        (
            TransactionInfo::FAILED,
            TransactionInfo::HOLDS_LOGIC_GRAPH_TRIGGER
        )
    );
    assert_eq!(world.props.len(), 1);
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

fn remove(entity: u32, discard_state: bool) -> Change {
    Change::RemoveProp {
        entity,
        discard_state,
    }
}

#[test]
fn a_removal_holds_the_prop_a_step_before_it_destroys_it() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    world.props.insert(40, prop(20.0));
    let id = ledger.commit(ME, None, vec![remove(40, false)]).unwrap();
    ledger.step(&mut world, 1000);
    assert_eq!(state(&ledger, id).0, TransactionInfo::PENDING);
    assert!(world.held.contains(&40), "held against players");
    assert_eq!(world.props.len(), 1, "not destroyed yet");
    ledger.step(&mut world, 1000);
    assert_eq!(state(&ledger, id).0, TransactionInfo::APPLIED);
    assert!(world.props.is_empty());
}

#[test]
fn items_that_land_while_a_chest_is_held_stop_its_removal() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    world.props.insert(40, prop(20.0));
    let id = ledger.commit(ME, None, vec![remove(40, false)]).unwrap();
    ledger.step(&mut world, 1000);
    world.stateful.insert(40, TransactionInfo::HOLDS_INVENTORY);
    ledger.step(&mut world, 1000);
    let info = ledger.info(ME, id).unwrap();
    assert_eq!(
        (info.state, info.reason, info.holds),
        (
            TransactionInfo::FAILED,
            Status::REFUSED,
            TransactionInfo::HOLDS_INVENTORY
        )
    );
    assert_eq!(world.props.len(), 1, "the chest stands");
    assert!(world.held.is_empty(), "and players reach it again");
}

#[test]
fn an_undo_holds_a_placed_chest_and_keeps_it_if_items_land() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    let id = ledger
        .commit(ME, None, vec![Change::PlaceProp(prop(20.0))])
        .unwrap();
    settle(&mut ledger, &mut world, id);
    let entity = ledger.entities(ME, id).unwrap()[0];
    ledger.undo(ME, id).unwrap();
    ledger.step(&mut world, 1000);
    assert_eq!(state(&ledger, id).0, TransactionInfo::UNDOING);
    assert!(world.held.contains(&entity));
    world
        .stateful
        .insert(entity, TransactionInfo::HOLDS_INVENTORY);
    ledger.step(&mut world, 1000);
    let info = ledger.info(ME, id).unwrap();
    assert_eq!(
        (info.state, info.holds),
        (
            TransactionInfo::UNDO_REFUSED,
            TransactionInfo::HOLDS_INVENTORY
        )
    );
    assert_eq!(world.props.len(), 1);
    assert!(world.held.is_empty());
}

#[test]
fn the_state_that_stopped_an_undo_clears_when_one_is_asked_again() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    let id = ledger
        .commit(ME, None, vec![Change::PlaceProp(prop(20.0))])
        .unwrap();
    settle(&mut ledger, &mut world, id);
    let entity = ledger.entities(ME, id).unwrap()[0];
    world.stateful.insert(entity, TransactionInfo::HOLDS_GROWTH);
    ledger.undo(ME, id).unwrap();
    settle(&mut ledger, &mut world, id);
    assert_eq!(
        ledger.info(ME, id).unwrap().holds,
        TransactionInfo::HOLDS_GROWTH
    );

    world.stateful.remove(&entity);
    ledger.undo(ME, id).unwrap();
    assert_eq!(ledger.info(ME, id).unwrap().holds, 0, "cleared when queued");
    settle(&mut ledger, &mut world, id);
    let info = ledger.info(ME, id).unwrap();
    assert_eq!((info.state, info.holds), (TransactionInfo::UNDONE, 0));
}

#[test]
fn a_creature_is_never_removed_even_discarding_state() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    world.props.insert(40, prop(20.0));
    world.creatures.insert(40);
    let id = ledger.commit(ME, None, vec![remove(40, true)]).unwrap();
    settle(&mut ledger, &mut world, id);
    assert_eq!(
        state(&ledger, id),
        (TransactionInfo::FAILED, Status::NOT_A_PROP)
    );
    assert_eq!(world.props.len(), 1);
}

#[test]
fn state_reads_this_build_lacks_turn_removal_and_undo_off() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    world.props.insert(40, prop(20.0));
    let placed = ledger
        .commit(ME, None, vec![Change::PlaceProp(prop(30.0))])
        .unwrap();
    settle(&mut ledger, &mut world, placed);
    world.facts_missing = true;
    let removal = ledger.commit(ME, None, vec![remove(40, true)]).unwrap();
    settle(&mut ledger, &mut world, removal);
    assert_eq!(
        state(&ledger, removal),
        (TransactionInfo::FAILED, Status::MISSING)
    );
    ledger.undo(ME, placed).unwrap();
    settle(&mut ledger, &mut world, placed);
    assert_eq!(
        state(&ledger, placed),
        (TransactionInfo::UNDO_REFUSED, Status::MISSING)
    );
    assert_eq!(world.props.len(), 2);
}

/// Steps until no work is left, at most ten steps.
fn run_out(ledger: &mut Ledger, world: &mut World) {
    for _ in 0..10 {
        ledger.step(world, 1000);
    }
}

/// A world whose props of item 77 are plants, landing a step after their
/// spawn.
fn crop_world() -> World {
    let mut world = World::new();
    world
        .trigger_items
        .insert(77, TransactionInfo::HOLDS_GROWTH);
    world.spawns_wait = true;
    world
}

#[test]
fn a_placed_crop_that_has_not_grown_is_taken_away_by_undo() {
    let mut world = crop_world();
    let mut ledger = Ledger::new(LIMITS);
    let id = ledger
        .commit(ME, None, vec![Change::PlaceProp(prop(20.0))])
        .unwrap();
    ledger.step(&mut world, 1000);
    world.land();
    settle(&mut ledger, &mut world, id);
    assert_eq!(state(&ledger, id).0, TransactionInfo::APPLIED);
    ledger.undo(ME, id).unwrap();
    settle(&mut ledger, &mut world, id);
    assert_eq!(state(&ledger, id).0, TransactionInfo::UNDONE);
}

#[test]
fn a_placed_crop_that_has_grown_stays() {
    let mut world = crop_world();
    let mut ledger = Ledger::new(LIMITS);
    let id = ledger
        .commit(ME, None, vec![Change::PlaceProp(prop(20.0))])
        .unwrap();
    ledger.step(&mut world, 1000);
    world.land();
    settle(&mut ledger, &mut world, id);
    let entity = ledger.entities(ME, id).unwrap()[0];
    world.triggers.insert(entity, vec![4]);
    ledger.undo(ME, id).unwrap();
    settle(&mut ledger, &mut world, id);
    let info = ledger.info(ME, id).unwrap();
    assert_eq!(
        (info.state, info.holds),
        (TransactionInfo::UNDO_REFUSED, TransactionInfo::HOLDS_GROWTH)
    );
}

#[test]
fn a_filter_gone_off_during_the_hold_stops_the_destroy() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    world.props.insert(40, prop(20.0));
    let id = ledger.commit(ME, None, vec![remove(40, false)]).unwrap();
    ledger.step(&mut world, 1000);
    world.filter_off = true;
    ledger.step(&mut world, 1000);
    assert_eq!(
        state(&ledger, id),
        (TransactionInfo::FAILED, Status::MISSING)
    );
    assert_eq!(world.props.len(), 1, "nothing goes unguarded");
    assert!(world.held.is_empty());
}

#[test]
fn a_filter_gone_off_during_an_undo_s_hold_keeps_the_prop() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    let id = ledger
        .commit(ME, None, vec![Change::PlaceProp(prop(20.0))])
        .unwrap();
    settle(&mut ledger, &mut world, id);
    ledger.undo(ME, id).unwrap();
    ledger.step(&mut world, 1000);
    world.filter_off = true;
    ledger.step(&mut world, 1000);
    assert_eq!(
        state(&ledger, id),
        (TransactionInfo::UNDO_REFUSED, Status::MISSING)
    );
    assert_eq!(world.props.len(), 1);
    assert!(world.held.is_empty());
}
