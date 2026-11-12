//! Placed logic triggers and plants, whose start state a player or time may change.

use super::*;

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
