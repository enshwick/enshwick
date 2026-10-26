//! An item with its own icon on a build without icons, as the dedicated
//! server: no icon served, the vanilla one kept.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::content::icons::{Icon, Pixels, Tint};
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

    /// The fake's reflection for an item edit.
    fn host() -> FakeHost {
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
        host
    }

    #[test]
    fn without_icons_the_item_keeps_the_vanilla_one() {
        let loader = host().install();
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
            .build_resource(&STAFF.key_by_hash(ITEM), &[0u8; 0x98])
            .unwrap();
        assert_eq!(&item[0x58..0x68], &[0; 16]);
    }
}
