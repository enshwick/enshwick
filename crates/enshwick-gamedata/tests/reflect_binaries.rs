//! Checks against the real game binaries. They never enter the repository, so
//! these need `ENSH_SERVER_EXE` and `ENSH_CLIENT_EXE` to point at local
//! copies and are ignored by default.

#[cfg(test)]
mod tests {
    use enshwick_gamedata::reflect::{Image, Layout, Reflection};

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
        let events: Vec<&str> = reflection
            .derived_from("keen::ecs::Event")
            .map(enshwick_gamedata::reflect::TypeInfo::name)
            .collect();
        for event in [
            "keen::ecs::HitEvent",
            "keen::ecs::EntityDiedEvent",
            "keen::ecs::RevivedPlayerEvent",
        ] {
            assert!(events.contains(&event), "{event}");
        }
        assert!(
            events.iter().all(|name| !name.contains('<')),
            "only structs derive: {events:?}"
        );
        let slot = "keen::EquipmentSlotType";
        assert_eq!(reflection.enum_value(slot, "Invalid"), Ok(0));
        assert_eq!(
            reflection.enum_value(slot, "BlueprintMaterial_Roof"),
            Ok(11)
        );
        assert!(reflection.type_info(slot).unwrap().enum_values().len() > 12);
    }

    fn with_binary(var: &str, check: impl FnOnce(&Image<'_>, &Reflection<'_>)) {
        let path = std::env::var(var).unwrap_or_else(|_| panic!("{var} names the game executable"));
        let bytes = std::fs::read(path).unwrap();
        let image = Image::parse(&bytes, Layout::File, None).unwrap();
        let reflection = Reflection::scan(&image);
        check(&image, &reflection);
    }

    #[test]
    #[ignore = "needs ENSH_SERVER_EXE"]
    fn server_binary_resolves() {
        with_binary("ENSH_SERVER_EXE", |image, reflection| {
            check_common(reflection);
            assert!(image.fingerprint().is_some());
        });
    }

    #[test]
    #[ignore = "needs ENSH_CLIENT_EXE"]
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
