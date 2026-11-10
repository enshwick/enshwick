//! Action arguments past the typed choices: a line of free text, and
//! players who are offline but known to the mod, from the console, a
//! command and the interface.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{ActionCall, Header, PlayerInfo, Status};
    use enshwick::actions::sheet::{ArgView, decode};
    use enshwick::actions::{Action, Arg, Outcome, Value};
    use enshwick::i18n::{Language, Texts};
    use enshwick::{Load, Mod, Off, PlayerKey};

    /// The values each run handed its handler.
    static RUNS: Mutex<Vec<Vec<Value>>> = Mutex::new(Vec::new());

    /// The accounts the mod knows: Ann, who is also online in the tests,
    /// Bob, who is offline, and two offline accounts that share the name
    /// Sam.
    fn known() -> Vec<(PlayerKey, String)> {
        vec![
            (PlayerKey::new(7), "Ann".into()),
            (PlayerKey::new(42), "Bob".into()),
            (PlayerKey::new(43), "Sam".into()),
            (PlayerKey::new(44), "Sam".into()),
            (PlayerKey::new(45), String::new()),
        ]
    }

    struct Bans;

    impl Mod for Bans {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let actions = load.actions(Texts::default())?;
            let ran = |request: &enshwick::actions::Request<'_>| {
                RUNS.lock().unwrap().push(request.args.to_vec());
                Outcome::done("ok")
            };
            actions.add(
                Action::new("say", "Announce")
                    .admins()
                    .arg(Arg::text("Message", 24)),
                ran,
            )?;
            actions.add(
                Action::new("ban", "Ban")
                    .admins()
                    .arg(Arg::player("Player"))
                    .arg(Arg::text("Reason", 40))
                    .known(|_ask| known()),
                ran,
            )?;
            actions.add(
                Action::new("tag", "Tag")
                    .admins()
                    .arg(Arg::text("Tag", 8))
                    .arg(Arg::number("Hours", 1.0..=48.0, 1.0)),
                ran,
            )?;
            actions.add(
                Action::per_player("allowed", "Allowed", |_ask, _subject| {
                    vec![
                        (PlayerKey::new(43), "Sam".into()),
                        (PlayerKey::new(7), "Ann".into()),
                    ]
                })
                .admins()
                .known(|_ask| known()),
                ran,
            )?;
            actions.add(
                Action::per_player("shield", "Shield", |_ask, _subject| {
                    vec![(PlayerKey::new(9), "Root".into())]
                })
                .admins()
                .with_self(),
                ran,
            )?;
            actions.add(
                Action::new("heal", "Heal")
                    .admins()
                    .with_self()
                    .arg(Arg::player("Player")),
                ran,
            )?;
            let count = || Arg::number("Count", 1.0..=999.0, 1.0).optional(Value::Number(1.0));
            actions.add(
                Action::new("give", "Give")
                    .admins()
                    .arg(Arg::player("Player"))
                    .arg(Arg::text("Item", 40))
                    .arg(count())
                    .known(|_ask| known()),
                ran,
            )?;
            actions.add(
                Action::new("kit", "Kit")
                    .admins()
                    .arg(Arg::choice("Kind", &[("basic", "Basic"), ("pro", "Pro")]))
                    .arg(count())
                    .arg(Arg::toggle("Bound").optional(Value::Toggle(false))),
                ran,
            )?;
            actions.add(
                Action::new("warn", "Warn")
                    .admins()
                    .arg(Arg::player("Player"))
                    .arg(Arg::number("Minutes", 1.0..=60.0, 1.0).optional(Value::Number(10.0)))
                    .arg(Arg::text("Reason", 40).optional(Value::Text(String::new())))
                    .known(|_ask| known()),
                ran,
            )?;
            let mid = Action::new("mid", "Mid")
                .arg(Arg::text("Note", 8).optional(Value::Text(String::new())))
                .arg(count());
            assert!(
                actions.add(mid, ran).is_err(),
                "an optional text that is not the last argument"
            );
            let early = Action::new("early", "Early")
                .arg(count())
                .arg(Arg::text("Item", 40));
            let refused = actions.add(early, ran).unwrap_err();
            assert!(
                format!("{refused:?}").contains("only its last arguments"),
                "{refused:?}"
            );
            let far = Arg::number("Count", 1.0..=9.0, 1.0).optional(Value::Number(10.0));
            assert!(
                actions
                    .add(Action::new("far", "Far").arg(far), ran)
                    .is_err(),
                "a default out of the argument's range"
            );
            actions.commands("b")?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Bans,
        r#"
        id = "you.bans"
        side = "server"
        "#
    );

    fn player(entity: u32, key: u64, name: &str, permissions: u8) -> PlayerInfo {
        let mut info = PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: 0,
            entity: enshwick::Entity(entity),
            key: PlayerKey::new(key),
            permissions,
            name_len: u8::try_from(name.len()).unwrap(),
            name: [0; 64],
        };
        info.name[..name.len()].copy_from_slice(name.as_bytes());
        info
    }

    fn start(players: &[PlayerInfo]) -> enshwick::test::Installed {
        let host = players
            .iter()
            .fold(enshwick::test::FakeHost::new(), |host, p| host.player(*p));
        let loader = host.install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    fn runs() -> Vec<Vec<Value>> {
        std::mem::take(&mut *RUNS.lock().unwrap())
    }

    fn text(text: &str) -> Value {
        Value::Text(text.into())
    }

    fn bob() -> Value {
        Value::Player {
            key: PlayerKey::new(42),
            name: "Bob".into(),
        }
    }

    #[test]
    fn a_text_argument_takes_the_rest_of_the_line_or_one_quoted_run() {
        let root = player(4, 9, "Root", PlayerInfo::CAN_KICK_BAN);
        let loader = start(&[root]);
        let chat = |line: &str| loader.type_in_chat(&root, 0, line);

        assert_eq!(
            chat("/b say Restart in  5 minutes ").as_deref(),
            Some("1ok")
        );
        assert_eq!(chat(r#"/b say "Hi all""#).as_deref(), Some("1ok"));
        assert_eq!(chat(r#"/b tag "red fox" 3"#).as_deref(), Some("1ok"));
        assert_eq!(chat("/b tag fox 3").as_deref(), Some("1ok"));
        assert_eq!(
            runs(),
            [
                vec![text("Restart in  5 minutes")],
                vec![text("Hi all")],
                vec![text("red fox"), Value::Number(3.0)],
                vec![text("fox"), Value::Number(3.0)],
            ]
        );

        assert_eq!(
            chat("/b say This line runs well past twenty-four bytes").as_deref(),
            Some("0Message takes at most 24 bytes."),
        );
        assert_eq!(
            loader
                .type_in_chat(&root, Language::UK_UA.0, "/b tag much-too-long 3")
                .as_deref(),
            Some("0Tag: щонайбільше 8 байтів."),
        );
        assert!(
            chat("/b say")
                .unwrap()
                .starts_with("0Write it so: say <Message>")
        );
        assert!(chat("/b tag fox").unwrap().starts_with('0'), "no hours");
        assert!(runs().is_empty(), "nothing refused reached the handler");
    }

    #[test]
    fn a_text_before_optional_arguments_takes_the_line_up_to_the_words_they_read() {
        let root = player(4, 9, "Root", PlayerInfo::CAN_KICK_BAN);
        let loader = start(&[root]);
        let chat = |line: &str| loader.type_in_chat(&root, 0, line);

        for line in [
            "/b give Bob Stone Axe",
            "/b give Bob Stone Axe 5",
            "/b give Bob 5",
            r#"/b give Bob "Stone 5""#,
            r#"/b give Bob "Year 2000" 3"#,
            "/b give Bob Stone Axe 5000",
            "/b give Bob Stone  Axe  7 ",
        ] {
            assert_eq!(chat(line).as_deref(), Some("1ok"), "{line}");
        }
        let given = |item: &str, count: f64| vec![bob(), text(item), Value::Number(count)];
        assert_eq!(
            runs(),
            [
                given("Stone Axe", 1.0),
                given("Stone Axe", 5.0),
                given("5", 1.0),
                given("Stone 5", 1.0),
                given("Year 2000", 3.0),
                given("Stone Axe 5000", 1.0),
                given("Stone  Axe", 7.0),
            ],
            "a count out of its range stays in the text; quotes keep a number in it"
        );
        assert_eq!(
            chat("/b give Bob").as_deref(),
            Some("0Write it so: give <player> <Item> [1-999]: Give"),
            "an optional argument shows in square brackets"
        );
        assert!(runs().is_empty());
    }

    #[test]
    fn optional_arguments_after_one_word_arguments_fill_in_order() {
        let root = player(4, 9, "Root", PlayerInfo::CAN_KICK_BAN);
        let loader = start(&[root]);
        let chat = |line: &str| loader.type_in_chat(&root, 0, line);
        let kit = |count: f64, bound: bool| {
            vec![Value::Choice(1), Value::Number(count), Value::Toggle(bound)]
        };

        for line in ["/b kit pro", "/b kit pro 3", "/b kit pro 3 on"] {
            assert_eq!(chat(line).as_deref(), Some("1ok"), "{line}");
        }
        assert_eq!(runs(), [kit(1.0, false), kit(3.0, false), kit(3.0, true)]);
        for line in [
            "/b kit pro on",
            "/b kit pro 3 on more",
            "/b kit",
            "/b kit 3",
        ] {
            assert_eq!(
                chat(line).as_deref(),
                Some("0Write it so: kit <basic|pro> [1-999] [on|off]: Kit"),
                "{line}"
            );
        }

        let run = |args: &str| loader.ask_actions(ActionCall::RUN, &root, 0, "kit", args);
        assert_eq!(run("c0").as_deref(), Some("1ok"), "the interface too");
        assert_eq!(run("c0\tn4").as_deref(), Some("1ok"));
        assert_eq!(
            runs(),
            [
                vec![Value::Choice(0), Value::Number(1.0), Value::Toggle(false)],
                vec![Value::Choice(0), Value::Number(4.0), Value::Toggle(false)],
            ]
        );
        assert!(
            run("").unwrap().starts_with('0'),
            "the kind is not optional"
        );
    }

    #[test]
    fn an_action_offers_the_player_who_looks_only_when_it_asks_to() {
        let ann = player(3, 7, "Ann", 0);
        let root = player(4, 9, "Root", PlayerInfo::CAN_KICK_BAN);
        let loader = start(&[ann, root]);
        let sheet = decode(
            &loader
                .ask_actions(ActionCall::SHEET, &root, 0, "", "")
                .unwrap(),
        );
        let entry = |id: &str| sheet.iter().find(|e| e.id == id).unwrap();
        let viewer = PlayerKey::new(9);

        assert_eq!(
            entry("shield").players,
            [
                (viewer, "Root".to_owned(), true),
                (PlayerKey::new(7), "Ann".to_owned(), false),
            ],
            "the viewer first, switched as the mod says"
        );
        assert_eq!(
            entry("heal").args[0].players[..2],
            [
                (viewer, "Root".to_owned()),
                (PlayerKey::new(7), "Ann".to_owned())
            ]
        );
        assert!(
            entry("allowed")
                .players
                .iter()
                .all(|(key, _, _)| *key != viewer),
            "a list that did not ask holds only the others"
        );
        assert!(
            entry("ban").args[0]
                .players
                .iter()
                .all(|(key, _)| *key != viewer),
            "and so does a player argument"
        );

        let flip = loader.ask_actions(ActionCall::RUN, &root, 0, "shield", "p0000000000000009\tb0");
        assert_eq!(flip.as_deref(), Some("1ok"));
        let root_value = Value::Player {
            key: viewer,
            name: "Root".into(),
        };
        assert_eq!(runs(), [vec![root_value, Value::Toggle(false)]]);
    }

    #[test]
    fn a_last_optional_text_takes_what_is_left_and_never_a_word_of_the_argument_before() {
        let root = player(4, 9, "Root", PlayerInfo::CAN_KICK_BAN);
        let loader = start(&[root]);
        let chat = |line: &str| loader.type_in_chat(&root, 0, line);

        for line in [
            "/b warn Bob",
            "/b warn Bob 5",
            "/b warn Bob 5 spamming a lot",
            "/b warn Bob spamming 5",
            "/b warn Bob 99 times",
        ] {
            assert_eq!(chat(line).as_deref(), Some("1ok"), "{line}");
        }
        let warned = |minutes: f64, reason: &str| vec![bob(), Value::Number(minutes), text(reason)];
        assert_eq!(
            runs(),
            [
                warned(10.0, ""),
                warned(5.0, ""),
                warned(5.0, "spamming a lot"),
                warned(10.0, "spamming 5"),
                warned(10.0, "99 times"),
            ],
            "the player keeps its one word; the text starts where a word reads as no minutes"
        );
        assert_eq!(
            chat("/b warn Nobody 5 why").as_deref(),
            Some("0No player named Nobody."),
            "the player is the first word, never the line less its last word"
        );
        assert_eq!(
            chat("/b warn").as_deref(),
            Some("0Write it so: warn <player> [1-60] [Reason]: Warn")
        );
        let long = format!("/b warn Bob {}", "x".repeat(41));
        assert_eq!(
            chat(&long).as_deref(),
            Some("0Reason takes at most 40 bytes.")
        );
        assert!(runs().is_empty());
    }

    #[test]
    fn a_text_from_the_interface_is_bounded_and_refuses_control_characters() {
        let root = player(4, 9, "Root", PlayerInfo::CAN_KICK_BAN);
        let loader = start(&[root]);
        let run = |args: &str| loader.ask_actions(ActionCall::RUN, &root, 0, "say", args);

        assert_eq!(run("tHello there").as_deref(), Some("1ok"));
        assert_eq!(runs(), [vec![text("Hello there")]]);
        assert!(
            run(&format!("t{}", "x".repeat(25)))
                .unwrap()
                .starts_with('0')
        );
        assert!(run("tbell\u{7}").unwrap().starts_with('0'));
        assert!(run("t").unwrap().starts_with('0'), "an empty text");
        assert!(runs().is_empty());

        let sheet = decode(
            &loader
                .ask_actions(ActionCall::SHEET, &root, 0, "", "")
                .unwrap(),
        );
        let say = &sheet.iter().find(|e| e.id == "say").unwrap().args[0];
        assert_eq!(say.kind, ArgView::TEXT);
        assert_eq!((say.min, say.max), (1.0, 24.0));
    }

    #[test]
    fn an_offline_account_the_mod_knows_resolves_by_name_shows_as_a_row_and_runs_by_key() {
        let root = player(4, 9, "Root", PlayerInfo::CAN_KICK_BAN);
        let ann = player(3, 7, "Ann", 0);
        let loader = start(&[root, ann]);

        assert_eq!(
            loader
                .type_in_chat(&root, 0, "/b ban bob griefing")
                .as_deref(),
            Some("1ok")
        );
        assert_eq!(
            enshwick::console::run(r#"you.bans ban Ann "spam, twice""#).as_deref(),
            Ok("ok")
        );
        let ann_value = Value::Player {
            key: PlayerKey::new(7),
            name: "Ann".into(),
        };
        assert_eq!(
            runs(),
            [
                vec![bob(), text("griefing")],
                vec![ann_value, text("spam, twice")]
            ]
        );
        assert_eq!(
            loader.type_in_chat(&root, 0, "/b ban Sam x").as_deref(),
            Some("0Several players are named Sam.")
        );
        assert_eq!(
            loader.type_in_chat(&root, 0, "/b ban Zed x").as_deref(),
            Some("0No player named Zed.")
        );

        let sheet = decode(
            &loader
                .ask_actions(ActionCall::SHEET, &root, 0, "", "")
                .unwrap(),
        );
        let ban = &sheet.iter().find(|e| e.id == "ban").unwrap().args[0];
        let options: Vec<&str> = ban.players.iter().map(|p| p.1.as_str()).collect();
        assert_eq!(
            options,
            ["Ann", "Bob", "Sam", "Sam"],
            "online first, then known, an account that is both once"
        );
        let allowed = sheet.iter().find(|e| e.id == "allowed").unwrap();
        let rows: Vec<(&str, bool)> = allowed
            .players
            .iter()
            .map(|p| (p.1.as_str(), p.2))
            .collect();
        assert_eq!(
            rows,
            [("Ann", true), ("Sam", true), ("Bob", false), ("Sam", false)],
            "an online player the list holds shows once, switched on"
        );

        let run =
            |action: &str, args: &str| loader.ask_actions(ActionCall::RUN, &root, 0, action, args);
        assert_eq!(
            run("ban", "p000000000000002a\ttcheating").as_deref(),
            Some("1ok")
        );
        assert_eq!(
            run("ban", "tbob\ttcheating").as_deref(),
            Some("1ok"),
            "a typed name"
        );
        assert_eq!(
            run("allowed", "p000000000000002a\tb1").as_deref(),
            Some("1ok")
        );
        assert_eq!(
            run("allowed", "tbob\tb0").as_deref(),
            Some("1ok"),
            "a typed name on a list"
        );
        assert_eq!(
            runs(),
            [
                vec![bob(), text("cheating")],
                vec![bob(), text("cheating")],
                vec![bob(), Value::Toggle(true)],
                vec![bob(), Value::Toggle(false)],
            ]
        );
        assert!(
            loader
                .type_in_chat(&root, 0, "/b ban")
                .is_some_and(|reply| reply.starts_with("0Write it so: ban ")),
            "a line without its player gets the usage"
        );
        assert_eq!(
            run("ban", "p0000000000000063\ttx").as_deref(),
            Some("0That player is no longer here."),
            "no account key in the reply"
        );
    }

    #[test]
    fn a_player_who_may_not_run_an_action_learns_nothing_of_the_accounts_it_knows() {
        let ann = player(3, 7, "Ann", 0);
        let loader = start(&[ann]);
        let denied = Some("0You may not do that.");

        for line in [
            "/b ban Bob x",
            "/b ban Zed x",
            "/b ban Sam x",
            "/b allowed Bob on",
        ] {
            assert_eq!(
                loader.type_in_chat(&ann, 0, line).as_deref(),
                denied,
                "{line}"
            );
        }
        let run =
            |action: &str, args: &str| loader.ask_actions(ActionCall::RUN, &ann, 0, action, args);
        for args in [
            "p000000000000002a\ttx",
            "p0000000000000063\ttx",
            "tBob\ttx",
            "tZed\ttx",
        ] {
            assert_eq!(run("ban", args).as_deref(), denied, "{args}");
        }
        assert!(runs().is_empty());
    }
}
