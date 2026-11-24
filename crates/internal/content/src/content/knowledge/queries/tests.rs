use super::*;
use crate::knowledge::KnowledgeId;

/// An action of the game's form: a knowledge test, or a query part with
/// its cached indices.
fn action(id: u32, indices: Option<(u64, u64)>) -> Value {
    let (kind, (world, player)) = indices.map_or((OF_KNOWLEDGE, (0, 0)), |at| (OF_QUERY, at));
    Value::Struct(vec![
        (TYPE.into(), Value::Name(kind.into())),
        (NAME.into(), Value::Text("label".into())),
        (
            QUERY.into(),
            Value::Struct(vec![(QUERY_DATA_ID.into(), hash_key(id))]),
        ),
        (QUERY_INDEX.into(), Value::Uint(world)),
        (PLAYER_INDEX.into(), Value::Uint(player)),
        ("invertResult".into(), Value::Bool(false)),
        ("isPlayerAction".into(), Value::Bool(false)),
    ])
}

fn query(name: &str, id: u32, player: bool, actions: Vec<Value>) -> Value {
    Value::Struct(vec![
        (NAME.into(), Value::Text(name.into())),
        (
            QUERY_ID.into(),
            Value::Struct(vec![("id".into(), hash_key(id))]),
        ),
        (QUERY_INDEX.into(), Value::Uint(0)),
        (TYPE.into(), Value::Name("AND".into())),
        (IS_PLAYER.into(), Value::Bool(player)),
        ("invertResult".into(), Value::Bool(false)),
        ("hasOptionalPlayerProgression".into(), Value::Bool(false)),
        (ACTIONS.into(), Value::List(actions)),
    ])
}

/// Two world queries, the second with a player twin, and a player query:
/// each side names a query past the world range.
fn database() -> Value {
    let queries = vec![
        query("first", 10, false, vec![action(30, Some((3, 3)))]),
        query("twin", 20, false, vec![action(1, None)]),
        query("twin", 20, true, vec![action(1, None)]),
        query("last", 30, true, vec![action(20, Some((1, 2)))]),
    ];
    let mut db = Value::Struct(vec![
        (PLAYER_START.into(), Value::Uint(2)),
        (QUERIES.into(), Value::List(queries)),
    ]);
    if let Some(Value::List(queries)) = db.field_mut(QUERIES) {
        for (at, query) in queries.iter_mut().enumerate() {
            set(query, QUERY_INDEX, Value::Uint(at as u64)).unwrap();
        }
    }
    db
}

fn number(value: &Value, name: &str) -> u64 {
    match value.field(name) {
        Some(Value::Uint(n)) => *n,
        other => panic!("{name}: {other:?}"),
    }
}

fn queries(db: &Value) -> &[Value] {
    match db.field(QUERIES) {
        Some(Value::List(queries)) => queries,
        other => panic!("{other:?}"),
    }
}

fn cached(query: &Value, part: usize) -> (u64, u64) {
    let Some(Value::List(actions)) = query.field(ACTIONS) else {
        panic!("{query:?}")
    };
    (
        number(&actions[part], QUERY_INDEX),
        number(&actions[part], PLAYER_INDEX),
    )
}

fn game(id: u32) -> KnowledgeQuery {
    KnowledgeQuery::game(GameQuery::new(id))
}

#[test]
fn a_world_query_ends_the_world_range_and_every_index_past_it_moves() {
    let world = NewQuery::new(
        Id::new("you.sets:world"),
        Scope::World,
        QueryForm::Any,
        vec![game(30)],
    );
    let player = NewQuery {
        invert: true,
        ..NewQuery::new(
            Id::new("you.sets:player"),
            Scope::Player,
            QueryForm::All,
            vec![
                game(20),
                KnowledgeQuery::is_set(KnowledgeId::new(5), Scope::Player),
            ],
        )
    };
    let (db, refused) = add_to(database(), &[player.clone(), world.clone()]).unwrap();
    assert!(refused.is_empty(), "{refused:?}");
    assert_eq!(number(&db, PLAYER_START), 3);
    let all = queries(&db);
    let ids: Vec<u32> = all.iter().filter_map(query_id).collect();
    assert_eq!(ids, [10, 20, world.key(), 20, 30, player.key()]);
    for (at, query) in all.iter().enumerate() {
        assert_eq!(number(query, QUERY_INDEX), at as u64, "{at}");
    }
    assert_eq!(
        cached(&all[0], 0),
        (4, 4),
        "a world part naming a player query"
    );
    assert_eq!(cached(&all[4], 0), (1, 3), "a player part naming the twins");
    assert_eq!(cached(&all[2], 0), (4, 4), "the mod's world query");
    assert_eq!(cached(&all[5], 0), (1, 3), "the mod's player query");
    assert_eq!(all[5].field(TYPE), Some(&Value::Name("AND".into())));
    assert_eq!(all[5].field("invertResult"), Some(&Value::Bool(true)));
    assert_eq!(all[2].field(TYPE), Some(&Value::Name("OR".into())));
    assert_eq!(all[2].field(IS_PLAYER), Some(&Value::Bool(false)));
    let Some(Value::List(parts)) = all[5].field(ACTIONS) else {
        panic!()
    };
    assert_eq!(
        parts[1].field(TYPE),
        Some(&Value::Name(OF_KNOWLEDGE.into()))
    );
}

