//! `enshwick.world.when_settled`: a script waits for its transaction's
//! placed prop, as a Rust mod does.

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use enshwick::ItemId;
    use enshwick::abi::{Header, TerrainMaterialInfo};
    use enshwick::content::Guid;
    use enshwick::test::FakeHost;

    use crate::harness::{logged, run};

    #[test]
    fn a_script_reads_placement_metadata_without_placing_a_prop() {
        let loader = run(
            FakeHost::new()
                .world()
                .item(ItemId::new(30), 1, 0)
                .item_places(ItemId::new(30), Guid([0x3a; 16]))
                .placement_box(ItemId::new(30), [-2.0, 0.0, -0.5], [3.0, 4.0, 0.5])
                .item(ItemId::new(31), 1, 0)
                .item_places(ItemId::new(31), Guid([0x3b; 16]))
                .item(ItemId::new(32), 1, 0)
                .item_places(ItemId::new(32), Guid([0x3c; 16]))
                .placement_box(ItemId::new(32), [0.0; 3], [0.0; 3]),
            r"
            local world = enshwick.world
            local info = world.prop_placement_info(30)
            assert(info.item == 30)
            assert(info.template == '3a3a3a3a-3a3a-3a3a-3a3a-3a3a3a3a3a3a')
            assert(info.local_box.min == vector.create(-2, 0, -0.5))
            assert(info.local_box.max == vector.create(3, 4, 0.5))
            assert(world.prop_placement_info(31).local_box == nil)
            assert(world.prop_placement_info(32).local_box == nil)
            assert(not pcall(world.prop_placement_info, 99))
            enshwick.log.info('placement metadata copied')
            ",
        );
        assert!(
            logged(&loader, "placement metadata copied"),
            "{:?}",
            loader.logs()
        );
        assert!(loader.props().is_empty());
        assert!(loader.transactions().is_empty());
    }

    #[test]
    fn a_script_gets_its_placed_prop_once_it_stands() {
        let loader = run(
            FakeHost::new()
                .world()
                .item(ItemId::new(30), 1, 0)
                .item_places(ItemId::new(30), Guid([0x3a; 16])),
            r"
            local world = enshwick.world
            enshwick.schedule.next_tick(function()
                local at = { position = vector.create(100, 40, 100), orientation = { 0, 0, 0, 1 } }
                local id = world.transaction():place_prop(30, at):commit()
                world.when_settled(id, 60, function(settled)
                    enshwick.log.info(`settled {settled.status} with {#settled.props} prop`)
                end)
            end)
            ",
        );
        loader.set_terrain(&[TerrainMaterialInfo {
            header: Header::of::<TerrainMaterialInfo>(),
            id: 9,
            flags: 0,
            hardness: 3,
            reserved: 0,
            item: ItemId::new(77),
            health: 500,
        }]);
        let pass = || {
            let _ran = loader.tick(Duration::from_secs(1));
        };
        pass();
        assert!(!logged(&loader, "settled"), "{:?}", loader.logs());
        loader.run_transactions();
        pass();
        assert!(
            logged(&loader, "settled Applied with 1 prop"),
            "{:?}",
            loader.logs()
        );
    }
}
