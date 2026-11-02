//! The ledger's caps, each mod's own transactions, a world's end and work in hand.

use super::*;

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
