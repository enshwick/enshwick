//! Terrain materials of a mod's own, as the loader receives and lists them.

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use enshwick::abi::{Status, TerrainMaterialInfo};
    use enshwick::content::{Guid, Id};
    use enshwick::terrain::{NewTerrainMaterial, Terrain};
    use enshwick::{Load, Mod, Off};

    const SAND: u8 = 9;

    /// Eleven more copies of the sand: one past the game's free ids, with
    /// the tar sand first.
    const SANDS: [&str; 11] = [
        "you.sands:sand-1",
        "you.sands:sand-2",
        "you.sands:sand-3",
        "you.sands:sand-4",
        "you.sands:sand-5",
        "you.sands:sand-6",
        "you.sands:sand-7",
        "you.sands:sand-8",
        "you.sands:sand-9",
        "you.sands:sand-10",
        "you.sands:sand-11",
    ];

    static TERRAIN: OnceLock<Terrain> = OnceLock::new();

    /// What each add answered, in order.
    static ADDED: Mutex<Vec<Result<u8, String>>> = Mutex::new(Vec::new());

    struct Sands;

    impl Mod for Sands {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let tar = NewTerrainMaterial::new(Id::new("you.sands:tar-sand"), SAND)
                .terrain_item(Guid([7; 16]))
                .health_points(7)
                .can_be_flattened(false);
            let mut answers = vec![load.add_terrain_material(tar)];
            for name in SANDS {
                answers
                    .push(load.add_terrain_material(NewTerrainMaterial::new(Id::new(name), SAND)));
            }
            let unknown = NewTerrainMaterial::new(Id::new("you.sands:odd"), 117);
            answers.push(load.add_terrain_material(unknown));
            let weak = NewTerrainMaterial::new(Id::new("you.sands:weak"), SAND).health_points(0);
            answers.push(load.add_terrain_material(weak));
            let harder = NewTerrainMaterial::new(Id::new("you.sands:harder"), SAND).hardness(6);
            answers.push(load.add_terrain_material(harder));
            let _first = TERRAIN.set(load.terrain());
            *ADDED.lock().unwrap() = answers
                .into_iter()
                .map(|answer| answer.map_err(|off| off.to_string()))
                .collect();
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Sands,
        r#"
        id = "you.sands"
        side = "both"
        "#
    );

    fn sand() -> TerrainMaterialInfo {
        enshwick::abi::headed!(TerrainMaterialInfo {
            id: SAND,
            flags: TerrainMaterialInfo::FLATTENS,
            hardness: 1,
            reserved: 0,
            item: enshwick::ItemId::new(77),
            health: 3,
        })
    }

    #[test]
    fn materials_take_the_free_ids_in_order_and_the_twelfth_is_refused() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Sands)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let added = ADDED.lock().unwrap().clone();
        let ids: Vec<u8> = added[..11].iter().map(|a| *a.as_ref().unwrap()).collect();
        assert_eq!(ids, (117..=127).collect::<Vec<u8>>());
        assert!(added[11].is_err(), "the twelfth: {:?}", added[11]);
        assert!(
            added[12].is_err(),
            "a material the game lacks: {:?}",
            added[12]
        );
        assert!(added[13].is_err(), "a health of 0: {:?}", added[13]);
        assert!(
            added[14].is_err(),
            "a hardness past Unbreakable: {:?}",
            added[14]
        );

        loader.set_terrain(&[sand()]);
        let materials = TERRAIN.get().unwrap().materials();
        let tar = materials
            .iter()
            .find(|m| m.terrain_material == 117)
            .unwrap();
        assert_eq!(
            tar.terrain_item,
            enshwick::ItemId::new(Guid([7; 16]).fnv1a())
        );
        assert_eq!(
            (tar.health_points, tar.hardness),
            (7, 1),
            "its hardness is the sand's"
        );
        assert!(!tar.can_be_flattened);
        assert_eq!(materials.len(), 12, "the sand and the 11 added");
    }
}

#[cfg(test)]
mod server_only {
    use std::sync::Mutex;

    use enshwick::abi::Status;
    use enshwick::content::Id;
    use enshwick::terrain::NewTerrainMaterial;
    use enshwick::{Load, Mod, Off};

    /// What the add answered.
    static ADDED: Mutex<Option<Result<u8, String>>> = Mutex::new(None);

    struct Tar;

    impl Mod for Tar {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let tar = NewTerrainMaterial::new(Id::new("you.tar:tar-sand"), 9);
            *ADDED.lock().unwrap() = Some(
                load.add_terrain_material(tar)
                    .map_err(|off| off.to_string()),
            );
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Tar,
        r#"
        id = "you.tar"
        side = "server"
        "#
    );

    #[test]
    fn a_mod_clients_do_not_run_is_refused_a_terrain_material() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of this module's `declare_mod!(Tar)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let added = ADDED.lock().unwrap().clone().unwrap();
        assert!(added.is_err(), "a server-only mod: {added:?}");
    }
}
