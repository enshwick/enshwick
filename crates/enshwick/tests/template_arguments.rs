//! A template argument: the interface sends the picked template's GUID, the
//! console names one by GUID, content id or vanilla name, and the server
//! refuses a template its game does not know before the handler runs.

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Mutex, OnceLock};

    use enshwick::abi::{ActionCall, PlayerInfo, Status};
    use enshwick::actions::sheet::ArgView;
    use enshwick::actions::{Action, Actions, Arg, Outcome, Value, Window, window};
    use enshwick::content::templates::NewTemplate;
    use enshwick::content::{Guid, Id};
    use enshwick::i18n::Texts;
    use enshwick::test::CREATURE;
    use enshwick::{Load, Mod, Off, PlayerKey};

    /// The values each run handed its handler.
    static RUNS: Mutex<Vec<Vec<Value>>> = Mutex::new(Vec::new());
    static ACTIONS: OnceLock<Actions> = OnceLock::new();
    /// How often the mod's patch of the vanilla creature built it.
    static PATCHED: AtomicUsize = AtomicUsize::new(0);

    const TAME: Id = Id::new("you.summoner:tame");
    const UNKNOWN: Guid = Guid::parse("00000000-0000-4000-8000-000000000001");

    struct Summoner;

    impl Mod for Summoner {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.add_template(NewTemplate::new(TAME, CREATURE))?;
            let creature = enshwick::content::types::TEMPLATE_RESOURCE.key(CREATURE)?;
            load.serve(creature, creature, |bytes, out| {
                PATCHED.fetch_add(1, Ordering::Relaxed);
                out.copy_from(bytes)
            })?;
            let actions = load.actions(Texts::default())?;
            actions.add(
                Action::new("summon", "Summon")
                    .admins()
                    .arg(Arg::template("Creature"))
                    .arg(Arg::number("Count", 1.0..=9.0, 1.0).optional(Value::Number(1.0))),
                |request| {
                    RUNS.lock().unwrap().push(request.args.to_vec());
                    Outcome::done("ok")
                },
            )?;
            actions.add(
                Action::new("near", "Near")
                    .admins()
                    .arg(Arg::template("Creature").optional(Value::Template(CREATURE))),
                |request| {
                    RUNS.lock().unwrap().push(request.args.to_vec());
                    Outcome::done("ok")
                },
            )?;
            actions.commands("m")?;
            let _first = ACTIONS.set(actions);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Summoner,
        r#"
        id = "you.summoner"
        side = "server"
        "#
    );

    fn root() -> PlayerInfo {
        let mut info = enshwick::abi::headed!(PlayerInfo {
            slot: 0,
            entity: enshwick::Entity::from_engine(4),
            key: PlayerKey::new(9),
            permissions: PlayerInfo::CAN_KICK_BAN,
            name_len: 4,
            name: [0; PlayerInfo::NAME_MAX],
        });
        info.name[..4].copy_from_slice(b"Root");
        info
    }

    fn start() -> enshwick::test::Installed {
        let loader = enshwick::test::FakeHost::new()
            .player(root())
            .templates()
            .install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    fn runs() -> Vec<Vec<Value>> {
        std::mem::take(&mut *RUNS.lock().unwrap())
    }

    #[test]
    fn a_window_offers_a_template_argument_and_the_interface_sends_the_picked_guid() {
        let loader = start();
        let summon = Window::new("summon", "Summon").action("summon");
        ACTIONS.get().unwrap().show(summon, &[root().key]).unwrap();
        let carried = loader.carried_windows(&root(), 0);
        let sheet = window::decode(&carried[0].1).unwrap().entries;
        assert_eq!(sheet[0].args[0].kind, ArgView::TEMPLATE);

        let run = |args: &str| loader.ask_actions(ActionCall::RUN, &root(), 0, "summon", args);
        assert_eq!(run(&format!("e{CREATURE}")).as_deref(), Some("1ok"));
        assert_eq!(
            run(&format!("e{}\tn3", TAME.guid())).as_deref(),
            Some("1ok"),
            "a mod's template passes as the game knows it"
        );
        assert_eq!(
            runs(),
            [
                vec![Value::Template(CREATURE), Value::Number(1.0)],
                vec![Value::Template(TAME.guid()), Value::Number(3.0)],
            ]
        );
    }

    #[test]
    fn a_template_the_game_does_not_know_is_refused_before_the_handler_runs() {
        let loader = start();
        let run = |args: &str| loader.ask_actions(ActionCall::RUN, &root(), 0, "summon", args);
        assert_eq!(
            run(&format!("e{UNKNOWN}")).as_deref(),
            Some(format!("0No template {UNKNOWN}.").as_str())
        );
        assert_eq!(
            loader
                .type_in_chat(&root(), 0, &format!("/m summon {UNKNOWN} 2"))
                .as_deref(),
            Some(format!("0No template {UNKNOWN}.").as_str())
        );
        assert!(
            run(&format!("i{CREATURE}")).unwrap().starts_with('0'),
            "an item in a template's place"
        );
        assert!(runs().is_empty());
    }

    #[test]
    fn a_vanilla_template_argument_is_known_without_building_it_through_other_patches() {
        let loader = start();
        let before = PATCHED.load(Ordering::Relaxed);
        let run = |args: &str| loader.ask_actions(ActionCall::RUN, &root(), 0, "summon", args);
        assert_eq!(run(&format!("e{CREATURE}")).as_deref(), Some("1ok"));
        assert_eq!(
            loader
                .type_in_chat(&root(), 0, &format!("/m summon {CREATURE}"))
                .as_deref(),
            Some("1ok")
        );
        assert_eq!(PATCHED.load(Ordering::Relaxed), before, "no patch ran");
        assert_eq!(runs().len(), 2);
    }

    #[test]
    fn the_console_names_a_template_by_its_guid_or_content_id() {
        let loader = start();
        let chat = |line: &str| loader.type_in_chat(&root(), 0, line);
        assert_eq!(
            chat(&format!("/m summon {CREATURE}")).as_deref(),
            Some("1ok")
        );
        assert_eq!(
            chat("/m summon you.summoner:tame 5").as_deref(),
            Some("1ok")
        );
        assert_eq!(
            runs(),
            [
                vec![Value::Template(CREATURE), Value::Number(1.0)],
                vec![Value::Template(TAME.guid()), Value::Number(5.0)],
            ]
        );
        assert_eq!(
            chat("/m summon you.summoner:wild").as_deref(),
            Some("0No template you.summoner:wild.")
        );
        assert_eq!(
            chat("/m summon Enemy_Wildbeast_Wolf").as_deref(),
            Some("0No template Enemy_Wildbeast_Wolf."),
            "a vanilla name needs the game's template names, which the fake host lacks"
        );
        assert_eq!(
            chat("/m help summon").as_deref(),
            Some("1summon <Creature> [1-9]: Summon")
        );
    }

    const WOLF: Guid = Guid::parse("00000000-0000-4000-8000-00000000f001");
    const CHEST: Guid = Guid::parse("00000000-0000-4000-8000-00000000c001");
    const CRATE: Guid = Guid::parse("00000000-0000-4000-8000-00000000c002");

    /// A host whose game names its vanilla templates: a wolf, a chest and a
    /// crate whose names differ only by case, and ten spawners of one name.
    fn start_named() -> enshwick::test::Installed {
        let spawners: Vec<(Guid, &str)> = (1..=10_u8)
            .map(|n| {
                let mut guid = Guid::parse("00000000-0000-4000-8000-00000000a000");
                guid.0[15] = n;
                (guid, "Spawner")
            })
            .collect();
        let named = [
            &[
                (WOLF, "Enemy_Wildbeast_Wolf"),
                (CHEST, "Prop_Chest"),
                (CRATE, "PROP_CHEST"),
            ][..],
            &spawners,
        ]
        .concat();
        let loader = enshwick::test::FakeHost::new()
            .player(root())
            .items()
            .workshops()
            .vanilla_templates(&named)
            .install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    #[test]
    fn the_console_names_a_vanilla_template_in_any_case_when_one_template_has_it() {
        let loader = start_named();
        let chat = |line: &str| loader.type_in_chat(&root(), 0, line);
        assert_eq!(
            chat("/m summon enemy_wildbeast_wolf 3").as_deref(),
            Some("1ok")
        );
        assert_eq!(chat("/m summon Prop_Chest").as_deref(), Some("1ok"));
        assert_eq!(
            runs(),
            [
                vec![Value::Template(WOLF), Value::Number(3.0)],
                vec![Value::Template(CHEST), Value::Number(1.0)],
            ],
            "an exact name wins over one in another case"
        );
    }

    #[test]
    fn a_template_name_several_templates_share_is_refused_with_their_guids() {
        let loader = start_named();
        let chat = |line: &str| loader.type_in_chat(&root(), 0, line);
        assert_eq!(
            chat("/m summon prop_chest").as_deref(),
            Some(
                format!("0Several templates are named prop_chest: {CHEST}, {CRATE}. Name one by its GUID.")
                    .as_str()
            )
        );
        let spawners = chat("/m summon spawner").unwrap();
        assert!(
            spawners.starts_with(
                "0Several templates are named spawner: 00000000-0000-4000-8000-00000000a001, "
            ),
            "{spawners}"
        );
        assert!(
            spawners.ends_with(
                "00000000-0000-4000-8000-00000000a008 and 2 more. Name one by its GUID."
            ),
            "at most eight are listed: {spawners}"
        );
        assert!(runs().is_empty());
    }

    #[test]
    fn a_last_optional_template_takes_the_rest_of_the_line_or_its_default() {
        let loader = start();
        let chat = |line: &str| loader.type_in_chat(&root(), 0, line);
        assert_eq!(chat("/m near").as_deref(), Some("1ok"));
        assert_eq!(chat("/m near you.summoner:tame").as_deref(), Some("1ok"));
        assert_eq!(
            runs(),
            [
                vec![Value::Template(CREATURE)],
                vec![Value::Template(TAME.guid())]
            ]
        );
    }
}
