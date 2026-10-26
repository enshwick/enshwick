//! An item with its own icon: the icon's UI image built from the vanilla
//! one, its pixels recoloured and served, and the item naming it only on a
//! client that shows served icons.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::content::icons::{Icon, Pixels, Tint, formats};
    use enshwick::content::items::NewItem;
    use enshwick::content::{Guid, Id, types};
    use enshwick::test::FakeHost;
    use enshwick::{Load, Mod, Off};

    const STAFF_ID: Id = Id::new("you.staffs:staff");
    const STAFF: Guid = STAFF_ID.guid();
    const VANILLA: Guid = Guid::parse("505ccfbc-440d-4d63-a3d6-d689d3d55e78");
    const ICON_ID: Id = Id::new("you.staffs:staff.icon");
    const ICON: Guid = ICON_ID.guid();
    const ITEM: u32 = 1;
    const IMAGE: u32 = 3;
    /// The vanilla icon's pixels: one BC1 block, all white, 4 by 4.
    const WHITE: [u8; 8] = [0xff, 0xff, 0x00, 0x00, 0, 0, 0, 0];
    const WHITE_HASH: [u32; 4] = [8, 1, 2, 3];

    struct Staffs;

    impl Mod for Staffs {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.staffs", side: BOTH)
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

    enshwick::declare_mod!(Staffs);

    /// The fake's reflection for an item edit, and with `layout` the UI
    /// image layout a client's table has.
    fn host(layout: bool) -> FakeHost {
        let mut host = FakeHost::new()
            .type_hash(types::ITEM_INFO.id(), ITEM)
            .type_hash(types::ITEM_REGISTRY.id(), 2)
            .type_hash(types::UI_IMAGE.id(), IMAGE)
            .type_size("item_stat", 0x28);
        for (id, at) in [
            ("item_info.id", 0),
            ("item_info.object", 4),
            ("item_info.name", 0x48),
            ("item_info.icon_image", 0x58),
            ("item_info.description", 0x68),
            ("item_info.flags", 0x78),
            ("item_info.pid_entity", 0x80),
            ("item_info.ui_values", 0x90),
        ] {
            host = host.constant(id, at);
        }
        for (id, at) in [
            ("item_stat.label", 0),
            ("item_stat.format", 4),
            ("item_stat.value", 8),
            ("item_stat.signed", 0x24),
        ] {
            host = host.constant(id, at);
        }
        host = host
            .constant("item_registry.refs", 0)
            .constant("item_registry.names", 0x38);
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
        // SAFETY: both functions are the exports of this file's `declare_mod!(Staffs)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let image = loader
            .build_resource(&ICON.key_by_hash(IMAGE), &vanilla_image())
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
            .build_resource(&STAFF.key_by_hash(ITEM), &[0u8; 0x98])
            .unwrap();
        assert_eq!(&item[0x58..0x68], &ICON.0);
    }
}
