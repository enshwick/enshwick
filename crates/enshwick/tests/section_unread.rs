//! A mod whose binary's declarations are of a newer format: the loader
//! learns the mod, without its exports, and why it stays off, in the
//! loader's own texts.

mod stale {
    pub(super) struct Stale;

    impl enshwick::Mod for Stale {
        fn init(_load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Stale,
        r#"
        id = "you.stale"
        side = "both"
        exports = ["content:locker"]
        "#
    );

    // A head of a format no enshwick writes yet, as a dependency built with
    // a newer enshwick would leave in the binary.
    enshwick::__schema_entry!("schema = 99\nmod = \"you.other\"\n");
}

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;

    use super::stale;

    #[test]
    fn a_mod_whose_declarations_are_newer_is_named_and_refused_without_exports() {
        // SAFETY: `enshwick_mod_query` returns null or the manifest of
        // `declare_mod!`, valid for the process.
        let manifest = unsafe { stale::enshwick_mod_query().as_ref() }.expect("a manifest");
        // SAFETY: the id points at a `'static` string literal of `declare_mod!`.
        assert_eq!(unsafe { manifest.id.as_str() }, Some("you.stale"));
        assert_eq!(manifest.export_count, 0);
        // SAFETY: the schema text lives as long as the manifest.
        assert_eq!(unsafe { manifest.points.as_str() }.unwrap_or_default(), "");
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of `declare_mod!(Stale)`.
        let status = unsafe {
            loader.start(
                stale::enshwick_mod_query,
                stale::enshwick_mod_init,
                stale::enshwick_mod_ready,
            )
        };
        assert_eq!(status, Status::REFUSED);
        assert_eq!(
            loader.faults(),
            [format!(
                "mods.off.section.newer\tformat=99\treads={}",
                enshwick::schema_section::FORMAT
            )]
        );
        assert!(stale::Stale::instance().is_none());
    }
}
