use super::*;

/// The category of the game's one page in [`registry`].
const CATEGORY: Guid = Guid([14; 16]);

fn query(id: u32) -> KnowledgeQuery {
    KnowledgeQuery::is_set(KnowledgeId::new(id), Scope::Player)
}

/// The query database read as `list`.
fn reads(list: Vec<(u32, Vec<u32>)>) -> LazyCell<Reads, impl FnOnce() -> Reads> {
    LazyCell::new(move || Some(list))
}

/// A query database a removal must leave unread: no step of another mod
/// asks through a game query.
fn unread() -> LazyCell<Reads, impl FnOnce() -> Reads> {
    LazyCell::new(|| panic!("the query database was read"))
}

fn registry() -> Value {
    let mut vanilla = NewQuest::new(
        Id::new("demo.vanilla:q"),
        Guid([1; 16]),
        vec![QuestStep::new(
            Guid([2; 16]),
            Guid([3; 16]),
            query(1),
            query(2),
        )],
    );
    vanilla.rewards = vec![QuestReward::Item(Guid([4; 16]))];
    let page = header(7, CATEGORY.fnv1a(), Guid([8; 16]), hash_key(0), Vec::new());
    let tutorial = header(9, 0, Guid([8; 16]), hash_key(0), Vec::new());
    let empty = Value::Struct(vec![
        (QUESTS.into(), Value::List(Vec::new())),
        (
            COLLECTIONS.into(),
            Value::List(vec![Value::Struct(page), Value::Struct(tutorial)]),
        ),
    ]);
    let mut held = empty;
    add_quest_to(&mut held, &vanilla).unwrap();
    if let Some(Value::List(quests)) = held.field_mut(QUESTS)
        && let Some(document) = quests[0].field_mut(DOCUMENT)
    {
        *document = hash_key(1_768_495_365);
    }
    held
}

fn quests(registry: &Value) -> &[Value] {
    match registry.field(QUESTS) {
        Some(Value::List(quests)) => quests,
        _ => &[],
    }
}

#[test]
fn a_quest_joins_the_journal_with_its_steps_rewards_and_the_game_s_document() {
    let new = NewQuest {
        rewards: vec![QuestReward::Experience(ExperienceValue::new(9))],
        ..NewQuest::new(
            Id::new("you.trail:trail"),
            Guid([5; 16]),
            vec![
                QuestStep::new(Guid([6; 16]), Guid([7; 16]), query(3), query(4)),
                QuestStep::new(Guid([8; 16]), Guid([9; 16]), query(4), query(5)),
            ],
        )
    };
    let mut added = registry();
    add_quest_to(&mut added, &new).unwrap();
    let quest = &quests(&added)[1];
    assert_eq!(
        entry_id(quest),
        Some(KnowledgeId::from_guid(new.id.guid()).get())
    );
    assert_eq!(quest.field(DOCUMENT), Some(&hash_key(1_768_495_365)));
    let Some(Value::List(steps)) = quest.field(ENTRIES) else {
        panic!("{quest:?}")
    };
    assert_eq!(steps.len(), 2);
    assert_ne!(entry_id(&steps[0]), entry_id(&steps[1]));
    assert_eq!(steps[1].field(SHOWN), Some(&query(4).to_value().unwrap()));
    let before = added.clone();
    assert!(add_quest_to(&mut added, &new).is_err(), "twice");
    assert_eq!(added, before, "left as it was");
}

#[test]
fn the_loader_fills_a_step_s_missing_queries_from_the_step_before_and_its_own_bools() {
    let mut new = NewQuest::new(
        Id::new("you.trail:trail"),
        Guid([5; 16]),
        vec![
            QuestStep::owned(Guid([6; 16]), Guid([7; 16])),
            QuestStep::owned(Guid([8; 16]), Guid([9; 16])),
            QuestStep {
                done_when: Some(query(40)),
                ..QuestStep::owned(Guid([10; 16]), Guid([11; 16]))
            },
            QuestStep {
                shown_when: Some(query(41)),
                ..QuestStep::owned(Guid([12; 16]), Guid([13; 16]))
            },
        ],
    );
    let mut parts = Vec::new();
    fill_owned(&mut new, |part| {
        parts.push(part);
        Ok(query(100 + u32::try_from(parts.len()).unwrap()))
    })
    .unwrap();
    assert_eq!(
        parts,
        ["started", "step-0.done", "step-1.done", "step-3.done"]
    );
    let filled: Vec<_> = new
        .steps
        .iter()
        .map(|s| (s.shown_when, s.done_when))
        .collect();
    assert_eq!(
        filled,
        [
            (Some(query(101)), Some(query(102))),
            (Some(query(102)), Some(query(103))),
            (Some(query(103)), Some(query(40))),
            (Some(query(41)), Some(query(104))),
        ]
    );
    assert_eq!(owned_scope(&QuestType::WorldQuest), Scope::World);
    assert_eq!(owned_scope(&QuestType::PlayerQuest), Scope::Player);
}

