//! Checks against the real game binaries. They never enter the repository, so
//! these run only where `ENSH_SERVER_EXE` and `ENSH_CLIENT_EXE` point at local
//! copies, and pass trivially elsewhere.

#[cfg(test)]
mod tests {
    use keen_reflect::{Image, Layout, Reflection};

    fn check_common(reflection: &Reflection<'_>) {
        let shape = "keen::ecs::TriggerShape";
        assert_eq!(reflection.field_offset(shape, "rangeX"), Ok(4));
        assert_eq!(reflection.field_offset(shape, "rangeY"), Ok(8));
        assert_eq!(reflection.field_offset(shape, "rangeZ"), Ok(0xc));
        assert!(reflection.type_info("keen::ecs::BaseIdComponent").is_ok());
        assert!(reflection.type_info("keen::BalancingTable").is_ok());
        let upgrade = reflection.system("upgrade_building_range").unwrap();
        assert_ne!(upgrade.entry(), 0);
        assert!(upgrade.writes().contains(&"TriggerShape"), "{upgrade:?}");
        assert!(reflection.system("add_buildzones_server").is_ok());
    }

    fn with_binary(var: &str, check: impl FnOnce(&Image<'_>, &Reflection<'_>)) {
        let Ok(path) = std::env::var(var) else {
            return;
        };
        let bytes = std::fs::read(path).unwrap();
        let image = Image::parse(&bytes, Layout::File, None).unwrap();
        let reflection = Reflection::scan(&image);
        check(&image, &reflection);
    }

    #[test]
    fn server_binary_resolves() {
        with_binary("ENSH_SERVER_EXE", |image, reflection| {
            check_common(reflection);
            assert!(image.fingerprint().is_some());
        });
    }

    #[test]
    fn client_binary_resolves() {
        with_binary("ENSH_CLIENT_EXE", |_, reflection| {
            check_common(reflection);
            assert_ne!(
                reflection.system("add_buildzones_client").unwrap().entry(),
                0
            );
        });
    }
}
