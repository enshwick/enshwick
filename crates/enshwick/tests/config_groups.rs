//! Settings in groups: a field that is a `Config` of its own is a table of
//! the settings file.

#[cfg(test)]
mod tests {
    use enshwick::config::Config;

    /// Comfort rules.
    #[enshwick::config]
    #[derive(Debug, Default, PartialEq)]
    struct Comfort {
        /// Comfort per piece of furniture, times this factor.
        #[config(range = 0.0..=10.0)]
        multiplier: f32,
        /// Whether players see it.
        shown: bool,
    }

    /// Rates of the game.
    #[enshwick::config]
    #[derive(Debug, Default, PartialEq)]
    struct Rates {
        /// The comfort rules.
        #[config(group)]
        comfort: Comfort,
        /// Recipes cost this many times their price.
        #[config(range = 1..=5)]
        cost: u32,
    }

    #[test]
    fn a_group_is_a_table_after_the_plain_settings() {
        let template = Rates::template();
        let cost = template.find("cost = 0").unwrap();
        let table = template.find("[comfort]").unwrap();
        assert!(cost < table, "{template}");
        assert!(
            template.contains("# The comfort rules.\n[comfort]\n"),
            "{template}"
        );
        assert!(template.contains("multiplier = 0.0"), "{template}");
        let read: Rates = toml::from_str(&template).unwrap();
        assert_eq!(read, Rates::default());
    }

    #[test]
    fn the_schema_names_every_grouped_field_by_its_dotted_key() {
        let schema = Rates::schema();
        let keys: Vec<&str> = schema.fields.iter().map(|f| f.key.as_str()).collect();
        assert_eq!(keys, ["cost", "comfort.multiplier", "comfort.shown"]);
        let multiplier = schema.field("comfort.multiplier").unwrap();
        assert_eq!(multiplier.text, "config.comfort.multiplier");
        assert_eq!(multiplier.max, Some(10.0));
        assert_eq!(schema.groups.len(), 1);
        assert_eq!(schema.groups[0].key, "comfort");
        assert_eq!(schema.groups[0].doc, "The comfort rules.");
        assert_eq!(
            schema.group_of("comfort.shown").map(|g| g.text.as_str()),
            Some("config.comfort")
        );
        let file: toml::Table = toml::from_str("[comfort]\nmultiplier = 20.0\n").unwrap();
        assert!(schema.check(&file).is_err());
    }

    #[test]
    fn a_group_reads_missing_keys_at_their_defaults_and_is_checked() {
        let mut rates: Rates = toml::from_str("cost = 3\n[comfort]\nmultiplier = 2.0\n").unwrap();
        assert_eq!(
            rates,
            Rates {
                comfort: Comfort {
                    multiplier: 2.0,
                    shown: false,
                },
                cost: 3,
            }
        );
        assert!(rates.validate().is_ok());
        rates.comfort.multiplier = 11.0;
        assert!(rates.validate().is_err());
    }
}
