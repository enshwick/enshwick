//! A mod that needs inventories on a build without them turns off at init.

#[cfg(test)]
mod tests {
    use enshwick_api::abi::Status;
    use enshwick_test::FakeHost;

    struct Looter;

    impl enshwick_api::Mod for Looter {
        fn manifest() -> &'static enshwick_api::abi::Manifest {
            enshwick_api::manifest!(id: "looter", side: SERVER)
        }

        fn init(load: &mut enshwick_api::Load<'_>) -> Result<Self, enshwick_api::Off> {
            load.inventories()?;
            Ok(Self)
        }
    }

    enshwick_api::declare_mod!(Looter);

    #[test]
    fn a_build_without_inventories_turns_the_mod_off() {
        let loader = FakeHost::new().without_inventories().install();
        // SAFETY: the export of `declare_mod!` above.
        assert_ne!(unsafe { loader.init(enshwick_mod_init) }, Status::OK);
        assert!(loader.inventory_requests().is_empty());
    }
}
