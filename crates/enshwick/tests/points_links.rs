//! A mod that extends another mod's points, asked through the fake loader
//! as the owner would ask.

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    use enshwick::abi::Status;
    use enshwick::points::{Decision, Order, Verdict};
    use enshwick::{Load, Mod, Off};

    #[enshwick::point(name = "you.my-mod:limit", mode = Override, output = u32, refusable)]
    #[derive(Clone, Copy, Debug)]
    pub(crate) struct Limit {
        pub(crate) level: u32,
    }

    /// A newer owner's declaration, with a field the links do not know.
    mod newer {
        #[enshwick::point(name = "you.my-mod:limit", mode = Override, output = u32, refusable)]
        #[derive(Clone, Copy, Debug)]
        pub(crate) struct Limit {
            pub(crate) level: u32,
            pub(crate) altars: u32,
        }
    }

    #[enshwick::point(name = "you.my-mod:seen", mode = Observe, output = u32)]
    #[derive(Clone, Copy, Debug)]
    pub(crate) struct Seen {
        pub(crate) level: u32,
    }

    static SEEN: AtomicU32 = AtomicU32::new(0);
    static SECOND_REFUSED: AtomicBool = AtomicBool::new(false);

    struct Extender;

    impl Mod for Extender {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(
                id: "you.extender",
                side: SERVER,
                recommends: ["you.my-mod" => ">=0.0.0-dev, <1.0"],
            )
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.extend_point(&Order::new().priority(5), |limit: &Limit, so_far: &u32| {
                assert!(limit.level != 13, "unlucky");
                match limit.level {
                    0 => Verdict::Refuse,
                    _ => Verdict::Set(so_far * 10 + 1),
                }
            })?;
            let second = load.extend_point(&Order::new(), |_: &Limit, so_far: &u32| {
                Verdict::Set(*so_far)
            });
            SECOND_REFUSED.store(second.is_err(), Ordering::Relaxed);
            load.observe_point(&Order::new(), |_: &Seen, value: &u32| {
                SEEN.store(*value, Ordering::Relaxed);
            })?;
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Extender);

    fn started() -> enshwick::test::Installed {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Extender)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        loader
    }

    #[test]
    fn a_link_changes_and_refuses_the_answer() {
        let loader = started();
        assert_eq!(
            loader.attached_points(),
            ["you.my-mod:limit", "you.my-mod:seen"]
        );
        assert_eq!(
            loader.ask_point(&Limit { level: 2 }, 3),
            Decision::Changed(31)
        );
        assert_eq!(loader.ask_point(&Limit { level: 0 }, 3), Decision::Refused);
    }

    #[test]
    fn a_mod_links_a_point_once() {
        let _loader = started();
        assert!(SECOND_REFUSED.load(Ordering::Relaxed));
    }

    #[test]
    fn a_newer_owner_serves_the_older_links() {
        let loader = started();
        let asked = newer::Limit {
            level: 2,
            altars: 9,
        };
        assert_eq!(loader.ask_point(&asked, 1), Decision::Changed(11));
    }

    #[test]
    fn a_panicking_link_is_turned_off() {
        let loader = started();
        assert_eq!(loader.ask_point(&Limit { level: 13 }, 3), Decision::Kept(3));
        assert_eq!(loader.ask_point(&Limit { level: 2 }, 3), Decision::Kept(3));
        assert!(loader.faults().is_empty(), "the mod stays on");
    }

    #[test]
    fn an_observer_sees_the_answer_and_changes_nothing() {
        let loader = started();
        assert_eq!(loader.ask_point(&Seen { level: 1 }, 7), Decision::Kept(7));
        assert_eq!(SEEN.load(Ordering::Relaxed), 7);
    }
}
