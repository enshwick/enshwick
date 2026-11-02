//! Props held against players for a step before their destroy, and what stops it.

use super::*;

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
