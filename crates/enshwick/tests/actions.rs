//! Player actions: the sheet, runs from the interface and the chat form, as
//! the loader asks them.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{ActionCall, Header, PlayerInfo, Status};
    use enshwick::actions::sheet::{ArgView, decode};
    use enshwick::actions::{Action, Arg, Outcome, Subject, Value, Via};
    use enshwick::i18n::{Language, Texts};
    use enshwick::{Load, Mod, Off};

    /// The players each Safe lets in: owner key, then the allowed.
    static ALLOWED: Mutex<Vec<(u64, String)>> = Mutex::new(Vec::new());
    static RUNS: Mutex<Vec<(String, Via)>> = Mutex::new(Vec::new());

    const TEXTS: &str = r#"
[en]
"access" = "Who may use it"
"access.help" = "Let players open the Safe"
"safe" = "Your Safe"
"none" = "No Safe of yours here"
[uk]
"access" = "Хто може користуватися"
"safe" = "Твій сейф"
"#;

    struct Safes;

    impl Mod for Safes {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "safes", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let actions = load.actions("safe", Texts::parse(TEXTS)?)?;
            actions.add(
                Action::per_player("access", "access", |_ask, subject| {
                    assert_eq!(subject.map(|s| s.id), Some(42));
                    ALLOWED.lock().unwrap().clone()
                })
                .help("access.help")
                .verbs("allow", "deny")
                .at(|ask| {
                    if ask.player.entity.0 == 0 {
                        Err(ask.say("none", &[]))
                    } else {
                        Ok(Subject::new(42, ask.say("safe", &[])))
                    }
                })
                .hint(),
                |request| {
                    RUNS.lock().unwrap().push(("access".into(), request.via));
                    let [Value::Player { key, name }, Value::Toggle(on)] = request.args else {
                        return Outcome::refused("bad");
                    };
                    let mut allowed = ALLOWED.lock().unwrap();
                    allowed.retain(|(k, _)| k != key);
                    if *on {
                        allowed.push((*key, name.clone()));
                    }
                    Outcome::done(format!("{name} {on}"))
                },
            )?;
            actions.add(
                Action::new("wipe", "Wipe")
                    .admins()
                    .arg(Arg::choice(
                        "What",
                        &[("all", "Everything"), ("half", "Half")],
                    ))
                    .arg(Arg::number("Minutes", 1.0..=10.0, 1.0)),
                |request| match request.args {
                    [Value::Choice(what), Value::Number(minutes)] => {
                        Outcome::done(format!("wipe {what} in {minutes}"))
                    }
                    _ => Outcome::refused("bad"),
                },
            )?;
            assert!(
                actions
                    .add(Action::new("allow", "Taken"), |_| Outcome::done(""))
                    .is_err()
            );
            assert!(
                actions
                    .add(Action::new("x", "No subject").hint(), |_| Outcome::done(""))
                    .is_err()
            );
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Safes);

    fn player(entity: u32, key: u64, name: &str, admin: bool) -> PlayerInfo {
        let mut info = PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: 0,
            entity,
            key,
            permissions: if admin { PlayerInfo::CAN_KICK_BAN } else { 0 },
            name_len: u8::try_from(name.len()).unwrap(),
            name: [0; 64],
        };
        info.name[..name.len()].copy_from_slice(name.as_bytes());
        info
    }

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "one walk through the three faces of the same declarations"
    )]
    fn one_declaration_gives_a_sheet_runs_and_a_chat_command() {
        let owner = player(2, 1, "Owl", false);
        let ann = player(3, 7, "Ann", false);
        let admin = player(4, 9, "Root", true);
        let loader = enshwick::test::FakeHost::new()
            .player(owner)
            .player(ann)
            .player(admin)
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(loader.commands()[0].0, "safe");

        // The owner's sheet in Ukrainian: the list with the others, no
        // administrators' action.
        let text = loader
            .ask_actions(ActionCall::SHEET, &owner, Language::UK_UA.0, "", "")
            .unwrap();
        let sheet = decode(&text);
        assert_eq!(sheet.len(), 1, "{text}");
        let access = &sheet[0];
        assert_eq!(access.label, "Хто може користуватися");
        assert_eq!(access.help, "Let players open the Safe");
        assert!(access.per_player && access.hint && access.offered);
        assert_eq!(access.subject, "Твій сейф");
        let names: Vec<&str> = access.players.iter().map(|p| p.1.as_str()).collect();
        assert_eq!(names, ["Ann", "Root"]);

        // A flip from the interface, then the sheet shows it.
        let reply = loader
            .ask_actions(
                ActionCall::RUN,
                &owner,
                0,
                "access",
                "p0000000000000007\tb1",
            )
            .unwrap();
        assert_eq!(reply, "1Ann true");
        let sheet = decode(
            &loader
                .ask_actions(ActionCall::SHEET, &owner, 0, "", "")
                .unwrap(),
        );
        assert!(sheet[0].players.iter().any(|p| p.0 == 7 && p.2));

        // The chat form: verbs, the id with on and off, help.
        assert_eq!(
            loader.command("safe", &owner, "deny ann").as_deref(),
            Some("Ann false")
        );
        assert_eq!(
            loader.command("safe", &owner, "access Ann on").as_deref(),
            Some("Ann true")
        );
        assert_eq!(
            loader.command("safe", &owner, "allow Nobody").as_deref(),
            Some("No player named Nobody.")
        );
        let help = loader.command("safe", &owner, "help").unwrap();
        assert!(help.contains("/safe allow <player>"), "{help}");
        assert!(!help.contains("wipe"), "{help}");
        assert!(
            loader
                .command("safe", &owner, "fly")
                .unwrap()
                .starts_with("No action fly")
        );
        assert_eq!(
            *RUNS.lock().unwrap().last().unwrap(),
            ("access".to_owned(), Via::Chat)
        );

        // An administrator's action with typed arguments, refused to others.
        let wipe = decode(
            &loader
                .ask_actions(ActionCall::SHEET, &admin, 0, "", "")
                .unwrap(),
        );
        let args = &wipe.iter().find(|e| e.id == "wipe").unwrap().args;
        assert_eq!(args[0].kind, ArgView::CHOICE);
        assert_eq!(args[0].choices, ["Everything", "Half"]);
        assert_eq!(
            loader.command("safe", &admin, "wipe half 3").as_deref(),
            Some("wipe 1 in 3")
        );
        assert_eq!(
            loader.command("safe", &admin, "wipe half 30").as_deref(),
            Some("Use it so: /safe wipe <all|half> <1-10>: Wipe")
        );
        assert_eq!(
            loader
                .ask_actions(ActionCall::RUN, &ann, 0, "wipe", "c0\tn2")
                .as_deref(),
            Some("0You may not do that.")
        );
        assert_eq!(
            loader
                .ask_actions(ActionCall::RUN, &admin, 0, "wipe", "c0\tn2")
                .as_deref(),
            Some("1wipe 0 in 2")
        );

        // Hints: the subject is at hand for a player with a character.
        assert_eq!(
            loader
                .ask_actions(ActionCall::HINTS, &owner, 0, "", "")
                .as_deref(),
            Some("access\tWho may use it\tYour Safe\n")
        );
        let ghost = player(0, 5, "Ghost", false);
        assert_eq!(
            loader
                .ask_actions(
                    ActionCall::RUN,
                    &ghost,
                    0,
                    "access",
                    "p0000000000000007\tb1"
                )
                .as_deref(),
            Some("0No Safe of yours here")
        );
    }
}
