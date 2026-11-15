//! What the loader turned off, through the safe layer on the fake host.
#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::{Load, Mod, Off};

    struct Watcher;

    impl Mod for Watcher {
        fn init(_load: &mut Load<'_>) -> Result<Self, Off> {
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Watcher,
        r#"id = "you.loader-off"
side = "both""#
    );

    #[test]
    fn a_mod_reads_each_part_the_loader_turned_off_with_its_first_reason() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: these are this file's generated exports.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert!(enshwick::mods::loader_off().is_empty(), "everything runs");
        loader.turn_loader_off(
            "the local player's held controls",
            "this build lacks ecs.player_ui_iterator",
        );
        loader.turn_loader_off("the local player's held controls", "a later reason");
        loader.turn_loader_off("screen reads", "a fault");
        let off = enshwick::mods::loader_off();
        let shown: Vec<(&str, &str)> = off
            .iter()
            .map(|off| (off.name.as_str(), off.reason.as_str()))
            .collect();
        assert_eq!(
            shown,
            [
                (
                    "the local player's held controls",
                    "this build lacks ecs.player_ui_iterator"
                ),
                ("screen reads", "a fault"),
            ]
        );
    }
}
