//! An item with its own icon on a build without icons, as the dedicated
//! server: no icon served, the vanilla one kept.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::content::icons::{Icon, Pixels, Tint};
    use enshwick::content::items::{ItemLayout, NewItem};
    use enshwick::content::{EngineFacts, Guid, Id, types};
    use enshwick::test::FakeHost;
    use enshwick::{Load, Mod, Off};

    const STAFF_ID: Id = Id::new("you.staffs:staff");
    const STAFF: Guid = STAFF_ID.guid();
    const VANILLA: Guid = Guid::parse("505ccfbc-440d-4d63-a3d6-d689d3d55e78");
    const ICON_ID: Id = Id::new("you.staffs:staff.icon");
    const ICON: Guid = ICON_ID.guid();
    const IMAGE: u32 = 3;

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

    /// The fake's reflection for an item edit, on a build whose table has no
    /// UI image layout.
    fn host() -> FakeHost {
        FakeHost::new()
            .items()
            .type_hash(types::UI_TEXTURE_RESOURCE.id(), IMAGE)
    }

    #[test]
    fn without_icons_the_item_keeps_the_vanilla_one() {
        let host = host();
        let layout = ItemLayout::resolve(&host).unwrap();
        let item = host.hash(types::ITEM_INFO.id()).unwrap();
        let size = host.size(types::ITEM_INFO.id()).unwrap();
        let loader = host.install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Staffs)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert!(
            !loader
                .resources()
                .iter()
                .any(|r| r.key == ICON.key_by_hash(IMAGE)),
            "a build without the layout serves no icon"
        );
        let item = loader
            .build_resource(&STAFF.key_by_hash(item), &vec![0u8; size])
            .unwrap();
        let icon = layout.icon_image;
        assert_eq!(&item[icon..icon + 16], &[0; 16]);
    }
}
