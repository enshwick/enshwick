//! `enshwick.daytime`: the clock a script reads and sets, and the clock
//! speed it opens while it starts, as a Rust mod does.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::test::FakeHost;

    use crate::harness::{logged, run, start};

    #[test]
    fn a_script_opens_its_clock_speed_at_start_and_sets_it() {
        let loader = run(
            FakeHost::new(),
            r"
            local speed = enshwick.daytime.clock_speed()
            speed:set(2.5)
            enshwick.log.info(`class {tostring(speed)}`)
            ",
        );
        assert_eq!(loader.clock_speed(), Some(2.5));
        assert!(logged(&loader, "class ClockSpeed"));
    }

    #[test]
    fn a_factor_the_loader_refuses_raises_an_error_naming_the_method() {
        let loader = run(
            FakeHost::new(),
            r"
            local ok, why = pcall(function()
                enshwick.daytime.clock_speed():set(-1)
            end)
            enshwick.log.info(`refused {not ok} {why}`)
            ",
        );
        assert!(logged(&loader, "refused true"));
        assert!(logged(&loader, "ClockSpeed:set"));
    }

    #[test]
    fn opening_the_clock_speed_after_the_start_raises_an_error() {
        let loader = run(
            FakeHost::new(),
            r"
            local open = enshwick.daytime.clock_speed
            enshwick.on('WorldSaved', function()
                local ok, why = pcall(open)
                enshwick.log.info(`late {not ok} {why}`)
            end)
            ",
        );
        let _handled = loader.raise::<enshwick::events::WorldSaved>(&());
        assert!(logged(&loader, "late true"), "{:?}", loader.logs());
        assert!(logged(&loader, "only while the mod starts"));
    }

    #[test]
    fn a_script_reads_the_clock_as_a_time_of_day() {
        let loader = run(
            FakeHost::new(),
            r"
            enshwick.on('WorldSaved', function()
                local now = enshwick.daytime.now()
                enshwick.log.info(`now {now and now.is_day} {now and now.speed}`)
            end)
            ",
        );
        loader.set_daytime(enshwick::abi::DaytimeInfo {
            header: enshwick::abi::Header::of::<enshwick::abi::DaytimeInfo>(),
            time: 12 * 3_600_000_000_000,
            day_start: 6 * 3_600_000_000_000,
            day_end: 22 * 3_600_000_000_000,
            day_length: 1_800_000_000_000,
            night_length: 720_000_000_000,
            speed: 1.0,
            flags: enshwick::abi::DaytimeInfo::DAY,
        });
        let _handled = loader.raise::<enshwick::events::WorldSaved>(&());
        assert!(logged(&loader, "now true 1"), "{:?}", loader.logs());
    }

    #[test]
    fn a_mod_whose_start_fails_stays_off() {
        let (_loader, status) = start(
            FakeHost::new(),
            "enshwick.daytime.clock_speed():set('fast')",
        );
        assert_ne!(status, Status::OK);
    }
}
