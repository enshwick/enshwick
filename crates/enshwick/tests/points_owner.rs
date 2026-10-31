//! A mod that owns an extension point, with links attached through the fake
//! loader as other mods would.

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, PoisonError};

    use enshwick::abi::Status;
    use enshwick::points::{Decision, OwnedPoint, Verdict};
    use enshwick::{Load, Mod, Off};

    #[enshwick::point(name = "you.my-mod:limit", mode = Override, output = u32, refusable)]
    #[derive(Clone, Copy, Debug)]
    pub(crate) struct Limit {
        pub(crate) level: u32,
    }

    /// The same point with a changed input field.
    mod changed {
        #[enshwick::point(name = "you.my-mod:limit", mode = Override, output = u32, refusable)]
        #[derive(Clone, Copy, Debug)]
        pub(crate) struct Limit {
            pub(crate) level: u64,
        }
    }

    /// The same point declared with another answer.
    mod other_answer {
        #[enshwick::point(name = "you.my-mod:limit", mode = Override, output = u16, refusable)]
        #[derive(Clone, Copy, Debug)]
        pub(crate) struct Limit {
            pub(crate) level: u32,
        }
    }

    /// The point of the mod's latest start; each test starts it again.
    static LIMIT: Mutex<Option<OwnedPoint<Limit>>> = Mutex::new(None);

    fn limit() -> Option<OwnedPoint<Limit>> {
        *LIMIT.lock().unwrap_or_else(PoisonError::into_inner)
    }

    struct Probe;

    impl Mod for Probe {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let limit = load.define_point::<Limit>()?;
            assert!(!limit.is_extended());
            *LIMIT.lock().unwrap_or_else(PoisonError::into_inner) = Some(limit);
            assert!(load.define_point::<Limit>().is_err(), "defined twice");
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Probe,
        r#"
        id = "you.my-mod"
        side = "server"
        "#
    );

    fn started() -> enshwick::test::Installed {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Probe)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(loader.defined_points(), ["you.my-mod:limit"]);
        loader
    }

    fn ask(level: u32) -> Decision<u32> {
        limit().map_or(Decision::Refused, |limit| limit.decide(&Limit { level }, 4))
    }

    #[test]
    fn without_links_the_owners_answer_stands() {
        let _loader = started();
        assert_eq!(ask(1), Decision::Kept(4));
    }

    #[test]
    fn links_run_in_turn_on_the_answer_so_far() {
        let loader = started();
        assert!(loader.extend_point(|_: &Limit, limit: &u32| Verdict::Set(limit * 2)));
        assert!(loader.extend_point(|limit: &Limit, so_far: &u32| {
            if limit.level == 0 {
                Verdict::Pass
            } else {
                Verdict::Set(so_far + limit.level)
            }
        }));
        assert!(limit().is_some_and(|limit| limit.is_extended()));
        assert_eq!(ask(1), Decision::Changed(9));
        assert_eq!(ask(0), Decision::Changed(8));
    }

    #[test]
    fn a_refusal_stops_the_chain() {
        let loader = started();
        assert!(loader.extend_point(|limit: &Limit, _: &u32| {
            if limit.level > 10 {
                Verdict::Refuse
            } else {
                Verdict::Pass
            }
        }));
        assert!(loader.extend_point(|_: &Limit, _: &u32| Verdict::Set(99)));
        assert_eq!(ask(11), Decision::Refused);
        assert_eq!(ask(1), Decision::Changed(99));
    }

    #[test]
    fn a_panicking_link_is_turned_off_and_the_rest_still_runs() {
        let loader = started();
        assert!(
            loader.extend_point(|limit: &Limit, _: &u32| -> Verdict<u32> {
                assert!(limit.level != 13, "unlucky");
                Verdict::Set(1)
            })
        );
        assert!(loader.extend_point(|_: &Limit, so_far: &u32| Verdict::Set(so_far + 10)));
        assert_eq!(ask(13), Decision::Changed(14));
        assert_eq!(
            ask(1),
            Decision::Changed(14),
            "the first link is off for good"
        );
        assert!(loader.faults().is_empty(), "the mod stays on");
    }

    #[test]
    fn a_link_with_another_declaration_is_refused() {
        let loader = started();
        assert!(!loader.extend_point(|_: &changed::Limit, _: &u32| Verdict::Set(1)));
        assert!(!loader.extend_point(|_: &other_answer::Limit, _: &u16| Verdict::Set(1)));
        assert_eq!(ask(1), Decision::Kept(4));
    }
}
