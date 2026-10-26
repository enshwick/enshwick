//! Game events reach a mod's handlers by type, with fields read by name, and
//! a mod lists every event type of the build.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::Status;
    use enshwick::ecs::Entity;
    use enshwick::{Load, Mod, Off};

    const DIED: u32 = 0x0042;
    const HIT: u32 = 0x0043;

    static DEAD: Mutex<Vec<Entity>> = Mutex::new(Vec::new());
    static EVERY: Mutex<Vec<u32>> = Mutex::new(Vec::new());
    static LISTED: Mutex<Vec<(&str, u32, u32)>> = Mutex::new(Vec::new());

    struct Watcher;

    impl Mod for Watcher {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.watcher", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let listed = load.event_types()?;
            *LISTED.lock().unwrap() = listed
                .iter()
                .map(|t| (t.name(), t.hash(), t.size()))
                .collect();
            let died = load.event_type("keen::ecs::EntityDiedEvent")?;
            let dead = died.field::<Entity>(load, "deadEntityId")?;
            assert!(died.field::<u32>(load, "nope").is_err());
            assert!(died.field::<u64>(load, "late").is_err());
            load.on_raw_game_event(died, move |event| {
                if let Some(entity) = event.get(dead) {
                    DEAD.lock().unwrap().push(entity);
                }
            })?;
            load.on_every_raw_game_event(|event| EVERY.lock().unwrap().push(event.type_hash()))?;
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Watcher);

    #[test]
    fn handlers_get_their_type_and_every_handler_gets_all() {
        let loader = enshwick::test::FakeHost::new()
            .event_type("keen::ecs::EntityDiedEvent", DIED, 16)
            .event_type("keen::ecs::HitEvent", HIT, 248)
            .field_offset("keen::ecs::EntityDiedEvent", "deadEntityId", 8)
            .field_offset("keen::ecs::EntityDiedEvent", "late", 12)
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Watcher)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(loader.game_event_types(), [DIED, 0]);
        let mut listed = LISTED.lock().unwrap().clone();
        listed.sort_unstable();
        assert_eq!(
            listed,
            [
                ("keen::ecs::EntityDiedEvent", DIED, 16),
                ("keen::ecs::HitEvent", HIT, 248)
            ]
        );

        let mut died = [0u8; 16];
        died[8..12].copy_from_slice(&4096u32.to_le_bytes());
        assert_eq!(loader.raise_game_event(DIED, &died), 2);
        assert_eq!(loader.raise_game_event(HIT, &[0; 248]), 1);
        assert_eq!(*DEAD.lock().unwrap(), [Entity(4096)]);
        assert_eq!(*EVERY.lock().unwrap(), [DIED, HIT]);
    }
}
