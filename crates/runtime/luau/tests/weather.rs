//! `enshwick.weather`: the fronts a script hears start and end, and the
//! phenomena calls, as a Rust mod's.

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use enshwick::abi::{Header, WeatherFront};
    use enshwick::test::FakeHost;

    use crate::harness::{logged, run};

    fn front(phenomenon: u8) -> WeatherFront {
        WeatherFront {
            header: Header::of::<WeatherFront>(),
            layer: 0,
            effect: WeatherFront::RAIN,
            position: [0; 3],
            radius: 300.0,
            reserved: 0,
            remaining: 90_000_000_000,
            phenomenon: [phenomenon; 16],
        }
    }

    #[test]
    fn a_script_hears_a_front_start_and_end_with_its_phenomenon() {
        let loader = run(
            FakeHost::new().world(),
            r"
            enshwick.weather.on_front(function(change)
                enshwick.log.info(`front {change.started} {change.front.kind} {change.front.phenomenon}`)
            end)
            ",
        );
        // The scheduler runs on wall time: `on_front` looks once a second.
        let poll = || {
            std::thread::sleep(Duration::from_millis(1_050));
            let _ran = loader.tick(Duration::from_secs(1));
        };
        loader.set_weather([0.0, 1.0, 0.0, 0.0], &[front(1)]);
        poll();
        assert!(
            logged(
                &loader,
                "front true Rain 01010101-0101-0101-0101-010101010101"
            ),
            "{:?}",
            loader.logs()
        );
        loader.set_weather([1.0, 0.0, 0.0, 0.0], &[]);
        poll();
        assert!(logged(&loader, "front false Rain"), "{:?}", loader.logs());
    }

    #[test]
    fn adding_a_phenomenon_without_the_weather_resource_raises_an_error() {
        let loader = run(
            FakeHost::new(),
            r#"
            local ok, why = pcall(enshwick.weather.add_weather_phenomenon, {
                id = "you.luau:drizzle",
                like = "01010101-0101-0101-0101-010101010101",
                values = { probability = 1 },
            })
            enshwick.log.info(`added {ok} {why}`)
            ok, why = pcall(enshwick.weather.change_weather_phenomenon, "01010101-0101-0101-0101-010101010101", { probability = 2 })
            enshwick.log.info(`changed {ok} {why}`)
            "#,
        );
        assert!(logged(&loader, "added false"), "{:?}", loader.logs());
        assert!(logged(&loader, "changed false"), "{:?}", loader.logs());
    }
}
