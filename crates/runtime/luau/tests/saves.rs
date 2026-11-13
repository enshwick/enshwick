//! `enshwick.world` saves: the saving event, the save on disk, holding saves
//! and the entities the save leaves out, as a Rust mod's.

#[cfg(test)]
mod tests {
    use enshwick::abi::{Event, Header, SaveInfo, SpawnRequest};
    use enshwick::test::FakeHost;

    use crate::harness::{logged, run};

    #[test]
    fn a_script_hears_a_save_begin_reads_it_holds_saves_and_spawns_outside_them() {
        let loader = run(
            FakeHost::new().world(),
            r#"
            enshwick.on("WorldSaving", function()
                local info = enshwick.world.save_info()
                enshwick.log.info(`saving over {info and info.size} bytes, {info and info.backups} backups`)
                enshwick.world.hold_saves(true)
                local ok = pcall(enshwick.world.save)
                enshwick.log.info(`save while held {ok}`)
                enshwick.world.hold_saves(false)
                enshwick.effects.spawn("03030303-0303-0303-0303-030303030303", vector.create(0, 0, 0), { do_not_save = true })
            end)
            "#,
        );
        loader.set_save_info(Some(SaveInfo {
            header: Header::of::<SaveInfo>(),
            world: [9; 16],
            saved_at: 1_762_300_000,
            size: 4_096,
            backups: 3,
            reserved: 0,
        }));
        // SAFETY: the event carries no data.
        let handled = unsafe { loader.raise_raw(Event::WORLD_SAVING, core::ptr::null()) };
        assert_eq!(handled, 1, "{:?}", loader.logs());
        assert!(
            logged(&loader, "saving over 4096 bytes, 3 backups"),
            "{:?}",
            loader.logs()
        );
        assert!(
            logged(&loader, "save while held false"),
            "{:?}",
            loader.logs()
        );
        assert!(!loader.saves_held());
        let flags: Vec<u64> = loader.spawns().into_iter().map(|(_, r)| r.flags).collect();
        assert_eq!(flags, [SpawnRequest::DO_NOT_SAVE], "{:?}", loader.logs());
    }

    #[test]
    fn a_script_s_hold_lets_go_at_shutdown_and_a_hold_from_shutdown_started_refuses_the_final_save()
    {
        let loader = run(
            FakeHost::new().world(),
            r#"
            enshwick.world.hold_saves(true)
            local shutdowns = 0
            enshwick.on("ShutdownStarted", function()
                shutdowns += 1
                if shutdowns == 2 then
                    enshwick.world.hold_saves(true)
                end
            end)
            "#,
        );
        assert!(loader.saves_held(), "{:?}", loader.logs());
        assert!(loader.shut_down(), "{:?}", loader.logs());
        assert!(!loader.shut_down(), "{:?}", loader.logs());
        assert!(loader.saves_held());
    }

    #[test]
    fn a_script_stops_the_server_once_and_a_second_stop_raises_why() {
        let loader = run(
            FakeHost::new().world(),
            r"
            enshwick.world.stop()
            local ok, why = pcall(enshwick.world.stop)
            enshwick.log.info(`again {ok}: {why}`)
            ",
        );
        assert!(loader.stop_asked(), "{:?}", loader.logs());
        assert!(logged(&loader, "again false"), "{:?}", loader.logs());
        assert!(
            logged(&loader, "refused by the world"),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn a_script_keeps_its_own_entity_out_of_the_save_and_never_a_vanilla_one() {
        let loader = run(
            FakeHost::new().world().prop(
                enshwick::Entity(500),
                enshwick::ItemId::new(30),
                [0.0; 3],
            ),
            r#"
            local decoy
            enshwick.on("WorldSaved", function()
                decoy = enshwick.effects.spawn("03030303-0303-0303-0303-030303030303", vector.create(0, 0, 0), {})
                local ok, why = pcall(enshwick.world.set_saved, decoy, false)
                enshwick.log.info(`same step left out {ok}: {why}`)
            end)
            enshwick.on("WorldSaving", function()
                enshwick.world.set_saved(decoy, false)
                local ok, why = pcall(enshwick.world.set_saved, 500, false)
                enshwick.log.info(`vanilla left out {ok}: {why}`)
            end)
            "#,
        );
        // SAFETY: the event carries no data.
        let spawned = unsafe { loader.raise_raw(Event::WORLD_SAVED, core::ptr::null()) };
        let _handlers = loader.tick(std::time::Duration::from_millis(50));
        // SAFETY: the event carries no data.
        let handled = unsafe { loader.raise_raw(Event::WORLD_SAVING, core::ptr::null()) };
        assert_eq!((spawned, handled), (1, 1), "{:?}", loader.logs());
        assert!(
            logged(
                &loader,
                "same step left out false: runtime error: enshwick.world.set_saved: the entity is not in the world"
            ),
            "{:?}",
            loader.logs()
        );
        let spawned: Vec<_> = loader
            .spawns()
            .into_iter()
            .map(|(id, _)| enshwick::Entity(id))
            .collect();
        assert_eq!(loader.unsaved(), spawned, "{:?}", loader.logs());
        assert!(
            logged(
                &loader,
                "vanilla left out false: runtime error: enshwick.world.set_saved: refused by the world"
            ),
            "{:?}",
            loader.logs()
        );
    }
}
