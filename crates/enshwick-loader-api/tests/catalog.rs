//! A mod's catalog reaches the Mods screen, and the player's pick reaches
//! the mod.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick_loader_api::abi::{Side, Status};
    use enshwick_loader_api::catalog::{self, Entry};
    use enshwick_loader_api::{Load, Mod, Off};

    static PICKED: Mutex<Vec<String>> = Mutex::new(Vec::new());

    struct Offering;

    impl Mod for Offering {
        fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
            enshwick_loader_api::manifest!(id: "offering", side: CLIENT)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.on_catalog_pick(|id| PICKED.lock().unwrap().push(id.to_owned()))?;
            catalog::publish(&[
                Entry::new("safes", "0.7.0").installed("0.6.1").ready(true),
                Entry::new("lanterns", "1.0.0").source("github.com/someone/lanterns"),
            ])?;
            Ok(Self)
        }
    }

    enshwick_loader_api::declare_mod!(Offering);

    #[test]
    fn the_catalog_shows_and_picks_come_back() {
        let loader = enshwick_loader_api::test::FakeHost::new()
            .side(Side::CLIENT)
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let shown = loader.published_catalog();
        assert_eq!(shown.len(), 2);
        assert_eq!(shown[0].installed.as_deref(), Some("0.6.1"));
        assert!(shown[0].ready && !shown[1].ready);
        assert!(loader.pick_catalog("safes"));
        assert_eq!(*PICKED.lock().unwrap(), ["safes"]);
    }
}
