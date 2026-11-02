use super::*;

#[test]
fn a_client_that_says_hello_gets_every_mod_s_join_values() {
    let values = std::collections::BTreeMap::from([
        ("you.trading-post".to_owned(), "{cost = 5}".to_owned()),
        ("zones".to_owned(), "{clusters = false}".to_owned()),
    ]);
    let messages = sync_messages(values);
    assert_eq!(messages.len(), 2);
    for message in &messages {
        assert_eq!(
            Message::read(message.kind(), &message.body()).as_ref(),
            Some(message)
        );
    }
    assert!(
        matches!(&messages[0], Message::Sync { id, values } if id == "you.trading-post" && values == "{cost = 5}")
    );
}
use enshwick::config::{Field, Stage};

fn schema() -> Schema {
    Schema::new(
        "",
        "config",
        vec![
            Field::new("limit", Stage::Runtime, &4),
            Field::new("token", Stage::Runtime, "x").with_secret(true),
        ],
    )
}

#[test]
fn messages_round_trip() {
    for message in [
        Message::Sync {
            id: "zones".into(),
            values: "{ a = 1 }".into(),
        },
        Message::Query {
            id: "you.trading-post".into(),
        },
        Message::Snapshot {
            id: "you.trading-post".into(),
            admin: true,
            schema: schema().to_text(),
            text: "limit = 4\n".into(),
        },
        Message::Edit {
            id: "you.trading-post".into(),
            key: "limit".into(),
            value: "6".into(),
        },
        Message::Result {
            ok: false,
            text: "Сейф".into(),
        },
        Message::Recipes {
            first: true,
            last: false,
            ids: vec![1, u32::MAX],
        },
        Message::Recipes {
            first: false,
            last: true,
            ids: Vec::new(),
        },
        Message::ActionsQuery { language: 34 },
        Message::ActionsSheet {
            first: true,
            last: false,
            id: "you.trading-post".into(),
            label: "Trading Post".into(),
            text: "A\taccess\t5\tWho\t\tYour stall\n".into(),
        },
        Message::ActionRun {
            id: "you.trading-post".into(),
            action: "access".into(),
            args: "p0000000000000007\tb1".into(),
        },
        Message::ActionResult {
            ok: true,
            text: "Ann may use it".into(),
        },
        Message::ActionHints {
            text: "you.trading-post\taccess\tWho\tYour stall\n".into(),
        },
    ] {
        assert_eq!(
            Message::read(message.kind(), &message.body()),
            Some(message)
        );
    }
    assert_eq!(Message::read(QUERY, &[5, 0, b'a']), None);
    assert_eq!(Message::read(42, &[]), None);
    assert_eq!(Message::read(RECIPES, &[3, 1, 2]), None);
    let most = Message::Recipes {
        first: true,
        last: true,
        ids: vec![7; 1400],
    };
    // The longest control message the frames carry.
    assert!(most.body().len() + 2 <= 720 * 8 - 2);
}

#[test]
fn secrets_never_leave_and_never_change() {
    let text = "# Limit.\nlimit = 4\ntoken = \"hunter2\"\n";
    let shown = redacted(&schema(), text);
    assert!(
        shown.contains("limit = 4") && !shown.contains("hunter2"),
        "{shown}"
    );
    let now = Instant::now();
    assert!(admit(&schema(), "limit", true, None, now).is_ok());
    assert!(admit(&schema(), "limit", false, None, now).is_err());
    assert!(admit(&schema(), "token", true, None, now).is_err());
    assert!(admit(&schema(), "limit", true, Some(now), now).is_err());
    assert_eq!(values_text("{ a = 1, b = \"x\" }"), "a = 1\nb = \"x\"\n");
}

#[test]
fn secrets_in_inline_tables_never_leave() {
    let schema = Schema::new(
        "",
        "config",
        vec![
            Field::new("auth.token", Stage::Runtime, "x").with_secret(true),
            Field::new("auth.user", Stage::Runtime, "x"),
            Field::new("outer.inner.key", Stage::Runtime, "x").with_secret(true),
            Field::new("outer.keep", Stage::Runtime, &1),
        ],
    );
    for text in [
        "auth = { token = \"hunter2\", user = \"ann\" }\nouter = { inner = { key = \"swordfish\" }, keep = 1 }\n",
        "[auth]\ntoken = \"hunter2\"\nuser = \"ann\"\n[outer]\ninner = { key = \"swordfish\" }\nkeep = 1\n",
        "auth.token = \"hunter2\"\nauth.user = \"ann\"\nouter.inner.key = \"swordfish\"\nouter.keep = 1\n",
    ] {
        let shown = redacted(&schema, text);
        assert!(
            !shown.contains("hunter2") && !shown.contains("swordfish"),
            "{shown}"
        );
        assert!(
            shown.contains("ann") && shown.contains("keep = 1"),
            "{shown}"
        );
    }
}

