use enshwick::abi::{Header, Status, TerrainEdit, TransactionInfo};
use rustc_hash::{FxHashMap, FxHashSet};

use super::{BRICK_VOXELS, Brick, Image, Ledger, Limits, Voxels};
use crate::mod_index::ModIndex;
use crate::transactions::cover;
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
}

impl World {
    /// A world of 40 by 40 pages: 10,240 voxels, 5,120 metres a side.
    fn new() -> Self {
        Self {
            pages: 40,
            ..Self::default()
        }
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

    fn restore(&mut self, brick: Brick, image: &[u16; BRICK_VOXELS]) -> Result<(), Status> {
        if let Some(reason) = self.restore_fails.take() {
            return Err(reason);
        }
        self.restores += 1;
        self.bricks.insert(brick, Box::new(*image));
        Ok(())
    }
}

/// A round dig of radius 4 at `at` metres on every axis: 64 bricks.
fn dig(at: f32) -> TerrainEdit {
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
    world.edit(&dig(30.0)).unwrap();
    let before = world.snapshot();

    let id = ledger.commit(ME, &[dig(20.0), dig(22.0)]).unwrap();
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
        .commit(ME, &[dig(20.0), dig(40.0), dig(60.0)])
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
    let id = ledger.commit(ME, &[dig(20.0), dig(40.0)]).unwrap();

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
    let id = ledger.commit(ME, &[dig(20.0), dig(40.0)]).unwrap();
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
        .commit(ME, &[dig(20.0), dig(40.0), dig(60.0)])
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
    let id = ledger.commit(ME, &[dig(20.0)]).unwrap();
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
    let first = ledger.commit(ME, &[dig(20.0)]).unwrap();
    let second = ledger.commit(ME, &[dig(22.0)]).unwrap();
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
    let id = ledger.commit(ME, &[dig(20.0)]).unwrap();
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
    let id = ledger.commit(ME, &[dig(20.0)]).unwrap();
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
            let id = ledger.commit(ME, &[dig(at)]).unwrap();
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
    assert_eq!(ledger.commit(ME, &[]), Err(Status::BAD_ARGS));
    assert_eq!(ledger.commit(ME, &[dig(20.0); 9]), Err(Status::BAD_ARGS));
    assert_eq!(
        ledger.commit(ME, &[dig(20.0); 5]),
        Err(Status::BAD_ARGS),
        "320 bricks"
    );
    let dry = TerrainEdit {
        flags: TerrainEdit::DRY_RUN,
        ..dig(20.0)
    };
    assert_eq!(ledger.commit(ME, &[dry]), Err(Status::BAD_ARGS));

    ledger.commit(ME, &[dig(20.0); 3]).unwrap();
    assert_eq!(
        ledger.commit(ME, &[dig(20.0); 2]),
        Err(Status::REFUSED),
        "the queue claims 192 of 256 bricks"
    );
    ledger.commit(ME, &[dig(20.0)]).unwrap();
}

#[test]
fn the_oldest_settled_transaction_is_forgotten_past_the_record_cap() {
    let mut world = World::new();
    let mut ledger = Ledger::new(Limits {
        records: 2,
        ..LIMITS
    });
    let first = ledger.commit(ME, &[dig(20.0)]).unwrap();
    let second = ledger.commit(ME, &[dig(40.0)]).unwrap();
    assert_eq!(
        ledger.commit(ME, &[dig(60.0)]),
        Err(Status::REFUSED),
        "both work"
    );
    ledger.step(&mut world, 1000);
    let third = ledger.commit(ME, &[dig(60.0)]).unwrap();
    assert_eq!(ledger.info(ME, first), None);
    assert!(ledger.info(ME, second).is_some());
    assert!(ledger.info(ME, third).is_some());
    assert_eq!(ledger.undo(ME, first), Err(Status::REFUSED));
}

#[test]
fn a_mod_sees_and_undoes_only_its_own_transactions() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    let id = ledger.commit(ME, &[dig(20.0)]).unwrap();
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
    let done = ledger.commit(ME, &[dig(20.0)]).unwrap();
    ledger.step(&mut world, 1000);
    let queued = ledger.commit(ME, &[dig(40.0)]).unwrap();
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
fn past_the_edge() -> TerrainEdit {
    TerrainEdit {
        position: [5119.0, 20.0, 20.0],
        ..dig(20.0)
    }
}

#[test]
fn a_transaction_leaving_the_world_fails_before_writing_anything() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    let id = ledger.commit(ME, &[dig(20.0), past_the_edge()]).unwrap();
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
    let first = ledger.commit(ME, &[dig(20.0)]).unwrap();
    ledger.step(&mut world, 1000);
    assert_eq!(state(&ledger, first).0, TransactionInfo::APPLIED);
    assert_eq!(
        ledger.commit(ME, &[dig(20.0), past_the_edge()]),
        Err(Status::OUTSIDE_WORLD)
    );
    let high = TerrainEdit {
        position: [20.0, 1023.0, 20.0],
        ..dig(20.0)
    };
    assert_eq!(ledger.commit(ME, &[high]), Err(Status::OUTSIDE_WORLD));
    ledger.forget();
    assert!(
        ledger.commit(ME, &[past_the_edge()]).is_ok(),
        "a new world's extent is not known until its first step"
    );
}

#[test]
fn a_brick_changed_between_undo_steps_stops_the_undo_before_it() {
    let mut world = World::new();
    let mut ledger = Ledger::new(LIMITS);
    let id = ledger.commit(ME, &[dig(20.0)]).unwrap();
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
    let id = ledger.commit(ME, &[dig(20.0), dig(40.0)]).unwrap();
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
        .commit(ME, &[dig(20.0), dig(40.0), dig(60.0), dig(80.0)])
        .unwrap();
    ledger.step(&mut world, 1000);
    let second = ledger
        .commit(ME, &[dig(100.0), dig(120.0), dig(140.0), dig(160.0)])
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
    let id = ledger.commit(ME, &[dig(20.0)]).unwrap();
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
    let id = ledger.commit(ME, &[dig(20.0)]).unwrap();
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
    let id = ledger.commit(ME, &[dig(20.0), dig(40.0)]).unwrap();
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
    let id = ledger.commit(ME, &[dig(20.0)]).unwrap();
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
