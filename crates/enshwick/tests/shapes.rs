//! A change deep inside a shared declaration refuses the link: nested
//! structs, enum variants and named values all count.

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Mutex, PoisonError};

    use enshwick::abi::Status;
    use enshwick::points::{Decision, OwnedPoint, Verdict};
    use enshwick::{Load, Mod, Off, Shape};

    /// The declarations both mods compile.
    mod base {
        use enshwick::Shape;

        #[derive(Clone, Copy, Debug, PartialEq, Eq, Shape)]
        #[repr(u8)]
        pub(crate) enum Kind {
            Food = 1,
            Tools = 2,
        }

        #[derive(Clone, Copy, Debug, PartialEq, Eq, Shape)]
        #[repr(transparent)]
        #[shape(values(OPEN, CLOSED))]
        pub(crate) struct Hours(pub(crate) u8);

        impl Hours {
            pub(crate) const OPEN: Self = Self(0);
            pub(crate) const CLOSED: Self = Self(1);
        }

        #[derive(Clone, Copy, Debug, PartialEq, Eq, Shape)]
        #[repr(C)]
        pub(crate) struct Stall {
            pub(crate) kind: Kind,
            pub(crate) hours: Hours,
            pub(crate) size: u16,
        }

        #[enshwick::event(name = "market:opened")]
        #[derive(Clone, Copy, Debug)]
        pub(crate) struct Opened {
            pub(crate) stall: Stall,
        }

        #[enshwick::point(name = "you.my-mod:rent", mode = Override, output = u32)]
        #[derive(Clone, Copy, Debug)]
        pub(crate) struct Rent {
            pub(crate) stall: Stall,
        }
    }

    /// `Kind` with a third variant, at the same size.
    mod more_variants {
        use enshwick::Shape;

        #[derive(Clone, Copy, Debug, Shape)]
        #[repr(u8)]
        pub(crate) enum Kind {
            Food = 1,
            Tools = 2,
            Cloth = 3,
        }

        #[derive(Clone, Copy, Debug, Shape)]
        #[repr(C)]
        pub(crate) struct Stall {
            pub(crate) kind: Kind,
            pub(crate) hours: super::base::Hours,
            pub(crate) size: u16,
        }

        #[enshwick::event(name = "market:opened")]
        #[derive(Clone, Copy, Debug)]
        pub(crate) struct Opened {
            pub(crate) stall: Stall,
        }

        #[enshwick::point(name = "you.my-mod:rent", mode = Override, output = u32)]
        #[derive(Clone, Copy, Debug)]
        pub(crate) struct Rent {
            pub(crate) stall: Stall,
        }
    }

    /// `Kind` with its variants renumbered.
    mod renumbered {
        use enshwick::Shape;

        #[derive(Clone, Copy, Debug, Shape)]
        #[repr(u8)]
        pub(crate) enum Kind {
            Food = 2,
            Tools = 1,
        }

        #[derive(Clone, Copy, Debug, Shape)]
        #[repr(C)]
        pub(crate) struct Stall {
            pub(crate) kind: Kind,
            pub(crate) hours: super::base::Hours,
            pub(crate) size: u16,
        }
    }

    /// `Hours` whose values mean the other thing.
    mod swapped_values {
        use enshwick::Shape;

        #[derive(Clone, Copy, Debug, Shape)]
        #[repr(transparent)]
        #[shape(values(OPEN, CLOSED))]
        pub(crate) struct Hours(pub(crate) u8);

        impl Hours {
            pub(crate) const OPEN: Self = Self(1);
            pub(crate) const CLOSED: Self = Self(0);
        }

        #[derive(Clone, Copy, Debug, Shape)]
        #[repr(C)]
        pub(crate) struct Stall {
            pub(crate) kind: super::base::Kind,
            pub(crate) hours: Hours,
            pub(crate) size: u16,
        }
    }

    /// `Stall` with a signed size, at the same layout.
    mod signed_size {
        use enshwick::Shape;

        #[derive(Clone, Copy, Debug, Shape)]
        #[repr(C)]
        pub(crate) struct Stall {
            pub(crate) kind: super::base::Kind,
            pub(crate) hours: super::base::Hours,
            pub(crate) size: i16,
        }

        #[enshwick::point(name = "you.my-mod:rent", mode = Override, output = u32)]
        #[derive(Clone, Copy, Debug)]
        pub(crate) struct Rent {
            pub(crate) stall: Stall,
        }
    }

    /// `Stall` with a field renamed.
    mod renamed_field {
        use enshwick::Shape;

        #[derive(Clone, Copy, Debug, Shape)]
        #[repr(C)]
        pub(crate) struct Stall {
            pub(crate) kind: super::base::Kind,
            pub(crate) hours: super::base::Hours,
            pub(crate) width: u16,
        }
    }

    /// The same declarations written again elsewhere.
    mod same {
        use enshwick::Shape;

        #[derive(Clone, Copy, Debug, Shape)]
        #[repr(u8)]
        pub(crate) enum Kind {
            Food = 1,
            Tools = 2,
        }

        #[derive(Clone, Copy, Debug, Shape)]
        #[repr(C)]
        pub(crate) struct Stall {
            pub(crate) kind: Kind,
            pub(crate) hours: super::base::Hours,
            pub(crate) size: u16,
        }

        #[enshwick::point(name = "you.my-mod:rent", mode = Override, output = u32)]
        #[derive(Clone, Copy, Debug)]
        pub(crate) struct Rent {
            pub(crate) stall: Stall,
        }
    }

    #[test]
    fn every_change_inside_a_field_changes_its_shape() {
        let base = base::Stall::SHAPE;
        assert_eq!(base, same::Stall::SHAPE);
        assert_ne!(base, more_variants::Stall::SHAPE);
        assert_ne!(base, renumbered::Stall::SHAPE);
        assert_ne!(base, swapped_values::Stall::SHAPE);
        assert_ne!(base, signed_size::Stall::SHAPE);
        assert_ne!(base, renamed_field::Stall::SHAPE);
        assert_eq!(size_of::<base::Stall>(), size_of::<more_variants::Stall>());
        assert_ne!(base::Kind::SHAPE, u8::SHAPE);
        assert_ne!(bool::SHAPE, u8::SHAPE);
        assert_eq!(base::Hours::OPEN.0, 0);
        assert_eq!(base::Hours::CLOSED.0, 1);
    }

    #[test]
    fn the_outer_field_hash_follows_the_nested_one() {
        use enshwick::services::ModEvent;
        assert_ne!(
            base::Opened::LAYOUT[0].hash,
            more_variants::Opened::LAYOUT[0].hash
        );
    }

    static OPENED: AtomicU32 = AtomicU32::new(0);
    static RENT: Mutex<Option<OwnedPoint<base::Rent>>> = Mutex::new(None);

    struct Market;

    impl Mod for Market {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.my-mod", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.on_mod_event(|opened: &base::Opened| {
                OPENED.fetch_add(u32::from(opened.stall.size), Ordering::Relaxed);
            })?;
            let rent = load.define_point::<base::Rent>()?;
            *RENT.lock().unwrap_or_else(PoisonError::into_inner) = Some(rent);
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Market);

    fn started() -> enshwick::test::Installed {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        loader
    }

    fn stall() -> base::Stall {
        base::Stall {
            kind: base::Kind::Food,
            hours: base::Hours::OPEN,
            size: 3,
        }
    }

    #[test]
    fn an_event_with_a_changed_nested_enum_reaches_no_one() {
        let loader = started();
        OPENED.store(0, Ordering::Relaxed);
        assert_eq!(loader.send_mod_event(&base::Opened { stall: stall() }), 1);
        let changed = more_variants::Opened {
            stall: more_variants::Stall {
                kind: more_variants::Kind::Cloth,
                hours: base::Hours::OPEN,
                size: 5,
            },
        };
        assert_eq!(loader.send_mod_event(&changed), 0);
        assert_eq!(OPENED.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn a_link_with_a_changed_nested_type_is_refused() {
        let loader = started();
        assert!(!loader.extend_point(|_: &more_variants::Rent, _: &u32| Verdict::Set(1)));
        assert!(!loader.extend_point(|_: &signed_size::Rent, _: &u32| Verdict::Set(1)));
        assert!(loader.extend_point(|_: &same::Rent, rent: &u32| Verdict::Set(rent + 1)));
        let rent = *RENT.lock().unwrap_or_else(PoisonError::into_inner);
        let decision = rent.map(|rent| rent.decide(&base::Rent { stall: stall() }, 10));
        assert_eq!(decision, Some(Decision::Changed(11)));
    }
}
