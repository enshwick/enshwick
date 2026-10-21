//! An item with its own icon: the icon's UI image built from the vanilla
//! one, its pixels recoloured and served, and the item naming it only on a
//! client that shows served icons.

#[cfg(test)]
mod tests {
    use enshwick_loader_api::abi::Status;
    use enshwick_loader_api::content::icons::{Icon, Pixels, Tint, UI_IMAGE, formats};
    use enshwick_loader_api::content::items::NewItem;
    use enshwick_loader_api::content::{Guid, Id, types};
    use enshwick_loader_api::test::FakeHost;
    use enshwick_loader_api::{Load, Mod, Off};

    const STAFF_ID: Id = Id::new("staffs:staff");
    const STAFF: Guid = STAFF_ID.guid();
    const VANILLA: Guid = Guid::parse("505ccfbc-440d-4d63-a3d6-d689d3d55e78");
    const ICON_ID: Id = Id::new("staffs:staff.icon");
    const ICON: Guid = ICON_ID.guid();
    /// The vanilla icon's pixels: one BC1 block, all white, 4 by 4.
    const WHITE: [u8; 8] = [0xff, 0xff, 0x00, 0x00, 0, 0, 0, 0];
    const WHITE_HASH: [u32; 4] = [8, 1, 2, 3];

    struct Staffs;

    impl Mod for Staffs {
        fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
            enshwick_loader_api::manifest!(id: "staffs", side: BOTH)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.add_item(NewItem {
                icon: Some(Icon {
                    id: ICON_ID,
                    like: VANILLA,
                    pixels: Pixels::Tint(Tint {
                        colour: [0, 0, 255],
                        amount: 100,
                    }),
                }),
                ..NewItem::new(STAFF_ID, VANILLA)
            })?;
            Ok(Self)
        }
    }

    enshwick_loader_api::declare_mod!(Staffs);

    /// The fake's reflection for an item edit, and with `layout` the UI
    /// image layout a client's table has.
    fn host(layout: bool) -> FakeHost {
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
        if layout {
            for (name, at) in [
                ("content", 0),
                ("width", 0x28),
                ("height", 0x2c),
                ("flags", 0x30),
                ("format", 0x31),
                ("levels", 0x32),
            ] {
                host = host.constant(&format!("ui_image.{name}"), at);
            }
        }
        host
    }

    /// The vanilla icon's UI image: its content, 4 by 4 BC1, one level.
    fn vanilla_image() -> Vec<u8> {
        let mut image = vec![0u8; 0x40];
        for (i, word) in WHITE_HASH.iter().enumerate() {
            image[i * 4..i * 4 + 4].copy_from_slice(&word.to_le_bytes());
        }
        image[0x28..0x2c].copy_from_slice(&4u32.to_le_bytes());
        image[0x2c..0x30].copy_from_slice(&4u32.to_le_bytes());
        image[0x30..0x33].copy_from_slice(&[1, formats::BC1, 1]);
        image
    }

    #[test]
    fn the_icon_is_recoloured_and_named_once_icons_show() {
        let loader = host(true)
            .archive(WHITE_HASH, WHITE.to_vec())
            .icons_ready()
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let image = loader
            .build_resource(&ICON.key(UI_IMAGE), &vanilla_image())
            .unwrap();
        assert_eq!((image[0x31], image[0x32]), (formats::RGBA8, 1));
        let served = loader.served_content();
        assert_eq!(served.len(), 1);
        let (hash, pixels) = &served[0];
        assert_eq!(image[..4], hash[0].to_le_bytes());
        assert_eq!(pixels.len(), 4 * 4 * 4);
        assert!(
            pixels
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == [0, 0, 255, 255]),
            "white turned blue"
        );
        let item = loader
            .build_resource(&STAFF.key(types::ITEM_INFO), &[0u8; 0x98])
            .unwrap();
        assert_eq!(&item[0x58..0x68], &ICON.0);
    }
}
