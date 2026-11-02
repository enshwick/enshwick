//! Terrain edits over steps and loaded columns, and their undo brick by brick.

use super::*;

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
