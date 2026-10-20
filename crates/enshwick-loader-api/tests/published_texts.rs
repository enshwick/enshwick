//! A mod's texts reach the loader, for its settings' labels.

#[cfg(test)]
mod tests {
    use enshwick_loader_api::abi::Status;
    use enshwick_loader_api::i18n::{Language, Texts};
    use enshwick_loader_api::{Load, Mod, Off};

    const TEXTS: &str = "[en]\n\"config.limit\" = \"Limit\"\n[uk]\n\"config.limit\" = \"Ліміт\"\n";

    struct Labelled;

    impl Mod for Labelled {
        fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
            enshwick_loader_api::manifest!(id: "labelled", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _texts = load.texts(TEXTS)?;
            Ok(Self)
        }
    }

    enshwick_loader_api::declare_mod!(Labelled);

    #[test]
    fn the_loader_keeps_the_mods_texts() {
        let loader = enshwick_loader_api::test::FakeHost::new().install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let kept = Texts::parse(&loader.published_texts().unwrap()).unwrap();
        assert_eq!(kept.get("config.limit", Language::UK_UA), Some("Ліміт"));
    }
}
