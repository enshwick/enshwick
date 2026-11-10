//! What a mod reads of the host itself: the game's code section, a hook
//! caller's frame and the archive's version.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{CallerFrame, Status};
    use enshwick::{Load, Mod, Off};

    static CODE: [u8; 4] = [0x90, 0xb9, 0x00, 0x02];

    /// The code section the mod read: its address and its bytes.
    static READ: Mutex<Option<(usize, Vec<u8>)>> = Mutex::new(None);

    struct Reader;

    impl Mod for Reader {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let read = load
                .code()
                .map(|code| (code.as_ptr().addr(), code.to_vec()));
            *READ.lock().unwrap() = read.ok();
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Reader,
        r#"
        id = "you.reader"
        side = "server"
        "#
    );

    fn start(fake: enshwick::test::FakeHost) -> enshwick::test::Installed {
        *READ.lock().unwrap() = None;
        let loader = fake.install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Reader)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.faults());
        loader
    }

    #[test]
    fn the_code_section_reads_at_its_own_address() {
        let _loader = start(enshwick::test::FakeHost::new().code(&CODE));
        assert_eq!(
            *READ.lock().unwrap(),
            Some((CODE.as_ptr().addr(), CODE.to_vec()))
        );
    }

    #[test]
    fn a_host_without_a_code_section_refuses_the_read() {
        let _loader = start(enshwick::test::FakeHost::new());
        assert_eq!(*READ.lock().unwrap(), None);
    }

    #[test]
    fn the_caller_frame_reads_as_the_host_gives_it() {
        let frame = CallerFrame {
            rsp: 0x1000,
            ..CallerFrame::default()
        };
        let _loader = start(enshwick::test::FakeHost::new().caller_frame(frame));
        let host = enshwick::host().unwrap();
        assert_eq!(host.caller_frame().rsp, 0x1000);
    }

    #[test]
    fn the_game_version_reads_or_is_missing() {
        let _loader = start(enshwick::test::FakeHost::new().game_version("1283531|release|2026"));
        let host = enshwick::host().unwrap();
        assert_eq!(host.game_version(), Ok("1283531|release|2026"));
    }

    #[test]
    fn a_host_without_an_archive_version_names_none() {
        let _loader = start(enshwick::test::FakeHost::new());
        assert!(enshwick::host().unwrap().game_version().is_err());
    }
}
