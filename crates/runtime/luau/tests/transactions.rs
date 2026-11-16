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
