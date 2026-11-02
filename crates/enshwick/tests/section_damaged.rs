//! A mod whose binary's declarations are damaged: the loader learns the
//! mod, without its exports, and why it stays off, in the loader's own
//! texts.

mod broken {
    pub(super) struct Broken;

    impl enshwick::Mod for Broken {
        fn init(_load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Broken,
        r#"
        id = "you.broken"
        side = "both"
        join = true
        exports = ["content:locker"]
        "#
    );

    /// A head of this format that names no mod.
    const HEAD: &str =
        enshwick::__private::concatcp!("schema = ", enshwick::schema_section::FORMAT, "\n");

    enshwick::__schema_entry!(HEAD);
}

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;

    use super::broken;

    #[test]
    fn a_mod_whose_declarations_are_damaged_is_named_and_refused_without_exports() {
        // SAFETY: `enshwick_mod_query` returns null or the manifest of
        // `declare_mod!`, valid until its shutdown, which no test runs.
        let manifest = unsafe { broken::enshwick_mod_query().as_ref() }.expect("a manifest");
        // SAFETY: the id points at a `'static` string literal of `declare_mod!`.
        assert_eq!(unsafe { manifest.id.as_str() }, Some("you.broken"));
        assert_eq!(manifest.export_count, 0);
        // SAFETY: the schema text lives as long as the manifest.
        assert_eq!(unsafe { manifest.points.as_str() }.unwrap_or_default(), "");
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of `declare_mod!(Broken)`.
        let status = unsafe { loader.start(broken::enshwick_mod_init, broken::enshwick_mod_ready) };
        assert_eq!(status, Status::REFUSED);
        let faults = loader.faults();
        let [fault] = faults.as_slice() else {
            panic!("one fault: {faults:?}");
        };
        let byte = fault
            .strip_prefix("mods.off.section.damaged\tbyte=")
            .unwrap_or_else(|| panic!("{fault}"));
        assert!(byte.parse::<usize>().is_ok(), "{fault}");
        assert!(broken::Broken::instance().is_none());
    }
}
