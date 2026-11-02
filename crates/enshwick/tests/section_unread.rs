//! A mod whose binary's declarations do not read: the loader learns the mod
//! and why it stays off, in the loader's own texts.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;

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
            "#
        );

        // A head of a format no enshwick writes, as a dependency built with
        // another enshwick would leave in the binary.
        enshwick::__schema_entry!("schema = 99\nmod = \"you.other\"\n");
    }

    #[test]
    fn a_mod_whose_declarations_do_not_read_is_named_and_refused_with_the_reason() {
        // SAFETY: `enshwick_mod_query` returns null or the manifest of
        // `declare_mod!`, valid until its shutdown, which no test runs.
        let manifest = unsafe { stale::enshwick_mod_query().as_ref() }.expect("a manifest");
        // SAFETY: the id points at a `'static` string literal of `declare_mod!`.
        assert_eq!(unsafe { manifest.id.as_str() }, Some("you.stale"));
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of `declare_mod!(Stale)`.
        let status = unsafe { loader.start(stale::enshwick_mod_init, stale::enshwick_mod_ready) };
        assert_eq!(status, Status::REFUSED);
        assert_eq!(
            loader.faults(),
            [format!(
                "mods.off.section.format\tformat=99\treads={}",
                enshwick::schema_section::FORMAT
            )]
        );
        assert!(stale::Stale::instance().is_none());
    }
}
