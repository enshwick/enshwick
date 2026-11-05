//! A world's save for mods: the saving event, the save on disk, and
//! holding saves.

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};
    use std::time::Duration;

    use enshwick::abi::{Event, Header, SaveInfo, SpawnRequest, Status};
    use enshwick::effects::{Effects, Spawn};
    use enshwick::events::{WorldSaved, WorldSaving};
    use enshwick::world::{World, WorldGuid};
    use enshwick::{Entity, Load, Mod, Off};

    static WORLD: OnceLock<World> = OnceLock::new();
    static EFFECTS: OnceLock<Effects> = OnceLock::new();
    static ORDER: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

    struct Keeper;

    impl Mod for Keeper {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = WORLD.set(load.world()?);
            let _first = EFFECTS.set(load.effects());
            load.on::<WorldSaving>(|()| ORDER.lock().unwrap().push("saving"))?;
            load.on::<WorldSaved>(|()| ORDER.lock().unwrap().push("saved"))?;
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

    #[test]
    fn a_mod_hears_a_save_begin_reads_the_save_and_holds_saves() {
        let loader = enshwick::test::FakeHost::new().world().install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Keeper)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let world = WORLD.get().unwrap();

        // SAFETY: the event carries no data.
        let saving = unsafe { loader.raise_raw(Event::WORLD_SAVING, core::ptr::null()) };
        // SAFETY: the event carries no data.
        let saved = unsafe { loader.raise_raw(Event::WORLD_SAVED, core::ptr::null()) };
        assert_eq!((saving, saved), (1, 1));
        assert_eq!(*ORDER.lock().unwrap(), ["saving", "saved"]);

        assert_eq!(world.save_info(), None, "no save before the first");
        loader.set_save_info(Some(SaveInfo {
            header: Header::of::<SaveInfo>(),
            world: [9; 16],
            saved_at: 1_762_300_000,
            size: 4_096,
            backups: 3,
            reserved: 0,
        }));
        let info = world.save_info().unwrap();
        assert_eq!(info.world, WorldGuid([9; 16]));
        assert_eq!(info.saved_at, Duration::from_secs(1_762_300_000));
        assert_eq!((info.size, info.backups), (4_096, 3));

        world.hold_saves(true).unwrap();
        assert!(loader.saves_held());
        assert_eq!(
            world.save(),
            Err(enshwick::WorldError::Refused),
            "a save asked while held is refused, as the loader refuses it"
        );
        world.hold_saves(false).unwrap();
        assert!(!loader.saves_held());
        world.save().unwrap();
        world.hold_saves(true).unwrap();
        loader.end_game();
        assert!(!loader.saves_held(), "a hold ends with its hosted game");

        let effects = EFFECTS.get().unwrap();
        let kept = effects.spawn([3; 16], [0.0; 3], Spawn::new()).unwrap();
        let passing = effects
            .spawn([3; 16], [0.0; 3], Spawn::new().do_not_save())
            .unwrap();
        let flags: Vec<_> = loader
            .spawns()
            .into_iter()
            .map(|(id, request)| (Entity(id), request.flags))
            .collect();
        assert_eq!(flags, [(kept, 0), (passing, SpawnRequest::DO_NOT_SAVE)]);
    }
}
