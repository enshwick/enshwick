//! The mod's step clock: the loader's steps move it, each counted at most
//! `MAX_STEP`, and it stands still between steps.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;
    use std::time::Duration;

    use enshwick::abi::{Status, TickData};
    use enshwick::events::Ticked;
    use enshwick::time::StepClock;
    use enshwick::{Load, Mod, Off};

    static CLOCK: OnceLock<&'static StepClock> = OnceLock::new();

    struct Timed;

    impl Mod for Timed {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let clock = load.step_clock()?;
            assert!(core::ptr::eq(clock, load.step_clock()?), "one clock");
            let _first = CLOCK.set(clock);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Timed,
        r#"
        id = "you.timed"
        side = "server"
        "#
    );

    #[test]
    fn ticks_move_the_clock_by_their_time_up_to_the_most_a_step_counts() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Timed)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let clock = CLOCK.get().unwrap();
        let start = clock.now();
        let step = Duration::from_millis(50);
        assert_eq!(loader.tick(step), 1, "the first step calls the clock");
        assert_eq!(clock.now(), start + step);
        assert_eq!(loader.tick(step), 0, "the period has not passed");
        assert_eq!(clock.now(), start + step, "no call, no time");
        assert_eq!(loader.tick(step), 1);
        assert_eq!(clock.now(), start + step * 3, "two steps of time at once");
        assert_eq!(loader.tick(Duration::from_secs(60)), 1);
        assert_eq!(
            clock.now(),
            start + step * 3 + StepClock::MAX_STEP,
            "a stall counts one long step"
        );
        let before = clock.now();
        let tick = TickData::new(1).with_elapsed(4, Duration::from_millis(200));
        assert_eq!(loader.raise::<Ticked>(&tick), 1, "one subscription");
        assert_eq!(clock.now(), before + Duration::from_millis(200));
    }
}
