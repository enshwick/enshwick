//! Weather and chunk events, as the loader hands them over.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;
    use std::time::Duration;

    use enshwick_loader_api::abi::{ChunkData, Header, Status, WeatherFront};
    use enshwick_loader_api::events::ChunkLoaded;
    use enshwick_loader_api::weather::{Forecast, FrontKind, Weather, WeatherState};
    use enshwick_loader_api::world::WorldError;
    use enshwick_loader_api::{Load, Mod, Off};

    static WEATHER: OnceLock<Weather> = OnceLock::new();

    struct Rainmaker;

    impl Mod for Rainmaker {
        fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
            enshwick_loader_api::manifest!(id: "rainmaker", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = WEATHER.set(load.weather());
            load.on::<ChunkLoaded>(|chunk| {
                enshwick_loader_api::info!("chunk {},{} loaded", chunk.x, chunk.z);
            })?;
            Ok(Self)
        }
    }

    enshwick_loader_api::declare_mod!(Rainmaker);

    #[test]
    fn weather_reads_and_starts_and_chunks_arrive() {
        let loader = enshwick_loader_api::test::FakeHost::new().install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let weather = WEATHER.get().unwrap();

        assert_eq!(weather.at([0.0; 3]), None, "no world runs yet");
        assert_eq!(weather.set(Forecast::Rain, None), Err(WorldError::Missing));
        let front = WeatherFront {
            header: Header::of::<WeatherFront>(),
            layer: 1,
            effect: WeatherFront::BLIZZARD,
            position: [5 << 32, 0, -(7 << 32)],
            radius: 300.0,
            reserved: 0,
            remaining: 90_000_000_000,
        };
        loader.set_weather([0.1, 0.2, 0.6, 0.1], &[front]);
        assert_eq!(
            weather.at([1.0, 2.0, 3.0]).unwrap().state(),
            WeatherState::Snow
        );
        let fronts = weather.fronts();
        assert_eq!(fronts.len(), 1);
        assert_eq!(fronts[0].kind, FrontKind::Blizzard);
        assert!(
            (fronts[0].position[0] - 5.0).abs() < 1e-9
                && (fronts[0].position[2] + 7.0).abs() < 1e-9
        );
        assert_eq!(fronts[0].remaining, Duration::from_secs(90));

        weather
            .set(Forecast::Clear, Some(Duration::from_mins(10)))
            .unwrap();
        weather.set(Forecast::Any, None).unwrap();
        assert_eq!(
            loader.forecasts(),
            [(WeatherFront::NONE, 600_000_000_000), (u32::MAX, 0)]
        );

        let chunk = ChunkData {
            header: Header::of::<ChunkData>(),
            index: 41,
            x: 1,
            z: 1,
            size: 256,
        };
        assert_eq!(loader.raise::<ChunkLoaded>(&chunk), 1);
    }
}
