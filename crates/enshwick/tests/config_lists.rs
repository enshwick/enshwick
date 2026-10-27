//! Settings that are lists: a range or choices apply to every element.

#[cfg(test)]
mod tests {
    use enshwick::config::{Config, Field, Stage};

    /// Altar rules.
    #[derive(Debug, PartialEq, serde::Deserialize, serde::Serialize, enshwick::Config)]
    #[serde(default)]
    struct Altars {
        /// Altars allowed at each Flame level.
        #[config(startup, range = 1..=32)]
        per_level: Vec<u16>,
    }

    impl Default for Altars {
        fn default() -> Self {
            Self {
                per_level: vec![2, 4, 8],
            }
        }
    }

    #[test]
    fn a_range_checks_every_element_of_a_list() {
        let field =
            Field::new("per_level", Stage::Startup, &[2, 4]).with_range(Some(1.0), Some(32.0));
        assert!(
            field
                .check(&toml::Value::try_from([1, 32]).unwrap())
                .is_ok()
        );
        assert!(
            field
                .check(&toml::Value::try_from([1, 33]).unwrap())
                .is_err()
        );
        assert!(field.check(&toml::Value::try_from(["a"]).unwrap()).is_err());
        let mut altars = Altars::default();
        assert!(altars.validate().is_ok());
        altars.per_level.push(40);
        assert!(altars.validate().is_err());
    }
}

/// A settings file with values outside their ranges still reads: each such
/// value is held to its range with a warning.
#[cfg(test)]
mod clamping {
    use std::sync::Mutex;

    use enshwick::abi::Status;

    /// Rules with ranges.
    #[derive(Debug, PartialEq, serde::Deserialize, serde::Serialize, enshwick::Config)]
    #[serde(default)]
    struct Rules {
        /// Altars allowed at each Flame level.
        #[config(startup, range = 1..=32)]
        per_level: Vec<u16>,
        /// How far, in metres.
        #[config(range = 0.5..=10.0)]
        reach: f32,
    }

    impl Default for Rules {
        fn default() -> Self {
            Self {
                per_level: vec![2, 4],
                reach: 1.0,
            }
        }
    }

    static READ: Mutex<Option<Rules>> = Mutex::new(None);

    struct Clamped;

    impl enshwick::Mod for Clamped {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.clamped", side: SERVER)
        }

        fn init(load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
            let rules: Rules = load.config()?;
            *READ
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(rules);
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Clamped);

    #[test]
    fn values_outside_their_range_are_held_to_it_with_a_warning() {
        let loader = enshwick::test::FakeHost::new()
            .settings("per_level = [0, 4, 40]\nreach = 25.0\n")
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let read = READ.lock().unwrap().take().unwrap();
        assert_eq!(read.per_level, [1, 4, 32]);
        assert!((read.reach - 10.0).abs() < f32::EPSILON);
        let warned: Vec<String> = loader
            .logs()
            .into_iter()
            .filter(|(_, line)| line.contains("outside"))
            .map(|(_, line)| line)
            .collect();
        assert_eq!(warned.len(), 3, "{warned:?}");
        assert!(
            warned
                .iter()
                .any(|l| l == "settings: per_level[2] is outside 1 to 32; 32 applies"),
            "{warned:?}"
        );
        assert!(!warned.iter().any(|l| l.contains("40")), "{warned:?}");
    }
}
