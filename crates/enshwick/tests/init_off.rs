//! A mod whose init turns it off: the loader learns why, as a text the
//! reader's language can show.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::{Load, Mod, Off};

    struct Lanterns;

    impl Mod for Lanterns {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _texts = load.texts_from(TEXTS)?;
            Err(Off::text("lanterns.off", &[("lit", "20")]))
        }
    }

    const TEXTS: &str = "[en]\n\"lanterns.off\" = \"{lit} lanterns are lit, too many.\"\n\
                         [uk]\n\"lanterns.off\" = \"Горить {lit} ліхтарів, забагато.\"\n";

    enshwick::test::declare_mod!(
        Lanterns,
        r#"
        id = "you.lanterns"
        side = "server"
        "#
    );

    #[test]
    fn an_init_that_turns_the_mod_off_tells_the_loader_why_in_its_texts() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::REFUSED);
        let faults = loader.faults();
        let [sent] = faults.as_slice() else {
            panic!("one reason, got {faults:?}");
        };
        let texts = enshwick::i18n::Texts::parse(&loader.published_texts().unwrap_or_default())
            .unwrap_or_default();
        assert_eq!(
            texts
                .reason(sent, enshwick::i18n::Language::UK_UA)
                .as_deref(),
            Some("Горить 20 ліхтарів, забагато.")
        );
        assert_eq!(
            texts
                .reason(sent, enshwick::i18n::Language::FALLBACK)
                .as_deref(),
            Some("20 lanterns are lit, too many.")
        );
    }
}
