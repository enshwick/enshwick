use super::{Config, ConfigError};

#[test]
fn a_text_refusal_reaches_the_loader_as_its_key_and_values() {
    let refused = ConfigError::text("config.min", &[("min", "2"), ("of\t=x", "a\tb")]);
    assert_eq!(refused.to_reason(), "config.min\tmin=2\tof  x=a b");
    assert_eq!(
        ConfigError::text("config.off", &[]).to_reason(),
        "config.off"
    );
    assert_eq!(ConfigError::from("plain").to_reason(), "plain");
}

/// Locker settings.
#[crate::config(file = "lockers")]
#[derive(Debug, PartialEq)]
#[config(text = "lockers.config")]
struct Lockers {
    /// How many.
    #[config(range = 1..=16)]
    limit: u32,
    /// The mode.
    #[config(choices = ["auto", "on"], text = "lockers.mode")]
    mode: String,
    /// The volume.
    #[config(range = 0.0..)]
    volume: f32,
}

impl Default for Lockers {
    fn default() -> Self {
        Self {
            limit: 4,
            mode: "auto".into(),
            volume: 1.0,
        }
    }
}

#[test]
fn a_type_names_its_file_or_writes_settings() {
    /// Plain.
    #[crate::config]
    #[derive(Default)]
    struct Plain {
        /// A value.
        value: u8,
    }
    assert_eq!(Plain::FILE, "settings");
    assert_eq!(Lockers::FILE, "lockers");
}

#[test]
fn a_missing_key_reads_as_its_default() {
    let read: Lockers = toml::from_str("limit = 9\n").unwrap();
    assert_eq!(
        read,
        Lockers {
            limit: 9,
            ..Lockers::default()
        }
    );
}

#[test]
fn the_schema_describes_ranges_choices_and_texts() {
    use super::Kind;

    let schema = Lockers::schema();
    assert_eq!(schema.doc, "Locker settings.");
    assert_eq!(schema.text, "lockers.config");
    let limit = schema.field("limit").unwrap();
    assert_eq!(
        (limit.kind, limit.min, limit.max),
        (Some(Kind::Integer), Some(1.0), Some(16.0))
    );
    assert_eq!(
        (limit.default.as_str(), limit.doc.as_str()),
        ("4", "How many.")
    );
    assert_eq!(limit.text, "config.limit");
    let mode = schema.field("mode").unwrap();
    assert_eq!(mode.choices, ["\"auto\"", "\"on\""]);
    assert_eq!(mode.text, "lockers.mode");
    let volume = schema.field("volume").unwrap();
    assert_eq!((volume.min, volume.max), (Some(0.0), None));
    assert_eq!(super::Schema::parse(&schema.to_text()).unwrap(), schema);
    assert!(Lockers::template().contains("# How many.\nlimit = 4"));
}

#[test]
fn validation_uses_the_schema() {
    let mut lockers = Lockers::default();
    assert!(lockers.validate().is_ok());
    lockers.limit = 17;
    assert!(
        lockers
            .validate()
            .unwrap_err()
            .to_string()
            .contains("1 to 16")
    );
    lockers.limit = 3;
    lockers.mode = "off".into();
    assert!(
        lockers
            .validate()
            .unwrap_err()
            .to_string()
            .contains("one of")
    );
}

#[test]
fn an_f32_at_its_decimal_bound_passes_and_reads_as_written() {
    /// Ratio.
    #[crate::config]
    #[derive(Debug)]
    struct Ratio {
        /// Share.
        #[config(range = 0.0..=0.1, choices = [0.05, 0.1, 1.0])]
        share: f32,
    }
    impl Default for Ratio {
        fn default() -> Self {
            Self { share: 0.1 }
        }
    }
    assert!(Ratio::default().validate().is_ok());
    assert!(Ratio::template().contains("share = 0.1\n"));
    assert_eq!(Ratio::schema().field("share").unwrap().default, "0.1");
    assert!(Ratio { share: 0.11 }.validate().is_err());
    let field = &Ratio::schema().fields[0];
    assert!(field.check(&toml::Value::Integer(1)).is_err());
    let whole = super::Field::new("whole", &1.0_f64).with_choices(&[0.5, 1.0]);
    assert!(whole.check(&toml::Value::Integer(1)).is_ok());
    assert!(whole.check(&toml::Value::Integer(2)).is_err());
}

#[test]
fn a_field_toml_cannot_hold_still_leaves_the_others_checked() {
    /// Mixed.
    #[crate::config]
    #[derive(Default)]
    struct Mixed {
        /// Keyed by number, which TOML cannot write.
        names: rustc_hash::FxHashMap<u32, String>,
        /// Checked.
        #[config(range = 1..=16)]
        limit: u32,
        /// Unset.
        #[config(range = 1..=4)]
        spare: Option<u8>,
        /// Beyond TOML's integers.
        #[config(range = 1..)]
        seed: u64,
    }
    let mut mixed = Mixed {
        names: [(1, "one".to_owned())].into_iter().collect(),
        limit: 99,
        seed: 1,
        ..Mixed::default()
    };
    assert!(mixed.validate().unwrap_err().to_string().contains("limit"));
    mixed.limit = 3;
    assert!(mixed.validate().is_ok());
    mixed.seed = u64::MAX;
    assert!(mixed.validate().unwrap_err().to_string().contains("seed"));
}

#[test]
fn the_template_parses_back_whatever_the_names_and_docs() {
    /** Block doc,
    over two lines. */
    #[crate::config]
    #[derive(Debug, Default, PartialEq)]
    struct Odd {
        /** A raw name,
        documented in a block. */
        r#type: u32,
        #[doc = "First line\nsecond line"]
        size: u8,
    }
    let template = Odd::template();
    let parsed: Odd = toml::from_str(&template).unwrap();
    assert_eq!(parsed, Odd::default());
    assert!(template.contains("#    over two lines."));
    assert!(template.contains("#second line\nsize = 0"));
    assert!(Odd::schema().field("type").is_some());
    assert_eq!(super::__entry("größe", &0), "\"größe\" = 0\n\n");
}

#[test]
fn a_key_with_an_empty_part_is_refused_by_name() {
    use super::{Field, Group, Schema};

    for key in ["", ".token", "token.", "auth..token"] {
        let schema = Schema::new("", "config", vec![Field::new(key, "x")]);
        let error = Schema::parse(&schema.to_text()).unwrap_err().to_string();
        assert!(error.contains(&format!("`{key}`")), "{error}");
    }
    let grouped = Schema::new("", "config", vec![Field::new("auth.token", "x")])
        .with_groups(vec![Group::new("auth..inner", "")]);
    let error = Schema::parse(&grouped.to_text()).unwrap_err().to_string();
    assert!(error.contains("`auth..inner`"), "{error}");
    let fine = Schema::new("", "config", vec![Field::new("auth.token", "x")]);
    assert_eq!(Schema::parse(&fine.to_text()).unwrap(), fine);
}
