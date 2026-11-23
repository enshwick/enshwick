use super::*;

/// The category of the game's one page in [`registry`].
const CATEGORY: Guid = Guid([14; 16]);

fn query(id: u32) -> KnowledgeQuery {
    KnowledgeQuery::is_set(KnowledgeId::new(id), Scope::Player)
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
        steps: vec![StepChange {
            step: 0,
            shown_when: None,
            done_when: Some(query(7)),
        }],
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
        steps: vec![StepChange {
            step: 1,
            shown_when: Some(query(1)),
            done_when: None,
        }],
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