#[cfg(feature = "unstable")]
#[test]
fn a_page_s_entries_show_on_knowledge_and_never_complete() {
    let page = NewJournalPage {
        id: Id::new("you.trail:lore"),
        category: CATEGORY,
        name: Guid([11; 16]),
        entries: vec![PageEntry {
            name: Guid([12; 16]),
            text: Guid([13; 16]),
            shown_when: query(6),
        }],
    };
    let mut added = registry();
    add_page_to(&mut added, &page).unwrap();
    let Some(Value::List(pages)) = added.field(COLLECTIONS) else {
        panic!("{added:?}")
    };
    let last = pages.last().unwrap();
    assert_eq!(last.field(LORE_CATEGORY), Some(&hash_key(CATEGORY.fnv1a())));
    let Some(Value::List(entries)) = last.field(ENTRIES) else {
        panic!("{last:?}")
    };
    assert_eq!(entries[0].field(SHOWN), Some(&query(6).to_value().unwrap()));
    assert_eq!(
        entries[0].field(DONE).and_then(|done| done.field("type")),
        Some(&Value::Name("Extern".into()))
    );
}

#[cfg(feature = "unstable")]
#[test]
fn a_page_of_a_category_no_game_page_has_is_refused_with_the_game_s() {
    let page = |category| NewJournalPage {
        id: Id::new("you.trail:lore"),
        category,
        name: Guid([11; 16]),
        entries: vec![PageEntry {
            name: Guid([12; 16]),
            text: Guid([13; 16]),
            shown_when: query(6),
        }],
    };
    let mut before = registry();
    let general = header(
        10,
        lore_category::GENERAL.fnv1a(),
        Guid([8; 16]),
        hash_key(0),
        Vec::new(),
    );
    list_mut(&mut before, COLLECTIONS)
        .unwrap()
        .push(Value::Struct(general));
    let mut added = before.clone();
    let own = Guid([10; 16]);
    let off = add_page_to(&mut added, &page(own)).unwrap_err().to_string();
    assert!(off.contains(&own.to_string()), "{off}");
    assert!(
        off.contains(&lore_category::GENERAL.to_string()),
        "a category enshwick names reads as the tag a mod passes: {off}"
    );
    assert!(
        off.contains(&format!("key {}", CATEGORY.fnv1a())),
        "one it does not name reads as its key: {off}"
    );
    assert_eq!(added, before, "left as it was");
    add_page_to(&mut added, &page(lore_category::GENERAL)).unwrap();
}

#[cfg(feature = "unstable")]
#[test]
fn the_tutorial_pages_category_0_is_no_category_a_page_may_take() {
    assert_eq!(held_categories(&registry()), [CATEGORY.fnv1a()]);
}

