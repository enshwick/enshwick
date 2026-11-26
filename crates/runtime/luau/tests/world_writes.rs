//! `enshwick.world` level and experience writes and revives, and
//! `enshwick.effects.drop_items` and permission changes, as a Rust mod's.

#[cfg(test)]
mod tests {
    use enshwick::abi::{
        PROGRESS_ADD_EXPERIENCE, PROGRESS_SET_EXPERIENCE, PROGRESS_SET_LEVEL, PlayerInfo,
    };
    use enshwick::test::FakeHost;
    use enshwick::{Entity, ItemId};

    use crate::harness::{logged, run};

    #[test]
    fn a_script_writes_a_player_s_level_and_experience_and_drops_a_stack() {
        let player = enshwick::abi::headed!(PlayerInfo {
            slot: 0,
            entity: Entity::from_engine(7),
            key: enshwick::PlayerKey::new(7),
            permissions: 0,
            name_len: 0,
            name: [0; PlayerInfo::NAME_MAX],
        });
        let loader = run(
            FakeHost::new()
                .world()
                .player(player)
                .item(ItemId::new(42), 100, 0),
            r#"
            enshwick.on_tick(0.1, function()
                enshwick.world.set_level(7, 10)
                enshwick.world.set_experience(7, 5)
                enshwick.world.add_experience(7, 500)
                local ok, why = pcall(enshwick.world.set_level, 8, 3)
                enshwick.log.info(`level of a stranger {ok}: {why}`)
                local pickup = enshwick.effects.drop_items(42, 5, vector.create(1, 2, 3), { level = 3, rarity = "Rare" })
                enshwick.log.info(`dropped {pickup}`)
            end)
            "#,
        );
        let _ran = loader.tick(std::time::Duration::from_millis(100));
        assert_eq!(
            loader.progress_writes(),
            [
                (Entity::from_engine(7), PROGRESS_SET_LEVEL, 10),
                (Entity::from_engine(7), PROGRESS_SET_EXPERIENCE, 5),
                (Entity::from_engine(7), PROGRESS_ADD_EXPERIENCE, 500),
            ],
            "{:?}",
            loader.logs()
        );
        assert!(
            logged(&loader, "level of a stranger false"),
            "{:?}",
            loader.logs()
        );
        let drops = loader.drops();
        assert_eq!(drops.len(), 1, "{:?}", loader.logs());
        let (entity, request) = drops[0];
        assert!(
            logged(&loader, &format!("dropped {}", entity.get())),
            "{:?}",
            loader.logs()
        );
        assert_eq!(request.item, ItemId(42));
        assert_eq!((request.count, request.level, request.rarity), (5, 3, 2));
    }

    #[test]
    fn a_script_takes_a_player_s_permission_and_gives_it_back() {
        let player = enshwick::abi::headed!(PlayerInfo {
            slot: 0,
            entity: Entity::from_engine(7),
            key: enshwick::PlayerKey::new(7),
            permissions: PlayerInfo::CAN_EDIT_WORLD | PlayerInfo::CAN_EDIT_BASE,
            name_len: 0,
            name: [0; PlayerInfo::NAME_MAX],
        });
        let loader = run(
            FakeHost::new().world().world_key("world0").player(player),
            r#"
            enshwick.on_tick(0.1, function()
                local key = enshwick.world.players()[1].key
                enshwick.world.change_permissions(key, { "CanEditWorld" }, {})
                enshwick.log.info(`taken {enshwick.world.players()[1].permissions}`)
                enshwick.world.change_permissions(key, {}, {})
                enshwick.log.info(`back {enshwick.world.players()[1].permissions}`)
            end)
            "#,
        );
        let _ran = loader.tick(std::time::Duration::from_millis(100));
        assert!(logged(&loader, "taken 4"), "{:?}", loader.logs());
        assert!(logged(&loader, "back 12"), "{:?}", loader.logs());
    }

    #[test]
    fn a_script_hears_what_became_of_its_level_write_after_the_next_step() {
        let player = enshwick::abi::headed!(PlayerInfo {
            slot: 0,
            entity: Entity::from_engine(7),
            key: enshwick::PlayerKey::new(7),
            permissions: 0,
            name_len: 0,
            name: [0; PlayerInfo::NAME_MAX],
        });
        let loader = run(
            FakeHost::new().world().player(player),
            r#"
            local wrote = false
            enshwick.on("ProgressWritten", function(written)
                enshwick.log.info(`written {written.entity} {written.what} {written.value} {written.outcome} {written.level} {written.experience} {written.gained}`)
            end)
            enshwick.on_tick(0.1, function()
                if not wrote then
                    wrote = true
                    enshwick.world.set_level(7, 12)
                end
            end)
            "#,
        );
        let step = std::time::Duration::from_millis(100);
        let _ran = loader.tick(step);
        assert!(!logged(&loader, "written"), "{:?}", loader.logs());
        let _ran = loader.tick(step);
        assert!(
            logged(&loader, "written 7 1 12 0 12 0 0"),
            "{:?}",
            loader.logs()
        );
    }
}
