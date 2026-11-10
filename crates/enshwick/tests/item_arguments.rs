//! An item argument: the interface sends the picked item's GUID, the
//! console names one by GUID or content id, and the server refuses an item
//! its game does not know before the handler runs.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{ActionCall, Header, PlayerInfo, Status};
    use enshwick::actions::sheet::{ArgView, decode};
    use enshwick::actions::{Action, Arg, Outcome, Value};
    use enshwick::content::Guid;
    use enshwick::i18n::Texts;
    use enshwick::{ItemId, Load, Mod, Off, PlayerKey};

    /// The values each run handed its handler.
    static RUNS: Mutex<Vec<Vec<Value>>> = Mutex::new(Vec::new());

    const TORCH: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
    const LAMP: Guid = Guid::named("you.spawner:lamp");
    const UNKNOWN: Guid = Guid::parse("00000000-0000-4000-8000-000000000001");

    struct Spawner;

    impl Mod for Spawner {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let actions = load.actions(Texts::default())?;
            actions.add(
                Action::new("spawn", "Spawn")
                    .admins()
                    .group("Items")
                    .arg(Arg::item("Item"))
                    .arg(Arg::number("Count", 1.0..=99.0, 1.0).optional(Value::Number(1.0))),
                |request| {
                    RUNS.lock().unwrap().push(request.args.to_vec());
                    Outcome::done("ok")
                },
            )?;
            actions.commands("s")?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Spawner,
        r#"
        id = "you.spawner"
        side = "server"
        "#
    );

    fn root() -> PlayerInfo {
        let mut info = PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: 0,
            entity: enshwick::Entity(4),
            key: PlayerKey::new(9),
            permissions: PlayerInfo::CAN_KICK_BAN,
            name_len: 4,
            name: [0; 64],
        };
        info.name[..4].copy_from_slice(b"Root");
        info
    }

    fn start() -> enshwick::test::Installed {
        let loader = enshwick::test::FakeHost::new()
            .player(root())
            .item(ItemId::from_guid(TORCH), 20, 0)
            .item(ItemId::from_guid(LAMP), 1, 0)
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
    fn the_sheet_offers_an_item_argument_and_the_interface_sends_the_picked_guid() {
        let loader = start();
        let sheet = decode(
            &loader
                .ask_actions(ActionCall::SHEET, &root(), 0, "", "")
                .unwrap(),
        );
        assert_eq!(sheet[0].args[0].kind, ArgView::ITEM);
        assert_eq!(sheet[0].group, "Items");
        assert_eq!(sheet[0].args[0].default, None);
        assert_eq!(
            sheet[0].args[1].default,
            Some(Value::Number(1.0)),
            "a window fills the count with its default"
        );

        let run = |args: &str| loader.ask_actions(ActionCall::RUN, &root(), 0, "spawn", args);
        assert_eq!(run(&format!("i{TORCH}")).as_deref(), Some("1ok"));
        assert_eq!(run(&format!("i{LAMP}\tn3")).as_deref(), Some("1ok"));
        assert_eq!(
            runs(),
            [
                vec![Value::Item(TORCH), Value::Number(1.0)],
                vec![Value::Item(LAMP), Value::Number(3.0)],
            ]
        );
    }

    #[test]
    fn an_item_the_game_does_not_know_is_refused_before_the_handler_runs() {
        let loader = start();
        let run = |args: &str| loader.ask_actions(ActionCall::RUN, &root(), 0, "spawn", args);
        assert_eq!(
            run(&format!("i{UNKNOWN}")).as_deref(),
            Some(format!("0No item {UNKNOWN}.").as_str())
        );
        assert_eq!(
            loader
                .type_in_chat(&root(), 0, &format!("/s spawn {UNKNOWN} 2"))
                .as_deref(),
            Some(format!("0No item {UNKNOWN}.").as_str())
        );
        assert!(
            run("tTorch").unwrap().starts_with('0'),
            "a text in an item's place"
        );
        assert!(runs().is_empty());
    }

    #[test]
    fn the_console_names_an_item_by_its_guid_or_content_id() {
        let loader = start();
        let chat = |line: &str| loader.type_in_chat(&root(), 0, line);
        assert_eq!(chat(&format!("/s spawn {TORCH}")).as_deref(), Some("1ok"));
        assert_eq!(chat("/s spawn you.spawner:lamp 5").as_deref(), Some("1ok"));
        assert_eq!(
            runs(),
            [
                vec![Value::Item(TORCH), Value::Number(1.0)],
                vec![Value::Item(LAMP), Value::Number(5.0)],
            ]
        );
        assert_eq!(
            chat("/s spawn Tool_Torch01").as_deref(),
            Some("0No item Tool_Torch01."),
            "a debug name needs the game's item registry, which the fake host lacks"
        );
        assert_eq!(
            chat("/s help spawn").as_deref(),
            Some("1spawn <Item> [1-99]: Spawn")
        );
    }
}
