//! An item with its own icon: the icon's UI image built from the vanilla
//! one, its pixels recoloured and served, and the item naming it only on a
//! client that shows served icons.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::content::icons::{Icon, ImageLayout, Pixels, Tint, formats};
    use enshwick::content::items::{ItemLayout, NewItem};
    use enshwick::content::{EngineFacts, Guid, Id, types};
    use enshwick::test::FakeHost;
    use enshwick::{Load, Mod, Off};

    const STAFF_ID: Id = Id::new("you.staffs:staff");
    const STAFF: Guid = STAFF_ID.guid();
    const VANILLA: Guid = Guid::parse("505ccfbc-440d-4d63-a3d6-d689d3d55e78");
    const ICON_ID: Id = Id::new("you.staffs:staff.icon");
    const ICON: Guid = ICON_ID.guid();
    /// The vanilla icon's pixels: one BC1 block, all white, 4 by 4.
    const WHITE: [u8; 8] = [0xff, 0xff, 0x00, 0x00, 0, 0, 0, 0];
    const WHITE_HASH: [u32; 4] = [8, 1, 2, 3];

    struct Staffs;

    impl Mod for Staffs {
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

    enshwick::test::declare_mod!(
        Staffs,
        r#"
        id = "you.staffs"
        side = "both"
        "#
    );

    /// The vanilla icon's UI image as `host` lays it out: its content, 4 by
    /// 4 BC1, one level.
    fn vanilla_image(host: &FakeHost) -> Vec<u8> {
        let layout = ImageLayout::resolve(host).unwrap();
        let mut image = vec![0u8; host.size(types::UI_TEXTURE_RESOURCE.id()).unwrap()];
        for (i, word) in WHITE_HASH.iter().enumerate() {
            let at = layout.content + i * 4;
            image[at..at + 4].copy_from_slice(&word.to_le_bytes());
        }
        for at in [layout.width, layout.height] {
            image[at..at + 4].copy_from_slice(&4u32.to_le_bytes());
        }
        image[layout.flags] = 1;
        image[layout.format] = formats::BC1;
        image[layout.levels] = 1;
        image
    }

    #[test]
    fn the_icon_is_recoloured_and_named_once_icons_show() {
        let host = FakeHost::new().items().ui_images();
        let images = ImageLayout::resolve(&host).unwrap();
        let items = ItemLayout::resolve(&host).unwrap();
        let item_hash = host.hash(types::ITEM_INFO.id()).unwrap();
        let image_hash = host.hash(types::UI_TEXTURE_RESOURCE.id()).unwrap();
        let item_size = host.size(types::ITEM_INFO.id()).unwrap();
        let vanilla = vanilla_image(&host);
        let loader = host
            .archive(WHITE_HASH, WHITE.to_vec())
            .icons_ready()
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Staffs)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let image = loader
            .build_resource(&ICON.key_by_hash(image_hash), &vanilla)
            .unwrap();
        assert_eq!(
            (image[images.format], image[images.levels]),
            (formats::RGBA8, 1)
        );
        let served = loader.served_content();
        assert_eq!(served.len(), 1);
        let (hash, pixels) = &served[0];
        assert_eq!(
            image[images.content..images.content + 4],
            hash[0].to_le_bytes()
        );
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
            .build_resource(&STAFF.key_by_hash(item_hash), &vec![0u8; item_size])
            .unwrap();
        let icon = items.icon_image;
        assert_eq!(&item[icon..icon + 16], &ICON.0);
    }
}
