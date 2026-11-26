//! Player actions: who may run them, how a window shows them, runs from the
//! interface and the console, as the loader asks them.

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Mutex, OnceLock};

    use enshwick::abi::{ActionCall, PlayerInfo, Status};
    use enshwick::actions::sheet::{ArgView, Entry};
    use enshwick::actions::{Action, Actions, Arg, Outcome, Subject, Value, Via, Window, window};
    use enshwick::console;
    use enshwick::i18n::{Language, Texts};
    use enshwick::{Load, Mod, Off};

    /// The players each locker lets in: owner key, then the allowed.
    static ALLOWED: Mutex<Vec<(enshwick::PlayerKey, String)>> = Mutex::new(Vec::new());
    static RUNS: Mutex<Vec<(String, Via)>> = Mutex::new(Vec::new());
    static ACTIONS: OnceLock<Actions> = OnceLock::new();
    /// How often the locker was looked for.
    static LOOKS: AtomicUsize = AtomicUsize::new(0);

    const TEXTS: &str = r#"
[en]
"access" = "Who may use it"
"access.help" = "Let players open the locker"
"locker" = "Your locker"
"none" = "No locker of yours here"
[uk]
"access" = "Хто може користуватися"
"locker" = "Твоя шафка"
"#;

    struct Lockers;

    impl Mod for Lockers {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let actions = load.actions(Texts::parse(TEXTS)?)?;
            actions.add(
                Action::per_player("access", "access", |_ask, subject| {
                    assert_eq!(subject.map(|s| s.id), Some(42));
                    ALLOWED.lock().unwrap().clone()
                })
                .help("access.help")
                .at(|ask| {
                    LOOKS.fetch_add(1, Ordering::Relaxed);
                    if ask.player.entity.get() == 0 {
                        Err(ask.say("none", &[]).into())
                    } else {
                        Ok(Subject::new(42, ask.say("locker", &[])))
                    }
                }),
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
                        RUNS.lock().unwrap().push(("wipe".into(), request.via));
                        Outcome::done(format!("wipe {what} in {minutes}"))
                    }
                    _ => Outcome::refused("bad"),
                },
            )?;
            assert!(
                actions
                    .add(Action::new("access", "Taken"), |_| Outcome::done(""))
                    .is_err()
            );
            let _first = ACTIONS.set(actions);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Lockers,
        r#"
        id = "you.lockers"
        side = "server"
        "#
    );

    fn player(entity: u32, key: u64, name: &str, admin: bool) -> PlayerInfo {
        let mut info = enshwick::abi::headed!(PlayerInfo {
            slot: 0,
            entity: enshwick::Entity::from_engine(entity),
            key: enshwick::PlayerKey::new(key),
            permissions: if admin { PlayerInfo::CAN_KICK_BAN } else { 0 },
            name_len: u32::try_from(name.len()).unwrap(),
            name: [0; PlayerInfo::NAME_MAX],
        });
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
        assert_eq!(status, Status::OK);
        loader
    }

    /// The entries a window naming the actions `ids` shows `viewer` in
    /// `language`.
    fn shown(
        loader: &enshwick::test::Installed,
        viewer: &PlayerInfo,
        language: Language,
        ids: &[&str],
    ) -> Vec<Entry> {
        let look = ids
            .iter()
            .fold(Window::new("look", "Look"), |w, id| w.action(id));
        ACTIONS.get().unwrap().show(look, &[viewer.key]).unwrap();
        let carried = loader.carried_windows(viewer, language.0);
        let (_, text) = carried.iter().find(|(id, _)| id == "look").unwrap();
        window::decode(text).unwrap().entries
    }

    #[test]
    fn who_may_run_what_is_answered_from_rights_alone_without_looking_for_a_subject() {
        let owner = player(2, 1, "Owl", false);
        let mut ann = player(3, 7, "Ann", false);
        let loader = start(&[owner, ann]);
        let allowed = |p: &PlayerInfo| loader.ask_actions(ActionCall::ALLOWED, p, 0, "", "");

        // At a join, then after a change of Ann's permissions.
        assert_eq!(allowed(&ann).as_deref(), Some("access\n"));
        ann.permissions = PlayerInfo::CAN_KICK_BAN;
        assert_eq!(allowed(&ann).as_deref(), Some("access\nwipe\n"));
        assert_eq!(allowed(&owner).as_deref(), Some("access\n"));
        assert_eq!(LOOKS.load(Ordering::Relaxed), 0, "no locker looked for");

        // A window that draws the action looks for it.
        assert_eq!(
            shown(&loader, &owner, Language::FALLBACK, &["access"]).len(),
            1
        );
        assert_eq!(LOOKS.load(Ordering::Relaxed), 1);
    }

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "one walk through the three faces of the same declarations"
    )]
    fn one_declaration_gives_a_window_entry_runs_and_a_console_form() {
        let owner = player(2, 1, "Owl", false);
        let ann = player(3, 7, "Ann", false);
        let admin = player(4, 9, "Root", true);
        let loader = start(&[owner, ann, admin]);

        // The owner's entry in Ukrainian: the list with the others.
        let sheet = shown(&loader, &owner, Language::UK_UA, &["access"]);
        assert_eq!(sheet.len(), 1, "{sheet:?}");
        let access = &sheet[0];
        assert_eq!(access.label, "Хто може користуватися");
        assert_eq!(access.help, "Let players open the locker");
        assert!(access.per_player && access.offered);
        assert_eq!(access.subject, "Твоя шафка");
        let names: Vec<&str> = access.players.iter().map(|p| p.1.as_str()).collect();
        assert_eq!(names, ["Ann", "Root"]);

        // A flip from the interface, then the window shows it.
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
        let sheet = shown(&loader, &owner, Language::FALLBACK, &["access"]);
        assert!(
            sheet[0]
                .players
                .iter()
                .any(|p| p.0 == enshwick::PlayerKey::new(7) && p.2)
        );

        // The console form: the console has no character, so no locker is at
        // hand; help lists every action; an unknown id is named.
        assert_eq!(
            console::run("you.lockers access Ann on").as_deref(),
            Ok("No locker of yours here")
        );
        let help = console::run("you.lockers help").unwrap();
        assert!(
            help.contains("access <player> <on|off>: Who may use it"),
            "{help}"
        );
        assert!(help.contains("wipe <all|half> <1-10>: Wipe"), "{help}");
        assert!(
            console::run("you.lockers fly")
                .unwrap()
                .starts_with("No action fly")
        );
        assert_eq!(
            loader.console(),
            [
                "you.lockers access Ann on",
                "you.lockers help",
                "you.lockers fly"
            ]
        );

        // An administrator's action with typed arguments, refused to others.
        let wipe = shown(&loader, &admin, Language::FALLBACK, &["wipe"]);
        let args = &wipe[0].args;
        assert_eq!(args[0].kind, ArgView::CHOICE);
        assert_eq!(args[0].choices, ["Everything", "Half"]);
        assert_eq!(
            console::run("you.lockers wipe half 3").as_deref(),
            Ok("wipe 1 in 3")
        );
        assert_eq!(
            *RUNS.lock().unwrap().last().unwrap(),
            ("wipe".to_owned(), Via::Console)
        );
        assert_eq!(
            console::run("you.lockers wipe half 30").as_deref(),
            Ok("Write it so: wipe <all|half> <1-10>: Wipe")
        );
        assert_eq!(
            console::run("you.lockers wipe 2 3").as_deref(),
            Ok("wipe 1 in 3"),
            "a choice by its number"
        );
        assert_eq!(
            console::run("you.lockers wipe 1 3").as_deref(),
            Ok("wipe 0 in 3")
        );
        assert_eq!(
            console::run("you.lockers wipe 3 3").as_deref(),
            Ok("Write it so: wipe <all|half> <1-10>: Wipe"),
            "a number past the choices"
        );
        assert_eq!(
            loader
                .ask_actions(ActionCall::CONSOLE, &owner, 0, "access Ann off", "")
                .as_deref(),
            Some("1Ann false")
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
            Some("0No locker of yours here")
        );
    }
}
