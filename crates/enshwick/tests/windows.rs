//! Carried windows: a mod of side server shows windows on players'
//! clients, each rendered in the reader's language with only what they may
//! run, and updates and closes them.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use enshwick::abi::{ActionCall, Header, PlayerInfo, Side, Status};
    use enshwick::actions::sheet::ArgView;
    use enshwick::actions::window::{ButtonView, ItemView, decode};
    use enshwick::actions::{Action, Actions, Arg, Outcome, Row, Value, Window};
    use enshwick::i18n::Language;
    use enshwick::{Load, Mod, Off, PlayerKey};

    static ACTIONS: OnceLock<Actions> = OnceLock::new();

    /// What the mod heard of closed windows: the player's key and the
    /// window, `None` for a player who left.
    static CLOSED: std::sync::Mutex<Vec<(u64, Option<String>)>> = std::sync::Mutex::new(Vec::new());

    /// How often the mod was asked whether a player may wave.
    static ALLOWS: AtomicUsize = AtomicUsize::new(0);

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
            actions.add(
                Action::new("wave", "Wave").allow(|_| {
                    ALLOWS.fetch_add(1, Ordering::Relaxed);
                    true
                }),
                done,
            )?;
            let shown = actions.clone();
            actions.add(
                Action::new("open", "Open the panel"),
                move |request| match shown.show(panel(), &[request.player.key]) {
                    Ok(()) => Outcome::done(""),
                    Err(why) => Outcome::refused(why.to_string()),
                },
            )?;
            actions.on_closed(|player, window| {
                CLOSED
                    .lock()
                    .unwrap()
                    .push((player.get(), window.map(str::to_owned)));
            });
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

    /// The panel with `count` players online, and the count the window
    /// `text` shows.
    fn roster(count: &str) -> Window {
        Window::new("panel", "panel").text("online", &[("count", count)])
    }

    fn shown_count(carried: &[(String, String)]) -> Option<String> {
        let (_, text) = carried.iter().find(|(id, _)| id == "panel")?;
        match decode(text)?.items.first()? {
            ItemView::Text(line) => Some(line.clone()),
            _ => None,
        }
    }

    #[test]
    fn an_update_changes_a_window_only_where_its_player_still_shows_it() {
        let root = player(4, 9, "Root", PlayerInfo::CAN_KICK_BAN);
        let eve = player(5, 6, "Eve", PlayerInfo::CAN_KICK_BAN);
        let loader = start(enshwick::test::FakeHost::new().player(root).player(eve));
        let actions = actions();
        let both = [root.key, eve.key];

        actions.show(roster("2"), &[root.key]).unwrap();
        assert_eq!(
            shown_count(&loader.carried_windows(&root, 0)).as_deref(),
            Some("2 online")
        );

        actions.update(roster("3"), &both).unwrap();
        assert_eq!(
            shown_count(&loader.carried_windows(&root, 0)).as_deref(),
            Some("3 online"),
            "where it shows, it updates"
        );
        assert!(
            loader.carried_windows(&eve, 0).is_empty(),
            "an update opens the window for no one"
        );

        loader.close_window(&root, "panel");
        assert_eq!(*CLOSED.lock().unwrap(), [(9, Some("panel".to_owned()))]);
        actions.update(roster("4"), &both).unwrap();
        assert!(
            loader.carried_windows(&root, 0).is_empty(),
            "a window its player closed stays closed"
        );
        assert_eq!(
            loader
                .ask_actions(ActionCall::WINDOW, &root, 0, "panel", "")
                .as_deref(),
            Some(""),
            "and the mod keeps it for them no more"
        );

        actions.show(roster("5"), &[root.key]).unwrap();
        assert_eq!(
            shown_count(&loader.carried_windows(&root, 0)).as_deref(),
            Some("5 online"),
            "a show opens it again"
        );
        assert!(
            actions.update(Window::new("Bad Id", "x"), &both).is_err(),
            "an update is checked as a show is"
        );
    }

    #[test]
    fn a_player_who_left_holds_no_window_and_the_mod_forgets_theirs() {
        let root = player(4, 9, "Root", PlayerInfo::CAN_KICK_BAN);
        let eve = player(5, 6, "Eve", PlayerInfo::CAN_KICK_BAN);
        let loader = start(enshwick::test::FakeHost::new().player(root).player(eve));
        let actions = actions();
        actions.show(roster("2"), &[root.key, eve.key]).unwrap();
        actions
            .show(Window::new("bans", "panel"), &[root.key])
            .unwrap();
        assert_eq!(loader.carried_windows(&root, 0).len(), 2);
        assert_eq!(loader.carried_windows(&eve, 0).len(), 1);

        loader.player_left(root.key);
        assert_eq!(*CLOSED.lock().unwrap(), [(9, None)]);
        assert!(loader.carried_windows(&root, 0).is_empty());
        for id in ["panel", "bans"] {
            assert_eq!(
                loader
                    .ask_actions(ActionCall::WINDOW, &root, 0, id, "")
                    .as_deref(),
                Some(""),
                "the mod forgot {id} for the player who left"
            );
        }
        actions.update(roster("1"), &[root.key, eve.key]).unwrap();
        assert!(loader.carried_windows(&root, 0).is_empty());
        assert_eq!(
            shown_count(&loader.carried_windows(&eve, 0)).as_deref(),
            Some("1 online"),
            "the player who stayed keeps and updates theirs"
        );
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
    fn a_button_giving_values_its_action_does_not_take_is_refused() {
        let root = player(4, 9, "Root", PlayerInfo::CAN_KICK_BAN);
        let _loader = start(enshwick::test::FakeHost::new().player(root));
        let show = |action: &str, values: &[Value]| {
            let row = Row::new("r", &[]).button("Go", action, values);
            actions().show(Window::new("w", "x").row(row), &[root.key])
        };
        let refused =
            |action: &str, values: &[Value]| show(action, values).unwrap_err().to_string();

        let text = |text: &str| Value::Text(text.into());
        assert!(refused("ban", &[text("Ann")]).contains("argument 1"));
        assert!(refused("ban", &[ann(), text(&"x".repeat(65))]).contains("argument 2"));
        assert!(refused("say", &[text("line\nbreak")]).contains("argument 1"));
        show("ban", &[ann(), text(&"x".repeat(64))]).expect("values at their bounds fit");
        show("ban", &[]).expect("a button may leave every value to its form");
    }

    #[test]
    fn a_render_asks_once_who_may_run_an_action_however_many_buttons_name_it() {
        let root = player(4, 9, "Root", PlayerInfo::CAN_KICK_BAN);
        let loader = start(enshwick::test::FakeHost::new().player(root));
        let row = (0..3).fold(Row::new("r", &[]), |row, _| row.button("Wave", "wave", &[]));
        let window = Window::new("w", "x")
            .row(row.clone())
            .row(row)
            .action("wave");
        actions().show(window, &[root.key]).unwrap();

        ALLOWS.store(0, Ordering::Relaxed);
        assert_eq!(loader.carried_windows(&root, 0).len(), 1);
        assert_eq!(ALLOWS.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn a_show_or_close_the_loader_refuses_leaves_the_window_the_player_holds() {
        let root = player(4, 9, "Root", PlayerInfo::CAN_KICK_BAN);
        let loader = start(enshwick::test::FakeHost::new().player(root));
        let actions = actions();
        let held = || {
            let text = loader.ask_actions(ActionCall::WINDOW, &root, 0, "panel", "");
            decode(&text.unwrap()).map(|view| view.title)
        };

        actions.show(panel(), &[root.key]).unwrap();
        let waiting: Vec<PlayerKey> = (0..enshwick::actions::window::MAX_WAITING as u64 - 1)
            .map(|n| PlayerKey::new(1000 + n))
            .collect();
        actions.show(Window::new("crowd", "x"), &waiting).unwrap();

        let second = Window::new("panel", "Second").action("wave");
        assert!(actions.show(second, &[root.key]).is_err(), "no room waits");
        assert_eq!(held().as_deref(), Some("Admin panel"));
        assert!(actions.close("panel", &[root.key]).is_err());
        assert_eq!(held().as_deref(), Some("Admin panel"));
    }

    #[test]
    fn the_fake_renders_a_players_windows_again_after_their_run_and_none_for_a_stranger() {
        let root = player(4, 9, "Root", PlayerInfo::CAN_KICK_BAN);
        let stranger = player(6, 99, "Gone", PlayerInfo::CAN_KICK_BAN);
        let loader = start(enshwick::test::FakeHost::new().player(root));
        let actions = actions();

        actions.show(panel(), &[root.key, stranger.key]).unwrap();
        assert!(
            loader.carried_windows(&stranger, 0).is_empty(),
            "not online"
        );
        let title = |carried: Vec<(String, String)>| decode(&carried[0].1).unwrap().title;
        assert_eq!(title(loader.carried_windows(&root, 0)), "Admin panel");
        assert_eq!(
            title(loader.carried_windows(&root, Language::UK_UA.0)),
            "Admin panel",
            "nothing waits, so nothing renders"
        );

        let ran = loader.ask_actions(ActionCall::RUN, &root, 0, "wave", "");
        assert_eq!(ran.as_deref(), Some("1ok"));
        assert_eq!(
            title(loader.carried_windows(&root, Language::UK_UA.0)),
            "Панель адміністратора"
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
    fn the_fake_refuses_a_malformed_window_call_as_the_loader_does() {
        use enshwick::abi::Str;
        use enshwick::actions::window::MAX_CALL_PLAYERS;

        let _loader = start(enshwick::test::FakeHost::new());
        let host = enshwick::host().expect("the mod's host");
        let keys = [PlayerKey::new(7); MAX_CALL_PLAYERS + 1];
        let call = |id: &str, players: *const PlayerKey, count: usize, close: u32| {
            let count = u32::try_from(count).unwrap();
            (host.api().action_window)(host.handle(), Str::new(id), players, count, close)
        };

        assert_eq!(
            call("panel", keys.as_ptr(), MAX_CALL_PLAYERS, 0),
            Status::OK
        );
        assert_eq!(
            call("panel", keys.as_ptr(), MAX_CALL_PLAYERS + 1, 0),
            Status::BAD_ARGS
        );
        assert_eq!(call("panel", core::ptr::null(), 1, 0), Status::BAD_ARGS);
        assert_eq!(call("panel", core::ptr::null(), 0, 0), Status::OK);
        assert_eq!(
            call("panel", keys.as_ptr(), 1, 3),
            Status::BAD_ARGS,
            "past the update"
        );
        assert_eq!(call("panel", keys.as_ptr(), 1, 2), Status::OK, "an update");
        assert_eq!(call("panel", keys.as_ptr(), 1, 1), Status::OK);
        assert_eq!(call("Bad Id", keys.as_ptr(), 1, 0), Status::BAD_ARGS);
        assert_eq!(call("", keys.as_ptr(), 1, 0), Status::BAD_ARGS);
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
