use super::*;

fn roll(kind: Kinds) -> Roll {
    Roll {
        kind,
        entity: Some(Entity(5)),
        node: 0,
        labels: 0,
        boss: false,
        locked: false,
    }
}

#[test]
fn filters_keep_their_kind_and_markers() {
    let corpse = roll(Kinds::CORPSES);
    let boss = Roll {
        boss: true,
        labels: 3,
        ..corpse
    };
    let chest = Roll {
        node: 9,
        ..roll(Kinds::CHESTS)
    };
    let locked = Roll {
        locked: true,
        ..chest
    };
    assert!(Source::kinds(Kinds::CONTAINERS).matches(&corpse));
    assert!(Source::kinds(Kinds::CONTAINERS).matches(&chest));
    assert!(!Source::kinds(Kinds::CONTAINERS).matches(&roll(Kinds::DESTROYED)));
    assert!(Source::bosses().matches(&boss) && !Source::bosses().matches(&corpse));
    assert!(!Source::bosses().matches(&locked));
    assert!(Source::locked_chests().matches(&locked));
    assert!(!Source::locked_chests().matches(&chest));
    assert!(Source::bosses().with_labels(&[3]).matches(&boss));
    assert!(!Source::bosses().with_labels(&[4]).matches(&boss));
    assert!(
        Source::kinds(Kinds::CHESTS)
            .with_nodes(&[9])
            .matches(&chest)
    );
    assert!(
        !Source::kinds(Kinds::CHESTS)
            .with_nodes(&[8])
            .matches(&chest)
    );
}

#[test]
fn a_single_roll_slot_holds_the_item_and_count_at_their_offsets() {
    let slot = Slot::new(0, 4, 16).unwrap();
    let (bytes, len) = slot.fill(0x0102_0304, 7);
    assert_eq!(len, 16);
    assert_eq!(bytes[..6], [4, 3, 2, 1, 7, 0]);
    assert!(bytes[6..].iter().all(|&b| b == 0));
    let moved = Slot::new(8, 2, 12).unwrap();
    let (bytes, len) = moved.fill(9, 3);
    assert_eq!((len, bytes[8], bytes[2]), (12, 9, 3));
}

#[test]
fn a_single_roll_slot_that_cannot_hold_its_fields_is_refused() {
    assert!(Slot::new(0, 4, 5).is_none(), "count past the end");
    assert!(Slot::new(14, 4, 16).is_none(), "item past the end");
    assert!(
        Slot::new(0, 4, SLOT_MAX + 1).is_none(),
        "larger than the buffer"
    );
    assert!(Slot::new(usize::MAX, 4, 16).is_none(), "overflow");
    assert!(Slot::new(0, 2, 16).is_none(), "count inside the item id");
    assert!(Slot::new(3, 2, 16).is_none(), "item id over the count");
    assert!(
        Slot::new(4, 2, 16).is_some(),
        "count just before the item id"
    );
}

#[test]
fn invalid_drops_are_refused() {
    let from = Source::bosses();
    assert!(Drop::new(ItemId::new(7), 0.5, from.clone()).check().is_ok());
    assert!(
        Drop::new(ItemId::new(0), 0.5, from.clone())
            .check()
            .is_err()
    );
    assert!(
        Drop::new(ItemId::new(7), 1.5, from.clone())
            .check()
            .is_err()
    );
    assert!(
        Drop::new(ItemId::new(7), f64::NAN, from.clone())
            .check()
            .is_err()
    );
    assert!(
        Drop::new(ItemId::new(7), 0.5, from.clone())
            .count(0, 1)
            .check()
            .is_err()
    );
    assert!(
        Drop::new(ItemId::new(7), 0.5, from.clone())
            .count(3, 2)
            .check()
            .is_err()
    );
    assert!(
        Drop::new(ItemId::new(7), 0.5, from.clone())
            .meter(1, 5, 5)
            .check()
            .is_ok()
    );
    assert!(
        Drop::new(ItemId::new(7), 0.5, from.clone())
            .meter(3, 2, 5)
            .check()
            .is_err()
    );
    assert!(
        Drop::new(ItemId::new(7), 0.5, from.clone())
            .meter(1, 6, 5)
            .check()
            .is_err()
    );
    assert!(
        Drop::new(ItemId::new(7), 0.5, from)
            .meter(0, 0, 0)
            .check()
            .is_err()
    );
}

#[test]
fn meters_roll_every_count_between_their_bounds() {
    let dice = Dice(AtomicU64::new(3));
    let drop = Drop::new(ItemId::new(7), 1.0, Source::bosses()).meter(2, 4, 5);
    let mut seen = [0usize; 6];
    for _ in 0..30_000 {
        let (_, _, fill) = drop.draw(&dice).unwrap();
        let charges = (5.0 * fill.unwrap().fraction() + 0.5).floor();
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "0 to 5"
        )]
        let charges = charges as usize;
        seen[charges] += 1;
    }
    assert_eq!((seen[0], seen[1], seen[5]), (0, 0, 0));
    assert!(
        seen[2..5].iter().all(|&n| (9_000..11_000).contains(&n)),
        "{seen:?}"
    );
}

#[test]
fn the_chance_holds_over_many_rolls() {
    let dice = Dice(AtomicU64::new(42));
    let drop = Drop::new(ItemId::new(7), 0.1, Source::bosses()).count(2, 4);
    let mut hits = 0;
    for _ in 0..100_000 {
        if let Some((item, n, fill)) = drop.draw(&dice) {
            assert_eq!((item, fill), (7, None));
            assert!((2..=4).contains(&n));
            hits += 1;
        }
    }
    assert!((9_000..11_000).contains(&hits), "{hits}");
    let never = Drop::new(ItemId::new(7), 0.0, Source::bosses());
    let always = Drop::new(ItemId::new(7), 1.0, Source::bosses());
    for _ in 0..1000 {
        assert_eq!(never.draw(&dice), None);
        assert_eq!(always.draw(&dice), Some((7, 1, None)));
    }
}

#[test]
fn one_of_spreads_over_its_items() {
    let dice = Dice(AtomicU64::new(7));
    let drop = Drop::one_of(
        &[ItemId::new(1), ItemId::new(2), ItemId::new(3)],
        1.0,
        Source::bosses(),
    );
    let mut seen = [0usize; 3];
    for _ in 0..30_000 {
        let (item, _, _) = drop.draw(&dice).unwrap();
        seen[item as usize - 1] += 1;
    }
    assert!(
        seen.iter().all(|&n| (9_000..11_000).contains(&n)),
        "{seen:?}"
    );
    assert!(Drop::one_of(&[], 0.5, Source::bosses()).check().is_err());
    assert!(
        Drop::one_of(&[ItemId::new(1), ItemId::new(0)], 0.5, Source::bosses())
            .check()
            .is_err()
    );
}