#[test]
fn two_changes_of_a_vanilla_quest_chain_and_a_step_past_its_own_is_refused() {
    let vanilla = KnowledgeId::from_guid(Id::new("demo.vanilla:q").guid());
    let first = QuestChange {
        rewards: Some(Vec::new()),
        for_all_players: Some(true),
        ..QuestChange::default()
    };
    let second = QuestChange {
        add_rewards: vec![QuestReward::Item(Guid([14; 16]))],
        steps: vec![StepEdit::Change(StepChange {
            step: 0,
            done_when: Some(query(7)),
            ..StepChange::default()
        })],
        ..QuestChange::default()
    };
    let mut twice = registry();
    change_quest_in(&mut twice, vanilla, &first).unwrap();
    change_quest_in(&mut twice, vanilla, &second).unwrap();
    let quest = &quests(&twice)[0];
    assert_eq!(
        quest.field(REWARDS),
        Some(&Value::List(vec![
            QuestReward::Item(Guid([14; 16])).to_value()
        ]))
    );
    assert_eq!(quest.field(FOR_ALL), Some(&Value::Bool(true)));
    let Some(Value::List(steps)) = quest.field(ENTRIES) else {
        panic!("{quest:?}")
    };
    assert_eq!(steps[0].field(DONE), Some(&query(7).to_value().unwrap()));
    assert_eq!(
        steps[0].field(SHOWN),
        Some(&query(1).to_value().unwrap()),
        "kept"
    );
    let past = QuestChange {
        steps: vec![StepEdit::Change(StepChange {
            step: 1,
            shown_when: Some(query(1)),
            ..StepChange::default()
        })],
        ..QuestChange::default()
    };
    let mut held = registry();
    let past = QuestChange {
        rewards: Some(Vec::new()),
        ..past
    };
    assert!(change_quest_in(&mut held, vanilla, &past).is_err());
    assert_eq!(held, registry(), "the rewards are kept when a step is past");
    assert!(change_quest_in(&mut held, KnowledgeId::new(5), &first).is_err());
}

fn vanilla() -> KnowledgeId {
    KnowledgeId::from_guid(Id::new("demo.vanilla:q").guid())
}

fn steps(quest: &Value) -> &[Value] {
    match quest.field(ENTRIES) {
        Some(Value::List(steps)) => steps,
        _ => &[],
    }
}

#[test]
fn every_field_of_a_vanilla_quest_and_its_step_changes_and_none_keeps_it() {
    let marker = Guid([20; 16]);
    let change = QuestChange {
        name: Some(Guid([21; 16])),
        source: Some(QuestSourceType::None),
        kind: Some(QuestType::WorldQuest),
        main: Some(true),
        priority: Some(7),
        tutorial: Some(true),
        document: Some(Guid([22; 16])),
        steps: vec![StepEdit::Change(StepChange {
            step: 0,
            name: Some(Guid([23; 16])),
            text: Some(Guid([24; 16])),
            progress: Some(vec![query(8)]),
            marker: Some(marker),
            voiceover: Some(Guid([25; 16])),
            icon: Some(ItemId::new(26)),
            level: Some(12),
            ..StepChange::default()
        })],
        ..QuestChange::default()
    };
    let mut changed = registry();
    change_quest_in(&mut changed, vanilla(), &change).unwrap();
    let quest = &quests(&changed)[0];
    assert_eq!(quest.field("name"), Some(&hash_key(Guid([21; 16]).fnv1a())));
    assert_eq!(quest.field("type"), Some(&QuestType::WorldQuest.to_value()));
    assert_eq!(quest.field("isMainQuest"), Some(&Value::Bool(true)));
    assert_eq!(quest.field("priority"), Some(&Value::Uint(7)));
    assert_eq!(quest.field("isTutorial"), Some(&Value::Bool(true)));
    assert_eq!(
        quest.field(DOCUMENT),
        Some(&hash_key(Guid([22; 16]).fnv1a()))
    );
    let step = &steps(quest)[0];
    assert_eq!(step.field("text"), Some(&hash_key(Guid([24; 16]).fnv1a())));
    assert_eq!(
        step.field(PROGRESS),
        Some(&Value::List(vec![query(8).to_value().unwrap()]))
    );
    assert_eq!(
        step.field("mapMarkerReference"),
        Some(&hash_key(marker.fnv1a()))
    );
    assert_eq!(step.field("voiceover"), Some(&Value::Guid(Guid([25; 16]))));
    assert_eq!(step.field("itemIconId"), Some(&hash_key(26)));
    assert_eq!(step.field("recommendedLevel"), Some(&Value::Uint(12)));
    assert_eq!(
        step.field(SHOWN),
        Some(&query(1).to_value().unwrap()),
        "kept"
    );
    assert_eq!(quest.field(FOR_ALL), Some(&Value::Bool(false)), "kept");

    let clear = QuestChange {
        steps: vec![StepEdit::Change(StepChange {
            marker: Some(Guid([0; 16])),
            ..StepChange::default()
        })],
        ..QuestChange::default()
    };
    change_quest_in(&mut changed, vanilla(), &clear).unwrap();
    assert_eq!(
        steps(&quests(&changed)[0])[0].field("mapMarkerReference"),
        Some(&hash_key(0)),
        "a tag of zeros clears the marker"
    );
}

