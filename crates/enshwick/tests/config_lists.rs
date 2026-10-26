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
