//! Settings in groups: a field that is a `Config` of its own is a table of
//! the settings file.

#[cfg(test)]
mod tests {
    use enshwick::config::{Config, Stage};

    /// Comfort rules.
    #[enshwick::config(default)]
    #[derive(Debug, Default, PartialEq)]
    struct Comfort {
        /// Comfort per piece of furniture, times this factor.
        #[config(runtime, range = 0.0..=10.0)]
        multiplier: f32,
        /// Players see the server's value.
        #[config(sync)]
        shown: bool,
    }

    /// Rates of the game.
    #[enshwick::config(default)]
    #[derive(Debug, Default, PartialEq)]
    struct Rates {
        /// The comfort rules.
        #[config(group)]
        comfort: Comfort,
        /// Recipes cost this many times their price.
        #[config(startup, range = 1..=5)]
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
    fn a_group_s_values_follow_their_stage_and_merge_back() {
        let mut rates = Rates {
            comfort: Comfort {
                multiplier: 2.0,
                shown: true,
            },
            cost: 3,
        };
        assert_eq!(
            rates.synced().as_deref(),
            Some("{ comfort = { shown = true } }")
        );
        assert_eq!(
            rates.values(Stage::Startup).as_deref(),
            Some("{ cost = 3 }")
        );
        assert_eq!(rates.values(Stage::Client), None);
        rates
            .merge_synced("{ comfort = { shown = false } }")
            .unwrap();
        assert!(!rates.comfort.shown);
        assert_eq!(rates.comfort.multiplier, 2.0);
        assert!(rates.validate().is_ok());
        rates.comfort.multiplier = 11.0;
        assert!(rates.validate().is_err());
    }
}

#[cfg(test)]
mod moved {
    use enshwick::abi::Status;
    use enshwick::migrations::Migrations;

    struct Mover;

    impl enshwick::Mod for Mover {
        fn init(load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
            load.migrate(
                Migrations::new().add("2", |m| m.rename_setting("per_level", "count.per_level")),
            )?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Mover,
        r#"
        id = "you.mover"
        side = "server"
        "#
    );

    #[test]
    fn a_flat_setting_renames_into_a_group() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(
            loader.migrate_settings("per_level = [2, 4]\n"),
            Ok("[count]\nper_level = [2, 4]\n".to_owned())
        );
    }
}
