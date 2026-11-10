//! `enshwick.world` level and experience writes and revives, and
//! `enshwick.effects.drop_items`, as a Rust mod's.

#[cfg(test)]
mod tests {
    use enshwick::abi::{
        Header, PROGRESS_ADD_EXPERIENCE, PROGRESS_SET_EXPERIENCE, PROGRESS_SET_LEVEL, PlayerInfo,
    };
    use enshwick::test::FakeHost;
    use enshwick::{Entity, ItemId};

    use crate::harness::{logged, run};

    #[test]
    fn a_script_writes_a_player_s_level_and_experience_and_drops_a_stack() {
        let player = PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: 0,
            entity: Entity(7),
            key: enshwick::PlayerKey::new(7),
            permissions: 0,
            name_len: 0,
            name: [0; 64],
        };
        let loader = run(
            FakeHost::new().world().player(player),
            r#"
            enshwick.world.set_level(7, 10)
            enshwick.world.set_experience(7, 5)
            enshwick.world.add_experience(7, 500)
            local ok, why = pcall(enshwick.world.set_level, 8, 3)
            enshwick.log.info(`level of a stranger {ok}: {why}`)
            local pickup = enshwick.effects.drop_items(42, 5, vector.create(1, 2, 3), { level = 3, rarity = "Rare" })
            enshwick.log.info(`dropped {pickup}`)
            "#,
        );
        assert_eq!(
            loader.progress_writes(),
            [
                (Entity(7), PROGRESS_SET_LEVEL, 10),
                (Entity(7), PROGRESS_SET_EXPERIENCE, 5),
                (Entity(7), PROGRESS_ADD_EXPERIENCE, 500),
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
            logged(&loader, &format!("dropped {entity}")),
            "{:?}",
            loader.logs()
        );
        assert_eq!(request.item, ItemId(42));
        assert_eq!((request.count, request.level, request.rarity), (5, 3, 2));
    }
}
