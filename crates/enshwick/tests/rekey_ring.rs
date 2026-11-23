//! Re-keys of several mods apply at once: a ring of three items rotates, a
//! chain gives each item the data its new item had, and a re-key to itself
//! or to another type is refused.

#[cfg(test)]
mod tests {
    use enshwick::abi::{ModHandle, ResourceKey, Status};
    use enshwick::content::{Guid, types};
    use enshwick::resources::{Registry, Value};
    use enshwick::test::{FakeHost, Installed, LOOT_ITEM, OTHER_ITEM, SPARE_ITEM};
    use enshwick::{Load, Mod, Off};

    struct Ring;

    impl Mod for Ring {
        fn init(_load: &mut Load<'_>) -> Result<Self, Off> {
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Ring,
        r#"
        id = "you.ring"
        side = "both"
        "#
    );

    /// Two mods re-key `SPARE_ITEM` to `LOOT_ITEM` and `LOOT_ITEM` to
    /// `OTHER_ITEM`; with `ring`, a third closes the ring with `OTHER_ITEM`
    /// to `SPARE_ITEM`; then this file's mod, which re-keys nothing.
    fn start(ring: bool) -> Installed {
        let loader = FakeHost::new().items().removal().install();
        let pack = |me, call: fn(&mut Load<'_>) -> Result<(), Off>| {
            // SAFETY: the fake host's table lives for the process.
            let status = unsafe {
                enshwick::__private::init_pack(
                    loader.api(),
                    ModHandle(me),
                    enshwick::abi::Side::SERVER,
                    call,
                )
            };
            assert_eq!(status, Status::OK, "{:?}", loader.logs());
        };
        pack(6, |load| load.rekey_item(SPARE_ITEM, LOOT_ITEM));
        pack(7, |load| load.rekey_item(LOOT_ITEM, OTHER_ITEM));
        if ring {
            pack(8, |load| load.rekey_item(OTHER_ITEM, SPARE_ITEM));
        }
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Ring)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    /// The vanilla item `item` as the game's first build serves it.
    fn built(loader: &Installed, item: Guid) -> Value {
        let host = enshwick::host().unwrap();
        let key = types::ITEM_INFO.key(item).unwrap();
        let built = loader
            .build_resource(&key, &host.game_resource(key).unwrap())
            .unwrap();
        Registry::game()
            .unwrap()
            .decode("demo::ItemInfo", &built)
            .unwrap()
    }

    /// The category the fake's vanilla `item` holds.
    fn category(item: Guid) -> Value {
        Value::Uint(u64::from(item.0[15]))
    }

    /// Asserts that each item of `rotation`, `(old, new)`, serves under its
    /// own GUID with the vanilla data of `new`.
    fn takes(loader: &Installed, rotation: &[(Guid, Guid)]) {
        for &(old, new) in rotation {
            let item = built(loader, old);
            assert_eq!(item.field("objectId"), Some(&Value::Guid(old)));
            assert_eq!(
                item.field("category"),
                Some(&category(new)),
                "{old} takes the data of {new}"
            );
        }
    }

    #[test]
    fn three_items_rekeyed_in_a_ring_by_three_mods_rotate() {
        let loader = start(true);
        takes(
            &loader,
            &[
                (SPARE_ITEM, LOOT_ITEM),
                (LOOT_ITEM, OTHER_ITEM),
                (OTHER_ITEM, SPARE_ITEM),
            ],
        );
        let logs = loader.logs();
        assert!(
            !logs.iter().any(|(_, line)| line.contains("refused")),
            "a rotation is no failure: {logs:?}"
        );
    }

    #[test]
    fn the_ring_rotates_whichever_item_the_game_builds_first() {
        let loader = start(true);
        takes(
            &loader,
            &[
                (OTHER_ITEM, SPARE_ITEM),
                (LOOT_ITEM, OTHER_ITEM),
                (SPARE_ITEM, LOOT_ITEM),
            ],
        );
    }

    #[test]
    fn a_chain_of_rekeys_gives_each_item_the_data_its_new_item_had() {
        let loader = start(false);
        takes(&loader, &[(SPARE_ITEM, LOOT_ITEM), (LOOT_ITEM, OTHER_ITEM)]);
    }

    /// A patch that builds nothing.
    unsafe extern "system" fn unbuilt(
        _base: *const u8,
        _base_len: u32,
        _user: *mut core::ffi::c_void,
        _out: *mut *mut u8,
        _out_len: *mut u32,
    ) -> Status {
        Status::REFUSED
    }

    #[test]
    fn a_rekey_to_itself_or_to_another_type_is_a_bad_argument() {
        let loader = FakeHost::new().items().removal().install();
        let mut answers = Vec::new();
        // SAFETY: the fake host's table lives for the process.
        let status = unsafe {
            enshwick::__private::init_pack(
                loader.api(),
                ModHandle(6),
                enshwick::abi::Side::SERVER,
                |_| {
                    let api = enshwick::host().ok_or_else(|| Off::new("no host"))?.api();
                    let key = types::ITEM_INFO.key(SPARE_ITEM)?;
                    let other_type = ResourceKey {
                        type_hash: key.type_hash ^ 1,
                        ..key
                    };
                    for from in [key, other_type, types::ITEM_INFO.key(LOOT_ITEM)?] {
                        answers.push((api.rekey_resource)(
                            ModHandle(6),
                            &raw const key,
                            &raw const from,
                            Some(unbuilt),
                            core::ptr::null_mut(),
                        ));
                    }
                    Ok(())
                },
            )
        };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(answers, [Status::BAD_ARGS, Status::BAD_ARGS, Status::OK]);
    }
}
