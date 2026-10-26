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
fn invalid_drops_are_refused() {
    let from = Source::bosses();
    assert!(Drop::new(7, 0.5, from.clone()).check().is_ok());
    assert!(Drop::new(0, 0.5, from.clone()).check().is_err());
    assert!(Drop::new(7, 1.5, from.clone()).check().is_err());
    assert!(Drop::new(7, f64::NAN, from.clone()).check().is_err());
    assert!(Drop::new(7, 0.5, from.clone()).count(0, 1).check().is_err());
    assert!(Drop::new(7, 0.5, from.clone()).count(3, 2).check().is_err());
    assert!(
        Drop::new(7, 0.5, from.clone())
            .meter(1, 5, 5)
            .check()
            .is_ok()
    );
    assert!(
        Drop::new(7, 0.5, from.clone())
            .meter(3, 2, 5)
            .check()
            .is_err()
    );
    assert!(
        Drop::new(7, 0.5, from.clone())
            .meter(1, 6, 5)
            .check()
            .is_err()
    );
    assert!(Drop::new(7, 0.5, from).meter(0, 0, 0).check().is_err());
}

#[test]
fn meters_roll_every_count_between_their_bounds() {
    let dice = Dice(AtomicU64::new(3));
    let drop = Drop::new(7, 1.0, Source::bosses()).meter(2, 4, 5);
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
    let drop = Drop::new(7, 0.1, Source::bosses()).count(2, 4);
    let mut hits = 0;
    for _ in 0..100_000 {
        if let Some((item, n, fill)) = drop.draw(&dice) {
            assert_eq!((item, fill), (7, None));
            assert!((2..=4).contains(&n));
            hits += 1;
        }
    }
    assert!((9_000..11_000).contains(&hits), "{hits}");
    let never = Drop::new(7, 0.0, Source::bosses());
    let always = Drop::new(7, 1.0, Source::bosses());
    for _ in 0..1000 {
        assert_eq!(never.draw(&dice), None);
        assert_eq!(always.draw(&dice), Some((7, 1, None)));
    }
}

#[test]
fn one_of_spreads_over_its_items() {
    let dice = Dice(AtomicU64::new(7));
    let drop = Drop::one_of(&[1, 2, 3], 1.0, Source::bosses());
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
        Drop::one_of(&[1, 0], 0.5, Source::bosses())
            .check()
            .is_err()
    );
}
