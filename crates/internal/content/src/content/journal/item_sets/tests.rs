use super::*;
use crate::content::knowledge::GameQuery;

const CATEGORY: ItemSetCategory = item_set_category::COLLECTIONS;
const VANILLA_SET: ItemSetId = ItemSetId::new(0x0000_0800);
const VANILLA_DONE: u32 = 0x0000_0801;

fn query(id: u32) -> Value {
    KnowledgeQuery::game(GameQuery::new(id)).to_value().unwrap()
}

/// A journal of one category holding one vanilla set, paid by one row.
fn registry() -> Value {
    let set = Value::Struct(vec![
        (SET_ID.into(), hash_key(VANILLA_SET.get())),
        ("setName".into(), hash_key(7)),
        (EXPERIENCE.into(), hash_key(9)),
        (
            ENTRIES.into(),
            Value::List(vec![
                ItemSetEntry::new(ItemId::new(5))
                    .to_value(0x0000_0802)
                    .unwrap(),
            ]),
        ),
        (RECIPES.into(), Value::List(Vec::new())),
        (DONE.into(), query(VANILLA_DONE)),
        (SHOWN.into(), query(0x0000_0803)),
    ]);
    let row = Value::Struct(vec![
        (
            "rewardKnowledge".into(),
            Value::Struct(vec![("id".into(), hash_key(0x0000_0804))]),
        ),
        (EXPERIENCE.into(), hash_key(9)),
        (REWARD_QUERY.into(), query(VANILLA_DONE)),
    ]);
    Value::Struct(vec![
        (
            CATEGORIES.into(),
            Value::List(vec![Value::Struct(vec![
                (CATEGORY_NAME.into(), hash_key(CATEGORY.get())),
                (ENTRIES.into(), Value::List(vec![set])),
            ])]),
        ),
        (REWARDS.into(), Value::List(vec![row])),
    ])
}

fn sets(registry: &Value) -> &[Value] {
    sets_of(&categories(registry)[0])
}

fn rows(registry: &Value) -> &[Value] {
    match registry.field(REWARDS) {
        Some(Value::List(rows)) => rows,
        other => panic!("{other:?}"),
    }
}

fn added(id: &'static str) -> AddedSet {
    AddedSet {
        new: NewItemSet {
            experience: Some(ExperienceValue::new(11)),
            recipe_rewards: vec![RecipeId::new(12)],
            ..NewItemSet::new(
                Id::new(id),
                CATEGORY,
                Guid([4; 16]),
                vec![
                    ItemSetEntry::new(ItemId::new(21)),
                    ItemSetEntry::new(ItemId::new(22)),
                ],
            )
        },
        category: CATEGORY.get(),
        done: KnowledgeQuery::game(GameQuery::new(31)),
        shown: KnowledgeQuery::game(GameQuery::new(32)),
        rewarded: KnowledgeId::new(33),
    }
}

#[test]
fn a_mod_s_set_joins_its_category_with_a_pay_row_on_its_completion_query() {
    let mut registry = registry();
    let new = added("you.sets:shells");
    new.apply(&mut registry).unwrap();
    let set = &sets(&registry)[1];
    assert_eq!(key(set, SET_ID), Some(new.new.id.guid().fnv1a()));
    assert_eq!(key(set, EXPERIENCE), Some(11));
    assert_eq!(query_of(set, DONE), Some(31));
    assert_eq!(query_of(set, SHOWN), Some(32));
    assert_eq!(set.field(RECIPES), Some(&Value::List(vec![hash_key(12)])));
    let Some(Value::List(entries)) = set.field(ENTRIES) else {
        panic!("{set:?}")
    };
    assert_eq!(entries.len(), 2);
    let done = entries[0].field(DONE).unwrap();
    assert_eq!(done.field("type"), Some(&Value::Name("SimpleFlag".into())));
    assert_eq!(
        done.field("compareValue"),
        Some(&Value::Uint(PICKED_UP.into()))
    );
    assert_eq!(
        query_of(&entries[1], DONE),
        Some(22),
        "the item's own pickup flag"
    );
    let row = &rows(&registry)[1];
    assert_eq!(query_of(row, REWARD_QUERY), Some(31));
    assert_eq!(key(row, EXPERIENCE), Some(11));
    assert!(
        new.apply(&mut registry).is_err(),
        "the journal holds the set's id now"
    );
    assert_eq!(rows(&registry).len(), 2, "a refused set adds no row");
}

#[test]
fn a_vanilla_set_changes_its_entries_queries_and_pay_row_together() {
    let mut registry = registry();
    let change = ItemSetChange {
        experience: Some(ExperienceValue::new(40)),
        entries: Some(vec![ItemSetEntry::new(ItemId::new(41))]),
        done_when: Some(KnowledgeQuery::game(GameQuery::new(42))),
        ..ItemSetChange::default()
    };
    change_in(&mut registry, VANILLA_SET, &change).unwrap();
    let set = &sets(&registry)[0];
    assert_eq!(key(set, EXPERIENCE), Some(40));
    assert_eq!(query_of(set, DONE), Some(42));
    assert_eq!(query_of(set, SHOWN), Some(0x0000_0803), "kept");
    let Some(Value::List(entries)) = set.field(ENTRIES) else {
        panic!("{set:?}")
    };
    assert_eq!(key(&entries[0], "itemId"), Some(41));
    assert_ne!(
        key(&entries[0], ENTRY_ID),
        Some(0x0000_0802),
        "a fresh entry id"
    );
    let row = &rows(&registry)[0];
    assert_eq!(query_of(row, REWARD_QUERY), Some(42));
    assert_eq!(key(row, EXPERIENCE), Some(40));
    assert!(change_in(&mut registry, ItemSetId::new(1), &change).is_err());
}

#[test]
fn a_removed_set_takes_its_pay_row_with_it() {
    let mut registry = registry();
    remove_in(&mut registry, VANILLA_SET).unwrap();
    assert!(sets(&registry).is_empty());
    assert!(rows(&registry).is_empty());
    assert!(remove_in(&mut registry, VANILLA_SET).is_err());
}

#[test]
fn a_category_no_set_is_listed_under_is_refused_with_the_names_it_may_take() {
    let registry = registry();
    assert_eq!(category_key(&registry, "set", CATEGORY), Ok(CATEGORY.get()));
    let why = category_key(&registry, "set", ItemSetCategory::new(9))
        .unwrap_err()
        .to_string();
    assert!(why.contains("Collections"), "{why}");
}

#[test]
fn the_journal_s_one_id_check_sees_item_sets_and_their_entries() {
    let registry = registry();
    assert!(super::super::holds(&registry, VANILLA_SET.get()));
    assert!(super::super::holds(&registry, 0x0000_0802), "a set entry");
    assert!(!super::super::holds(&registry, 0x0000_0805));
}
