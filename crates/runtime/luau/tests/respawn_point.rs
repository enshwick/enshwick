//! `enshwick.world.respawn_point` and `set_respawn_point`: a script reads and
//! moves a player's respawn point, as a Rust mod does.

#[cfg(test)]
mod tests {
    use enshwick::Entity;
    use enshwick::abi::{PlayerInfo, RespawnPointInfo};
    use enshwick::test::FakeHost;

    use crate::harness::{logged, run};

    #[test]
    fn a_script_moves_a_player_s_respawn_point_to_an_altar() {
        let loader = run(
            FakeHost::new()
                .world()
                .player(enshwick::abi::headed!(PlayerInfo {
                    slot: 0,
                    entity: Entity::from_engine(2),
                    key: enshwick::PlayerKey::new(7),
                    permissions: 0,
                    name_len: 0,
                    name: [0; PlayerInfo::NAME_MAX],
                }))
                .spawn_point(
                    Entity::from_engine(40),
                    RespawnPointInfo::BASE,
                    [10.0, 2.0, 4.0],
                )
                .spawn_point(
                    Entity::from_engine(41),
                    RespawnPointInfo::NO_RESPAWN,
                    [0.0; 3],
                )
                .respawns_at(Entity::from_engine(2), Entity::from_engine(41)),
            r"
            enshwick.schedule.next_tick(function()
                local world = enshwick.world
                local player = world.player(2)
                assert(world.respawn_point(player).kind == 'NoRespawn')
                local ok, why = pcall(world.set_respawn_point, player, 41)
                assert(not ok and string.find(tostring(why), 'not a spawn point'), tostring(why))
                world.set_respawn_point(player, 40)
                local point = world.respawn_point(player)
                assert(point.entity == 40)
                assert(point.kind == 'Base')
                assert(point.transform.position == vector.create(10, 2, 4))
                enshwick.log.info('respawn point moved')
            end)
            ",
        );
        let _ran = loader.tick(core::time::Duration::from_secs(1));
        assert!(
            logged(&loader, "respawn point moved"),
            "{:?}",
            loader.logs()
        );
    }
}
