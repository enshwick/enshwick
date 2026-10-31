//! A mod that needs inventories on a build without them turns off at init.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::test::FakeHost;

    struct Looter;

    impl enshwick::Mod for Looter {
        fn init(load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
            load.inventories()?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Looter,
        r#"
        id = "you.looter"
        side = "server"
        "#
    );

    #[test]
    fn a_build_without_inventories_turns_the_mod_off() {
        let loader = FakeHost::new().without_inventories().install();
        // SAFETY: `enshwick_mod_init` is the export of this file's `declare_mod!(Looter)`.
        assert_ne!(unsafe { loader.init(enshwick_mod_init) }, Status::OK);
        assert!(loader.inventory_requests().is_empty());
    }
}
