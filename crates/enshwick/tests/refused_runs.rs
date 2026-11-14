//! Refused runs: a mod hears each run of its actions the platform refused
//! before the handler, from the mod's own checks or the loader's.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{ActionCall, Header, PlayerInfo, Status};
    use enshwick::actions::{Action, Arg, Outcome, RefusedReason, Via};
    use enshwick::content::Guid;
    use enshwick::i18n::Texts;
    use enshwick::{ItemId, Load, Mod, Off, PlayerKey};

    /// What the mod heard of each refused run: who, from where, the action,
    /// the text given, how many values were read, and why.
    type Heard = (String, Via, String, String, usize, RefusedReason);

    static HEARD: Mutex<Vec<Heard>> = Mutex::new(Vec::new());
    static RAN: Mutex<u32> = Mutex::new(0);

    const TORCH: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
    const UNKNOWN: Guid = Guid::parse("00000000-0000-4000-8000-000000000001");

    struct Giver;

    impl Mod for Giver {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let actions = load.actions(Texts::default())?;
            actions.add(
                Action::new("give", "Give")
                    .admins()
                    .arg(Arg::player("Player"))
                    .arg(Arg::item("Item")),
                |_request| {
                    *RAN.lock().unwrap() += 1;
                    Outcome::done("given")
                },
            )?;
            actions.on_refused(|refused| {
                HEARD.lock().unwrap().push((
                    refused.player.name.clone(),
                    refused.via,
                    refused.action.to_owned(),
                    refused.text.to_owned(),
                    refused.values.len(),
                    refused.reason,
                ));
            });
            actions.commands("g")?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Giver,
        r#"
        id = "you.giver"
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

    fn ann() -> PlayerInfo {
        player(3, 7, "Ann", 0)
    }

    fn root() -> PlayerInfo {
        player(4, 9, "Root", PlayerInfo::CAN_KICK_BAN)
    }

    fn start() -> enshwick::test::Installed {
        let loader = enshwick::test::FakeHost::new()
            .player(ann())
            .player(root())
            .item(ItemId::from_guid(TORCH), 20, 0)
            .install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    fn heard() -> Vec<Heard> {
        std::mem::take(&mut *HEARD.lock().unwrap())
    }

    fn heard_one(name: &str, via: Via, text: &str, reason: RefusedReason) -> Heard {
        (name.into(), via, "give".into(), text.into(), 0, reason)
    }

    #[test]
    fn an_unknown_item_an_unknown_player_and_no_right_each_reach_the_mod_once() {
        let loader = start();
        let lines = [
            (
                root(),
                format!("give Ann {UNKNOWN}"),
                RefusedReason::UnknownItem,
            ),
            (
                root(),
                format!("give Zed {TORCH}"),
                RefusedReason::UnknownPlayer,
            ),
            (ann(), format!("give Ann {TORCH}"), RefusedReason::NoRight),
        ];
        for (who, line, reason) in &lines {
            let reply = loader.type_in_chat(who, 0, &format!("/g {line}")).unwrap();
            assert!(reply.starts_with('0'), "{line}: {reply}");
            let name = who.name_text().into_owned();
            assert_eq!(heard(), [heard_one(&name, Via::Chat, line, *reason)]);
        }
        assert_eq!(*RAN.lock().unwrap(), 0);
    }

    #[test]
    fn a_refused_interface_run_names_its_arguments_and_the_values_read() {
        let loader = start();
        let args = format!("p0000000000000007\ti{UNKNOWN}");
        let reply = loader.ask_actions(ActionCall::RUN, &root(), 0, "give", &args);
        assert!(reply.unwrap().starts_with('0'));
        assert_eq!(
            heard(),
            [(
                "Root".into(),
                Via::Interface,
                "give".into(),
                args,
                2,
                RefusedReason::UnknownItem
            )],
            "the player and the item were read before the item was refused"
        );
        let unknown = loader.ask_actions(ActionCall::RUN, &root(), 0, "take", "");
        assert!(unknown.unwrap().starts_with('0'));
        let (_, _, action, _, _, reason) = heard().remove(0);
        assert_eq!(
            (action.as_str(), reason),
            ("take", RefusedReason::UnknownAction)
        );
    }

    #[test]
    fn a_run_the_loader_refused_reaches_the_mod_with_the_loader_s_reason() {
        let loader = start();
        assert!(loader.refuse_run(
            &ann(),
            ActionCall::RUN,
            ActionCall::REFUSED_RATE,
            "give",
            "p0000000000000007",
        ));
        assert!(loader.refuse_run(
            &ann(),
            ActionCall::CHAT,
            ActionCall::REFUSED_BUSY,
            "GIVE Ann torch",
            "",
        ));
        assert_eq!(
            heard(),
            [
                heard_one(
                    "Ann",
                    Via::Interface,
                    "p0000000000000007",
                    RefusedReason::Rate
                ),
                heard_one("Ann", Via::Chat, "GIVE Ann torch", RefusedReason::Busy),
            ]
        );
        assert!(loader.refuse_run(&ann(), ActionCall::RUN, 99, "give", ""));
        assert!(loader.refuse_run(&ann(), 99, ActionCall::REFUSED_RATE, "give", ""));
        assert!(
            heard().is_empty(),
            "a reason or question this build does not know"
        );
        assert_eq!(*RAN.lock().unwrap(), 0);
    }
}
