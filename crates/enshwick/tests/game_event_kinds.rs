//! Typed game events resolve their types and fields by the loader's engine
//! ids, never by an engine name.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use enshwick::abi::Status;
    use enshwick::ecs::Entity;
    use enshwick::game_events::{
        AltarUsed, Built, CraftRequested, Crafted, DamageTypes, Died, EquipmentChanged,
        ExperienceGained, Hit, Interacted, Kind, LevelGained, LootOpened, PickedUp, Revived,
        TornDown,
    };
    use enshwick::test::FakeHost;
    use enshwick::{Load, Mod, Off};

    const HIT: u32 = 0x4000;
    const DIED: u32 = 0x4001;
    const HISTORY: u32 = 0x4002;
    const CREDIT: u32 = 0x4003;

    static HITS: Mutex<Vec<Hit>> = Mutex::new(Vec::new());
    static DEATHS: Mutex<Vec<Died>> = Mutex::new(Vec::new());
    static SUBSCRIBED: AtomicUsize = AtomicUsize::new(0);

    fn quietly<E: Kind>(load: &mut Load<'_>) -> Result<(), Off> {
        load.on_game::<E>(|_| {})?;
        SUBSCRIBED.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    struct Watcher;

    impl Mod for Watcher {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.kinds", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.on_game::<Hit>(|hit| HITS.lock().unwrap().push(*hit))?;
            load.on_game::<Died>(|died| DEATHS.lock().unwrap().push(*died))?;
            SUBSCRIBED.fetch_add(2, Ordering::Relaxed);
            quietly::<LevelGained>(load)?;
            quietly::<ExperienceGained>(load)?;
            quietly::<CraftRequested>(load)?;
            quietly::<Crafted>(load)?;
            quietly::<Interacted>(load)?;
            quietly::<Built>(load)?;
            quietly::<TornDown>(load)?;
            quietly::<PickedUp>(load)?;
            quietly::<AltarUsed>(load)?;
            quietly::<LootOpened>(load)?;
            quietly::<EquipmentChanged>(load)?;
            quietly::<Revived>(load)?;
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Watcher);

    /// A fake host answering every engine id of the API, each type id 64
    /// bytes under a hash of its own and each offset id 8, then the hit,
    /// death and kill credit laid out apart, and the world's types.
    fn host() -> FakeHost {
        let mut host = FakeHost::new();
        for (index, id) in (0x100u32..).zip(enshwick::__private::ENGINE_IDS) {
            host = if id.contains('.') {
                host.constant(id, 8)
            } else {
                host.type_hash(id, index).type_size(id, 64)
            };
        }
        let fields = [
            ("hit_event.attacker", 8),
            ("hit_event.target", 12),
            ("hit_event.source", 16),
            ("hit_event.root_target", 20),
            ("hit_event.weapon", 24),
            ("hit_event.health_change", 28),
            ("hit_event.stamina_change", 32),
            ("hit_event.damage_types", 36),
            ("hit_event.attack", 38),
            ("hit_event.target_faction", 39),
            ("died_event.entity", 8),
            ("hit_history.last_index", 0),
            ("hit_history.entries", 8),
            ("hit_history.end", 8 + 2 * 64),
            ("experience_credit.last_player", 4),
        ];
        for (id, offset) in fields {
            host = host.constant(id, offset);
        }
        host.type_hash("hit_event", HIT)
            .type_hash("died_event", DIED)
            .type_size("died_event", 16)
            .type_hash("hit_history", HISTORY)
            .type_size("hit_history", 152)
            .type_hash("experience_credit", CREDIT)
            .type_size("experience_credit", 8)
            .type_hash("keen::ecs::CurrentTransform", 1)
            .type_hash("keen::ecs::UsedItem", 2)
    }

    #[test]
    fn every_typed_kind_resolves_by_engine_ids_alone_and_decodes_its_fields() {
        let mut history = [0u8; 152];
        history[..4].copy_from_slice(&1u32.to_le_bytes());
        history[8 + 64 + 8..8 + 64 + 12].copy_from_slice(&5u32.to_le_bytes());
        let mut credit = [0u8; 8];
        credit[4..].copy_from_slice(&6u32.to_le_bytes());
        let loader = host()
            .component(77, HISTORY, &history)
            .component(77, CREDIT, &credit)
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Watcher)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(
            loader.game_event_types().len(),
            SUBSCRIBED.load(Ordering::Relaxed)
        );

        let mut hit = [0u8; 64];
        for (at, value) in [(8, 5u32), (12, 77), (16, 9), (20, 0), (24, 3)] {
            hit[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        hit[28..32].copy_from_slice(&(-40i32).to_le_bytes());
        hit[36..38].copy_from_slice(&DamageTypes::FIRE.0.to_le_bytes());
        hit[38] = 1;
        hit[39] = 2;
        assert_eq!(loader.raise_game_event(HIT, &hit), 1);
        let hits = HITS.lock().unwrap();
        let [hit] = hits.as_slice() else {
            panic!("{hits:?}")
        };
        assert_eq!(hit.attacker, Some(Entity(5)));
        assert_eq!(hit.target, Entity(77));
        assert_eq!(hit.source, Some(Entity(9)));
        assert_eq!(hit.root_target, None);
        assert_eq!(hit.weapon, Some(Entity(3)));
        assert_eq!((hit.health_change, hit.stamina_change), (-40, 0));
        assert_eq!(hit.damage_types, DamageTypes::FIRE);
        assert_eq!((hit.attack.0, hit.target_faction), (1, 2));

        let mut died = [0u8; 16];
        died[8..12].copy_from_slice(&77u32.to_le_bytes());
        assert_eq!(loader.raise_game_event(DIED, &died), 1);
        let deaths = DEATHS.lock().unwrap();
        let [died] = deaths.as_slice() else {
            panic!("{deaths:?}")
        };
        assert_eq!(died.entity, Entity(77));
        assert_eq!(died.killer, Some(Entity(5)), "{:?}", loader.logs());
        assert_eq!(died.killer_player, Some(Entity(6)));
    }
}
