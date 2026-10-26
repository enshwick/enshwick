//! The clock: reads, a time set and a factor on the speed, as the loader
//! receives them.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::{DaytimeInfo, Header, Status};
    use enshwick::daytime::{Clock, ClockSpeed};
    use enshwick::events::NightStarted;
    use enshwick::world::WorldError;
    use enshwick::{Load, Mod, Off};

    const HOUR: i64 = 3_600_000_000_000;

    static CLOCK: OnceLock<(Clock, ClockSpeed)> = OnceLock::new();

    struct Timed;

    impl Mod for Timed {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.timed", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = CLOCK.set((load.clock(), load.clock_speed()?));
            load.on::<NightStarted>(|()| enshwick::info!("night"))?;
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Timed);

    #[test]
    fn the_clock_reads_sets_and_speeds_up() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let (clock, speed) = CLOCK.get().unwrap();

        assert_eq!(clock.now(), None, "no world runs yet");
        assert_eq!(clock.set_hours(3.0), Err(WorldError::Missing));
        loader.set_daytime(DaytimeInfo {
            header: Header::of::<DaytimeInfo>(),
            time: 7 * HOUR,
            day_start: 6 * HOUR,
            day_end: 18 * HOUR,
            day_length: 1_800_000_000_000,
            night_length: 720_000_000_000,
            speed: 1.0,
            flags: DaytimeInfo::DAY,
        });
        let now = clock.now().unwrap();
        assert!((now.hours - 7.0).abs() < 1e-4);
        assert!(now.is_day);
        assert!((now.night_starts - 18.0).abs() < 1e-4);
        assert_eq!(now.day_length.as_secs(), 1800);

        clock.set_hours(-2.5).unwrap();
        let now = clock.now().unwrap();
        assert!((now.hours - 21.5).abs() < 1e-4, "taken modulo a day");
        assert!(!now.is_day);
        assert_eq!(clock.set_hours(f32::NAN), Err(WorldError::Refused));

        assert_eq!(loader.clock_speed(), Some(1.0));
        speed.set(2.0).unwrap();
        assert_eq!(loader.clock_speed(), Some(2.0));
        assert_eq!(speed.set(-1.0), Err(WorldError::Refused));
        assert_eq!(speed.set(ClockSpeed::MAX + 1.0), Err(WorldError::Refused));

        assert_eq!(loader.raise::<NightStarted>(&()), 1);
    }
}
