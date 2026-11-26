//! The respawn point's catalogue rows against the real server binary, which
//! never enters the repository: these need `ENSH_SERVER_EXE` to point at a
//! local copy and are ignored by default.

#[cfg(test)]
mod tests {
    use enshwick_base::__private::ids;
    use enshwick_gamedata::catalogue::{constant, type_name};
    use enshwick_gamedata::reflect::{Image, Layout, Reflection};

    fn with_server(check: impl FnOnce(&Reflection<'_>)) {
        let path =
            std::env::var("ENSH_SERVER_EXE").expect("ENSH_SERVER_EXE names the game executable");
        let bytes = std::fs::read(path).unwrap();
        let image = Image::parse(&bytes, Layout::File, None).unwrap();
        check(&Reflection::scan(&image));
    }

    #[test]
    #[ignore = "needs ENSH_SERVER_EXE"]
    fn the_spawn_point_target_s_entity_is_where_the_game_writes_it() {
        with_server(|reflection| {
            let at = |id| constant(id, reflection).unwrap_or_else(|| panic!("{id}"));
            let target_info = reflection.type_info(type_name(ids::TARGET_INFO)).unwrap();
            let entity = at(ids::SELECTED_TARGETS_TARGETS)
                + at(ids::TARGET_TYPE_SPAWN_POINT) * u64::from(target_info.size())
                + at(ids::TARGET_INFO_ENTITY);
            // `sys.player_update_important_locations` writes it at +0x52c
            // (server 26430253).
            assert_eq!(entity, 0x52c);
        });
    }

    #[test]
    #[ignore = "needs ENSH_SERVER_EXE"]
    fn the_spawn_types_and_the_change_event_match_the_game_s_code() {
        with_server(|reflection| {
            let at = |id| constant(id, reflection).unwrap_or_else(|| panic!("{id}"));
            let kinds = [
                ids::PLAYER_SPAWN_TYPE_START_LOCATION,
                ids::PLAYER_SPAWN_TYPE_SAVE_POINT,
                ids::PLAYER_SPAWN_TYPE_BASE,
                ids::PLAYER_SPAWN_TYPE_NO_RESPAWN,
            ];
            assert_eq!(kinds.map(at), [0, 1, 2, 3]);
            let event = reflection
                .type_info(type_name(ids::PLAYER_SPAWN_INFO_CHANGE_EVENT))
                .unwrap();
            // The game pushes 48 bytes and fills +8, +24 and +40.
            assert_eq!(event.size(), 0x30);
            assert_eq!(at(ids::PLAYER_SPAWN_INFO_CHANGE_EVENT_CHARACTER), 8);
            assert_eq!(at(ids::PLAYER_SPAWN_INFO_CHANGE_EVENT_BASE), 24);
            assert_eq!(at(ids::PLAYER_SPAWN_INFO_CHANGE_EVENT_SCENE), 40);
            for id in [
                ids::PLAYER_SPAWN_POINT_TARGET,
                ids::BASE_ID_COMPONENT_BASE_ID,
                ids::STORED_SCENE_ID_UNIQUE_ID,
            ] {
                let _ = at(id);
            }
        });
    }
}
