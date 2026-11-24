use super::*;
use enshwick_typed::typed::runtime::Shaped as _;

fn query(id: u32) -> Value {
    Value::Struct(vec![
        (
            "knowledgeOrQueryId".into(),
            Value::Struct(vec![("value".into(), Value::Uint(id.into()))]),
        ),
        ("compareValue".into(), Value::Uint(0)),
        ("compareOperator".into(), Value::Name("Equals".into())),
        ("type".into(), Value::Name("Extern".into())),
        ("isExplicitPlayerKnowledgeQuery".into(), Value::Bool(false)),
    ])
}

fn rule(id: u32, grants: u32) -> Value {
    Value::Struct(vec![
        (EVENT.into(), Value::Bool(false)),
        (QUERY.into(), query(id)),
        (
            UNLOCK.into(),
            Value::List(vec![
                unlock("t", KnowledgeId::new(grants), Grant::Set).unwrap(),
            ]),
        ),
    ])
}

/// One world rule and one player rule.
fn triggers() -> Value {
    Value::Struct(vec![
        (WORLD.into(), Value::List(vec![rule(1, 11)])),
        (PLAYER.into(), Value::List(vec![rule(2, 12)])),
    ])
}

#[test]
fn a_mod_s_rule_follows_the_game_s_on_its_side() {
    let mut triggers = triggers();
    let unlocks = unlocks(
        "r",
        &[RuleGrant {
            id: KnowledgeId::new(13),
            grant: Grant::Flag(2),
        }],
    )
    .unwrap();
    Edit::Add(Scope::Player, query(3), unlocks)
        .apply(&mut triggers)
        .unwrap();
    let rules = rules_of(&triggers);
    assert_eq!(rules.len(), 3);
    let added = &rules[2];
    assert_eq!(
        added.id,
        RuleId {
            scope: Scope::Player,
            index: 1
        }
    );
    assert_eq!(added.when.map(|q| q.id), Some(KnowledgeId::new(3)));
    assert_eq!(
        added.grant,
        [RuleGrant {
            id: KnowledgeId::new(13),
            grant: Grant::Flag(2)
        }]
    );
    assert!(!added.on_event);
}

#[test]
fn a_mod_s_rule_takes_nothing_of_the_rule_it_copies_but_fields_the_api_does_not_know() {
    const OPERATION: &str = "requiredKnowledgeOperation";
    let mut first = rule(1, 11);
    let Value::Struct(fields) = &mut first else {
        unreachable!()
    };
    let held = unlock("t", KnowledgeId::new(99), Grant::Set).unwrap();
    fields.push((OPERATION.into(), held.clone()));
    fields.push(("newerField".into(), Value::Uint(7)));
    let mut triggers = Value::Struct(vec![
        (WORLD.into(), Value::List(vec![first])),
        (PLAYER.into(), Value::List(Vec::new())),
    ]);
    let unlocks = unlocks(
        "r",
        &[RuleGrant {
            id: KnowledgeId::new(13),
            grant: Grant::Set,
        }],
    )
    .unwrap();
    Edit::Add(Scope::Player, query(3), unlocks)
        .apply(&mut triggers)
        .unwrap();
    let Some(Value::List(players)) = triggers.field(PLAYER) else {
        unreachable!()
    };
    let blank = GameKnowledgeQueryTrigger::SHAPE.blank();
    assert_eq!(players[0].field(OPERATION), blank.field(OPERATION));
    assert_ne!(players[0].field(OPERATION), Some(&held));
    assert_eq!(players[0].field("newerField"), Some(&Value::Uint(7)));
}

#[test]
fn a_removed_rule_keeps_its_place_with_a_query_the_game_skips() {
    let mut triggers = triggers();
    let world = RuleId {
        scope: Scope::World,
        index: 0,
    };
    Edit::Remove(world).apply(&mut triggers).unwrap();
    let rules = rules_of(&triggers);
    assert_eq!(rules.len(), 2, "nothing moves");
    assert_eq!(rules[0].when, None, "query 0");
    assert_eq!(rules[1].when.map(|q| q.id), Some(KnowledgeId::new(2)));
    assert!(
        Edit::Remove(RuleId {
            scope: Scope::World,
            index: 5
        })
        .apply(&mut triggers)
        .is_err()
    );
}

#[test]
fn a_changed_rule_takes_its_new_query_and_grants() {
    let mut triggers = triggers();
    let player = RuleId {
        scope: Scope::Player,
        index: 0,
    };
    let unlocks = unlocks(
        "c",
        &[RuleGrant {
            id: KnowledgeId::new(21),
            grant: Grant::Set,
        }],
    )
    .unwrap();
    Edit::Change(player, Some(query(20)), Some(unlocks))
        .apply(&mut triggers)
        .unwrap();
    let rules = rules_of(&triggers);
    assert_eq!(rules[1].when.map(|q| q.id), Some(KnowledgeId::new(20)));
    assert_eq!(
        rules[1].grant,
        [RuleGrant {
            id: KnowledgeId::new(21),
            grant: Grant::Set
        }]
    );
}

#[test]
fn a_counter_grant_or_none_is_refused_since_a_rule_repeats_while_it_holds() {
    assert!(
        unlocks(
            "r",
            &[RuleGrant {
                id: KnowledgeId::new(1),
                grant: Grant::Add(1)
            }]
        )
        .is_err()
    );
    assert!(unlocks("r", &[]).is_err());
    assert!(
        unlocks(
            "r",
            &[RuleGrant {
                id: KnowledgeId::new(1),
                grant: Grant::Flag(32)
            }]
        )
        .is_err()
    );
    let op = unlock("r", KnowledgeId::new(4), Grant::Flag(3)).unwrap();
    assert_eq!(op.field("value"), Some(&Value::Uint(8)));
    assert_eq!(
        grant_of(&op),
        Some(RuleGrant {
            id: KnowledgeId::new(4),
            grant: Grant::Flag(3)
        })
    );
}
