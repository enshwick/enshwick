//! Player teleports: a player moved to a position or beside another player,
//! as the loader receives the requests.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::{PlayerInfo, Status, TeleportRequest};
    use enshwick::content::EngineFacts;
    use enshwick::effects::{Effects, Teleport};
    use enshwick::test::{FakeHost, facts};
    use enshwick::{Entity, Load, Mod, Off, PlayerKey, WorldError};

    static EFFECTS: OnceLock<Effects> = OnceLock::new();

    struct Gathering;

    impl Mod for Gathering {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = EFFECTS.set(load.effects());
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Gathering,
        r#"
        id = "you.gathering"
        side = "server"
        "#
    );

    fn player(entity: u32, key: u64) -> PlayerInfo {
        enshwick::abi::headed!(PlayerInfo {
            slot: 0,
            entity: Entity::from_engine(entity),
            key: PlayerKey::new(key),
            permissions: 0,
            name_len: 0,
            name: [0; PlayerInfo::NAME_MAX],
        })
    }

    /// A `CurrentTransform` standing at `[x, y, z]` metres.
    fn standing([x, y, z]: [i64; 3]) -> Vec<u8> {
        [x << 32, y << 32, z << 32]
            .iter()
            .flat_map(|c| c.to_le_bytes())
            .collect()
    }

    #[test]
    fn a_player_goes_to_a_position_or_beside_another_player_and_one_offline_is_gone() {
        let host = FakeHost::new().world();
        let transform = host.hash(facts::CURRENT_TRANSFORM.id()).unwrap();
        let loader = host
            .player(player(5, 7))
            .player(player(6, 8))
            .template(Entity::from_engine(5), enshwick::content::Guid([5; 16]))
            .template(Entity::from_engine(6), enshwick::content::Guid([6; 16]))
            .component(Entity::from_engine(6), transform, &standing([40, 12, -3]))
            .install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let effects = EFFECTS.get().unwrap();
        let (ann, bob, gone) = (PlayerKey::new(7), PlayerKey::new(8), PlayerKey::new(9));

        effects
            .teleport_player(ann, [1.0, 2.0, 3.0], Teleport::new().facing_yaw(0.0))
            .unwrap();
        effects
            .teleport_player_to(ann, bob, Teleport::new())
            .unwrap();
        assert_eq!(
            effects.teleport_player(gone, [0.0; 3], Teleport::new()),
            Err(WorldError::Gone)
        );
        assert_eq!(
            effects.teleport_player_to(ann, gone, Teleport::new()),
            Err(WorldError::Gone),
            "the target is offline"
        );
        assert_eq!(
            effects.teleport_player_to(bob, ann, Teleport::new()),
            Err(WorldError::Gone),
            "the target has no place in the world"
        );

        let teleports = loader.teleports();
        assert_eq!(teleports.len(), 2);
        assert_eq!(teleports[0].entity, Entity::from_engine(5));
        assert_eq!(teleports[0].flags, TeleportRequest::ORIENTED);
        assert_eq!(teleports[0].position, [1.0, 2.0, 3.0]);
        assert_eq!(teleports[1].entity, Entity::from_engine(5));
        assert_eq!(
            teleports[1].flags,
            TeleportRequest::BEST_SPAWN,
            "beside another player, the game finds a free spot"
        );
        assert_eq!(teleports[1].position, [40.0, 12.0, -3.0]);
    }

    #[test]
    fn a_teleport_beside_a_player_is_unavailable_on_a_build_without_current_transform() {
        let loader = FakeHost::new()
            .world()
            .without_fact(facts::CURRENT_TRANSFORM)
            .player(player(5, 7))
            .player(player(6, 8))
            .install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let effects = EFFECTS.get().unwrap();

        assert_eq!(
            effects.teleport_player_to(PlayerKey::new(7), PlayerKey::new(8), Teleport::new()),
            Err(WorldError::Unavailable),
            "both players are connected; the build cannot say where the target stands"
        );
        assert!(loader.teleports().is_empty());
    }
}
