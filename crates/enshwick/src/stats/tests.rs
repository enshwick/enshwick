use super::*;

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-5
}

fn def() -> StatDef {
    StatDef::additive("t").range(-1000.0, 1000.0)
}

#[test]
fn operations_apply_in_their_order_whatever_the_input_order() {
    let forward = [(Op::Add, 10.0), (Op::Multiply, 2.0), (Op::Add, 5.0)];
    let backward = [(Op::Add, 5.0), (Op::Multiply, 2.0), (Op::Add, 10.0)];
    assert!(close(aggregate(&def(), forward), 30.0));
    assert!(close(aggregate(&def(), backward), 30.0));
}

#[test]
fn the_greatest_max_is_a_floor_and_the_last_override_wins() {
    let mods = [(Op::Add, 5.0), (Op::Max, 20.0), (Op::Max, 12.0)];
    assert!(close(aggregate(&def(), mods), 20.0));
    let mods = [(Op::Override, 1.0), (Op::Add, 500.0), (Op::Override, 7.0)];
    assert!(close(aggregate(&def(), mods), 7.0));
}

#[test]
fn the_range_holds_and_bad_values_are_skipped() {
    assert!(close(aggregate(&def(), [(Op::Add, 1e9)]), 1000.0));
    assert!(close(
        aggregate(&def(), [(Op::Add, f32::NAN), (Op::Add, 2.0)]),
        2.0
    ));
    assert!(close(
        aggregate(&def(), [(Op::Multiply, f32::INFINITY)]),
        0.0
    ));
    let factor = StatDef::multiplicative("f").range(0.0, 10.0);
    assert!(close(
        aggregate(&factor, [(Op::Multiply, 1.5), (Op::Multiply, 2.0)]),
        3.0
    ));
    assert!(close(aggregate(&factor, [(Op::Add, -5.0)]), 0.0));
}

#[test]
fn flags_read_as_zero_or_one() {
    let flag = StatDef::flag("f");
    assert!(close(aggregate(&flag, []), 0.0));
    assert!(close(
        aggregate(&flag, [(Op::Add, 1.0), (Op::Add, 1.0)]),
        1.0
    ));
    assert!(close(
        aggregate(&flag, [(Op::Add, 1.0), (Op::Override, 0.0)]),
        0.0
    ));
}

#[test]
fn definitions_out_of_their_own_range_are_refused() {
    assert!(StatDef::additive("x").range(1.0, 2.0).check().is_err());
    assert!(StatDef::additive("x").range(2.0, 1.0).check().is_err());
    assert!(StatDef::multiplicative("x").check().is_ok());
    assert!(
        StatDef::additive("x")
            .starting_at(f32::NAN)
            .check()
            .is_err()
    );
}

#[test]
fn built_in_kinds_are_valid_and_their_keys_differ() {
    let mut keys: Vec<_> = builtin::ALL.iter().map(|(id, _)| key_of(id)).collect();
    for (id, def) in builtin::ALL {
        assert!(def.to_def().check().is_ok(), "{id}");
        assert!(id.starts_with("enshwick:"));
    }
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), builtin::ALL.len());
}

#[test]
fn item_stats_round_trip_through_postcard() {
    let stats = ItemStats {
        modifiers: vec![
            ItemModifier {
                stat: key_of("a:b"),
                op: Op::Multiply,
                value: 1.25,
            },
            ItemModifier {
                stat: key_of("a:c"),
                op: Op::Override,
                value: -3.0,
            },
        ],
        ..ItemStats::default()
    };
    let bytes = postcard::to_allocvec(&stats).unwrap();
    assert!(bytes.len() < 32);
    assert_eq!(postcard::from_bytes::<ItemStats>(&bytes).unwrap(), stats);
}

fn deliver<F: Fn(&StatChange)>(change: &StatChangeData, handler: &F) {
    // SAFETY: `change` is a whole `StatChange` borrowed for the call, and
    // `user` points at `handler`, an `F` alive for the call and only read
    // through shared references.
    unsafe { run_changed::<F>(change, core::ptr::from_ref(handler).cast_mut().cast()) };
}

#[test]
fn a_change_shorter_than_its_first_version_reaches_no_handler() {
    let seen = core::cell::RefCell::new(Vec::new());
    let handler = |change: &StatChange| seen.borrow_mut().push((change.entity, change.new));
    let mut change = StatChangeData {
        header: Header::of::<StatChangeData>(),
        entity: Entity(2),
        stat: StatRef { index: 0, key: 1 },
        old: 0.0,
        new: 50.0,
        reserved: 0,
    };
    deliver(&change, &handler);
    change.header.size = 12;
    deliver(&change, &handler);
    assert_eq!(*seen.borrow(), [(Entity(2), 50.0)]);
}
