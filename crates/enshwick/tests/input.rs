//! The game controls a player holds, as the server's world keeps them.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::Duration;

    use enshwick::abi::{Header, PlayerInfo, Status};
    use enshwick::content::EngineFacts;
    use enshwick::controls::GameAction;
    use enshwick::test::facts;
    use enshwick::{Entity, Load, Mod, Off, PlayerKey};

    /// What each look read: the player's held controls, and a stranger's.
    static SEEN: Mutex<Vec<(Option<Vec<GameAction>>, bool)>> = Mutex::new(Vec::new());

    struct Watcher;

    impl Mod for Watcher {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let input = load.input()?;
            load.on_tick(Duration::from_millis(100), move |_| {
                let held = input.held(ANN).unwrap().map(|held| held.iter().collect());
                let stranger = input.held(Entity(99)).unwrap().is_none();
                SEEN.lock().unwrap().push((held, stranger));
            })?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Watcher,
        r#"
        id = "you.watcher"
        side = "server"
        "#
    );

    const ANN: Entity = Entity(40);

    fn ann() -> PlayerInfo {
        PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: 0,
            entity: ANN,
            key: PlayerKey::new(7),
            permissions: 0,
            name_len: 0,
            name: [0; 64],
        }
    }

    #[test]
    fn a_player_reads_the_controls_the_build_names_and_a_stranger_reads_none() {
        let fake = enshwick::test::FakeHost::new()
            .world()
            .player_input()
            .player(ann());
        let ty = fake.hash(facts::PLAYER_INPUT.id()).unwrap();
        let at = fake.offset(facts::PLAYER_INPUT_DIGITAL.id()).unwrap();
        let size = fake.size(facts::PLAYER_INPUT.id()).unwrap();
        let input = |bits: u64| {
            let mut bytes = vec![0u8; size];
            bytes[at..][..8].copy_from_slice(&bits.to_le_bytes());
            bytes
        };
        let loader = fake.component(ANN, ty, &input(0)).install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Watcher)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let _idle = loader.tick(Duration::from_millis(100));
        assert!(loader.set_component(ANN, ty, &input(1 << 30 | 1 << 5 | 1 << 12)));
        let _blocking = loader.tick(Duration::from_millis(100));
        assert_eq!(
            *SEEN.lock().unwrap(),
            [
                (Some(vec![]), true),
                (
                    Some(vec![GameAction::ContextualAction, GameAction::Block]),
                    true
                ),
            ],
            "bit 12 has no name on this build"
        );
    }

    #[test]
    fn a_build_without_the_held_controls_turns_input_off() {
        let fake = enshwick::test::FakeHost::new()
            .world()
            .player_input()
            .without_fact(facts::PLAYER_INPUT_DIGITAL);
        let loader = fake.install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Watcher)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_ne!(status, Status::OK, "init fails with input off");
    }
}
