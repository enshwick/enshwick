//! Typed game events resolve their types and fields by the loader's engine
//! ids, never by an engine name.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use enshwick::Entity;
    use enshwick::abi::Status;
    use enshwick::events::Kind;
    use enshwick::game_events::{
        AcceptOfferedInteractionEvent, AttackCategoryMask, BuildingPickUpEvent, BuildingPlaceEvent,
        BuildingTearDownEvent, CraftRecipeEvent, CraftRecipeSuccessEvent, DamageTypeMask,
        EntityDiedEvent, EquipmentSlot, FlameAltarEvent, HandleEquipmentSlotChangedEvent, HitEvent,
        LevelUpEvent, LootInteractionEvent, RevivedPlayerEvent, XpGainedEvent,
    };
    use enshwick::test::FakeHost;
    use enshwick::{Load, Mod, Off};

    const HIT: u32 = 0x4000;
    const DIED: u32 = 0x4001;
    const HISTORY: u32 = 0x4002;
    const CREDIT: u32 = 0x4003;
    const EQUIPPED: u32 = 0x4004;

    static HITS: Mutex<Vec<HitEvent>> = Mutex::new(Vec::new());
    static DEATHS: Mutex<Vec<EntityDiedEvent>> = Mutex::new(Vec::new());
    static EQUIPPED_SLOTS: Mutex<Vec<EquipmentSlot>> = Mutex::new(Vec::new());
    static SUBSCRIBED: AtomicUsize = AtomicUsize::new(0);

    fn quietly<E: Kind>(load: &mut Load<'_>) -> Result<(), Off> {
        load.on::<E>(|_| {})?;
        SUBSCRIBED.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    struct Watcher;

    impl Mod for Watcher {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.on::<HitEvent>(|hit| HITS.lock().unwrap().push(*hit))?;
            load.on::<EntityDiedEvent>(|died| DEATHS.lock().unwrap().push(*died))?;
            load.on::<HandleEquipmentSlotChangedEvent>(|changed| {
                EQUIPPED_SLOTS.lock().unwrap().push(changed.equipment_slot);
            })?;
            SUBSCRIBED.fetch_add(3, Ordering::Relaxed);
            quietly::<LevelUpEvent>(load)?;
            quietly::<XpGainedEvent>(load)?;
            quietly::<CraftRecipeEvent>(load)?;
            quietly::<CraftRecipeSuccessEvent>(load)?;
            quietly::<AcceptOfferedInteractionEvent>(load)?;
            quietly::<BuildingPlaceEvent>(load)?;
            quietly::<BuildingTearDownEvent>(load)?;
            quietly::<BuildingPickUpEvent>(load)?;
            quietly::<FlameAltarEvent>(load)?;
            quietly::<LootInteractionEvent>(load)?;
            quietly::<RevivedPlayerEvent>(load)?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Watcher,
        r#"
        id = "you.kinds"
        side = "server"
        "#
    );

    /// A fake host answering every engine id of the API, each type id 64
    /// bytes under a hash of its own and each offset or enum constant id 8,
    /// then the hit, death, kill credit and equipment change laid out
    /// apart, engine bits and slots unlike the API's, and the world's types.
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
            ("hit_event.root_source_id", 8),
            ("hit_event.target_id", 12),
            ("hit_event.source_id", 16),
            ("hit_event.root_target_id", 20),
            ("hit_event.weapon_pide_id", 24),
            ("hit_event.health_change", 28),
            ("hit_event.stamina_change", 32),
            ("hit_event.damage_types", 36),
            ("hit_event.attack_category", 38),
            ("hit_event.target_faction", 39),
            ("entity_died_event.dead_entity_id", 8),
            ("hit_history.last_index", 0),
            ("hit_history.entries", 8),
            ("hit_history.end", 8 + 2 * 64),
            ("experience_credit.last_player", 4),
            ("handle_equipment_slot_changed_event.equipment_slot", 12),
            ("damage_type.magical_fire", 0),
            ("damage_type.physical_cut", 5),
            ("attack_category.melee", 2),
            ("equipment_slot.shield", 40),
        ];
        for (id, offset) in fields {
            host = host.constant(id, offset);
        }
        host.type_hash("hit_event", HIT)
            .type_hash("entity_died_event", DIED)
            .type_size("entity_died_event", 16)
            .type_hash("hit_history", HISTORY)
            .type_size("hit_history", 152)
            .type_hash("experience_credit", CREDIT)
            .type_hash("handle_equipment_slot_changed_event", EQUIPPED)
            .type_size("experience_credit", 8)
            .type_hash("current_transform", 1)
            .type_hash("used_item", 2)
    }

    #[test]
    fn every_typed_kind_resolves_by_engine_ids_alone_and_decodes_its_fields() {
        let mut history = [0u8; 152];
        history[..4].copy_from_slice(&1u32.to_le_bytes());
        history[8 + 64 + 8..8 + 64 + 12].copy_from_slice(&5u32.to_le_bytes());
        let mut credit = [0u8; 8];
        credit[4..].copy_from_slice(&6u32.to_le_bytes());
        let loader = host()
            .component(Entity(77), HISTORY, &history)
            .component(Entity(77), CREDIT, &credit)
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
        hit[36..38].copy_from_slice(&(1u16 | 1 << 5 | 1 << 15).to_le_bytes());
        hit[38] = 1 << 2;
        hit[39] = 2;
        assert_eq!(loader.raise_game_event(HIT, &hit), 1);
        let hits = HITS.lock().unwrap();
        let [hit] = hits.as_slice() else {
            panic!("{hits:?}")
        };
        assert_eq!(hit.root_source_id, Some(Entity(5)));
        assert_eq!(hit.target_id, Entity(77));
        assert_eq!(hit.source_id, Some(Entity(9)));
        assert_eq!(hit.root_target_id, None);
        assert_eq!(hit.weapon_pide_id, Some(Entity(3)));
        assert_eq!((hit.health_change, hit.stamina_change), (-40, 0));
        assert_eq!(
            hit.damage_types,
            DamageTypeMask::MAGICAL_FIRE | DamageTypeMask::PHYSICAL_CUT,
            "engine bits the API does not name drop"
        );
        assert_eq!(hit.attack_category, AttackCategoryMask::MELEE);
        assert_eq!(hit.target_faction, 2);

        let mut died = [0u8; 16];
        died[8..12].copy_from_slice(&77u32.to_le_bytes());
        assert_eq!(loader.raise_game_event(DIED, &died), 1);
        let deaths = DEATHS.lock().unwrap();
        let [died] = deaths.as_slice() else {
            panic!("{deaths:?}")
        };
        assert_eq!(died.dead_entity_id, Entity(77));
        #[cfg(feature = "unstable")]
        {
            assert_eq!(died.killer, Some(Entity(5)), "{:?}", loader.logs());
            assert_eq!(died.killer_player, Some(Entity(6)));
        }

        let mut equipped = [0u8; 64];
        for slot in [40u8, 41] {
            equipped[12] = slot;
            assert_eq!(loader.raise_game_event(EQUIPPED, &equipped), 1);
        }
        assert_eq!(
            *EQUIPPED_SLOTS.lock().unwrap(),
            [EquipmentSlot::Shield, EquipmentSlot::Other(41)]
        );
    }
}
