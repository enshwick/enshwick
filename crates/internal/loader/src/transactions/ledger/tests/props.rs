//! Props placed and removed, their undo, and the state that stops either.

use super::*;

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
