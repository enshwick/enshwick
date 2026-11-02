//! What another mod exports: the fake loader, as the loader, refuses a mod
//! a point, event, service or call of a mod outside that mod's exports, and
//! keeps what it exports.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::points::{Order, Verdict};
    use enshwick::{Load, Mod, Off};

    #[enshwick::point(name = "price", of = lockers, mode = Override, output = u32)]
    #[derive(Clone, Copy, Debug)]
    pub(crate) struct Price {
        pub(crate) locker: u32,
    }

    #[enshwick::event(name = "opened", of = lockers)]
    #[derive(Clone, Copy, Debug)]
    pub(crate) struct Opened {
        pub(crate) locker: u32,
    }

    #[enshwick::service(name = "keys", of = lockers)]
    pub(crate) struct Keys {
        pub(crate) count: extern "system" fn(locker: u32) -> u32,
    }

    extern "system" fn count(_locker: u32) -> u32 {
        2
    }

    static KEYS: Keys = Keys { count };

    /// What the haggler reaches of `lockers`.
    struct Haggler(bool);

    impl Mod for Haggler {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.extend_point(&Order::new(), |_: &Price, price: &u32| {
                Verdict::Set(price / 2)
            })?;
            load.on_mod_event(|_: &Opened| {})?;
            Ok(Self(load.optional_service::<Keys>().is_some()))
        }
    }

    enshwick::test::declare_mod!(
        Haggler,
        r#"
        id = "you.haggler"
        side = "server"

        [requires]
        lockers = "you.lockers"
        "#
    );

    fn start(host: enshwick::test::FakeHost) -> (enshwick::test::Installed, Status) {
        let loader = host.service(&KEYS).install();
        loader.define_point::<Price>();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Haggler)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        (loader, status)
    }

    #[test]
    fn what_another_mod_exports_reaches_the_mod() {
        let host =
            enshwick::test::FakeHost::new().exports("you.lockers", &["price", "opened", "keys"]);
        let (_loader, status) = start(host);
        assert_eq!(status, Status::OK);
        assert!(Haggler::instance().is_some_and(|haggler| haggler.0));
    }

    #[test]
    fn a_point_another_mod_does_not_export_turns_the_mod_off() {
        let host = enshwick::test::FakeHost::new().exports("you.lockers", &["opened", "keys"]);
        let (loader, status) = start(host);
        assert_ne!(status, Status::OK);
        assert!(loader.attached_points().is_empty());
    }

    #[test]
    fn an_event_another_mod_does_not_export_turns_the_mod_off() {
        let host = enshwick::test::FakeHost::new().exports("you.lockers", &["price", "keys"]);
        let (_loader, status) = start(host);
        assert_ne!(status, Status::OK);
    }

    #[test]
    fn a_service_another_mod_does_not_export_is_not_found() {
        let host = enshwick::test::FakeHost::new().exports("you.lockers", &["price", "opened"]);
        let (_loader, status) = start(host);
        assert_eq!(status, Status::OK);
        assert!(Haggler::instance().is_some_and(|haggler| !haggler.0));
    }

    /// The same for calls.
    #[cfg(feature = "unstable")]
    mod calls {
        use enshwick::abi::Status;
        use enshwick::services::Caller;
        use enshwick::{Load, Mod, Off};

        #[enshwick::call(name = "rent", of = lockers, output = u32)]
        #[derive(Clone, Copy, Debug)]
        pub(crate) struct Rent {
            pub(crate) locker: u32,
        }

        struct Renter(Caller<Rent>);

        impl Mod for Renter {
            fn init(load: &mut Load<'_>) -> Result<Self, Off> {
                Ok(Self(load.caller::<Rent>()?))
            }
        }

        enshwick::test::declare_mod!(
            Renter,
            r#"
            id = "you.renter"
            side = "server"

            [requires]
            lockers = "you.lockers"
            "#
        );

        fn start(exports: &[&str]) -> (enshwick::test::Installed, Status) {
            let loader = enshwick::test::FakeHost::new()
                .exports("you.lockers", exports)
                .install();
            loader.serve_call(|rent: &Rent| rent.locker * 3);
            // SAFETY: both functions are the exports of this module's `declare_mod!(Renter)`.
            let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
            (loader, status)
        }

        #[test]
        fn a_call_another_mod_does_not_export_turns_the_mod_off() {
            assert_ne!(start(&["price"]).1, Status::OK);
        }

        #[test]
        fn a_call_another_mod_exports_reaches_it() {
            let (_loader, status) = start(&["rent"]);
            assert_eq!(status, Status::OK);
            let renter = Renter::instance().map(|renter| renter.0.call(&Rent { locker: 2 }));
            assert_eq!(renter, Some(Ok(6)));
        }
    }
}
