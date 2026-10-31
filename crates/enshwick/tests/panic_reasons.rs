//! A panic in a mod names its message in the reason the loader shows, and
//! its place in the mod's log.

#[cfg(test)]
mod tests {
    use enshwick::abi::{Level, Status};
    use enshwick::{Load, Mod, Off};

    struct Lamps;

    impl Mod for Lamps {
        fn init(_load: &mut Load<'_>) -> Result<Self, Off> {
            panic!("the lamp oil ran out")
        }
    }

    enshwick::test::declare_mod!(
        Lamps,
        r#"
        id = "you.lamps"
        side = "server"
        "#
    );

    #[test]
    fn a_panicking_init_turns_the_mod_off_with_the_panic_message() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::FAULTED);
        assert_eq!(loader.faults(), ["init panicked: the lamp oil ran out"]);
    }

    #[test]
    fn a_panic_logs_its_message_and_where_it_happened() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: the exports of `declare_mod!` above.
        let _status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        let logs = loader.logs();
        let line = logs
            .iter()
            .find(|(level, line)| *level == Level::ERROR && line.starts_with("panicked"))
            .map(|(_, line)| line.as_str());
        let line = line.unwrap_or_else(|| panic!("a panic line, got {logs:?}"));
        assert!(line.contains("the lamp oil ran out"), "{line}");
        assert!(line.contains("panic_reasons.rs:"), "{line}");
    }
}
