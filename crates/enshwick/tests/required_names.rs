//! A mod that links a point of a mod it requires: the fake loader, as the
//! loader, refuses the link when the required mod defines no such point by
//! the mod's init, and keeps it when the mod is only recommended.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::points::{Decision, Order, Verdict};
    use enshwick::{Load, Mod, Off};

    #[enshwick::point(name = "price", of = lockers, mode = Override, output = u32)]
    #[derive(Clone, Copy, Debug)]
    pub(crate) struct Price {
        pub(crate) locker: u32,
    }

    #[enshwick::point(name = "you.lanterns:glow", mode = Override, output = u32)]
    #[derive(Clone, Copy, Debug)]
    pub(crate) struct Glow {
        pub(crate) lantern: u32,
    }

    struct Haggler;

    impl Mod for Haggler {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.extend_point(&Order::new(), |_: &Glow, glow: &u32| Verdict::Set(glow + 1))?;
            load.extend_point(&Order::new(), |_: &Price, price: &u32| {
                Verdict::Set(price / 2)
            })?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Haggler,
        r#"
        id = "you.haggler"
        side = "server"

        [requires]
        lockers = "you.lockers"

        [recommends]
        lanterns = "you.lanterns"
        "#
    );

    fn start(loader: &enshwick::test::Installed) -> Status {
        // SAFETY: both functions are the exports of this file's `declare_mod!(Haggler)`.
        unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) }
    }

    #[test]
    fn a_link_to_a_required_mod_s_point_it_does_not_define_turns_the_mod_off() {
        let loader = enshwick::test::FakeHost::new().install();
        assert_ne!(start(&loader), Status::OK);
        assert_eq!(loader.attached_points(), ["you.lanterns:glow"]);
    }

    #[test]
    fn a_link_to_a_required_mod_s_defined_point_and_to_an_optional_one_s_attach() {
        let loader = enshwick::test::FakeHost::new().install();
        loader.define_point::<Price>();
        assert_eq!(start(&loader), Status::OK);
        assert_eq!(
            loader.attached_points(),
            ["you.lanterns:glow", "you.lockers:price"]
        );
        assert_eq!(
            loader.ask_point(&Price { locker: 1 }, 8),
            Decision::Changed(4)
        );
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

        fn start(loader: &enshwick::test::Installed) -> Status {
            // SAFETY: both functions are the exports of this module's `declare_mod!(Renter)`.
            unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) }
        }

        #[test]
        fn a_call_a_required_mod_does_not_serve_turns_the_mod_off() {
            let loader = enshwick::test::FakeHost::new().install();
            assert_ne!(start(&loader), Status::OK);
        }

        #[test]
        fn a_call_a_required_mod_serves_reaches_it() {
            let loader = enshwick::test::FakeHost::new().install();
            loader.serve_call(|rent: &Rent| rent.locker * 3);
            assert_eq!(start(&loader), Status::OK);
            let renter = Renter::instance().map(|renter| renter.0.call(&Rent { locker: 2 }));
            assert_eq!(renter, Some(Ok(6)));
        }
    }
}
