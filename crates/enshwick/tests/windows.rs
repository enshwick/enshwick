//! Carried windows: a mod of side server shows windows on players'
//! clients, each rendered in the reader's language with only what they may
//! run, and updates and closes them.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::{ActionCall, Header, PlayerInfo, Side, Status};
    use enshwick::actions::sheet::ArgView;
    use enshwick::actions::window::{ButtonView, ItemView, decode};
    use enshwick::actions::{Action, Actions, Arg, Outcome, Row, Value, Window};
    use enshwick::i18n::Language;
    use enshwick::{Load, Mod, Off, PlayerKey};

    static ACTIONS: OnceLock<Actions> = OnceLock::new();

    const TEXTS: &str = r#"
[en]
"panel" = "Admin panel"
"online" = "{count} online"

[uk]
"panel" = "Панель адміністратора"
"online" = "{count} у грі"
"#;

    fn ann() -> Value {
        Value::Player {
            key: PlayerKey::new(7),
            name: "Ann".into(),
        }
    }

    fn panel() -> Window {
        Window::new("panel", "panel")
            .text("online", &[("count", "2")])
            .row(Row::new("Ann", &[]).button("Ban", "ban", &[ann()]))
            .action("say")
    }

    struct Admin;

    impl Mod for Admin {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let texts = load.texts_from(TEXTS)?;
            let actions = load.actions(texts)?;
            let done = |_: &enshwick::actions::Request<'_>| Outcome::done("ok");
            actions.add(
                Action::new("ban", "Ban")
                    .admins()
                    .arg(Arg::player("Player"))
                    .arg(Arg::text("Reason", 64)),
                done,
            )?;
            actions.add(
                Action::new("say", "Announce")
                    .admins()
                    .arg(Arg::text("Message", 120)),
                done,
            )?;
            actions.add(Action::new("wave", "Wave"), done)?;
            let shown = actions.clone();
            actions.add(
                Action::new("open", "Open the panel"),
                move |request| match shown.show(panel(), &[request.player.key]) {
                    Ok(()) => Outcome::done(""),
                    Err(why) => Outcome::refused(why.to_string()),
                },
            )?;
            let _first = ACTIONS.set(actions);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Admin,
        r#"
        id = "you.admin"
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

    fn start(host: enshwick::test::FakeHost) -> enshwick::test::Installed {
        let loader = host.install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    fn actions() -> &'static Actions {
        ACTIONS.get().unwrap()
    }

    #[test]
    fn an_admin_gets_the_window_in_their_language_and_a_player_gets_nothing() {
        let root = player(4, 9, "Root", PlayerInfo::CAN_KICK_BAN);
        let bob = player(5, 8, "Bob", 0);
        let loader = start(enshwick::test::FakeHost::new().player(root).player(bob));

        assert_eq!(
            loader
                .ask_actions(ActionCall::RUN, &root, 0, "open", "")
                .as_deref(),
            Some("1")
        );
        let carried = loader.carried_windows(&root, Language::UK_UA.0);
        assert_eq!(carried.len(), 1);
        assert_eq!(carried[0].0, "panel");
        let view = decode(&carried[0].1).unwrap();
        assert_eq!(view.title, "Панель адміністратора");
        assert_eq!(
            view.items,
            [
                ItemView::Text("2 у грі".into()),
                ItemView::Row {
                    text: "Ann".into(),
                    buttons: vec![ButtonView {
                        label: "Ban".into(),
                        action: "ban".into(),
                        args: vec![Value::Player {
                            key: PlayerKey::new(7),
                            name: String::new()
                        }],
                    }],
                },
                ItemView::Action("say".into()),
            ]
        );
        let ids: Vec<&str> = view.entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["ban", "say"], "the entries of the actions it names");
        assert_eq!(view.entries[0].args[1].kind, ArgView::TEXT);

        assert_eq!(
            loader
                .ask_actions(ActionCall::RUN, &bob, 0, "open", "")
                .as_deref(),
            Some("1")
        );
        assert!(
            loader.carried_windows(&bob, 0).is_empty(),
            "Bob may not run the actions the window names"
        );
    }

    #[test]
    fn a_window_shown_again_updates_in_place_and_a_close_takes_it_away() {
        let root = player(4, 9, "Root", PlayerInfo::CAN_KICK_BAN);
        let loader = start(enshwick::test::FakeHost::new().player(root));
        let actions = actions();

        actions.show(panel(), &[root.key]).unwrap();
        actions
            .show(Window::new("note", "Note").text("Hello", &[]), &[root.key])
            .unwrap();
        actions
            .show(Window::new("panel", "Second").action("wave"), &[root.key])
            .unwrap();
        let carried = loader.carried_windows(&root, 0);
        let ids: Vec<&str> = carried.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, ["panel", "note"], "the update keeps its place");
        assert_eq!(decode(&carried[0].1).unwrap().title, "Second");

        actions.close("panel", &[root.key]).unwrap();
        let carried = loader.carried_windows(&root, 0);
        assert_eq!(carried.len(), 1);
        assert_eq!(carried[0].0, "note");
    }

    #[test]
    fn a_window_shown_to_one_player_leaves_another_players_window_of_that_id_alone() {
        let root = player(4, 9, "Root", PlayerInfo::CAN_KICK_BAN);
        let eve = player(5, 8, "Eve", PlayerInfo::CAN_KICK_BAN);
        let loader = start(enshwick::test::FakeHost::new().player(root).player(eve));
        let actions = actions();
        let title = |of: &PlayerInfo| {
            let carried = loader.carried_windows(of, 0);
            assert_eq!(carried.len(), 1, "{carried:?}");
            decode(&carried[0].1).unwrap().title
        };

        actions
            .show(Window::new("mine", "Root's").action("wave"), &[root.key])
            .unwrap();
        actions
            .show(Window::new("mine", "Eve's").action("wave"), &[eve.key])
            .unwrap();
        assert_eq!(
            title(&root),
            "Root's",
            "each player keeps the one shown to them"
        );
        assert_eq!(title(&eve), "Eve's");

        actions.close("mine", &[eve.key]).unwrap();
        assert!(loader.carried_windows(&eve, 0).is_empty());
        actions
            .show(
                Window::new("mine", "Both").action("wave"),
                &[root.key, eve.key],
            )
            .unwrap();
        assert_eq!(title(&root), "Both");
        assert_eq!(title(&eve), "Both");
    }

    #[test]
    fn a_close_forgets_the_window_so_a_later_update_finds_none() {
        let root = player(4, 9, "Root", PlayerInfo::CAN_KICK_BAN);
        let loader = start(enshwick::test::FakeHost::new().player(root));
        let actions = actions();

        actions.show(panel(), &[root.key]).unwrap();
        assert_eq!(loader.carried_windows(&root, 0).len(), 1);
        actions.close("panel", &[root.key]).unwrap();
        assert_eq!(
            loader.ask_actions(ActionCall::WINDOW, &root, 0, "panel", ""),
            Some(String::new()),
            "the mod keeps no closed window to render"
        );
    }

    #[test]
    fn a_window_past_its_bounds_or_naming_an_unknown_action_is_refused() {
        let root = player(4, 9, "Root", PlayerInfo::CAN_KICK_BAN);
        let _loader = start(enshwick::test::FakeHost::new().player(root));
        let actions = actions();
        let to = [root.key];

        let refused = |window: Window| actions.show(window, &to).unwrap_err().to_string();
        assert!(refused(Window::new("Bad Id", "x")).contains("ids are 1 to 32"));
        assert!(refused(Window::new("w", "x").action("fly")).contains("no action fly"));
        let over = Row::new("r", &[]).button("Wave", "wave", &[Value::Toggle(true)]);
        assert!(refused(Window::new("w", "x").row(over)).contains("takes 0"));
        let long = (0..=enshwick::actions::window::MAX_ITEMS)
            .fold(Window::new("w", "x"), |w, _| w.text("t", &[]));
        assert!(refused(long).contains("items"));
    }

    #[test]
    fn a_client_without_a_hosted_game_shows_no_window() {
        let _loader = start(enshwick::test::FakeHost::new().side(Side::CLIENT));
        assert!(
            actions()
                .show(Window::new("w", "x"), &[PlayerKey::new(7)])
                .is_err()
        );
    }

    #[test]
    fn a_hosted_or_single_player_game_shows_the_host_its_own_window() {
        let host = player(2, 7, "Host", PlayerInfo::CAN_KICK_BAN);
        let loader = start(
            enshwick::test::FakeHost::new()
                .side(Side::CLIENT)
                .hosting(true)
                .player(host),
        );
        actions().show(panel(), &[host.key]).unwrap();
        assert_eq!(loader.carried_windows(&host, 0).len(), 1);
    }
}
