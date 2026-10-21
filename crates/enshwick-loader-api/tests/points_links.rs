//! A mod that extends another mod's points, asked through the fake loader
//! as the owner would ask.

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use enshwick_loader_api::abi::Status;
    use enshwick_loader_api::points::{Decision, Order, Verdict};
    use enshwick_loader_api::{Load, Mod, Off};

    #[enshwick_loader_api::point(name = "probe:limit", mode = Override, output = u32, refusable)]
    #[derive(Clone, Copy, Debug)]
    pub(crate) struct Limit {
        pub(crate) level: u32,
    }

    /// A newer owner's declaration, with a field the links do not know.
    mod newer {
        #[enshwick_loader_api::point(name = "probe:limit", mode = Override, output = u32, refusable)]
        #[derive(Clone, Copy, Debug)]
        pub(crate) struct Limit {
            pub(crate) level: u32,
            pub(crate) altars: u32,
        }
    }

    #[enshwick_loader_api::point(name = "probe:seen", mode = Observe, output = u32)]
    #[derive(Clone, Copy, Debug)]
    pub(crate) struct Seen {
        pub(crate) level: u32,
    }

    static SEEN: AtomicU32 = AtomicU32::new(0);

    struct Extender;

    impl Mod for Extender {
        fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
            enshwick_loader_api::manifest!(
                id: "extender",
                side: SERVER,
                recommends: ["probe" => ">=0.0.0-dev, <1.0"],
            )
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            // Attached first, it runs last: its priority is higher.
            load.extend_point(&Order::new().priority(5), |_: &Limit, limit: &u32| {
                Verdict::Set(limit + 1)
            })?;
            load.extend_point(&Order::new(), |limit: &Limit, so_far: &u32| {
                assert!(limit.level != 13, "unlucky");
                match limit.level {
                    0 => Verdict::Refuse,
                    _ => Verdict::Set(so_far * 10),
                }
            })?;
            load.observe_point(&Order::new(), |_: &Seen, value: &u32| {
                SEEN.store(*value, Ordering::Relaxed);
            })?;
            Ok(Self)
        }
    }

    enshwick_loader_api::declare_mod!(Extender);

    fn started() -> enshwick_loader_api::test::Installed {
        let loader = enshwick_loader_api::test::FakeHost::new().install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        loader
    }

    #[test]
    fn links_run_by_priority() {
        let loader = started();
        assert_eq!(
            loader.attached_points(),
            ["probe:limit", "probe:limit", "probe:seen"]
        );
        assert_eq!(
            loader.ask_point(&Limit { level: 2 }, 3),
            Decision::Changed(31)
        );
        assert_eq!(loader.ask_point(&Limit { level: 0 }, 3), Decision::Refused);
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
        assert_eq!(
            loader.ask_point(&Limit { level: 13 }, 3),
            Decision::Changed(4)
        );
        assert_eq!(
            loader.ask_point(&Limit { level: 2 }, 3),
            Decision::Changed(4)
        );
        assert!(loader.faults().is_empty(), "the mod stays on");
    }

    #[test]
    fn an_observer_sees_the_answer_and_changes_nothing() {
        let loader = started();
        assert_eq!(loader.ask_point(&Seen { level: 1 }, 7), Decision::Kept(7));
        assert_eq!(SEEN.load(Ordering::Relaxed), 7);
    }
}
