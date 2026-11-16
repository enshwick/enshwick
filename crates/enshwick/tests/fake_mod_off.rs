//! A mod the loader turns off after a failed init or ready runs none of the
//! callbacks it registered, in the fake as in the game.

/// A mod whose ready panics after its init registered a filter and a
/// handler.
#[cfg(test)]
mod failed_ready {
    use std::sync::atomic::{AtomicU32, Ordering};

    use enshwick::abi::Status;
    use enshwick::chat::ChatVerdict;
    use enshwick::{Host, Load, Mod, Off, PlayerKey};

    /// How often the filter and the handler ran.
    static RAN: AtomicU32 = AtomicU32::new(0);

    struct Broken;

    impl Mod for Broken {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.on_chat(|line| {
                RAN.fetch_add(1, Ordering::Relaxed);
                ChatVerdict::Rewrite(line.text.replace("bad", "good"))
            })?;
            load.on_chat_said(|_| {
                RAN.fetch_add(1, Ordering::Relaxed);
            })?;
            load.on_tick(core::time::Duration::ZERO, |_| {
                RAN.fetch_add(1, Ordering::Relaxed);
            })?;
            Ok(Self)
        }

        fn ready(&'static self, _host: Host) {
            panic!("ready broke");
        }
    }

    enshwick::test::declare_mod!(
        Broken,
        r#"
        id = "you.broken"
        side = "server"
        "#
    );

    #[test]
    fn a_mod_whose_ready_panics_is_off_and_its_filter_and_handler_never_run() {
        let loader = enshwick::test::FakeHost::new().world().install();
        // SAFETY: the exports of this module's `declare_mod!`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::FAULTED);
        assert!(loader.is_off());
        assert_eq!(
            loader.say(PlayerKey::new(7), "a bad day").as_deref(),
            Some("a bad day"),
            "the line passes as if no filter were there"
        );
        assert_eq!(
            loader.tick(core::time::Duration::from_millis(100)),
            0,
            "no tick handler of an off mod runs or counts"
        );
        assert_eq!(RAN.load(Ordering::Relaxed), 0);
    }
}

/// A mod whose init registered a filter and then turned the mod off.
#[cfg(test)]
mod failed_init {
    use std::sync::atomic::{AtomicU32, Ordering};

    use enshwick::abi::Status;
    use enshwick::chat::ChatVerdict;
    use enshwick::{Load, Mod, Off, PlayerKey};

    /// How often the filter and the handler ran.
    static RAN: AtomicU32 = AtomicU32::new(0);

    struct Halfway;

    impl Mod for Halfway {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.on_chat(|_| {
                RAN.fetch_add(1, Ordering::Relaxed);
                ChatVerdict::Drop
            })?;
            load.on_chat_said(|_| {
                RAN.fetch_add(1, Ordering::Relaxed);
            })?;
            Err(Off::new("not on this server"))
        }
    }

    enshwick::test::declare_mod!(
        Halfway,
        r#"
        id = "you.halfway"
        side = "server"
        "#
    );

    #[test]
    fn a_mod_whose_init_fails_after_registering_is_off_and_its_callbacks_never_run() {
        let loader = enshwick::test::FakeHost::new().world().install();
        // SAFETY: the exports of this module's `declare_mod!`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::REFUSED);
        assert!(loader.is_off());
        assert_eq!(
            loader.say(PlayerKey::new(7), "hello").as_deref(),
            Some("hello")
        );
        assert_eq!(RAN.load(Ordering::Relaxed), 0);
    }
}
