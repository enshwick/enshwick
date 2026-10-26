//! Terrain: materials and edits, as the loader receives them.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::{Header, Status, TerrainEdit, TerrainMaterialInfo};
    use enshwick::terrain::{Shape, Terrain};
    use enshwick::world::WorldError;
    use enshwick::{Load, Mod, Off};

    static TERRAIN: OnceLock<Terrain> = OnceLock::new();

    struct Digger;

    impl Mod for Digger {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.digger", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = TERRAIN.set(load.terrain());
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Digger);

    #[test]
    fn edits_reach_the_loader_as_asked() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Digger)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let terrain = TERRAIN.get().unwrap();

        assert!(terrain.materials().is_empty());
        assert_eq!(
            terrain.dig([1.0; 3], 2.0, Shape::Round, &[]),
            Err(WorldError::Missing),
            "no world runs yet"
        );
        let rock = TerrainMaterialInfo {
            header: Header::of::<TerrainMaterialInfo>(),
            id: 9,
            flags: 0,
            hardness: 3,
            reserved: 0,
            item: 77,
            health: 500,
        };
        loader.set_terrain(&[rock]);
        let materials = terrain.materials();
        assert_eq!(materials.len(), 1);
        assert_eq!(
            (materials[0].terrain_material, materials[0].terrain_item),
            (9, 77)
        );
        assert!(!materials[0].can_be_flattened);

        terrain
            .add([10.0, 20.0, 30.0], 3.0, 1.5, 9, Shape::Round)
            .unwrap();
        terrain
            .dig([10.0, 20.0, 30.0], 4.0, Shape::Square, &[9])
            .unwrap();
        assert!(
            terrain
                .dry()
                .flatten([10.0, 20.0, 30.0], 2.0, 9, Shape::Round)
                .unwrap()
        );
        assert_eq!(
            terrain.dig([-1.0, 0.0, 0.0], 2.0, Shape::Round, &[]),
            Err(WorldError::Refused)
        );

        let edits = loader.terrain_edits();
        assert_eq!(edits.len(), 3);
        assert_eq!((edits[0].kind, edits[0].material), (TerrainEdit::ADD, 9));
        assert!((edits[0].height - 1.5).abs() < f32::EPSILON);
        assert_eq!(
            (edits[1].kind, edits[1].shape),
            (TerrainEdit::DIG, TerrainEdit::SQUARE)
        );
        assert_eq!(edits[1].protect[1], 1 << 1, "id 9 is bit 1 of byte 1");
        assert_eq!(edits[2].flags, TerrainEdit::DRY_RUN);
    }
}