#[test]
fn step_edits_apply_in_order_and_an_inserted_step_takes_an_id_the_journal_does_not_hold() {
    let inserted = |at, shown| {
        StepEdit::Insert(StepInsert {
            at,
            step: QuestStep::new(
                Guid([30; 16]),
                Guid([31; 16]),
                query(shown),
                query(shown + 1),
            ),
        })
    };
    let change = QuestChange {
        steps: vec![
            inserted(1, 50),
            inserted(0, 60),
            StepEdit::Move(StepMove { from: 0, to: 2 }),
            StepEdit::Remove(0),
        ],
        ..QuestChange::default()
    };
    let before = registry();
    let mut changed = before.clone();
    change_quest_in(&mut changed, vanilla(), &change).unwrap();
    let held = steps(&quests(&changed)[0]);
    // [v] -> [v, a] -> [b, v, a] -> [v, a, b] -> [a, b]
    let shown: Vec<_> = held.iter().map(|s| s.field(SHOWN).cloned()).collect();
    assert_eq!(
        shown,
        [
            Some(query(50).to_value().unwrap()),
            Some(query(60).to_value().unwrap())
        ]
    );
    let ids: Vec<u32> = held.iter().filter_map(entry_id).collect();
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);
    assert!(ids.iter().all(|&id| id != 0 && !holds(&before, id)));

    let mut again = before.clone();
    change_quest_in(&mut again, vanilla(), &change).unwrap();
    assert_eq!(
        again, changed,
        "the same changes in the same order agree on the ids"
    );
}

#[test]
fn a_step_edit_that_does_not_fit_is_refused_and_leaves_the_journal_as_it_was() {
    let refused = [
        StepEdit::Remove(1),
        StepEdit::Move(StepMove { from: 0, to: 1 }),
        StepEdit::Insert(StepInsert {
            at: 2,
            step: QuestStep::new(Guid([30; 16]), Guid([31; 16]), query(1), query(2)),
        }),
        StepEdit::Insert(StepInsert {
            at: 0,
            step: QuestStep::owned(Guid([30; 16]), Guid([31; 16])),
        }),
        StepEdit::Remove(0),
    ];
    for edit in refused {
        let mut held = registry();
        let change = QuestChange {
            name: Some(Guid([21; 16])),
            steps: vec![edit.clone()],
            ..QuestChange::default()
        };
        assert!(
            change_quest_in(&mut held, vanilla(), &change).is_err(),
            "{edit:?}"
        );
        assert_eq!(held, registry(), "{edit:?}");
    }
}

#[test]
fn a_quest_another_mod_chains_on_stays_and_one_only_the_game_or_the_mod_chains_on_goes() {
    let id = vanilla();
    let chained = |name: &'static str| {
        NewQuest::new(
            Id::new(name),
            Guid([5; 16]),
            vec![QuestStep::new(
                Guid([6; 16]),
                Guid([7; 16]),
                KnowledgeQuery::is_set(id, Scope::Player),
                query(4),
            )],
        )
    };
    let game = registry();
    let mut held = registry();
    let other = chained("other.mod:after");
    add_quest_to(&mut held, &other).unwrap();
    let before = held.clone();
    let off = remove_quest_in(&mut held, id, found(&before, &game), &unread()).unwrap_err();
    assert!(off.to_string().contains("another mod's step"), "{off}");
    assert_eq!(held, before);

    let own = found(&game, &game);
    remove_quest_in(&mut held, id, own, &unread()).unwrap();
    assert_eq!(quests(&held).len(), 1);
    assert!(!holds(&held, id.get()));
    for step in step_ids(Id::new("demo.vanilla:q"), 1) {
        assert!(
            !holds(&held, step),
            "no step id of the removed quest is left"
        );
    }
    assert!(
        remove_quest_in(&mut held, id, own, &unread()).is_err(),
        "gone"
    );
}

/// The journal a patch found, `found`, against the game's, `game`.
const fn found<'a>(found: &'a Value, game: &'a Value) -> Others<'a> {
    Others { found, game }
}

/// [`registry`] with a second vanilla quest, whose one step asks for
/// `shown` and `done`.
fn with_second(shown: u32, done: u32) -> (Value, KnowledgeId) {
    let second = NewQuest::new(
        Id::new("demo.vanilla:second"),
        Guid([20; 16]),
        vec![QuestStep::new(
            Guid([21; 16]),
            Guid([22; 16]),
            query(shown),
            query(done),
        )],
    );
    let mut game = registry();
    add_quest_to(&mut game, &second).unwrap();
    (game, KnowledgeId::from_guid(second.id.guid()))
}

