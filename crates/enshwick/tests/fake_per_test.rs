//! The fake host starts a mod afresh on every install, so tests that share a
//! process under `cargo test` each see a mod of their own.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::test::FakeHost;
    use enshwick::{Load, Mod, Off};

    /// Test settings.
    #[derive(Default, serde::Deserialize, serde::Serialize, enshwick::Config)]
    struct Settings {
        /// A rate.
        rate: u32,
    }

    struct Rates {
        rate: u32,
    }

    impl Mod for Rates {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let settings: Settings = load.config()?;
            Ok(Self {
                rate: settings.rate,
            })
        }
    }

    enshwick::test::declare_mod!(
        Rates,
        r#"
        id = "you.rates"
        side = "server"
        "#
    );

    fn started_with(settings: &str) -> Option<u32> {
        let loader = FakeHost::new().settings(settings).install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Rates)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        Rates::instance().map(|rates| rates.rate)
    }

    #[test]
    fn a_second_install_in_one_process_starts_the_mod_again() {
        assert_eq!(started_with("rate = 2\n"), Some(2));
        assert_eq!(started_with("rate = 3\n"), Some(3));
    }
}
