//! The flags `declare_mod!` writes into a mod's manifest from its table.

#[cfg(test)]
mod tests {
    use enshwick::abi::ManifestFlags;

    mod tool {
        #[expect(
            dead_code,
            reason = "the test reads its manifest alone; the mod never starts"
        )]
        pub(super) struct Inspector;

        impl enshwick::Mod for Inspector {
            fn init(_load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
                Ok(Self)
            }
        }

        enshwick::test::declare_mod!(
            Inspector,
            r#"
            id = "you.inspector"
            side = "both"
            reload = true
            developer = true
            "#
        );
    }

    mod plain {
        #[expect(
            dead_code,
            reason = "the test reads its manifest alone; the mod never starts"
        )]
        pub(super) struct Lanterns;

        impl enshwick::Mod for Lanterns {
            fn init(_load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
                Ok(Self)
            }
        }

        enshwick::test::declare_mod!(
            Lanterns,
            r#"
            id = "you.lanterns"
            side = "both"
            join = true
            "#
        );
    }

    #[test]
    fn a_developer_tool_carries_its_flag_and_other_mods_do_not() {
        // SAFETY: `enshwick_mod_query` returns the `'static` manifest of
        // `declare_mod!`, which no thread writes.
        let tool = unsafe { &*tool::enshwick_mod_query() };
        assert!(tool.flags.contains(ManifestFlags::DEVELOPER));
        assert!(tool.flags.contains(ManifestFlags::RELOAD));
        assert!(!tool.flags.contains(ManifestFlags::JOIN));
        // SAFETY: `enshwick_mod_query` returns the `'static` manifest of
        // `declare_mod!`, which no thread writes.
        let plain = unsafe { &*plain::enshwick_mod_query() };
        assert!(plain.flags.contains(ManifestFlags::JOIN));
        assert!(!plain.flags.contains(ManifestFlags::DEVELOPER));
    }
}