#[test]
fn a_step_another_mod_put_in_or_changed_in_a_vanilla_quest_holds_the_quest_and_the_game_s_own_do_not()
 {
    let id = vanilla();
    let (game, second) = with_second(4, 5);
    let insert = QuestChange {
        steps: vec![StepEdit::Insert(StepInsert {
            at: 1,
            step: QuestStep::new(Guid([23; 16]), Guid([24; 16]), query(4), query(id.get())),
        })],
        ..QuestChange::default()
    };
    let change = QuestChange {
        steps: vec![StepEdit::Change(StepChange {
            step: 0,
            done_when: Some(query(id.get())),
            ..StepChange::default()
        })],
        ..QuestChange::default()
    };
    for edit in [insert, change] {
        let mut held = game.clone();
        change_quest_in(&mut held, second, &edit).unwrap();
        let before = held.clone();
        let off = remove_quest_in(&mut held, id, found(&before, &game), &unread()).unwrap_err();
        assert!(
            off.to_string()
                .contains(&format!("another mod's step in the quest {second}")),
            "{off}"
        );
        assert_eq!(held, before);
        let mut own = before.clone();
        remove_quest_in(&mut own, id, found(&game, &game), &unread()).unwrap();
    }

    let (game, _) = with_second(id.get(), 5);
    let mut held = game.clone();
    remove_quest_in(&mut held, id, found(&game, &game), &unread()).unwrap();
    assert!(!holds(&held, id.get()), "the game's own step asks for it");
    let renamed = QuestChange {
        name: Some(Guid([25; 16])),
        ..QuestChange::default()
    };
    let (game, second) = with_second(id.get(), 5);
    let mut held = game.clone();
    change_quest_in(&mut held, second, &renamed).unwrap();
    let before = held.clone();
    remove_quest_in(&mut held, id, found(&before, &game), &unread()).unwrap();
    assert!(
        !holds(&held, id.get()),
        "another mod's change of a field that asks nothing"
    );
}

#[cfg(feature = "unstable")]
#[test]
fn a_page_entry_another_mod_put_in_asks_through_a_game_query_and_holds_the_quest() {
    use crate::content::knowledge::GameQuery;

    let id = vanilla();
    let asking = 0x0000_0a01;
    let change = PageChange {
        entries: vec![StepEdit::Insert(StepInsert {
            at: 0,
            step: QuestStep {
                shown_when: Some(KnowledgeQuery::game(GameQuery::new(asking))),
                ..QuestStep::owned(Guid([41; 16]), Guid([42; 16]))
            },
        })],
        ..PageChange::default()
    };
    let game = registry();
    let mut held = game.clone();
    change_page_in(&mut held, EntryId::new(7), None, &change).unwrap();
    let before = held.clone();
    let db = || reads(vec![(asking, vec![id.get()])]);
    let off = remove_quest_in(&mut held, id, found(&before, &game), &db()).unwrap_err();
    assert!(
        off.to_string()
            .contains("another mod's entry in the journal page 7"),
        "{off}"
    );
    assert_eq!(held, before);
    let other = || reads(vec![(asking, vec![1])]);
    remove_quest_in(&mut held, id, found(&before, &game), &other()).unwrap();
}

/// `journal` with one item set category holding `sets`.
fn with_sets(mut journal: Value, sets: Vec<Value>) -> Value {
    let category = Value::Struct(vec![
        ("categoryName".into(), hash_key(3)),
        (ENTRIES.into(), Value::List(sets)),
    ]);
    if let Value::Struct(fields) = &mut journal {
        fields.push(("itemSetCategories".into(), Value::List(vec![category])));
    }
    journal
}

/// An item set of id `set` completed when `done` is set.
fn item_set(set: u32, done: u32) -> Value {
    Value::Struct(vec![
        ("setId".into(), hash_key(set)),
        (
            "completionKnowledgeQuery".into(),
            query(done).to_value().unwrap(),
        ),
    ])
}

