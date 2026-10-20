//! An item with its own icon on a build without icons, as the dedicated
//! server: no icon served, the vanilla one kept.

#[cfg(test)]
mod tests {
    use enshwick_loader_api::abi::Status;
    use enshwick_loader_api::content::icons::{Icon, Pixels, Tint};
    use enshwick_loader_api::content::items::NewItem;
    use enshwick_loader_api::content::{Guid, types};
    use enshwick_loader_api::test::FakeHost;
    use enshwick_loader_api::{Load, Mod, Off};

    const STAFF: Guid = Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31");
    const VANILLA: Guid = Guid::parse("505ccfbc-440d-4d63-a3d6-d689d3d55e78");
    const ICON: Guid = Guid::parse("3c1f2a9e-7d4b-4e8a-9f60-1a2b3c4d5e6f");

    struct Staffs;

    impl Mod for Staffs {
        fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
            enshwick_loader_api::manifest!(id: "staffs", side: BOTH)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.add_item(NewItem {
                icon: Some(Icon {
                    guid: ICON,
                    like: VANILLA,
                    pixels: Pixels::Tint(Tint {
                        colour: [0, 0, 255],
                        amount: 100,
                    }),
                }),
                ..NewItem::new(STAFF, VANILLA, "Staff")
            })?;
            Ok(Self)
        }
    }

    enshwick_loader_api::declare_mod!(Staffs);

    /// The fake's reflection for an item edit.
    fn host() -> FakeHost {
        let mut host = FakeHost::new().type_size("keen::ImpactUiValue", 0x28);
        for (field, at) in [
            ("itemId", 0),
            ("objectId", 4),
            ("name", 0x48),
            ("iconImage", 0x58),
            ("description", 0x68),
            ("flags", 0x78),
            ("pidEntity", 0x80),
            ("uiValues", 0x90),
        ] {
            host = host.field_offset("keen::ItemInfo", field, at);
        }
        for (field, at) in [
            ("locaId", 0),
            ("valueFormat", 4),
            ("value", 8),
            ("isSigned", 0x24),
        ] {
            host = host.field_offset("keen::ImpactUiValue", field, at);
        }
        host = host
            .field_offset("keen::ItemRegistryResource", "itemRefs", 0)
            .field_offset("keen::ItemRegistryResource", "dbgNames", 0x38);
        host
    }

    #[test]
    fn without_icons_the_item_keeps_the_vanilla_one() {
        let loader = host().install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert!(
            !loader
                .resources()
                .iter()
                .any(|r| r.key == ICON.key(types::UI_IMAGE)),
            "a build without the layout serves no icon"
        );
        let item = loader
            .build_resource(&STAFF.key(types::ITEM_INFO), &[0u8; 0x98])
            .unwrap();
        assert_eq!(&item[0x58..0x68], &[0; 16]);
    }
}
