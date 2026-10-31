//! A mod's texts reach the loader, for its settings' labels: the file
//! `declare_mod!` embedded, and texts handed over later.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::i18n::{Language, Texts};
    use enshwick::{Load, Mod, Off};

    struct Labelled;

    impl Mod for Labelled {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let texts = load.texts()?;
            assert_eq!(texts.get("config.limit", Language::UK_UA), Some("Ліміт"));
            // As a pack's texts arrive after the mod's own.
            let _more = load.texts_from(
                "[en]
\"locker.name\" = \"Locker\"
",
            )?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Labelled,
        r#"
        id = "you.labelled"
        side = "server"
        "#,
        texts = "tests/texts/labelled.toml",
    );

    #[test]
    fn the_loader_keeps_the_embedded_texts_and_those_handed_over_later() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Labelled)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let kept = Texts::parse(&loader.published_texts().unwrap()).unwrap();
        assert_eq!(kept.get("config.limit", Language::UK_UA), Some("Ліміт"));
        assert_eq!(kept.get("locker.name", Language::UK_UA), Some("Locker"));
    }

    #[test]
    fn a_mod_without_a_texts_file_is_off_when_it_asks_for_its_texts() {
        struct Untexted;

        impl Mod for Untexted {
            fn init(load: &mut Load<'_>) -> Result<Self, Off> {
                load.texts()?;
                Ok(Self)
            }
        }

        enshwick::test::declare_mod!(
            Untexted,
            r#"
            id = "you.untexted"
            side = "server"
            "#
        );

        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: `enshwick_mod_init` is the export of this test's `declare_mod!(Untexted)`.
        let status = unsafe { loader.init(enshwick_mod_init) };
        assert_eq!(status, Status::REFUSED);
        assert!(
            loader
                .logs()
                .iter()
                .any(|(_, line)| line.contains("no src/texts.toml")),
            "{:?}",
            loader.logs()
        );
    }
}
