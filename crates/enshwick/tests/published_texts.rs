//! A mod's texts reach the loader, for its settings' labels.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::i18n::{Language, Texts};
    use enshwick::{Load, Mod, Off};

    const TEXTS: &str = "[en]\n\"config.limit\" = \"Limit\"\n[uk]\n\"config.limit\" = \"Ліміт\"\n";

    struct Labelled;

    impl Mod for Labelled {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.labelled", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _texts = load.texts(TEXTS)?;
            // As a pack's texts arrive after the mod's own.
            let _more = load.texts(
                "[en]
\"locker.name\" = \"Locker\"
",
            )?;
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Labelled);

    #[test]
    fn the_loader_keeps_the_mods_texts() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Labelled)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let kept = Texts::parse(&loader.published_texts().unwrap()).unwrap();
        assert_eq!(kept.get("config.limit", Language::UK_UA), Some("Ліміт"));
        assert_eq!(kept.get("locker.name", Language::UK_UA), Some("Locker"));
    }
}
