//! The local player's held controls when none is held: a read with no
//! control down is a mask of no actions, not a missing one.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{Side, Status};
    use enshwick::controls::GameAction;
    use enshwick::{Load, Mod, Off};

    /// The held actions each frame read, `None` for no mask.
    static READ: Mutex<Vec<Option<Vec<GameAction>>>> = Mutex::new(Vec::new());

    struct Idle;

    impl Mod for Idle {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let input = load.input()?;
            load.on_frame(move |_| {
                let held = input.local_held().map(|held| held.iter().collect());
                READ.lock().unwrap().push(held);
            })?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Idle,
        r#"
        id = "you.idle"
        side = "both"
        "#
    );

    #[test]
    fn a_read_with_no_control_held_is_a_mask_of_no_actions_and_no_read_is_none() {
        let loader = enshwick::test::FakeHost::new()
            .side(Side::CLIENT)
            .player_input()
            .install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Idle)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let _first = loader.draw_frame();
        loader.hold_locally(Some(0));
        let _idle = loader.draw_frame();
        loader.hold_locally(Some(1 << 12));
        let _unnamed = loader.draw_frame();
        loader.hold_locally(None);
        let _unread = loader.draw_frame();
        let read = READ.lock().unwrap();
        assert_eq!(read[0], None, "the first ask starts the reads");
        assert_eq!(read[1], Some(vec![]), "nothing held is a mask");
        assert_eq!(
            read[2],
            Some(vec![]),
            "a bit no name of the build maps keeps the mask"
        );
        assert_eq!(read[3], None, "no read is no mask");
    }
}
