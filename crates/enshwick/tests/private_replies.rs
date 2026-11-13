//! A handler's private reply reaches the player who typed the command, and
//! no player for the console or a remote caller, who have no account.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::{Header, Level, PlayerInfo, Status};
    use enshwick::actions::{Action, Outcome};
    use enshwick::commands::{self, Caller};
    use enshwick::i18n::Texts;
    use enshwick::net::Peer;
    use enshwick::world::{Notice, World};
    use enshwick::{Load, Mod, Off, PlayerKey};

    static WORLD: OnceLock<World> = OnceLock::new();
    /// What each `notify` answered, in order.
    static HOW: std::sync::Mutex<Vec<Notice>> = std::sync::Mutex::new(Vec::new());

    struct Whisper;

    impl Mod for Whisper {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = WORLD.set(load.world()?);
            let actions = load.actions(Texts::default())?;
            actions.add(Action::new("whisper", "Whisper"), |request| {
                let world = WORLD.get().unwrap();
                world.tell(request.player, "psst in chat");
                let how = world.notify(request.player, "psst on screen");
                HOW.lock().unwrap().push(how);
                Outcome::done("whispered")
            })?;
            actions.commands("wh")?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Whisper,
        r#"
        id = "you.whisper"
        side = "server"
        "#
    );

    fn ann() -> PlayerInfo {
        let mut info = PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: 0,
            entity: enshwick::Entity(3),
            key: PlayerKey::new(7),
            permissions: 0,
            name_len: 3,
            name: [0; 64],
        };
        info.name[..3].copy_from_slice(b"Ann");
        info
    }

    fn start() -> enshwick::test::Installed {
        let loader = enshwick::test::FakeHost::new()
            .world()
            .player(ann())
            .net_peer(Peer::Player(PlayerKey::new(7)))
            .install();
        HOW.lock().unwrap().clear();
        // SAFETY: the three functions are the exports of this file's `declare_mod!`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    fn warned(loader: &enshwick::test::Installed) -> usize {
        loader
            .logs()
            .iter()
            .filter(|(level, line)| *level == Level::WARN && line.contains("goes to no player"))
            .count()
    }

    #[test]
    fn a_private_reply_to_a_remote_caller_or_the_console_reaches_no_player() {
        let loader = start();

        let remote = commands::run("/wh whisper", &Caller::new("rcon")).unwrap();
        assert!(remote.is_done(), "{}", remote.text());
        assert_eq!(
            enshwick::console::run("you.whisper whisper").as_deref(),
            Ok("whispered")
        );

        assert!(
            loader.chat().is_empty(),
            "no chat line: {:?}",
            loader.chat()
        );
        assert!(
            loader.notices().is_empty(),
            "no notice: {:?}",
            loader.notices()
        );
        assert_eq!(warned(&loader), 1, "logged once: {:?}", loader.logs());
        assert_eq!(*HOW.lock().unwrap(), [Notice::Nobody, Notice::Nobody]);
    }

    #[test]
    fn a_private_reply_to_a_player_who_typed_the_command_reaches_that_player_alone() {
        let loader = start();

        assert!(loader.type_in_chat(&ann(), 0, "/wh whisper").is_some());

        let ann = PlayerKey::new(7);
        assert_eq!(loader.chat(), [(ann, "psst in chat".to_owned())]);
        assert_eq!(loader.notices(), [(ann, "psst on screen".to_owned())]);
        assert_eq!(warned(&loader), 0);
    }
}
