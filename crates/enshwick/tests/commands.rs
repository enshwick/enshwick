//! Commands: a mod's actions named `/admin-tools` and `/adm`, run from chat,
//! for a remote caller and listed, with the rights of who runs them.

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};
    use std::time::Duration;

    use enshwick::abi::{ActionCall, Header, PlayerInfo, Status};
    use enshwick::actions::sheet::decode;
    use enshwick::actions::{Action, Actions, Arg, Outcome, Value, Via};
    use enshwick::commands::{self, Caller, CommandSet, MAX_LINE, RunCommandError};
    use enshwick::i18n::{Language, Texts};
    use enshwick::schedule::Scheduler;
    use enshwick::world::PlayerPermissions;
    use enshwick::{Load, Mod, Off};

    /// Each run: the action, where it came from and who ran it.
    static RUNS: Mutex<Vec<(String, Via, String)>> = Mutex::new(Vec::new());
    static ACTIONS: OnceLock<Actions> = OnceLock::new();
    static SCHEDULER: OnceLock<Scheduler> = OnceLock::new();

    struct AdminTools;

    fn ran(action: &str, request: &enshwick::actions::Request<'_>) {
        RUNS.lock()
            .unwrap()
            .push((action.into(), request.via, request.player.name.clone()));
    }

    impl Mod for AdminTools {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = SCHEDULER.set(load.scheduler()?);
            let actions = load.actions(Texts::default())?;
            actions.add(Action::new("online", "Players online"), |request| {
                ran("online", request);
                Outcome::done("2 online")
            })?;
            actions.add(
                Action::new("kick", "Kick a player")
                    .admins()
                    .arg(Arg::player("Player")),
                |request| {
                    ran("kick", request);
                    match request.args {
                        [Value::Player { name, .. }] => Outcome::done(format!("kicked {name}")),
                        _ => Outcome::refused("bad"),
                    }
                },
            )?;
            actions.add(
                Action::new("repair", "Repair the base").needs(PlayerPermissions::CanEditBase),
                |request| {
                    ran("repair", request);
                    Outcome::done("repaired")
                },
            )?;
            actions.add(Action::new("boom", "Break"), |request| {
                assert!(
                    !matches!(request.via, Via::Chat | Via::Console),
                    "a handler that panics in a typed line"
                );
                Outcome::done("fine")
            })?;
            assert!(
                actions.commands("ad\tm").is_err(),
                "a tab would break a list of names"
            );
            assert!(actions.commands("Adm").is_err(), "not a plain word");
            assert!(
                actions.commands("admin-tools-x").is_err(),
                "an alias longer than the name in the mod's id"
            );
            assert!(actions.commands("help").is_err(), "a reserved word");
            actions.commands("adm")?;
            assert!(actions.commands("t").is_err(), "named once already");
            let _first = ACTIONS.set(actions);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        AdminTools,
        r#"
        id = "you.admin-tools"
        side = "server"
        "#
    );

    fn player(entity: u32, key: u64, name: &str, permissions: u8) -> PlayerInfo {
        let mut info = PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: 0,
            entity: enshwick::Entity(entity),
            key: enshwick::PlayerKey::new(key),
            permissions,
            name_len: u8::try_from(name.len()).unwrap(),
            name: [0; 64],
        };
        info.name[..name.len()].copy_from_slice(name.as_bytes());
        info
    }

    fn start(host: enshwick::test::FakeHost) -> enshwick::test::Installed {
        let loader = host.install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        RUNS.lock().unwrap().clear();
        loader
    }

    /// Each set's name, alias (empty for none) and mod id.
    fn names(list: Vec<CommandSet>) -> Vec<(String, String, String)> {
        list.into_iter()
            .map(|c| (c.name, c.alias.unwrap_or_default(), c.mod_id))
            .collect()
    }

    fn runs() -> Vec<(String, Via, String)> {
        std::mem::take(&mut *RUNS.lock().unwrap())
    }

    #[test]
    fn a_player_runs_the_mod_s_actions_from_chat_by_name_or_alias_with_their_own_rights() {
        let ann = player(3, 7, "Ann", 0);
        let root = player(4, 9, "Root", PlayerInfo::CAN_KICK_BAN);
        let loader = start(enshwick::test::FakeHost::new().player(ann).player(root));

        assert_eq!(
            loader.type_in_chat(&ann, 0, "/adm online").as_deref(),
            Some("12 online")
        );
        assert_eq!(
            loader
                .type_in_chat(&ann, 0, "/ADMIN-TOOLS  online")
                .as_deref(),
            Some("12 online")
        );
        assert_eq!(
            runs(),
            [
                ("online".into(), Via::Chat, "Ann".into()),
                ("online".into(), Via::Chat, "Ann".into()),
            ]
        );

        let refused = loader.type_in_chat(&ann, 0, "/adm kick Root").unwrap();
        assert!(refused.starts_with('0'), "{refused}");
        let uk = Language::UK_UA.0;
        assert_eq!(
            loader.type_in_chat(&ann, uk, "/adm kick Root").as_deref(),
            Some("0Тобі цього не можна."),
            "the refusal in the player's language"
        );
        assert_eq!(
            loader.type_in_chat(&root, 0, "/adm kick ann").as_deref(),
            Some("1kicked Ann"),
            "a player's name matches a player online"
        );
        assert!(
            loader
                .type_in_chat(&root, 0, "/adm kick Nobody")
                .unwrap()
                .starts_with('0')
        );
        assert!(
            loader
                .type_in_chat(&ann, 0, "/adm dance")
                .unwrap()
                .starts_with('0')
        );

        let help = loader.type_in_chat(&ann, 0, "/adm help").unwrap();
        assert!(help.contains("online") && !help.contains("kick"), "{help}");
        let help = loader.type_in_chat(&root, 0, "/adm").unwrap();
        assert!(help.contains("kick"), "{help}");

        assert_eq!(
            loader.type_in_chat(&ann, 0, "/me waves"),
            None,
            "another command stays in chat"
        );
        assert_eq!(
            loader.type_in_chat(&ann, 0, "adm online"),
            None,
            "no slash, plain chat"
        );
    }

    #[test]
    fn an_action_that_needs_a_permission_is_listed_and_run_only_for_players_holding_it() {
        let guest = player(3, 7, "Guest", 0);
        let friend = player(
            4,
            8,
            "Friend",
            PlayerInfo::CAN_EDIT_BASE | PlayerInfo::CAN_ACCESS_INVENTORIES,
        );
        let loader = start(enshwick::test::FakeHost::new().player(guest).player(friend));
        let ids = |p: &PlayerInfo| -> Vec<String> {
            decode(&loader.ask_actions(ActionCall::SHEET, p, 0, "", "").unwrap())
                .into_iter()
                .map(|entry| entry.id)
                .collect()
        };

        assert_eq!(ids(&guest), ["online", "boom"]);
        assert_eq!(ids(&friend), ["online", "repair", "boom"]);
        assert!(
            loader
                .type_in_chat(&guest, 0, "/adm repair")
                .unwrap()
                .starts_with('0')
        );
        assert_eq!(
            loader.type_in_chat(&friend, 0, "/adm repair").as_deref(),
            Some("1repaired")
        );
        let reply = loader
            .ask_actions(ActionCall::RUN, &guest, 0, "repair", "")
            .unwrap();
        assert!(
            reply.starts_with('0'),
            "the interface is checked the same: {reply}"
        );

        assert_eq!(loader.rules_changed(), 0);
        ACTIONS.get().unwrap().rules_changed().unwrap();
        assert_eq!(loader.rules_changed(), 1);
    }

    #[test]
    fn another_mod_runs_a_command_line_for_a_remote_caller_with_the_permissions_it_gives() {
        let ann = player(3, 7, "Ann", 0);
        let _loader = start(
            enshwick::test::FakeHost::new()
                .player(ann)
                .other_mod_commands("you.weather", "wx")
                .console_answer("wx", "rain"),
        );
        let rcon = Caller::new("rcon 10.0.0.2");

        let online = commands::run("/adm online", &rcon).unwrap();
        assert!(online.is_done());
        assert_eq!(online.text(), "2 online");
        assert_eq!(
            runs(),
            [("online".into(), Via::Remote, "rcon 10.0.0.2".into())]
        );
        assert!(!commands::run("adm kick Ann", &rcon).unwrap().is_done());
        let admin = rcon.clone().with(PlayerPermissions::CanKickBan);
        assert_eq!(
            commands::run("adm kick Ann", &admin).unwrap().text(),
            "kicked Ann"
        );
        assert!(
            commands::run("/adm repair", &Caller::new("owner").with_all())
                .unwrap()
                .is_done()
        );

        assert_eq!(
            commands::run("/wx", &rcon).unwrap().text(),
            "rain",
            "another mod's command"
        );
        assert_eq!(
            commands::run("/nobody x", &rcon),
            Err(RunCommandError::Unknown)
        );
        assert_eq!(commands::run(" ", &rcon), Err(RunCommandError::Empty));
        let longest = format!("/adm online{}", " ".repeat(MAX_LINE - 11));
        assert_eq!(longest.len(), MAX_LINE);
        assert!(
            commands::run(&longest, &rcon).unwrap().is_done(),
            "the longest line runs"
        );
        let long = format!("/adm online {}", "x".repeat(MAX_LINE));
        assert_eq!(commands::run(&long, &rcon), Err(RunCommandError::TooLong));

        assert_eq!(
            names(commands::list()),
            [
                ("weather".into(), "wx".into(), "you.weather".into()),
                ("admin-tools".into(), "adm".into(), "you.admin-tools".into()),
            ],
            "in load order"
        );
    }

    #[test]
    fn a_line_from_another_thread_runs_on_the_game_thread_and_answers_there() {
        let loader = start(enshwick::test::FakeHost::new());
        let (tx, rx) = std::sync::mpsc::channel();
        let scheduler = *SCHEDULER.get().unwrap();
        std::thread::spawn(move || {
            commands::run_later(
                scheduler,
                "/adm online".into(),
                Caller::new("rcon"),
                move |reply| {
                    tx.send(reply.map(|outcome| outcome.text().to_owned()))
                        .unwrap();
                },
            );
        })
        .join()
        .unwrap();
        assert!(
            rx.try_recv().is_err(),
            "nothing runs before the game thread's pass"
        );
        let _ran = loader.tick(Duration::from_millis(100));
        assert_eq!(rx.try_recv().unwrap(), Ok("2 online".to_owned()));
    }

    #[test]
    fn an_alias_a_mod_loaded_before_holds_leaves_the_mod_its_full_name() {
        let ann = player(3, 7, "Ann", 0);
        let loader = start(
            enshwick::test::FakeHost::new()
                .player(ann)
                .other_mod_commands("you.lists", "adm")
                .console_answer("adm", "lists"),
        );
        assert_eq!(
            loader
                .type_in_chat(&ann, 0, "/admin-tools online")
                .as_deref(),
            Some("12 online"),
            "the full name always works"
        );
        assert_eq!(
            loader.type_in_chat(&ann, 0, "/adm online"),
            None,
            "the alias is the first mod's"
        );
        assert_eq!(
            commands::run("/adm", &Caller::new("rcon")).unwrap().text(),
            "lists"
        );
        assert_eq!(
            names(commands::list()),
            [
                ("lists".into(), "adm".into(), "you.lists".into()),
                (
                    "admin-tools".into(),
                    String::new(),
                    "you.admin-tools".into()
                ),
            ]
        );
    }

    #[test]
    fn an_alias_the_mod_holds_leaves_a_mod_loaded_after_its_full_name() {
        let ann = player(3, 7, "Ann", 0);
        let loader = start(
            enshwick::test::FakeHost::new()
                .player(ann)
                .later_mod_commands("you.lists", "adm")
                .console_answer("lists", "lists"),
        );
        assert_eq!(
            loader.type_in_chat(&ann, 0, "/adm online").as_deref(),
            Some("12 online")
        );
        assert_eq!(
            commands::run("/lists", &Caller::new("rcon"))
                .unwrap()
                .text(),
            "lists"
        );
        assert_eq!(
            names(commands::list()),
            [
                ("admin-tools".into(), "adm".into(), "you.admin-tools".into()),
                ("lists".into(), String::new(), "you.lists".into()),
            ]
        );
    }

    #[test]
    fn an_alias_that_is_another_mod_s_name_stays_that_mod_s() {
        let ann = player(3, 7, "Ann", 0);
        let loader = start(
            enshwick::test::FakeHost::new()
                .player(ann)
                .later_mod_commands("they.adm", "a"),
        );
        assert_eq!(loader.type_in_chat(&ann, 0, "/adm online"), None);
        assert!(
            loader
                .type_in_chat(&ann, 0, "/admin-tools online")
                .is_some()
        );
    }

    #[test]
    fn a_mod_whose_name_another_mod_s_commands_have_runs_on_without_commands() {
        let ann = player(3, 7, "Ann", 0);
        let loader = start(
            enshwick::test::FakeHost::new()
                .player(ann)
                .other_mod_commands("they.admin-tools", "at"),
        );
        assert_eq!(loader.type_in_chat(&ann, 0, "/admin-tools online"), None);
        assert_eq!(loader.type_in_chat(&ann, 0, "/adm online"), None);
        let reply = loader
            .ask_actions(ActionCall::RUN, &ann, 0, "online", "")
            .unwrap();
        assert_eq!(reply, "12 online", "the action keeps its other paths");
    }

    #[test]
    fn a_handler_that_panics_in_a_command_turns_off_only_the_mod_s_commands() {
        let ann = player(3, 7, "Ann", 0);
        let loader = start(enshwick::test::FakeHost::new().player(ann));
        assert_eq!(
            loader.type_in_chat(&ann, 0, "/adm boom").as_deref(),
            Some("0")
        );
        assert_eq!(
            loader.type_in_chat(&ann, 0, "/adm online").as_deref(),
            Some("0This mod's commands are off after an error."),
            "the mod's commands are off: refused, saying why"
        );
        assert_eq!(
            commands::run("/adm online", &Caller::new("rcon")),
            Ok(Outcome::refused(
                "This mod's commands are off after an error."
            )),
            "for remote callers too"
        );
        let reply = loader
            .ask_actions(ActionCall::RUN, &ann, 0, "boom", "")
            .unwrap();
        assert_eq!(reply, "1fine", "the action keeps its other paths");
        assert!(
            loader
                .logs()
                .iter()
                .any(|(_, line)| line.contains("commands off")),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn a_handler_that_panics_on_the_console_turns_off_only_the_mod_s_commands() {
        let ann = player(3, 7, "Ann", 0);
        let console = player(0, 0, "console", 0x3f);
        let loader = start(enshwick::test::FakeHost::new().player(ann));
        assert_eq!(
            loader
                .ask_actions(ActionCall::CONSOLE, &console, 0, "boom", "")
                .as_deref(),
            Some("0")
        );
        let reply = loader
            .ask_actions(ActionCall::RUN, &ann, 0, "boom", "")
            .unwrap();
        assert_eq!(reply, "1fine", "the mod runs on");
        assert_eq!(
            loader.type_in_chat(&ann, 0, "/adm online").as_deref(),
            Some("0This mod's commands are off after an error.")
        );
    }

    #[test]
    fn a_player_has_exactly_the_permissions_their_bits_hold() {
        let friend = enshwick::world::Player {
            entity: enshwick::Entity(2),
            slot: 0,
            key: enshwick::PlayerKey::new(7),
            permissions: PlayerInfo::CAN_EDIT_BASE | PlayerInfo::CAN_RECEIVE_EXP,
            name: "Ann".into(),
        };
        let held: Vec<PlayerPermissions> = PlayerPermissions::ALL
            .into_iter()
            .filter(|p| friend.has(*p))
            .collect();
        assert_eq!(
            held,
            [
                PlayerPermissions::CanEditBase,
                PlayerPermissions::CanReceiveExp
            ]
        );
    }
}
