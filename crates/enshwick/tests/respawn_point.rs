//! A player's respawn point through the fake host: read, set, refused, and
//! the game's change event heard typed.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{PlayerInfo, RespawnPointInfo, Status};
    use enshwick::content::Guid;
    use enshwick::game_events::{PlayerSpawnInfoChangeEvent, UniqueSceneId};
    use enshwick::world::{Player, PlayerSpawnType, World};
    use enshwick::{Entity, Load, Mod, Off, PlayerKey, WorldError};

    static WORLD: Mutex<Option<World>> = Mutex::new(None);
    static HEARD: Mutex<Vec<PlayerSpawnInfoChangeEvent>> = Mutex::new(Vec::new());

    const CHARACTER: Entity = Entity::from_engine(2);
    const ALTAR: Entity = Entity::from_engine(40);
    const SPIRE: Entity = Entity::from_engine(41);
    const NOWHERE: Entity = Entity::from_engine(42);
    const CHEST: Entity = Entity::from_engine(43);
    const BASE: Guid = Guid([0xb2; 16]);

    struct Homes;

    impl Mod for Homes {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            *WORLD.lock().unwrap() = Some(load.world()?);
            load.on::<PlayerSpawnInfoChangeEvent>(|moved| HEARD.lock().unwrap().push(*moved))?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(Homes, "id = \"test.respawn-point\"\nside = \"server\"");

    fn info() -> PlayerInfo {
        enshwick::abi::headed!(PlayerInfo {
            slot: 0,
            entity: CHARACTER,
            key: PlayerKey::new(7),
            permissions: 0,
            name_len: 0,
            name: [0; PlayerInfo::NAME_MAX],
        })
    }

    fn start() -> (enshwick::test::Installed, World, Player) {
        start_on(enshwick::test::FakeHost::new())
    }

    fn start_on(host: enshwick::test::FakeHost) -> (enshwick::test::Installed, World, Player) {
        let host = host
            .world()
            .game_events()
            .player(info())
            .spawn_point(ALTAR, RespawnPointInfo::BASE, [10.0, 2.0, 4.0])
            .spawn_point(SPIRE, RespawnPointInfo::SAVE_POINT, [-50.0, 30.0, 8.0])
            .spawn_point(NOWHERE, RespawnPointInfo::NO_RESPAWN, [0.0; 3])
            .spawn_point_names(ALTAR, Some(BASE), None)
            .spawn_point_names(SPIRE, None, Some(UniqueSceneId(7)))
            .respawns_at(CHARACTER, SPIRE)
            .install();
        // SAFETY: the exports belong to this test's declared mod.
        let status =
            unsafe { host.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let world = WORLD.lock().unwrap().unwrap();
        let player = world.player(CHARACTER).unwrap();
        (host, world, player)
    }

    #[test]
    fn a_player_s_respawn_point_reads_back_after_a_mod_sets_it() {
        let (_host, world, player) = start();
        let spire = world.respawn_point(&player).unwrap().unwrap();
        assert_eq!(spire.entity, SPIRE);
        assert_eq!(spire.kind, PlayerSpawnType::SavePoint);
        assert_eq!(spire.transform.position, [-50.0, 30.0, 8.0]);

        world.set_respawn_point(&player, ALTAR).unwrap();
        let altar = world.respawn_point(&player).unwrap().unwrap();
        assert_eq!(altar.entity, ALTAR);
        assert_eq!(altar.kind, PlayerSpawnType::Base);
    }

    #[test]
    fn only_a_spawn_point_players_respawn_at_is_taken() {
        let (_host, world, player) = start();
        for refused in [NOWHERE, CHEST] {
            assert_eq!(
                world.set_respawn_point(&player, refused),
                Err(WorldError::NotASpawnPoint)
            );
        }
        assert_eq!(world.respawn_point(&player).unwrap().unwrap().entity, SPIRE);
    }

    #[test]
    fn a_player_who_left_has_no_respawn_point_and_cannot_get_one() {
        let (_host, world, mut player) = start();
        player.entity = Entity::from_engine(9);
        assert_eq!(world.respawn_point(&player), Ok(None));
        assert_eq!(
            world.set_respawn_point(&player, ALTAR),
            Err(WorldError::Gone)
        );
    }

    #[test]
    fn off_the_game_thread_or_with_the_world_stopped_the_calls_refuse() {
        let (host, world, player) = start();
        let elsewhere = player.clone();
        let answers = std::thread::spawn(move || {
            (
                world.respawn_point(&elsewhere),
                world.set_respawn_point(&elsewhere, ALTAR),
            )
        })
        .join()
        .unwrap();
        assert_eq!(answers.0, Err(WorldError::WrongThread));
        assert_eq!(answers.1, Err(WorldError::WrongThread));
        host.stop_world();
        assert_eq!(world.respawn_point(&player), Err(WorldError::Unavailable));
        assert_eq!(
            world.set_respawn_point(&player, ALTAR),
            Err(WorldError::Unavailable)
        );
    }

    #[test]
    fn the_change_event_names_a_base_or_a_scene_and_never_both() {
        let (host, ..) = start();
        let character = Guid([0xa1; 16]);
        let base = Guid([0xb2; 16]);
        assert_eq!(host.respawn_point_moved(character, Some(base), None), 1);
        assert_eq!(
            host.respawn_point_moved(character, None, Some(UniqueSceneId(7))),
            1
        );
        let heard = HEARD.lock().unwrap();
        assert_eq!(heard[0].player_character_id, character);
        assert_eq!(heard[0].base_id, Some(base));
        assert_eq!(heard[0].scene_instance_id, None);
        assert_eq!(heard[1].base_id, None);
        assert_eq!(heard[1].scene_instance_id, Some(UniqueSceneId(7)));
    }

    #[test]
    fn a_mod_s_set_raises_the_change_event_with_the_base_else_the_scene() {
        let (host, world, player) = start();
        world.set_respawn_point(&player, ALTAR).unwrap();
        world.set_respawn_point(&player, SPIRE).unwrap();
        let heard = HEARD.lock().unwrap();
        let mut character = [0; 16];
        character[..4].copy_from_slice(&CHARACTER.get().to_le_bytes());
        assert_eq!(heard.len(), 2);
        assert_eq!(heard[0].player_character_id, Guid(character));
        assert_eq!(heard[0].base_id, Some(BASE));
        assert_eq!(heard[0].scene_instance_id, None);
        assert_eq!(heard[1].base_id, None);
        assert_eq!(heard[1].scene_instance_id, Some(UniqueSceneId(7)));
        drop(host);
    }

    #[test]
    fn without_entity_commands_a_point_still_reads_but_cannot_be_set() {
        let (_host, world, player) =
            start_on(enshwick::test::FakeHost::new().without_entity_commands());
        assert_eq!(world.respawn_point(&player).unwrap().unwrap().entity, SPIRE);
        assert_eq!(
            world.set_respawn_point(&player, ALTAR),
            Err(WorldError::Unavailable)
        );
    }
}
