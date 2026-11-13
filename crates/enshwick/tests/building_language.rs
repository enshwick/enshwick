//! A building event's handler reads the language of the player whose
//! character placed or removed the building.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{BuildingData, Header, PlayerInfo, Status};
    use enshwick::events::{BuildingPlaced, BuildingRemoved};
    use enshwick::i18n::{Language, language};
    use enshwick::{Entity, ItemId, Load, Mod, Off, PlayerKey};

    /// The language each handler read, in order.
    static READ: Mutex<Vec<Language>> = Mutex::new(Vec::new());

    struct Labels;

    impl Mod for Labels {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.on::<BuildingPlaced>(|_| READ.lock().unwrap().push(language()))?;
            load.on::<BuildingRemoved>(|_| READ.lock().unwrap().push(language()))?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Labels,
        r#"
        id = "you.labels"
        side = "server"
        "#
    );

    fn character(entity: u32, key: u64) -> PlayerInfo {
        PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: 0,
            entity: Entity(entity),
            key: PlayerKey::new(key),
            permissions: 0,
            name_len: 0,
            name: [0; 64],
        }
    }

    fn built(player: u32, kind: u8) -> BuildingData {
        BuildingData {
            header: Header::of::<BuildingData>(),
            entity: Entity(4096),
            player: Entity(player),
            item: ItemId::new(1_302_084_846),
            kind,
            replay: 0,
            template: [0; 16],
            position: [1.0, 2.0, 3.0],
        }
    }

    #[test]
    fn a_building_event_reads_in_the_language_of_the_player_who_built() {
        let host = Language::ALL
            .iter()
            .copied()
            .find(|l| *l != Language::UK_UA && *l != Language::FALLBACK)
            .unwrap();
        let loader = enshwick::test::FakeHost::new()
            .language(host)
            .player(character(2, 7))
            .player(character(3, 8))
            .player_language(PlayerKey::new(7), Language::UK_UA)
            .install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());

        assert_eq!(
            loader.raise::<BuildingPlaced>(&built(2, BuildingData::PLACED)),
            1
        );
        assert_eq!(
            loader.raise::<BuildingRemoved>(&built(2, BuildingData::TORN_DOWN)),
            1
        );
        assert_eq!(
            loader.raise::<BuildingPlaced>(&built(3, BuildingData::PLACED)),
            1
        );
        assert_eq!(
            loader.raise::<BuildingPlaced>(&built(9, BuildingData::PLACED)),
            1
        );

        assert_eq!(
            *READ.lock().unwrap(),
            [Language::UK_UA, Language::UK_UA, Language::FALLBACK, host],
            "the builder's, the fallback for a game without Enshwick, the host's for no player"
        );
        assert_eq!(language(), host, "the host's after the events");
    }
}
