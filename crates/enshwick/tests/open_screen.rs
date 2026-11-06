//! The game's screen the player has open, as a frame reads it and as its
//! changes reach a mod.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{Side, Status};
    use enshwick::ui::{HudScreenType, ScreenChanged};
    use enshwick::{Load, Mod, Off};

    static READ: Mutex<Vec<Option<HudScreenType>>> = Mutex::new(Vec::new());
    static CHANGES: Mutex<Vec<ScreenChanged>> = Mutex::new(Vec::new());

    struct Watcher;

    impl Mod for Watcher {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.on_frame(|frame| READ.lock().unwrap().push(frame.open_screen()))?;
            load.on_screen_changed(|change| CHANGES.lock().unwrap().push(*change))?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Watcher,
        r#"
        id = "you.watcher"
        side = "both"
        "#
    );

    #[test]
    fn a_frame_reads_the_open_screen_and_each_change_is_heard_once() {
        let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Watcher)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let _menu = loader.draw_frame();
        for screen in [
            Some(HudScreenType::InGame),
            Some(HudScreenType::InGame),
            Some(HudScreenType::StorageChest),
            None,
        ] {
            loader.open_screen(screen);
            let _drawn = loader.draw_frame();
        }
        assert_eq!(
            *READ.lock().unwrap(),
            [
                None,
                Some(HudScreenType::InGame),
                Some(HudScreenType::InGame),
                Some(HudScreenType::StorageChest),
                None,
            ]
        );
        let changes: Vec<_> = CHANGES
            .lock()
            .unwrap()
            .iter()
            .map(|c| (c.before, c.now))
            .collect();
        assert_eq!(
            changes,
            [
                (None, Some(HudScreenType::InGame)),
                (
                    Some(HudScreenType::InGame),
                    Some(HudScreenType::StorageChest)
                ),
                (Some(HudScreenType::StorageChest), None),
            ],
            "the same screen twice is no change"
        );
    }

    #[test]
    fn a_server_hears_no_screen_changes() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Watcher)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_ne!(
            status,
            Status::OK,
            "on_frame and on_screen_changed are client only"
        );
    }
}
