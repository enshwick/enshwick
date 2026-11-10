//! A player's level and experience written in the server's level step, a
//! dead character revived where it lies, and items dropped on the ground.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::{
        Header, PROGRESS_ADD_EXPERIENCE, PROGRESS_SET_EXPERIENCE, PROGRESS_SET_LEVEL, PlayerInfo,
        Status,
    };
    use enshwick::affixes::ItemRarity;
    use enshwick::effects::{Effects, ItemDrop};
    use enshwick::test::{FakeHost, Installed};
    use enshwick::world::{Meter, World};
    use enshwick::{Entity, ItemId, Load, Mod, Off, WorldError};

    static WORLD: OnceLock<(World, Effects)> = OnceLock::new();

    struct Writes;

    impl Mod for Writes {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = WORLD.set((load.world()?, load.effects()));
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Writes,
        r#"
        id = "you.world-writes"
        side = "server"
        "#
    );

    /// A health set: id 10, the values' offset at 4, flags at 6, count at
    /// 7, then health in slot 0 and its maximum in slot 7.
    fn health(current: i32, max: i32) -> Vec<u8> {
        let mut bytes = 10u32.to_le_bytes().to_vec();
        bytes.extend(8u16.to_le_bytes());
        bytes.extend([1, 8]);
        let values = [current, 0, 0, 0, 0, 0, 0, max];
        bytes.extend(values.iter().flat_map(|v| v.to_le_bytes()));
        bytes
    }

    fn player(entity: u32) -> PlayerInfo {
        PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: 0,
            entity: Entity(entity),
            key: enshwick::PlayerKey::new(u64::from(entity)),
            permissions: 0,
            name_len: 0,
            name: [0; 64],
        }
    }

    /// The fake with the stat reads' facts, started with this file's mod.
    fn started(host: FakeHost) -> (Installed, World, Effects) {
        let loader = host
            .world()
            .type_hash("attribute_set.health", 1)
            .type_hash("attribute_set.stamina", 2)
            .type_hash("attribute_set.mana", 3)
            .type_hash("attribute_set.level", 4)
            .type_hash("attribute_set.experience", 5)
            .constant("ecs.level_data_offset", 4)
            .constant("ecs.level_flags", 6)
            .constant("ecs.attribute_count", 7)
            .constant("attribute.health", 10)
            .constant("attribute.health_max_slot", 7)
            .constant("attribute.stamina", 20)
            .constant("attribute.stamina_max_slot", 2)
            .constant("attribute.mana", 30)
            .constant("attribute.mana_max_slot", 1)
            .constant("attribute.level", 40)
            .constant("attribute.experience", 50)
            .constant("attribute.experience_needed_slot", 2)
            .install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Writes)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let (world, effects) = *WORLD.get().unwrap();
        (loader, world, effects)
    }

    #[test]
    fn level_and_experience_writes_wait_for_the_level_step_for_players_in_game() {
        let (loader, world, _) = started(FakeHost::new().player(player(7)));

        assert_eq!(world.set_level(Entity(7), 10), Ok(()));
        assert_eq!(world.set_experience(Entity(7), 5), Ok(()));
        assert_eq!(world.add_experience(Entity(7), 500), Ok(()));
        assert_eq!(world.set_level(Entity(8), 3), Err(WorldError::Refused));
        assert_eq!(
            loader.progress_writes(),
            [
                (Entity(7), PROGRESS_SET_LEVEL, 10),
                (Entity(7), PROGRESS_SET_EXPERIENCE, 5),
                (Entity(7), PROGRESS_ADD_EXPERIENCE, 500),
            ]
        );
        for _ in 3..256 {
            assert_eq!(world.add_experience(Entity(7), 1), Ok(()));
        }
        assert_eq!(
            world.add_experience(Entity(7), 1),
            Err(WorldError::Refused),
            "256 writes wait for the step"
        );
        let _handled = loader.tick(std::time::Duration::from_millis(50));
        assert_eq!(world.add_experience(Entity(7), 1), Ok(()), "a new step");
    }

    #[test]
    fn a_dead_character_is_revived_with_its_health_kept_in_bounds_and_a_living_one_is_refused() {
        let (_loader, world, _) = started(
            FakeHost::new()
                .component(Entity(7), 1, &health(0, 200))
                .component(Entity(8), 1, &health(0, 200))
                .component(Entity(9), 1, &health(150, 200)),
        );
        let meter = |current| Meter { current, max: 200 };

        assert_eq!(world.set_health(Entity(7), 50), Err(WorldError::Refused));
        assert_eq!(world.revive(Entity(7), 50), Ok(meter(50)));
        assert_eq!(world.vitals(Entity(7)).map(|v| v.health), Some(meter(50)));
        assert_eq!(world.revive(Entity(7), 80), Err(WorldError::Refused));
        assert_eq!(world.revive(Entity(8), 999), Ok(meter(200)));
        assert_eq!(world.revive(Entity(9), 10), Err(WorldError::Refused));
        assert_eq!(
            world.set_health(Entity(9), 100),
            Ok(meter(100)),
            "the living"
        );
        assert_eq!(world.revive(Entity(10), 10), Err(WorldError::Gone));
    }

    #[test]
    fn a_drop_carries_its_item_count_level_rarity_and_place_and_refuses_no_items() {
        let (loader, _, effects) = started(FakeHost::new().item(ItemId(0x2a), 100, 0));

        assert_eq!(
            effects.drop_items(ItemId(0x2b), 5, [0.0; 3], ItemDrop::new()),
            Err(WorldError::Refused),
            "an item the game does not know"
        );
        let how = ItemDrop::new().level(3).rarity(ItemRarity::Rare);
        let dropped = effects.drop_items(ItemId(0x2a), 5, [1.0, 2.0, -3.0], how);
        let entity = dropped.unwrap();
        assert_eq!(
            effects.drop_items(ItemId(0x2a), 0, [0.0; 3], ItemDrop::new()),
            Err(WorldError::Refused)
        );
        assert_eq!(
            effects.drop_items(ItemId(0x2a), 65_536, [0.0; 3], ItemDrop::new()),
            Err(WorldError::Refused)
        );
        let drops = loader.drops();
        assert_eq!(drops.len(), 1, "{drops:?}");
        let (handed, request) = drops[0];
        assert_eq!(handed, entity);
        assert_eq!(request.item, ItemId(0x2a));
        assert_eq!((request.count, request.level, request.rarity), (5, 3, 2));
        assert_eq!(request.position, [1 << 32, 2 << 32, -(3 << 32)]);
    }
}
