//! A mod's optional features: one that turns off on this game build leaves
//! the mod and its other features running, and the loader learns which and
//! why.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::{Load, Mod, Off};

    struct Lanterns;

    impl Mod for Lanterns {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _texts = load.texts(TEXTS)?;
            let glow = load.feature("glow", |load| load.symbol("lantern.glow"));
            assert_eq!(glow, None);
            let light = load.feature("light", |load| load.symbol("lantern.light"));
            assert_eq!(light, Some(0x1_4000_2000));
            let flicker = load.feature("flicker", |load| {
                load.symbol("lantern.flicker")
                    .map_err(|_| Off::text("flicker.off", &[("symbol", "lantern.flicker")]))
            });
            assert_eq!(flicker, None);
            Ok(Self)
        }
    }

    const TEXTS: &str = "[en]\n\"flicker.off\" = \"No flicker: {symbol} is missing.\"\n\
                         [uk]\n\"flicker.off\" = \"Без мерехтіння: бракує {symbol}.\"\n";

    enshwick::test::declare_mod!(
        Lanterns,
        r#"
        id = "you.lanterns"
        side = "server"
        "#
    );

    #[test]
    fn a_feature_that_turns_off_leaves_the_mod_running_and_says_why() {
        let loader = enshwick::test::FakeHost::new()
            .symbol("lantern.light", 0x1_4000_2000)
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let features = loader.features();
        assert_eq!(
            features[..2],
            [
                (
                    "glow".to_owned(),
                    Some("symbol lantern.glow: missing on this game build".to_owned())
                ),
                ("light".to_owned(), None),
            ]
        );
        let (name, sent) = &features[2];
        assert_eq!(name, "flicker");
        let sent = sent.as_deref().unwrap_or_default();
        let texts = enshwick::i18n::Texts::parse(&loader.published_texts().unwrap_or_default())
            .unwrap_or_default();
        assert_eq!(
            texts
                .reason(sent, enshwick::i18n::Language::UK_UA)
                .as_deref(),
            Some("Без мерехтіння: бракує lantern.flicker.")
        );
        assert_eq!(
            texts
                .reason(sent, enshwick::i18n::Language::FALLBACK)
                .as_deref(),
            Some("No flicker: lantern.flicker is missing.")
        );
    }
}
