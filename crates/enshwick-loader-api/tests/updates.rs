//! A mod's updates reach the Mods screen, and the player's pick reaches the
//! mod.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick_loader_api::abi::{Side, Status};
    use enshwick_loader_api::updates::{self, Update};
    use enshwick_loader_api::{Load, Mod, Off};

    static PICKED: Mutex<Vec<String>> = Mutex::new(Vec::new());

    struct Offering;

    impl Mod for Offering {
        fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
            enshwick_loader_api::manifest!(id: "offering", side: CLIENT)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.on_update_pick(|id| PICKED.lock().unwrap().push(id.to_owned()))?;
            updates::publish(&[
                Update::new("safes", "0.7.0").ready(true),
                Update::new("lanterns", "1.0.0").note("update it by hand"),
            ])?;
            Ok(Self)
        }
    }

    enshwick_loader_api::declare_mod!(Offering);

    #[test]
    fn updates_show_and_picks_come_back() {
        let loader = enshwick_loader_api::test::FakeHost::new()
            .side(Side::CLIENT)
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let shown = loader.published_updates();
        assert_eq!(shown.len(), 2);
        assert!(shown[0].ready && !shown[1].ready);
        assert!(loader.pick_update("safes"));
        assert_eq!(*PICKED.lock().unwrap(), ["safes"]);
    }
}
