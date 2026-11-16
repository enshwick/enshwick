use super::combat::combat;
use super::*;

#[test]
fn a_stopped_game_leaves_no_entities_or_values_behind() {
    let state = State::new();
    state.with_world(|world| {
        world
            .watched
            .entry(7)
            .or_default()
            .ask(ModIndex::new(1), true);
    });
    let mut values = Snapshot::default();
    values.values.insert(7, Box::new([2.0]));
    values.combat.insert(
        7,
        Combat {
            deals: [2.0; TYPES],
            takes: [1.0; TYPES],
        },
    );
    state.swap_in(values);
    state.look.store(true, Ordering::Relaxed);
    assert!(state.reads.hits.load(Ordering::Acquire));
    state.forget();
    assert!(state.world.relock().is_none());
    assert!(state.snapshot().load().values.is_empty());
    assert!(
        !state.reads.hits.load(Ordering::Acquire),
        "no hit reads the gone world's factors"
    );
    assert!(!state.look.load(Ordering::Relaxed));
}

#[test]
fn the_inventory_parts_forget_the_stats_at_a_hosted_game_s_end() {
    let mut values = Snapshot::default();
    values.combat.insert(
        8,
        Combat {
            deals: [2.0; TYPES],
            takes: [1.0; TYPES],
        },
    );
    STATE.swap_in(values);
    STATE.look.store(true, Ordering::Relaxed);
    let loader: &'static Loader = Box::leak(Box::new(crate::start::tests::bare_loader()));
    crate::engine::parts::Part::forget_in(crate::inventory::PARTS, "stats", loader);
    assert!(!STATE.reads.hits.load(Ordering::Acquire));
    assert!(!STATE.look.load(Ordering::Relaxed));
    assert!(STATE.world.relock().is_none());
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
            StatDef::multiplicative().range(0.0, 100.0),
        ),
        ("enshwick:resistance", StatDef::additive().range(-1.0, 1.0)),
        (
            "enshwick:damage_fire",
            StatDef::multiplicative().range(0.0, 100.0),
        ),
        (
            "enshwick:resistance_fire",
            StatDef::additive().range(-1.0, 1.0),
        ),
    ])
}

#[test]
fn ids_are_content_ids_and_say_whose_they_are() {
    let asked =
        |mod_id: &str, id: &str| full_id(mod_id, id).map(|a| (a.full, a.text.into_string(), a.own));
    let some = |full: &str, text: &str, own| Some((full.to_owned(), text.to_owned(), own));
    assert_eq!(asked("rpg", "crit"), None);
    assert_eq!(
        asked("me.rpg", "rpg:luck"),
        some("rpg:luck", "luck.name", false)
    );
    assert_eq!(
        asked("rpg", "rpg:crit"),
        some("rpg:crit", "crit.name", true)
    );
    assert_eq!(
        asked("rpg", "enshwick:damage"),
        some("enshwick:damage", "damage.name", false)
    );
    assert_eq!(asked("rpg", "Crit"), None);
    assert_eq!(asked("rpg", "a:b:c"), None);
    assert_eq!(
        asked("rpg", "rpg:charm/fire.big"),
        some("rpg:charm/fire.big", "charm/fire.big.name", true)
    );
    assert_eq!(asked("rpg", &format!("rpg:{}", "a".repeat(124))), None);
}

#[test]
fn values_follow_the_sources_in_their_order() {
    let sealed = combat_kinds();
    let list = [
        source(StatSource::ITEM, StatModifier::MULTIPLY, 2, 1.5, 900, 7),
        source(StatSource::BUFF, StatModifier::ADD, 3, 0.25, 0x99, 0),
        source(StatSource::ENTITY, StatModifier::ADD, 3, 0.25, 1, 0),
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
fn hits_count_as_changed_only_while_the_snapshot_holds_hit_factors() {
    let mut next = Snapshot::default();
    let factors = Combat {
        deals: [1.5; TYPES],
        takes: [1.0; TYPES],
    };
    next.combat.insert(7, factors);
    assert!(next.changes_hits());
    assert!(
        !Snapshot::default().changes_hits(),
        "an empty snapshot changes no hit"
    );
}

#[test]
fn saved_modifiers_round_trip() {
    let saved = vec![Saved {
        player: 77,
        owner: "rpg".into(),
        key: 5,
        stat: "rpg:crit".into(),
        op: StatModifier::ADD,
        value: 0.1,
    }];
    let bytes = postcard::to_allocvec(&saved).unwrap();
    assert_eq!(postcard::from_bytes::<Vec<Saved>>(&bytes).unwrap(), saved);
}

#[test]
fn one_mod_stopping_its_watch_leaves_another_mods_watch() {
    let (a, b) = (ModIndex::new(1), ModIndex::new(2));
    let mut watched = Watched::default();
    watched.ask(a, true);
    watched.ask(b, true);
    watched.ask(b, true);
    watched.ask(b, false);
    assert_eq!(watched.asked, [a], "b's second ask is the same ask");
    watched.ask(b, false);
    assert_eq!(watched.asked, [a], "b took back no ask of a");
    watched.ask(a, false);
    assert!(watched.asked.is_empty(), "nobody watches it");
}

#[test]
fn shown_values_a_full_link_had_no_room_for_go_again_at_the_next_step() {
    use own::Sent;
    let def = StatDef::additive().in_group(enshwick_core::stats::StatGroup::Primary);
    let mut sealed = sealed(&[("enshwick:luck", def.clone())]);
    sealed.kinds[0].def = Some((def, "luck.name".into(), ModIndex::LOADER.get()));
    let mut world = World::default();
    let watched = Watched {
        player: Some(9),
        values: Box::new([0.5]),
        ..Watched::default()
    };
    world.watched.insert(7, watched);
    let change = StatChange {
        header: Header::of::<StatChange>(),
        entity: Entity(7),
        stat: StatRef {
            index: 0,
            key: key_of("enshwick:luck").0,
        },
        old: 0.0,
        new: 0.5,
        reserved: 0,
    };
    let mut tries = 0;
    own::send_due(&sealed, &mut world, &[change], |_| {
        tries += 1;
        Sent::NoRoom
    });
    assert_eq!(world.values_owed, [7], "owed to the next step");
    own::send_due(&sealed, &mut world, &[change], |_| {
        tries += 1;
        Sent::Done
    });
    assert_eq!(tries, 2, "the owed player and its change go as one message");
    assert!(world.values_owed.is_empty());
    own::send_due(&sealed, &mut world, &[change], |_| Sent::NotReady);
    assert!(world.values_owed.is_empty(), "its hello asks for them");
}

#[test]
fn a_kind_shows_only_while_the_mod_that_defines_it_runs() {
    let loader = crate::start::tests::loader_running(&["you.rpg"]);
    let mut sealed = sealed(&[
        ("enshwick:luck", StatDef::additive()),
        ("you.rpg:curse", StatDef::additive()),
    ]);
    sealed.kinds[0].def = Some((
        StatDef::additive(),
        "luck.name".into(),
        ModIndex::LOADER.get(),
    ));
    sealed.kinds[1].def = Some((StatDef::additive(), "curse.name".into(), 0));
    assert!(own::runs(&loader, &sealed, 0), "the loader's own");
    assert!(own::runs(&loader, &sealed, 1), "a running mod's");
    loader.mods[0].off.store(true, Ordering::Release);
    assert!(!own::runs(&loader, &sealed, 1), "a mod turned off");
    assert!(own::runs(&loader, &sealed, 0), "the loader's own stays");
}
