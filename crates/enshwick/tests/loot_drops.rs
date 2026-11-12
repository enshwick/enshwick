//! Loot from destroyed things links only where the build names the single
//! roll's output slot.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::loot::{Drop, Kinds, Source};
    use enshwick::test::{FakeHost, facts};
    use enshwick::{Load, Mod, Off};

    const ROLL: u64 = 0x1_4010_0000;
    const ROLL_ONE: u64 = 0x1_4020_0000;

    struct Drops {
        destroyed: Result<(), String>,
    }

    impl Mod for Drops {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let destroyed = load
                .add_loot(Drop::new(
                    enshwick::ItemId::new(7),
                    0.5,
                    Source::kinds(Kinds::DESTROYED),
                ))
                .map_err(|off| off.to_string());
            Ok(Self { destroyed })
        }
    }

    enshwick::test::declare_mod!(
        Drops,
        r#"
        id = "you.drops"
        side = "server"
        "#
    );

    fn build() -> FakeHost {
        rolls(ROLL, ROLL_ONE)
    }

    /// The pieces the loot links read, with the container roll at `roll`
    /// and the single roll at `roll_one`.
    fn rolls(roll: u64, roll_one: u64) -> FakeHost {
        FakeHost::new()
            .symbol("ecs.type_index", 0x1_4064_5cc0)
            .symbol("ecs.component_read_raw", 0x1_4064_6b00)
            .symbol("ecs.slot_root", 0x1_4093_5420)
            .symbol("inventory.give", 0x1_4030_0000)
            .symbol("loot.roll", roll)
            .symbol("loot.roll_one", roll_one)
            .constant("ecs.context_entity_world", 0x30)
            .constant("ecs.registry_type_map", 0x30)
            .constant("ecs.registry_type_map_fallback", 0x88)
            .constant("loot.txn_context", 0x10)
            .constant("loot.txn_entity", 0x18)
            .constant("loot.request_node", 0x08)
            .constant("loot.request_labels", 0x0c)
            .world()
            .loot()
    }

    /// The build's loot slot: `size` bytes, its item and count side by side.
    fn slot(host: FakeHost, size: u32) -> FakeHost {
        host.type_size(facts::LOOT_SLOT.id(), size)
            .constant(facts::LOOT_SLOT_ITEM.id(), 0)
            .constant(facts::LOOT_SLOT_COUNT.id(), 4)
    }

    fn linked(host: FakeHost) -> (Result<(), String>, Vec<u64>) {
        let loader = host.install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Drops)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let destroyed = Drops::instance().unwrap().destroyed.clone();
        let targets = loader.hooks().iter().map(|hook| hook.target).collect();
        (destroyed, targets)
    }

    #[test]
    fn the_single_roll_links_where_the_build_names_its_slot() {
        let (destroyed, targets) = linked(slot(build(), 16));
        assert_eq!(destroyed, Ok(()));
        assert_eq!(targets, [ROLL, ROLL_ONE]);
    }

    #[test]
    fn a_slot_too_small_for_its_fields_keeps_the_single_roll_vanilla() {
        let (destroyed, targets) = linked(slot(build(), 5));
        assert!(destroyed.unwrap_err().contains(facts::LOOT_SLOT.id()));
        assert_eq!(targets, [ROLL]);
    }

    #[test]
    fn a_build_without_the_slot_keeps_the_single_roll_vanilla() {
        let (destroyed, targets) = linked(build());
        assert!(destroyed.unwrap_err().contains(facts::LOOT_SLOT.id()));
        assert_eq!(targets, [ROLL]);
    }

    /// A data pack's drops, which go when the pack turns off.
    mod pack {
        use enshwick::abi::Status;
        use enshwick::packs::{Pack, PackFiles, forget_loot};
        use enshwick::test::Installed;
        use enshwick::{Load, Mod, Off};

        type RollFn = unsafe extern "system" fn(
            res: *mut u8,
            out: *mut u8,
            generation: *mut u8,
            labels: *mut u8,
            request: *const u8,
        ) -> *mut u8;

        /// The game's single roll here: it rolls nothing.
        unsafe extern "system" fn rolls_nothing(
            res: *mut u8,
            _out: *mut u8,
            _generation: *mut u8,
            _labels: *mut u8,
            _request: *const u8,
        ) -> *mut u8 {
            // SAFETY: `roll_one` passes its own status byte.
            unsafe { res.write(1) };
            res
        }

        /// The address the build names for the single roll: `rolls_nothing`.
        fn roll_one_target() -> u64 {
            (rolls_nothing as RollFn as *const ()).addr() as u64
        }

        struct Relics;

        impl Mod for Relics {
            fn init(load: &mut Load<'_>) -> Result<Self, Off> {
                let files = PackFiles::new().with(
                    "loot.toml",
                    "[[drop]]\nitems = [\"you.lockers:locker\"]\nchance = 1.0\nfrom = \"destroyed\"\n",
                );
                load.add_pack(&Pack::parse("you.relics", &files)?)?;
                Ok(Self)
            }
        }

        enshwick::test::declare_mod!(
            Relics,
            r#"
            id = "you.relics"
            side = "server"
            "#
        );

        fn start() -> Installed {
            let roll_one = roll_one_target();
            let loader = super::slot(super::rolls(super::ROLL, roll_one), 16)
                .exports("you.lockers", &["content:locker"])
                .install();
            // SAFETY: both functions are the exports of this module's `declare_mod!(Relics)`.
            let status =
                unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
            assert_eq!(status, Status::OK, "{:?}", loader.logs());
            loader
        }

        /// The item id and count a single roll gives, through the mods'
        /// link, as the game would roll it; `None` when it stays empty.
        fn roll_one(loader: &Installed) -> Option<(u32, u16)> {
            let roll_one = roll_one_target();
            let linked = loader.hooks().into_iter().find(|h| h.target == roll_one)?;
            // SAFETY: the detour linked at the single roll has its signature.
            let entry: RollFn =
                unsafe { core::mem::transmute(usize::try_from(linked.detour).ok()?) };
            let (mut res, mut out) = (1u8, [0u8; 16]);
            // SAFETY: the status byte and a 16-byte slot, the layout `start`
            // names, live across the call; the roll reads no other argument.
            unsafe {
                entry(
                    &raw mut res,
                    out.as_mut_ptr(),
                    core::ptr::null_mut(),
                    core::ptr::null_mut(),
                    core::ptr::null(),
                )
            };
            let item = u32::from_le_bytes(out[..4].try_into().ok()?);
            let count = u16::from_le_bytes(out[4..6].try_into().ok()?);
            (res == 0).then_some((item, count))
        }

        #[test]
        fn a_pack_s_drop_fills_a_single_roll_until_the_pack_turns_off() {
            let loader = start();
            let locker = enshwick::content::Guid::named("you.lockers:locker").fnv1a();
            assert_eq!(roll_one(&loader), Some((locker, 1)));
            assert_eq!(forget_loot("you.relics"), Ok(()));
            assert_eq!(roll_one(&loader), None);
        }
    }

    /// Affixes on random loot hear the container rolls.
    #[cfg(feature = "unstable")]
    mod affixes {
        use enshwick::abi::Status;
        use enshwick::affixes::AffixLoot;
        use enshwick::loot::Source;
        use enshwick::{Load, Mod, Off};

        struct Rolled;

        impl Mod for Rolled {
            fn init(load: &mut Load<'_>) -> Result<Self, Off> {
                load.add_loot_affixes(AffixLoot::new(Source::bosses(), 0.5))?;
                Ok(Self)
            }
        }

        enshwick::test::declare_mod!(
            Rolled,
            r#"
            id = "you.rolled"
            side = "server"
            "#
        );

        #[test]
        fn loot_affixes_link_the_container_roll() {
            let loader = super::build().inventory_requests().install();
            // SAFETY: both functions are the exports of this module's `declare_mod!(Rolled)`.
            let status =
                unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
            assert_eq!(status, Status::OK, "{:?}", loader.logs());
            let targets: Vec<u64> = loader.hooks().iter().map(|h| h.target).collect();
            assert!(targets.contains(&super::ROLL), "{targets:?}");
        }
    }
}
