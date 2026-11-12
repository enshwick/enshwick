//! Pastes, erases and blocks, what they announce, and the rights asked for an actor.

use super::*;

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
