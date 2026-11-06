//! Voxel edits: every write of the world's voxels, as the loader raises it
//! after the step, and a replay that keeps edits across a restart.

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use enshwick::abi::{Header, Status, TerrainMaterialInfo};
    use enshwick::voxels::{EditSource, Voxel, VoxelChange, VoxelMaterial, VoxelPos, Voxels};
    #[cfg(feature = "unstable")]
    use enshwick::world::World;
    use enshwick::{Load, Mod, Off};

    #[cfg(feature = "unstable")]
    static WORLD: OnceLock<World> = OnceLock::new();
    static VOXELS: OnceLock<Voxels> = OnceLock::new();
    /// The edits the mod heard, with their source.
    static HEARD: Mutex<Vec<(EditSource, Vec<VoxelChange>)>> = Mutex::new(Vec::new());
    /// The box of each edit the mod heard.
    static BOXES: Mutex<Vec<(VoxelPos, [u32; 3])>> = Mutex::new(Vec::new());

    struct Keeper;

    impl Mod for Keeper {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            #[cfg(feature = "unstable")]
            let _first = WORLD.set(load.world()?);
            let _first = VOXELS.set(load.voxels());
            load.on_voxels_changed(|edit| {
                HEARD
                    .lock()
                    .unwrap()
                    .push((edit.source, edit.changes.clone()));
                BOXES.lock().unwrap().push((edit.min, edit.size));
            })?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Keeper,
        r#"
        id = "you.keeper"
        side = "server"
        "#
    );

    const ROCK: Voxel = Voxel::new(VoxelMaterial::Terrain(9), 255);
    const AIR: Voxel = Voxel::new(VoxelMaterial::Air, 0);

    fn change(at: VoxelPos, before: Voxel, after: Voxel) -> VoxelChange {
        VoxelChange::new(at, before, after)
    }

    fn started() -> enshwick::test::Installed {
        started_over(VoxelPos::new(10, 20, 30), [3, 2, 2], &[ROCK; 12])
    }

    fn started_over(min: VoxelPos, size: [u32; 3], ground: &[Voxel]) -> enshwick::test::Installed {
        HEARD.lock().unwrap().clear();
        BOXES.lock().unwrap().clear();
        let loader = enshwick::test::FakeHost::new()
            .world()
            .voxels(min, size, ground)
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Keeper)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        loader.set_terrain(&[TerrainMaterialInfo {
            header: Header::of::<TerrainMaterialInfo>(),
            id: 9,
            flags: 0,
            hardness: 3,
            reserved: 0,
            item: enshwick::ItemId::new(21),
            health: 500,
        }]);
        loader
    }

    #[test]
    fn a_player_s_dig_reaches_the_mod_as_the_game_s_edit_with_each_changed_voxel() {
        let loader = started();
        let dug = change(VoxelPos::new(10, 20, 30), ROCK, AIR);

        assert_eq!(loader.voxels_changed(EditSource::Game, &[dug]), 1);
        assert_eq!(*HEARD.lock().unwrap(), [(EditSource::Game, vec![dug])]);
    }

    #[test]
    fn an_edit_names_the_box_of_the_game_s_bricks_it_wrote() {
        let loader = started();
        let dug = change(VoxelPos::new(10, 20, 30), ROCK, AIR);
        let far = change(VoxelPos::new(17, 20, 30), ROCK, AIR);

        assert_eq!(loader.voxels_changed(EditSource::Game, &[dug, far]), 1);
        assert_eq!(
            *BOXES.lock().unwrap(),
            [(VoxelPos::new(8, 16, 24), [16, 8, 8])]
        );
    }

    #[test]
    fn a_write_that_changes_nothing_raises_no_edit() {
        let loader = started();
        let same = change(VoxelPos::new(10, 20, 30), ROCK, ROCK);

        assert_eq!(loader.voxels_changed(EditSource::Game, &[same]), 0);
        assert_eq!(loader.voxels_changed(EditSource::OtherMod, &[]), 0);
        assert!(HEARD.lock().unwrap().is_empty());
    }

    #[test]
    fn another_mod_s_edit_is_told_apart_from_this_mod_s() {
        let loader = started();
        let filled = change(VoxelPos::new(11, 20, 30), AIR, ROCK);

        let _ran = loader.voxels_changed(EditSource::OtherMod, &[filled]);
        let _ran = loader.voxels_changed(EditSource::ThisMod, &[filled]);
        let sources: Vec<_> = HEARD.lock().unwrap().iter().map(|h| h.0).collect();
        assert_eq!(sources, [EditSource::OtherMod, EditSource::ThisMod]);
    }

    #[test]
    #[cfg(feature = "unstable")]
    fn a_replay_of_stored_edits_reaches_the_mod_as_its_own_with_only_the_voxels_it_changed() {
        let loader = started();
        let dug = change(VoxelPos::new(10, 20, 30), ROCK, AIR);
        let up = change(VoxelPos::new(12, 21, 30), ROCK, AIR);
        let far = change(VoxelPos::new(12, 21, 31), ROCK, AIR);
        let _ran = loader.voxels_changed(EditSource::Game, &[dug, up, far]);
        let stored: Vec<VoxelChange> = HEARD
            .lock()
            .unwrap()
            .iter()
            .filter(|(source, _)| *source != EditSource::ThisMod)
            .flat_map(|(_, changes)| changes.clone())
            .collect();
        HEARD.lock().unwrap().clear();

        let world = WORLD.get().unwrap();
        let voxels = VOXELS.get().unwrap();
        let mut kept = voxels.read(VoxelPos::new(10, 20, 30), [3, 2, 2]).unwrap();
        for change in &stored {
            assert!(kept.set(change.at, change.after));
        }
        world.transaction().write_voxels(&kept).commit().unwrap();
        loader.run_transactions();

        assert_eq!(
            *HEARD.lock().unwrap(),
            [(EditSource::ThisMod, vec![dug, up, far])],
            "in the box's order, x fastest, then y, then z"
        );
    }

    #[test]
    #[cfg(feature = "unstable")]
    fn a_transaction_and_its_undo_reach_the_mod_brick_by_brick_as_its_own() {
        let loader = started_over(VoxelPos::new(6, 20, 30), [4, 1, 1], &[AIR, AIR, AIR, ROCK]);
        let world = WORLD.get().unwrap();
        let mut filled = VOXELS
            .get()
            .unwrap()
            .read(VoxelPos::new(6, 20, 30), [4, 1, 1])
            .unwrap();
        for x in 6..9 {
            assert!(filled.set(VoxelPos::new(x, 20, 30), ROCK));
        }
        let id = world.transaction().write_voxels(&filled).commit().unwrap();
        loader.run_transactions();
        let at = |x| VoxelPos::new(x, 20, 30);
        assert_eq!(
            *HEARD.lock().unwrap(),
            [
                (
                    EditSource::ThisMod,
                    vec![change(at(6), AIR, ROCK), change(at(7), AIR, ROCK)]
                ),
                (EditSource::ThisMod, vec![change(at(8), AIR, ROCK)]),
            ],
            "one edit per brick of 8 voxels"
        );
        assert_eq!(
            *BOXES.lock().unwrap(),
            [
                (VoxelPos::new(0, 16, 24), [8; 3]),
                (VoxelPos::new(8, 16, 24), [8; 3])
            ]
        );
        HEARD.lock().unwrap().clear();

        world.undo(id).unwrap();
        loader.run_transactions();
        assert_eq!(
            *HEARD.lock().unwrap(),
            [
                (
                    EditSource::ThisMod,
                    vec![change(at(6), ROCK, AIR), change(at(7), ROCK, AIR)]
                ),
                (EditSource::ThisMod, vec![change(at(8), ROCK, AIR)]),
            ],
            "the undo puts the voxels back"
        );
    }
}