#[test]
fn a_query_whose_id_its_side_holds_or_whose_part_names_no_query_is_left_out() {
    let lost = NewQuery::new(
        Id::new("you.sets:lost"),
        Scope::World,
        QueryForm::All,
        vec![game(99)],
    );
    let chained = NewQuery::new(
        Id::new("you.sets:chained"),
        Scope::Player,
        QueryForm::All,
        vec![game(Id::new("you.sets:kept").guid().fnv1a())],
    );
    let kept = NewQuery::new(
        Id::new("you.sets:kept"),
        Scope::World,
        QueryForm::All,
        vec![KnowledgeQuery::is_set(KnowledgeId::new(5), Scope::World)],
    );
    let (db, refused) = add_to(
        database(),
        &[lost, kept.clone(), kept.clone(), chained.clone()],
    )
    .unwrap();
    assert_eq!(refused.len(), 2, "{refused:?}");
    assert!(
        refused[0].to_string().contains("no query 99"),
        "{}",
        refused[0]
    );
    assert!(
        refused[1].to_string().contains("holds it"),
        "{}",
        refused[1]
    );
    assert_eq!(number(&db, PLAYER_START), 3, "one world query took room");
    let all = queries(&db);
    assert_eq!(query_id(&all[2]), Some(kept.key()));
    assert_eq!(query_id(&all[5]), Some(chained.key()));
    assert_eq!(
        cached(&all[5], 0),
        (2, 2),
        "a part naming an earlier mod query"
    );
    assert_eq!(cached(&all[0], 0), (4, 4));
}

#[test]
fn a_query_without_parts_or_one_of_more_than_two_is_refused() {
    let id = Id::new("you.sets:x");
    let part = KnowledgeQuery::is_set(KnowledgeId::new(5), Scope::World);
    assert!(
        NewQuery::new(id, Scope::World, QueryForm::All, vec![])
            .check()
            .is_err()
    );
    assert!(
        NewQuery::new(id, Scope::World, QueryForm::OneOf, vec![part; 3])
            .check()
            .is_err()
    );
    assert!(
        NewQuery::new(id, Scope::World, QueryForm::OneOf, vec![part; 2])
            .check()
            .is_ok()
    );
    let count = KnowledgeQuery::at_least(KnowledgeId::new(5), Scope::World, 0);
    assert!(
        NewQuery::new(id, Scope::World, QueryForm::Any, vec![count])
            .check()
            .is_err(),
        "a count from 0"
    );
}

#[test]
fn a_query_reads_its_own_knowledge_and_that_of_the_queries_it_names() {
    let db = Value::Struct(vec![(
        QUERIES.into(),
        Value::List(vec![
            query(
                "a",
                1,
                false,
                vec![action(0x10, None), action(2, Some((1, 1)))],
            ),
            query(
                "b",
                2,
                false,
                vec![action(0x20, None), action(1, Some((0, 0)))],
            ),
            query("c", 3, true, vec![action(0x30, None), action(0x10, None)]),
            query("d", 4, false, Vec::new()),
        ]),
    )]);
    assert_eq!(
        knowledge_read(&db),
        vec![
            (1, vec![0x10, 0x20]),
            (2, vec![0x10, 0x20]),
            (3, vec![0x10, 0x30]),
            (4, Vec::new()),
        ],
        "a named query's ids count, a cycle ends, and each id once"
    );
    assert!(knowledge_read(&Value::Struct(Vec::new())).is_empty());
}
