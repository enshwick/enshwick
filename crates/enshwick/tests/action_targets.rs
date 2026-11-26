//! Actions run on the object a client names, as the loader carries it to
//! the server and hands it back to the action's `at` and handler.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use enshwick::abi::{PlayerInfo, Side, Status};
    use enshwick::actions::{Action, ActionTarget, Actions, Outcome, Subject};
    use enshwick::i18n::Texts;
    use enshwick::{Load, Mod, Off};

    static ACTIONS: OnceLock<Actions> = OnceLock::new();
    /// How often the list action's `at` ran; each test runs in a process of its own.
    static ASKED: AtomicUsize = AtomicUsize::new(0);

    struct Safes;

    impl Mod for Safes {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let actions = load.actions(Texts::default())?;
            actions.add(
                Action::new("switch", "Switch").at(|ask| {
                    let at = ask.target.and_then(|t| t.position);
                    match at {
                        Some([x, _, _]) if x.abs() < 100.0 => Ok(Subject::new(7, "The safe")),
                        Some(_) => Err("That safe is out of reach".into()),
                        None => Err("Open a safe first".into()),
                    }
                }),
                |request| {
                    let entity = request.target.and_then(|t| t.entity);
                    Outcome::done(format!("switched {entity:?}"))
                },
            )?;
            actions.add(
                Action::per_player("access", "Who may open", |ask, _subject| {
                    match ask.target.and_then(|t| t.entity) {
                        Some(entity) if entity.get() == 4096 => {
                            vec![(enshwick::PlayerKey::new(0x63), "Olga".to_owned())]
                        }
                        _ => Vec::new(),
                    }
                })
                .at(|ask| {
                    ASKED.fetch_add(1, Ordering::Relaxed);
                    match ask.target {
                        Some(_) => Ok(Subject::new(7, "The safe")),
                        None => Err("Open a safe first".into()),
                    }
                }),
                |request| match request.args {
                    [enshwick::actions::Value::Player { name, .. }, _] => {
                        Outcome::done(format!("{name} switched"))
                    }
                    _ => Outcome::refused("bad"),
                },
            )?;
            let _kept = ACTIONS.set(actions);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Safes,
        r#"
        id = "you.safes"
        side = "both"
        "#
    );

    fn start(host: enshwick::test::FakeHost) -> enshwick::test::Installed {
        let loader = host.install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Safes)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        loader
    }

    fn owner() -> PlayerInfo {
        enshwick::abi::headed!(PlayerInfo {
            slot: 0,
            entity: enshwick::Entity::from_engine(2),
            key: enshwick::PlayerKey::new(7),
            permissions: 0,
            name_len: 0,
            name: [0; PlayerInfo::NAME_MAX],
        })
    }

    #[test]
    fn the_server_hands_the_named_object_to_the_action_s_at_and_handler() {
        let loader = start(enshwick::test::FakeHost::new().player(owner()));
        let near = ActionTarget::entity(enshwick::Entity::from_engine(4096))
            .with_position([3.0, 1.0, -2.0]);
        assert_eq!(
            loader
                .ask_action_at(&owner(), 0, "switch", "", near)
                .as_deref(),
            Some("1switched Some(Entity(4096))")
        );
        let far = ActionTarget::at([500.0, 1.0, 0.0]);
        assert_eq!(
            loader
                .ask_action_at(&owner(), 0, "switch", "", far)
                .as_deref(),
            Some("0That safe is out of reach")
        );
        let run = enshwick::abi::ActionCall::RUN;
        assert_eq!(
            loader
                .ask_actions(run, &owner(), 0, "switch", "")
                .as_deref(),
            Some("0Open a safe first")
        );
    }

    #[test]
    fn an_offline_player_on_the_named_object_s_list_is_switched_from_its_panel() {
        let loader = start(enshwick::test::FakeHost::new().player(owner()));
        let safe = ActionTarget::entity(enshwick::Entity::from_engine(4096));
        let off = "p0000000000000063\tb0";
        assert_eq!(
            loader
                .ask_action_at(&owner(), 0, "access", off, safe)
                .as_deref(),
            Some("1Olga switched")
        );
    }

    #[test]
    fn one_run_finds_its_subject_once_from_the_panel_and_the_console() {
        let loader = start(enshwick::test::FakeHost::new().player(owner()));
        let safe = ActionTarget::entity(enshwick::Entity::from_engine(4096));
        let off = "p0000000000000063\tb0";
        let ran = loader.ask_action_at(&owner(), 0, "access", off, safe);
        assert_eq!(ran.as_deref(), Some("1Olga switched"));
        assert_eq!(ASKED.swap(0, Ordering::Relaxed), 1, "from the panel");

        let console = enshwick::abi::ActionCall::CONSOLE;
        let ran = loader.ask_actions(console, &owner(), 0, "access Olga off", "");
        assert!(
            ran.is_some_and(|reply| reply.starts_with('0')),
            "no safe at hand"
        );
        assert_eq!(ASKED.load(Ordering::Relaxed), 1, "from the console");
    }

    #[test]
    fn a_client_sends_the_object_with_the_run_and_refuses_an_endless_position() {
        let loader = start(enshwick::test::FakeHost::new().side(Side::CLIENT));
        let actions = ACTIONS.get().expect("init kept the actions");
        actions
            .run_at(
                "switch",
                &[],
                ActionTarget::entity(enshwick::Entity::from_engine(9)),
            )
            .unwrap();
        actions.run("switch", &[]).unwrap();
        assert!(
            actions
                .run_at("switch", &[], ActionTarget::at([f64::NAN, 0.0, 0.0]))
                .is_err()
        );
        assert_eq!(loader.action_runs().len(), 2);
        assert_eq!(
            loader.action_targets(),
            [
                Some(ActionTarget::entity(enshwick::Entity::from_engine(9))),
                None
            ]
        );
    }
}
