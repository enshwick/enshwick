//! What a client's frame reads of the game: the interaction prompt, the
//! object the open screen belongs to and the local player's held controls.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{Side, Status};
    use enshwick::controls::GameAction;
    use enshwick::ui::{HudScreenType, Prompt};
    use enshwick::{Entity, Load, Mod, Off};

    /// What each frame read: the prompt, the screen's host and the held
    /// actions.
    type Read = (Option<Prompt>, Option<Entity>, Option<Vec<GameAction>>);

    static READ: Mutex<Vec<Read>> = Mutex::new(Vec::new());
    static HOSTS: Mutex<Vec<(Option<HudScreenType>, Option<Entity>)>> = Mutex::new(Vec::new());

    struct Watcher;

    impl Mod for Watcher {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let input = load.input()?;
            load.on_frame(move |frame| {
                let held = input.local_held().map(|held| held.iter().collect());
                READ.lock()
                    .unwrap()
                    .push((frame.prompt(), frame.screen_host(), held));
            })?;
            load.on_screen_changed(|change| HOSTS.lock().unwrap().push((change.now, change.host)))?;
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
    fn a_frame_reads_the_prompt_the_screen_s_host_and_the_local_held_controls() {
        let loader = enshwick::test::FakeHost::new()
            .side(Side::CLIENT)
            .player_input()
            .install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Watcher)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let _menu = loader.draw_frame();
        loader.show_prompt(Some(("Chest", 7)));
        loader.hold_locally(Some(1 << 30 | 1 << 12));
        let _looking = loader.draw_frame();
        loader.show_prompt(None);
        loader.open_screen(Some(HudScreenType::StorageChest));
        loader.host_screen(Some(Entity(512)));
        let _open = loader.draw_frame();
        loader.host_screen(Some(Entity(513)));
        let _next = loader.draw_frame();
        let read = READ.lock().unwrap();
        assert_eq!(read[0], (None, None, None), "nothing outside a game");
        let prompt = read[1].0.as_ref().map(|p| (p.title.as_str(), p.verb));
        assert_eq!(prompt, Some(("Chest", 7)));
        assert_eq!(read[1].1, None);
        assert_eq!(
            read[1].2,
            Some(vec![GameAction::Block]),
            "bit 12 has no name on this build"
        );
        assert_eq!(read[2].1, Some(Entity(512)));
        assert_eq!(read[2].0, None, "the prompt went");
        assert_eq!(
            *HOSTS.lock().unwrap(),
            [
                (Some(HudScreenType::StorageChest), Some(Entity(512))),
                (Some(HudScreenType::StorageChest), Some(Entity(513))),
            ],
            "the next chest on the same screen is a change"
        );
    }
}
