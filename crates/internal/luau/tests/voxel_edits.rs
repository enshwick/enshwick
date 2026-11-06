//! `enshwick.voxels.on_voxels_changed`: the world's voxel writes a script
//! hears, as a Rust mod's.

#[cfg(test)]
mod tests {
    use enshwick::test::FakeHost;
    use enshwick::voxels::{EditSource, Voxel, VoxelChange, VoxelMaterial, VoxelPos};

    use crate::harness::{logged, run};

    #[test]
    fn a_script_hears_each_write_with_its_source_and_changed_voxels() {
        let loader = run(
            FakeHost::new().world(),
            r"
            enshwick.voxels.on_voxels_changed(function(edit)
                local first = edit.changes[1]
                enshwick.log.info(`heard {edit.source} {#edit.changes} at {first.at.x} {first.at.y} {first.at.z} fill {first.before.fill} to {first.after.fill}`)
            end)
            ",
        );
        let rock = Voxel::new(VoxelMaterial::Terrain(9), 255);
        let dug = VoxelChange::new(VoxelPos::new(10, 20, 30), rock, Voxel::default());

        assert_eq!(loader.voxels_changed(EditSource::Game, &[dug]), 1);
        assert!(
            logged(&loader, "heard Game 1 at 10 20 30 fill 255 to 0"),
            "{:?}",
            loader.logs()
        );
        let _ran = loader.voxels_changed(EditSource::ThisMod, &[dug]);
        assert!(logged(&loader, "heard ThisMod 1"));
    }
}
