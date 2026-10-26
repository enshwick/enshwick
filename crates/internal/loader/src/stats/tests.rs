use super::combat::combat;
use super::*;

#[test]
fn a_stopped_game_leaves_no_entities_or_values_behind() {
    with_state(|state| {
        state.watched.entry(7).or_default().asked = true;
    });
    let mut values = Snapshot::default();
    values.values.insert(7, Box::new([2.0]));
    snapshot().store(Arc::new(values));
    forget();
    assert!(STATE.relock().is_none());
    assert!(snapshot().load().values.is_empty());
}

fn sealed(kinds: &[(&str, StatDef)]) -> Sealed {
    let defs: Box<[StatDef]> = kinds.iter().map(|(_, d)| d.clone()).collect();
    Sealed {
        by_key: kinds
            .iter()
            .enumerate()
            .map(|(i, (id, _))| (key_of(id).0, u32::try_from(i).unwrap()))
            .collect(),
        defaults: defs.iter().map(|d| d.default).collect(),
        defs,
        buffs: FxHashMap::default(),
        handlers: Box::new([]),
        damage: 0,
        resistance: 1,
        maxima: [u32::MAX; 3],
        by_type: {
            let mut t = [None; TYPES];
            t[3] = Some((2, 3));
            t
        },
        affixes: Box::new([]),
        kinds: kinds
            .iter()
            .map(|(id, _)| Kind {
                id: (*id).into(),
                def: None,
            })
            .collect(),
    }
}

fn combat_kinds() -> Sealed {
    sealed(&[
        (
            "enshwick:damage",
            StatDef::multiplicative("d").range(0.0, 100.0),
        ),
        (
            "enshwick:resistance",
            StatDef::additive("r").range(-1.0, 1.0),
        ),
        (
            "enshwick:damage_fire",
            StatDef::multiplicative("df").range(0.0, 100.0),
        ),
        (
            "enshwick:resistance_fire",
            StatDef::additive("rf").range(-1.0, 1.0),
        ),
    ])
}

#[test]
fn ids_take_the_mod_s_namespace_and_say_whose_they_are() {
    assert_eq!(full_id(&["rpg"], "crit"), Some(("rpg:crit".into(), true)));
    assert_eq!(
        full_id(&["me.rpg", "rpg"], "crit"),
        Some(("rpg:crit".into(), true)),
        "a renamed mod's bare names stay where its data lives"
    );
    assert_eq!(
        full_id(&["me.rpg", "rpg"], "me.rpg:luck"),
        Some(("me.rpg:luck".into(), true))
    );
    assert_eq!(
        full_id(&["rpg"], "rpg:crit"),
        Some(("rpg:crit".into(), true))
    );
    assert_eq!(
        full_id(&["rpg"], "enshwick:damage"),
        Some(("enshwick:damage".into(), false))
    );
    assert_eq!(full_id(&["rpg"], "Crit"), None);
    assert_eq!(full_id(&["rpg"], "a:b:c"), None);
}

#[test]
fn values_follow_the_sources_in_their_order() {
    let sealed = combat_kinds();
    let list = [
        source(StatSource::ITEM, StatModifierData::MULTIPLY, 2, 1.5, 900, 7),
        source(StatSource::BUFF, StatModifierData::ADD, 3, 0.25, 0x99, 0),
        source(StatSource::ENTITY, StatModifierData::ADD, 3, 0.25, 1, 0),
    ];
    let values = work_out(&sealed, &list);
    let want = [1.0f32, 0.0, 1.5, 0.5];
    assert!(
        values
            .iter()
            .zip(want)
            .all(|(a, b)| a.to_bits() == b.to_bits())
    );
}

#[test]
fn hit_factors_combine_every_type_with_each_type() {
    let sealed = combat_kinds();
    assert_eq!(combat(&sealed, &[1.0, 0.0, 1.0, 0.0]), None);
    let close = |a: f32, b: f32| (a - b).abs() < 1e-6;
    let row = combat(&sealed, &[2.0, 0.5, 1.5, 0.25]).unwrap();
    assert!(close(row.deals[3], 3.0));
    assert!(close(row.deals[0], 2.0));
    assert!(close(row.deals[HEALING], 1.0));
    assert!(close(row.takes[3], 0.25));
    assert!(close(row.takes[0], 0.5));
    let immune = combat(&sealed, &[1.0, 2.0, 1.0, 0.0]).unwrap();
    assert!(
        close(immune.takes[1], 0.0),
        "resistance stops at taking nothing"
    );
}

#[test]
fn hits_without_factors_stay_as_they_are() {
    let mut amounts = [10.0; TYPES];
    on_hit(&mut amounts, 1, 2);
    assert!(amounts.iter().all(|a| a.to_bits() == 10.0f32.to_bits()));
}

#[test]
fn saved_modifiers_round_trip() {
    let saved = vec![Saved {
        player: 77,
        owner: "rpg".into(),
        key: 5,
        stat: "rpg:crit".into(),
        op: StatModifierData::ADD,
        value: 0.1,
    }];
    let bytes = postcard::to_allocvec(&saved).unwrap();
    assert_eq!(postcard::from_bytes::<Vec<Saved>>(&bytes).unwrap(), saved);
}