#[test]
fn a_player_who_is_no_administrator_asks_at_the_query_rate() {
    let mut queries = Queries::default();
    let start = Instant::now();
    let asked = (0..100).filter(|_| queries.admit(7, false, start)).count();
    assert_eq!(f64::from(u32::try_from(asked).unwrap()), QUERY_BURST);
    assert!(!queries.admit(7, false, start));
    assert!(
        queries.admit(8, false, start),
        "each player has a rate of his own"
    );
    let later = start + Duration::from_secs(1);
    let again = (0..100).filter(|_| queries.admit(7, false, later)).count();
    assert_eq!(f64::from(u32::try_from(again).unwrap()), QUERY_RATE);
}

#[test]
fn an_administrator_s_queries_are_not_rated() {
    let mut queries = Queries::default();
    let now = Instant::now();
    assert!((0..1000).all(|_| queries.admit(7, true, now)));
    assert!(queries.rates.is_empty());
}

#[test]
fn a_player_who_left_is_forgotten() {
    let mut queries = Queries::default();
    let now = Instant::now();
    while queries.admit(7, false, now) {}
    queries.forget(7);
    assert!(queries.rates.is_empty());
    assert!(
        queries.admit(7, false, now),
        "a new join starts with a full burst"
    );
}

#[test]
fn queries_and_edits_are_rated_and_answers_are_not() {
    let rated = [
        Message::Query { id: String::new() },
        Message::Edit {
            id: "zones".into(),
            key: "radius".into(),
            value: "5".into(),
        },
        Message::ActionsQuery { language: 0 },
    ];
    assert!(rated.iter().all(Message::is_rated));
    let free = [
        Message::Result {
            ok: true,
            text: String::new(),
        },
        Message::Sync {
            id: "zones".into(),
            values: String::new(),
        },
    ];
    assert!(!free.iter().any(Message::is_rated));
}

fn over_the_rate(queries: &mut Queries, key: u64, now: Instant) {
    while queries.admit(key, false, now) {}
}

#[test]
fn a_query_or_edit_over_the_rate_is_answered_busy_and_not_served() {
    let mut queries = Queries::default();
    let now = Instant::now();
    over_the_rate(&mut queries, 7, now);
    let busy = Gate::Busy(Message::Result {
        ok: false,
        text: texts::fill("remote.slow", &[]),
    });
    for message in [
        Message::Query { id: String::new() },
        Message::Edit {
            id: "zones".into(),
            key: "radius".into(),
            value: "5".into(),
        },
    ] {
        assert_eq!(queries.gate(7, false, &message, now), busy);
    }
}

#[test]
fn an_actions_query_over_the_rate_gets_no_answer_at_all() {
    let mut queries = Queries::default();
    let now = Instant::now();
    over_the_rate(&mut queries, 7, now);
    let message = Message::ActionsQuery { language: 0 };
    assert_eq!(queries.gate(7, false, &message, now), Gate::Drop);
}

#[test]
fn rated_messages_pass_until_the_burst_is_spent() {
    let mut queries = Queries::default();
    let now = Instant::now();
    let message = Message::Query { id: String::new() };
    let passed = (0..100)
        .take_while(|_| queries.gate(7, false, &message, now) == Gate::Pass)
        .count();
    assert_eq!(f64::from(u32::try_from(passed).unwrap()), QUERY_BURST);
}

#[test]
fn unrated_messages_and_administrators_always_pass() {
    let mut queries = Queries::default();
    let now = Instant::now();
    over_the_rate(&mut queries, 7, now);
    let run = Message::ActionRun {
        id: "zones".into(),
        action: "clear".into(),
        args: String::new(),
    };
    assert_eq!(queries.gate(7, false, &run, now), Gate::Pass);
    let query = Message::Query { id: String::new() };
    assert!((0..100).all(|_| queries.gate(8, true, &query, now) == Gate::Pass));
}
