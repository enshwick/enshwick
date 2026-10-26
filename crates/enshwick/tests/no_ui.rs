//! The interface stays off on a server.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::Status;
    use enshwick::{Load, Mod, Off};

    static REASONS: Mutex<Vec<String>> = Mutex::new(Vec::new());

    struct Both;

    impl Mod for Both {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.both", side: BOTH)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let mut reasons = REASONS.lock().unwrap();
            if let Err(off) = load.ui() {
                reasons.push(off.to_string());
            }
            if let Err(off) = load.on_frame(|_| {}) {
                reasons.push(off.to_string());
            }
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Both);

    #[test]
    fn a_server_part_is_told_the_interface_is_for_clients() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(
            *REASONS.lock().unwrap(),
            [
                "the interface: on clients only",
                "frame callbacks: on clients only"
            ]
        );
    }
}
