//! Typed game events resolve their types and fields by the loader's engine
//! ids, never by an engine name.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use enshwick::Entity;
    use enshwick::abi::Status;
    use enshwick::content::EngineFacts;
    use enshwick::events::Kind;
    use enshwick::game_events::{
        AcceptOfferedInteractionEvent, AttackCategoryMask, BuildingPickUpEvent, BuildingPlaceEvent,
        BuildingTearDownEvent, CraftRecipeEvent, CraftRecipeSuccessEvent, DamageTypeMask,
        EntityDiedEvent, EquipmentSlot, FlameAltarEvent, HandleEquipmentSlotChangedEvent, HitEvent,
        LevelUpEvent, LootInteractionEvent, RevivedPlayerEvent, XpGainedEvent,
    };
    use enshwick::test::FakeHost;
    use enshwick::test::facts::{self, Fact};
    use enshwick::{Load, Mod, Off};

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
    /// then the game events' preset and the world's types.
    fn host() -> FakeHost {
        let mut host = FakeHost::new();
        for (index, id) in (0x100u32..).zip(enshwick::__private::ENGINE_IDS) {
            host = if id.contains('.') {
                host.constant(id, 8)
            } else {
                host.type_hash(id, index).type_size(id, 64)
            };
        }
        host.game_events().world()
    }

    /// `value` at the offset `field` names in `bytes`.
    fn put(host: &FakeHost, bytes: &mut [u8], field: Fact, value: &[u8]) {
        let at = host.offset(field.id()).unwrap();
        bytes[at..at + value.len()].copy_from_slice(value);
    }

    /// A blob of the size `ty` names.
    fn blob(host: &FakeHost, ty: Fact) -> Vec<u8> {
        vec![0; host.size(ty.id()).unwrap()]
    }

    /// The engine bit `fact` numbers.
    fn bit(host: &FakeHost, fact: Fact) -> u16 {
        1 << host.offset(fact.id()).unwrap()
    }

    /// A hit history whose last hit, the second, came from entity 5, and an
    /// experience credit naming player 6.
    fn kill_credit(host: &FakeHost) -> (Vec<u8>, Vec<u8>) {
        let mut history = blob(host, facts::HIT_HISTORY);
        let last = 1u32.to_le_bytes();
        put(host, &mut history, facts::HIT_HISTORY_LAST_INDEX, &last);
        let second = host.offset(facts::HIT_HISTORY_ENTRIES.id()).unwrap()
            + host.size(facts::HIT_EVENT.id()).unwrap();
        let attacker = 5u32.to_le_bytes();
        put(
            host,
            &mut history[second..],
            facts::HIT_EVENT_ROOT_SOURCE_ID,
            &attacker,
        );
        let mut credit = blob(host, facts::EXPERIENCE_CREDIT);
        let player = 6u32.to_le_bytes();
        put(
            host,
            &mut credit,
            facts::EXPERIENCE_CREDIT_LAST_PLAYER,
            &player,
        );
        (history, credit)
    }

    /// A melee hit by 5 through 9 with weapon 3 on 77 of faction 2, for 40
    /// health, of magical fire, physical cut and an engine bit the API does
    /// not name.
    fn hit(host: &FakeHost) -> Vec<u8> {
        let mut hit = blob(host, facts::HIT_EVENT);
        for (field, value) in [
            (facts::HIT_EVENT_ROOT_SOURCE_ID, 5u32),
            (facts::HIT_EVENT_TARGET_ID, 77),
            (facts::HIT_EVENT_SOURCE_ID, 9),
            (facts::HIT_EVENT_ROOT_TARGET_ID, 0),
            (facts::HIT_EVENT_WEAPON_PIDE_ID, 3),
        ] {
            put(host, &mut hit, field, &value.to_le_bytes());
        }
        let health = (-40i32).to_le_bytes();
        put(host, &mut hit, facts::HIT_EVENT_HEALTH_CHANGE, &health);
        let damage = bit(host, facts::DAMAGE_TYPE_MAGICAL_FIRE)
            | bit(host, facts::DAMAGE_TYPE_PHYSICAL_CUT)
            | 1 << 15;
        put(
            host,
            &mut hit,
            facts::HIT_EVENT_DAMAGE_TYPES,
            &damage.to_le_bytes(),
        );
        let melee = u8::try_from(bit(host, facts::ATTACK_CATEGORY_MELEE)).unwrap();
        put(host, &mut hit, facts::HIT_EVENT_ATTACK_CATEGORY, &[melee]);
        put(host, &mut hit, facts::HIT_EVENT_TARGET_FACTION, &[2]);
        hit
    }

    #[test]
    fn every_typed_kind_resolves_by_engine_ids_alone_and_decodes_its_fields() {
        let host = host();
        let hash = |ty: Fact| host.hash(ty.id()).unwrap();
        let (hit_event, died_event, equipped_event) = (
            hash(facts::HIT_EVENT),
            hash(facts::ENTITY_DIED_EVENT),
            hash(facts::HANDLE_EQUIPMENT_SLOT_CHANGED_EVENT),
        );
        let (history_type, credit_type) =
            (hash(facts::HIT_HISTORY), hash(facts::EXPERIENCE_CREDIT));
        let shield = u8::try_from(host.offset(facts::EQUIPMENT_SLOT_SHIELD.id()).unwrap()).unwrap();
        let (history, credit) = kill_credit(&host);
        let hit = hit(&host);

        let mut died = blob(&host, facts::ENTITY_DIED_EVENT);
        put(
            &host,
            &mut died,
            facts::ENTITY_DIED_EVENT_DEAD_ENTITY_ID,
            &77u32.to_le_bytes(),
        );
        let mut equipped = blob(&host, facts::HANDLE_EQUIPMENT_SLOT_CHANGED_EVENT);
        let slot = host
            .offset(facts::HANDLE_EQUIPMENT_SLOT_CHANGED_EVENT_EQUIPMENT_SLOT.id())
            .unwrap();

        let loader = host
            .component(Entity(77), history_type, &history)
            .component(Entity(77), credit_type, &credit)
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Watcher)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(
            loader.game_event_types().len(),
            SUBSCRIBED.load(Ordering::Relaxed)
        );

        assert_eq!(loader.raise_game_event(hit_event, &hit), 1);
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

        assert_eq!(loader.raise_game_event(died_event, &died), 1);
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

        for number in [shield, shield + 1] {
            equipped[slot] = number;
            assert_eq!(loader.raise_game_event(equipped_event, &equipped), 1);
        }
        assert_eq!(
            *EQUIPPED_SLOTS.lock().unwrap(),
            [EquipmentSlot::Shield, EquipmentSlot::Other(shield + 1)]
        );
    }
}
