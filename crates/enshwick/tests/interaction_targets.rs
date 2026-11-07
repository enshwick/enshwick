//! The entity each player's interaction prompt offers, as the server's world
//! keeps it.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::Duration;

    use enshwick::abi::{Header, PlayerInfo, Status};
    use enshwick::content::EngineFacts;
    use enshwick::test::facts;
    use enshwick::{Entity, Load, Mod, Off, PlayerKey};

    /// What each look read: the player's prompt target, and a stranger's.
    static SEEN: Mutex<Vec<(Option<Entity>, Option<Entity>)>> = Mutex::new(Vec::new());

    struct Watcher;

    impl Mod for Watcher {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let targets = load.interaction_targets()?;
            load.on_tick(Duration::from_millis(100), move |_| {
                let seen = (
                    targets.target(ANN).unwrap(),
                    targets.target(Entity(99)).unwrap(),
                );
                SEEN.lock().unwrap().push(seen);
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
    fn a_player_reads_the_prompt_s_target_slot_only_and_a_stranger_reads_none() {
        let fake = enshwick::test::FakeHost::new()
            .world()
            .interaction_targets()
            .player(ann());
        let ty = fake.hash(facts::SELECTED_TARGETS.id()).unwrap();
        let size = fake.size(facts::SELECTED_TARGETS.id()).unwrap();
        let info = fake.size(facts::TARGET_INFO.id()).unwrap();
        let entity = fake.offset(facts::TARGET_INFO_ENTITY.id()).unwrap();
        let targets = |by_slot: &[(usize, u32)]| {
            let mut bytes = vec![0u8; size];
            for &(slot, id) in by_slot {
                bytes[slot * info + entity..][..4].copy_from_slice(&id.to_le_bytes());
            }
            bytes
        };
        let loader = fake.component(ANN, ty, &targets(&[(0, 77)])).install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Watcher)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let _combat_only = loader.tick(Duration::from_millis(100));
        assert!(loader.set_component(ANN, ty, &targets(&[(0, 77), (2, 512)])));
        let _chest = loader.tick(Duration::from_millis(100));
        assert_eq!(
            *SEEN.lock().unwrap(),
            [(None, None), (Some(Entity(512)), None)],
            "slot 0 is the combat target, not the prompt's"
        );
    }

    #[test]
    fn a_build_without_the_target_s_enum_turns_the_targets_off() {
        let fake = enshwick::test::FakeHost::new()
            .world()
            .interaction_targets()
            .without_fact(facts::TARGET_TYPE);
        let loader = fake.install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Watcher)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_ne!(status, Status::OK, "init fails with the targets off");
    }
}
