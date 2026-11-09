use super::*;

#[test]
fn a_client_that_says_hello_gets_every_mod_s_settings() {
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

#[test]
fn a_run_s_object_with_unknown_flags_or_an_endless_position_reads_as_none() {
    let sent_back = |flags: u32, x: f64| {
        let message = Message::ActionRun {
            id: "you.safes".into(),
            action: "switch".into(),
            args: String::new(),
            target: Some(ActionTarget {
                flags,
                position: [x, 0.0, 0.0],
                ..ActionTarget::NONE
            }),
        };
        match Message::read(message.kind(), &message.body()) {
            Some(Message::ActionRun { target, .. }) => target,
            _ => None,
        }
    };
    let finite = 1.5;
    assert!(sent_back(ActionTarget::POSITION, finite).is_some());
    assert_eq!(sent_back(ActionTarget::POSITION, f64::NAN), None);
    assert_eq!(sent_back(ActionTarget::POSITION, f64::INFINITY), None);
    assert_eq!(sent_back(8, finite), None);
    assert_eq!(sent_back(ActionTarget::ENTITY | 8, finite), None);
    assert_eq!(sent_back(0, finite), None);
    assert_eq!(
        sent_back(ActionTarget::ENTITY, f64::NAN).map(|t| t.position),
        Some([0.0; 3]),
        "a position the flags do not set is not read"
    );
}

#[test]
fn an_entity_only_run_with_junk_position_bytes_writes_back_to_its_own_message() {
    let mut body = Message::ActionRun {
        id: "a".into(),
        action: "b".into(),
        args: String::new(),
        target: Some(ActionTarget {
            flags: ActionTarget::ENTITY,
            entity: enshwick::Entity(7),
            ..ActionTarget::NONE
        }),
    }
    .body();
    let at = body.len() - 24;
    body[at..].fill(0xff);
    let kind = Message::ActionRun {
        id: String::new(),
        action: String::new(),
        args: String::new(),
        target: None,
    }
    .kind();
    let message = Message::read(kind, &body).expect("an entity-only run reads");
    assert_eq!(Message::read(kind, &message.body()), Some(message));
}
use enshwick::config::Field;

fn schema() -> Schema {
    Schema::new(
        "",
        "config",
        vec![
            Field::new("limit", &4),
            Field::new("token", "x").with_secret(true),
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
        Message::ActionsLanguage { language: 34 },
        Message::ActionRun {
            id: "you.trading-post".into(),
            action: "access".into(),
            args: "p0000000000000007\tb1".into(),
            target: None,
        },
        Message::ActionRun {
            id: "you.safes".into(),
            action: "switch".into(),
            args: String::new(),
            target: Some(ActionTarget {
                flags: ActionTarget::ENTITY | ActionTarget::POSITION,
                entity: enshwick::Entity(4096),
                position: [10.5, -2.0, 7.25],
                ..ActionTarget::NONE
            }),
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

fn horn() -> Carried {
    Carried {
        name: "horn".into(),
        label: "Сурма".into(),
        action: "honk".into(),
        keys: [0x23, 0],
        fires: 1,
        hold_ms: 500,
    }
}

fn controls(controls: Vec<Carried>) -> Message {
    Message::Controls {
        id: "you.cars".into(),
        name: "Cars".into(),
        controls,
    }
}

#[test]
fn a_server_s_controls_round_trip_up_to_their_bound() {
    let most = Message::Controls {
        id: format!("{}.{}", "a".repeat(31), "b".repeat(32)),
        name: "й".repeat(MAX_CARRIED_TEXT / 2),
        controls: (0..MAX_CARRIED)
            .map(|at| Carried {
                name: format!("{at:0>32}"),
                label: "й".repeat(MAX_CARRIED_TEXT / 2),
                action: "a".repeat(32),
                keys: [0x23, 0x24],
                ..horn()
            })
            .collect(),
    };
    assert!(most.body().len() + 2 <= crate::wire::MAX_CONTROL);
    for message in [controls(Vec::new()), controls(vec![horn()]), most] {
        assert_eq!(
            Message::read(message.kind(), &message.body()),
            Some(message)
        );
    }
}

#[test]
fn a_client_refuses_a_server_s_controls_out_of_bounds() {
    let refused = |message: Message| Message::read(CONTROLS, &message.body()).is_none();
    assert!(refused(controls(vec![horn(); MAX_CARRIED + 1])), "too many");
    let long = "x".repeat(MAX_CARRIED_TEXT + 1);
    assert!(refused(controls(vec![Carried {
        label: long.clone(),
        ..horn()
    }])));
    for id in [long.as_str(), "", "you.cars:horn", "cars", "You.Cars"] {
        assert!(
            refused(Message::Controls {
                id: id.into(),
                name: "Cars".into(),
                controls: Vec::new(),
            }),
            "{id:?} is no mod id"
        );
    }
    assert!(refused(controls(vec![horn(), horn()])), "a name twice");
    assert!(
        refused(Message::Controls {
            id: "you.cars".into(),
            name: "x".repeat(MAX_CARRIED_TEXT + 1),
            controls: Vec::new(),
        }),
        "a mod's name too long"
    );
    for bad in [
        Carried {
            name: "Horn".into(),
            ..horn()
        },
        Carried {
            action: String::new(),
            ..horn()
        },
        Carried {
            keys: [0x01, 0],
            ..horn()
        },
        Carried { fires: 0, ..horn() },
        Carried { fires: 8, ..horn() },
    ] {
        assert!(refused(controls(vec![bad.clone()])), "{bad:?}");
    }
    let mut cut_short = controls(vec![horn()]).body();
    cut_short.pop();
    assert_eq!(Message::read(CONTROLS, &cut_short), None);
}

#[test]
fn a_server_s_windows_round_trip_and_a_client_refuses_them_out_of_bounds() {
    let window = |id: &str, window: &str, text: String| Message::Window {
        id: id.into(),
        window: window.into(),
        open: true,
        text,
    };
    let biggest = "x".repeat(enshwick::actions::window::MAX_BYTES);
    for message in [
        window("you.admin", "panel", "W\tAdmin\nE\tsay\n".into()),
        Message::Window {
            id: "you.admin".into(),
            window: "panel".into(),
            open: false,
            text: "W\tAdmin\n".into(),
        },
        window("you.admin", "panel", biggest),
        Message::WindowClose {
            id: "you.admin".into(),
            window: "bans".into(),
        },
    ] {
        assert_eq!(
            Message::read(message.kind(), &message.body()),
            Some(message)
        );
    }
    let refused = |message: Message| Message::read(message.kind(), &message.body()).is_none();
    let past = "x".repeat(enshwick::actions::window::MAX_BYTES + 1);
    assert!(
        refused(window("you.admin", "panel", past)),
        "past the bytes"
    );
    assert!(
        refused(window("You.Admin", "panel", "W\tt\n".into())),
        "no mod id"
    );
    assert!(
        refused(window("you.admin", "Panel One", "W\tt\n".into())),
        "no window id"
    );
    assert!(refused(Message::WindowClose {
        id: "you.admin".into(),
        window: String::new(),
    }));
    let mut flag = window("you.admin", "panel", "W\tt\n".into()).body();
    let at = 2 + "you.admin".len() + 2 + "panel".len();
    flag[at] = 2;
    assert_eq!(Message::read(WINDOW, &flag), None, "an open flag past 1");
}

#[test]
fn a_mod_s_carried_controls_replace_its_earlier_ones_and_the_mods_are_bounded() {
    let lights = Carried {
        name: "lights".into(),
        ..horn()
    };
    let kept = carried_after(&[], "you.cars", "Cars", &[horn(), lights.clone()]);
    let kept = carried_after(&kept, "you.boats", "Boats", &[horn()]);
    let kept = carried_after(&kept, "you.cars", "Autos", &[lights]);
    let ids: Vec<String> = kept.iter().map(Heard::id).collect();
    assert_eq!(ids, ["you.cars:lights", "you.boats:horn"], "in its place");
    assert_eq!(kept[0].mod_name, "Autos");
    assert_eq!(
        carried_after(&kept, "you.cars", "Autos", &[kept[0].control.clone()]),
        kept,
        "sent again, nothing moves"
    );
    let mut full = Vec::new();
    for at in 0..MAX_CARRIED_MODS {
        full = carried_after(&full, &format!("mod{at}"), "M", &[horn()]);
    }
    assert_eq!(carried_after(&full, "one-more", "M", &[horn()]), full);
    let replaced = carried_after(&full, "mod3", "M", &[]);
    assert_eq!(
        replaced.len(),
        MAX_CARRIED_MODS - 1,
        "a known mod still changes"
    );
}

#[test]
fn a_text_is_cut_at_a_character_s_edge() {
    assert_eq!(cut("Сурма", 3), "С");
    assert_eq!(cut("horn", 10), "horn");
    assert_eq!(cut("horn", 0), "");
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
}

#[test]
fn secrets_in_inline_tables_never_leave() {
    let schema = Schema::new(
        "",
        "config",
        vec![
            Field::new("auth.token", "x").with_secret(true),
            Field::new("auth.user", "x"),
            Field::new("outer.inner.key", "x").with_secret(true),
            Field::new("outer.keep", &1),
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
        Message::ActionsLanguage { language: 0 },
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
fn a_language_message_over_the_rate_gets_no_answer_at_all() {
    let mut queries = Queries::default();
    let now = Instant::now();
    over_the_rate(&mut queries, 7, now);
    let message = Message::ActionsLanguage { language: 0 };
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
fn action_runs_have_a_rate_of_their_own_and_over_it_are_busy_and_not_run() {
    let mut queries = Queries::default();
    let now = Instant::now();
    over_the_rate(&mut queries, 7, now);
    let run = Message::ActionRun {
        id: "zones".into(),
        action: "clear".into(),
        args: String::new(),
        target: None,
    };
    let passed = (0..100)
        .take_while(|_| queries.gate(7, false, &run, now) == Gate::Pass)
        .count();
    assert_eq!(
        f64::from(u32::try_from(passed).unwrap()),
        RUN_BURST,
        "spent queries leave the runs alone"
    );
    assert!(matches!(
        queries.gate(7, false, &run, now),
        Gate::Busy(Message::Result { ok: false, .. })
    ));
    let query = Message::Query { id: String::new() };
    let mut fresh = Queries::default();
    while fresh.gate(8, false, &run, now) == Gate::Pass {}
    assert_eq!(fresh.gate(8, false, &query, now), Gate::Pass);
}

#[test]
fn unrated_messages_and_administrators_always_pass() {
    let mut queries = Queries::default();
    let now = Instant::now();
    over_the_rate(&mut queries, 7, now);
    let sync = Message::Sync {
        id: "zones".into(),
        values: String::new(),
    };
    assert_eq!(queries.gate(7, false, &sync, now), Gate::Pass);
    let query = Message::Query { id: String::new() };
    assert!((0..100).all(|_| queries.gate(8, true, &query, now) == Gate::Pass));
}
