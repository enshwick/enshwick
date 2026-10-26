//! The mod's step clock: the loader's steps move it, each counted at most
//! `MAX_STEP`, and it stands still between steps.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;
    use std::time::Duration;

    use enshwick::abi::{Header, Status, TickData};
    use enshwick::events::Ticked;
    use enshwick::time::StepClock;
    use enshwick::{Load, Mod, Off};

    static CLOCK: OnceLock<&'static StepClock> = OnceLock::new();

    struct Timed;

    impl Mod for Timed {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.timed", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let clock = load.step_clock()?;
            assert!(core::ptr::eq(clock, load.step_clock()?), "one clock");
            let _first = CLOCK.set(clock);
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Timed);

    #[test]
    fn steps_move_the_clock_and_it_stands_still_between_them() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Timed)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let clock = CLOCK.get().unwrap();
        let tick = |step| TickData {
            header: Header::of::<TickData>(),
            step,
        };
        assert_eq!(loader.raise::<Ticked>(&tick(1)), 1, "one subscription");
        assert_eq!(clock.now(), Duration::ZERO, "the first step starts it");
        std::thread::sleep(Duration::from_millis(400));
        assert_eq!(clock.now(), Duration::ZERO, "no step, no time");
        assert_eq!(loader.raise::<Ticked>(&tick(2)), 1);
        assert_eq!(
            clock.now(),
            StepClock::MAX_STEP,
            "a stall counts one long step"
        );
    }
}
