//! Fog of war the local player uncovers, as a client mod hears it.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::Duration;

    use enshwick::abi::{FogInfo, Side, Status};
    use enshwick::map::FogOfWarUncovered;
    use enshwick::{Load, Mod, Off};

    static HEARD: Mutex<Vec<FogOfWarUncovered>> = Mutex::new(Vec::new());

    struct Explorer;

    impl Mod for Explorer {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.on_fog_uncovered(|new| HEARD.lock().unwrap().push(*new))?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Explorer,
        r#"
        id = "you.explorer"
        side = "both"
        "#
    );

    /// A fog of 4 × 3 pixels of 8 m whose last window of uncovering is
    /// `window`, the pixels 1 to 2 of row 1.
    fn fog(window: u64) -> FogInfo {
        FogInfo {
            width: 4,
            height: 3,
            metres_per_pixel: 8.0,
            size_z: 24.0,
            session: 1,
            uncovered: window,
            uncovered_pixels: 2,
            uncovered_min: [1, 1],
            uncovered_size: [2, 1],
            ..FogInfo::EMPTY
        }
    }

    #[test]
    fn a_client_hears_each_new_window_of_uncovered_fog_once_with_its_ground() {
        let loader = enshwick::test::FakeHost::new()
            .side(Side::CLIENT)
            .fog(fog(3), vec![0; 12])
            .install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Explorer)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let _first_look = loader.draw_frame();
        loader.set_fog(fog(4), vec![0; 12]);
        let _too_soon = loader.draw_frame();
        assert!(
            HEARD.lock().unwrap().is_empty(),
            "the first look and a look too soon"
        );
        std::thread::sleep(Duration::from_millis(260));
        let _new_window = loader.draw_frame();
        std::thread::sleep(Duration::from_millis(260));
        let _same_window = loader.draw_frame();
        let heard = HEARD.lock().unwrap();
        assert_eq!(heard.len(), 1, "{heard:?}");
        assert_eq!(
            (heard[0].min, heard[0].max, heard[0].pixels),
            ([8.0, 8.0], [24.0, 16.0], 2)
        );
    }

    #[test]
    fn a_server_hears_no_uncovered_fog() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Explorer)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_ne!(status, Status::OK, "the fog of war is the client's");
    }
}