#[test]
fn an_item_set_another_mod_put_in_holds_the_quest_and_the_game_s_own_does_not() {
    let id = vanilla();
    let game = with_sets(registry(), vec![item_set(70, id.get())]);
    let before = with_sets(
        registry(),
        vec![item_set(70, id.get()), item_set(71, id.get())],
    );
    let mut held = before.clone();
    let off = remove_quest_in(&mut held, id, found(&before, &game), &unread()).unwrap_err();
    assert!(
        off.to_string().contains("another mod's item set 71"),
        "{off}"
    );
    assert_eq!(held, before);
    let mut held = game.clone();
    remove_quest_in(&mut held, id, found(&game, &game), &unread()).unwrap();
    assert!(!holds(&held, id.get()), "the game's own set asks for it");
}

#[test]
fn a_quest_whose_step_another_mod_asks_for_stays() {
    let id = vanilla();
    let step = step_ids(Id::new("demo.vanilla:q"), 1)[0];
    let other = NewQuest::new(
        Id::new("other.mod:step"),
        Guid([5; 16]),
        vec![QuestStep::new(
            Guid([6; 16]),
            Guid([7; 16]),
            query(4),
            query(step),
        )],
    );
    let game = registry();
    let mut held = registry();
    add_quest_to(&mut held, &other).unwrap();
    let before = held.clone();
    let off = remove_quest_in(&mut held, id, found(&before, &game), &unread()).unwrap_err();
    assert!(off.to_string().contains("another mod's step"), "{off}");
    assert_eq!(held, before);
}

#[test]
fn a_quest_another_mod_asks_for_through_a_game_query_stays() {
    use crate::content::knowledge::GameQuery;

    let id = vanilla();
    let (asking, unrelated) = (0x0000_0a01, 0x0000_0a02);
    let through = |game: u32| {
        NewQuest::new(
            Id::new("other.mod:through"),
            Guid([5; 16]),
            vec![QuestStep::new(
                Guid([6; 16]),
                Guid([7; 16]),
                KnowledgeQuery::game(GameQuery::new(game)),
                query(4),
            )],
        )
    };
    let db = || reads(vec![(asking, vec![id.get()]), (unrelated, vec![1, 2])]);
    let game = registry();

    let mut held = registry();
    add_quest_to(&mut held, &through(asking)).unwrap();
    let before = held.clone();
    let off = remove_quest_in(&mut held, id, found(&before, &game), &db()).unwrap_err();
    assert!(off.to_string().contains("another mod's step"), "{off}");
    assert_eq!(held, before);

    let mut held = registry();
    add_quest_to(&mut held, &through(unrelated)).unwrap();
    let before = held.clone();
    let off = remove_quest_in(
        &mut held,
        id,
        found(&before, &game),
        &LazyCell::new(|| None),
    );
    assert!(
        off.is_err(),
        "a database that does not read keeps the quest"
    );
    assert_eq!(held, before);
    remove_quest_in(&mut held, id, found(&before, &game), &db()).unwrap();
    assert!(
        !holds(&held, id.get()),
        "a query that reads other knowledge"
    );
}

#[cfg(feature = "unstable")]
#[test]
fn a_vanilla_page_changes_its_fields_and_entries_and_is_removed() {
    let page = EntryId::new(7);
    let change = PageChange {
        name: Some(Guid([40; 16])),
        priority: Some(3),
        entries: vec![StepEdit::Insert(StepInsert {
            at: 0,
            step: QuestStep {
                shown_when: Some(query(9)),
                ..QuestStep::owned(Guid([41; 16]), Guid([42; 16]))
            },
        })],
        ..PageChange::default()
    };
    let mut held = registry();
    change_page_in(&mut held, page, Some(CATEGORY.fnv1a()), &change).unwrap();
    let Some(Value::List(pages)) = held.field(COLLECTIONS) else {
        panic!("{held:?}")
    };
    assert_eq!(
        pages[0].field("name"),
        Some(&hash_key(Guid([40; 16]).fnv1a()))
    );
    assert_eq!(pages[0].field("priority"), Some(&Value::Uint(3)));
    let entry = &steps(&pages[0])[0];
    assert_eq!(entry.field(SHOWN), Some(&query(9).to_value().unwrap()));
    assert_eq!(
        entry.field(DONE),
        Some(&never()),
        "a page entry never completes"
    );
    remove_page_in(&mut held, page).unwrap();
    assert!(remove_page_in(&mut held, page).is_err());
}
