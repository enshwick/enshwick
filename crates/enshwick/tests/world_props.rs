//! Props in transactions: placing, removing, the state a player would lose, and undo.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::Entity;
    use enshwick::WorldError;
    use enshwick::abi::{Header, PlayerInfo, Status, TerrainMaterialInfo};
    use enshwick::world::{HeldState, TransactionStatus, World, WorldTransform};
    use enshwick::{ItemId, Load, Mod, Off};

    static WORLD: OnceLock<World> = OnceLock::new();

    struct Decorator;

    impl Mod for Decorator {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.decorator", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = WORLD.set(load.world()?);
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Decorator);

    const CHEST: ItemId = ItemId::new(30);
    const PEBBLE: ItemId = ItemId::new(31);
    const OLD_CHEST: Entity = Entity(40);

    fn started() -> (enshwick::test::Installed, World) {
        started_with(&[])
    }

    /// The fake started, its props of `triggers` logic triggers.
    fn started_with(triggers: &[ItemId]) -> (enshwick::test::Installed, World) {
        let host = triggers
            .iter()
            .fold(enshwick::test::FakeHost::new(), |host, item| {
                host.item_triggers(*item)
            });
        started_from(host)
    }

    /// The fake started from `host`, with the items and the old chest.
    fn started_from(host: enshwick::test::FakeHost) -> (enshwick::test::Installed, World) {
        let loader = host
            .type_hash("current_transform", 1)
            .type_hash("used_item", 2)
            .item(CHEST, 1, 0)
            .item_places(CHEST, [0x3a; 16])
            .item(PEBBLE, 99, 0)
            .prop(OLD_CHEST, CHEST, [50.0, 40.0, 50.0])
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Decorator)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        loader.set_terrain(&[TerrainMaterialInfo {
            header: Header::of::<TerrainMaterialInfo>(),
            id: 9,
            flags: 0,
            hardness: 3,
            reserved: 0,
            item: ItemId::new(77),
            health: 500,
        }]);
        (loader, *WORLD.get().unwrap())
    }

    fn here() -> WorldTransform {
        WorldTransform::at([100.0, 40.0, 100.0])
    }

    #[test]
    fn a_placed_prop_is_listed_and_its_undo_takes_it_away() {
        let (loader, world) = started();
        let id = world
            .transaction()
            .place_prop(CHEST, here())
            .commit()
            .unwrap();
        assert!(world.transaction_entities(id).is_empty(), "not placed yet");
        loader.run_transactions();
        assert_eq!(
            world.transaction_status(id),
            Some(TransactionStatus::Applied)
        );
        let placed = world.transaction_entities(id);
        assert_eq!(placed.len(), 1);
        assert!(
            loader
                .props()
                .iter()
                .any(|(entity, item, _)| *entity == placed[0] && *item == CHEST)
        );

        world.undo(id).unwrap();
        loader.run_transactions();
        assert_eq!(
            world.transaction_status(id),
            Some(TransactionStatus::Undone)
        );
        assert!(
            !loader
                .props()
                .iter()
                .any(|(entity, ..)| *entity == placed[0])
        );
    }

    #[test]
    fn an_item_that_places_nothing_is_refused() {
        let (loader, world) = started();
        assert_eq!(
            world.transaction().place_prop(PEBBLE, here()).commit(),
            Err(WorldError::Refused)
        );
        assert!(loader.transactions().is_empty());
    }

    #[test]
    fn a_prop_holding_each_kind_of_state_is_not_removed() {
        for kind in [
            HeldState::Inventory,
            HeldState::Growth,
            HeldState::FactoryStation,
            HeldState::NpcBed,
            HeldState::StoreAssociatedNpcIds,
            HeldState::CustomPlayerString,
            HeldState::Fuel,
            HeldState::LogicGraphNodeConnected,
            HeldState::LogicGraphTrigger,
        ] {
            let (loader, world) = started();
            loader.set_prop_state(OLD_CHEST, &[kind]);
            let id = world.transaction().remove_prop(OLD_CHEST).commit().unwrap();
            loader.run_transactions();
            match world.transaction_status(id) {
                Some(TransactionStatus::Failed(WorldError::HoldsState(state))) => {
                    assert!(state.holds(kind), "{kind:?}");
                    assert_eq!(state.iter().count(), 1);
                }
                other => panic!("{kind:?}: {other:?}"),
            }
            assert!(
                loader
                    .props()
                    .iter()
                    .any(|(entity, ..)| *entity == OLD_CHEST),
                "{kind:?}: the prop stays"
            );
            drop(loader);
        }
    }

    #[test]
    fn a_removal_that_discards_state_cannot_be_undone() {
        let (loader, world) = started();
        loader.set_prop_state(OLD_CHEST, &[HeldState::Inventory]);
        let id = world
            .transaction()
            .remove_prop_discarding_state(OLD_CHEST)
            .commit()
            .unwrap();
        loader.run_transactions();
        assert_eq!(
            world.transaction_status(id),
            Some(TransactionStatus::Applied)
        );
        assert!(loader.props().is_empty());

        world.undo(id).unwrap();
        loader.run_transactions();
        match world.transaction_status(id) {
            Some(TransactionStatus::UndoRefused(WorldError::HoldsState(state))) => {
                assert!(state.holds(HeldState::Inventory));
            }
            other => panic!("{other:?}"),
        }
        assert!(
            loader.props().is_empty(),
            "nothing comes back without its items"
        );
    }

    #[test]
    fn an_undo_never_takes_away_a_placed_chest_that_has_gained_items() {
        let (loader, world) = started();
        let id = world
            .transaction()
            .place_prop(CHEST, here())
            .commit()
            .unwrap();
        loader.run_transactions();
        let placed = world.transaction_entities(id)[0];
        loader.set_prop_state(placed, &[HeldState::Inventory]);

        world.undo(id).unwrap();
        loader.run_transactions();
        match world.transaction_status(id) {
            Some(TransactionStatus::UndoRefused(WorldError::HoldsState(state))) => {
                assert!(state.holds(HeldState::Inventory));
            }
            other => panic!("{other:?}"),
        }
        assert!(loader.props().iter().any(|(entity, ..)| *entity == placed));

        loader.set_prop_state(placed, &[]);
        world.undo(id).unwrap();
        loader.run_transactions();
        assert_eq!(
            world.transaction_status(id),
            Some(TransactionStatus::Undone)
        );
    }

    #[test]
    fn a_plain_prop_removed_comes_back_on_undo() {
        let (loader, world) = started();
        let id = world.transaction().remove_prop(OLD_CHEST).commit().unwrap();
        loader.run_transactions();
        assert_eq!(
            world.transaction_status(id),
            Some(TransactionStatus::Applied)
        );
        assert!(loader.props().is_empty());
        world.undo(id).unwrap();
        loader.run_transactions();
        assert_eq!(
            world.transaction_status(id),
            Some(TransactionStatus::Undone)
        );
        let props = loader.props();
        assert_eq!(props.len(), 1);
        assert_eq!((props[0].1, props[0].2), (CHEST, [50.0, 40.0, 50.0]));
    }

    #[test]
    fn removing_something_that_is_no_prop_fails() {
        let (loader, world) = started();
        let id = world
            .transaction()
            .remove_prop(Entity(999))
            .commit()
            .unwrap();
        loader.run_transactions();
        assert_eq!(
            world.transaction_status(id),
            Some(TransactionStatus::Failed(WorldError::NotAProp))
        );
    }

    #[test]
    fn a_placed_trigger_left_as_it_was_is_undone_and_a_changed_one_stays() {
        let (loader, world) = started_with(&[CHEST]);
        let untouched = world
            .transaction()
            .place_prop(CHEST, here())
            .commit()
            .unwrap();
        let changed = world
            .transaction()
            .place_prop(CHEST, WorldTransform::at([200.0, 40.0, 200.0]))
            .commit()
            .unwrap();
        loader.run_transactions();
        loader.change_trigger_settings(world.transaction_entities(changed)[0]);

        world.undo(untouched).unwrap();
        world.undo(changed).unwrap();
        loader.run_transactions();
        assert_eq!(
            world.transaction_status(untouched),
            Some(TransactionStatus::Undone)
        );
        match world.transaction_status(changed) {
            Some(TransactionStatus::UndoRefused(WorldError::HoldsState(state))) => {
                assert!(state.holds(HeldState::LogicGraphTrigger));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_trigger_placed_before_is_not_removed() {
        let (loader, world) = started_with(&[CHEST]);
        let id = world.transaction().remove_prop(OLD_CHEST).commit().unwrap();
        loader.run_transactions();
        match world.transaction_status(id) {
            Some(TransactionStatus::Failed(WorldError::HoldsState(state))) => {
                assert!(state.holds(HeldState::LogicGraphTrigger));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(loader.props().len(), 1);
    }

    const PET: ItemId = ItemId::new(32);
    const PET_ENTITY: Entity = Entity(41);
    const SEED: ItemId = ItemId::new(33);
    const CROP: Entity = Entity(42);

    #[test]
    fn a_creature_or_an_altar_is_no_prop_to_place_or_remove() {
        let (loader, world) = started_from(
            enshwick::test::FakeHost::new()
                .item(PET, 1, 0)
                .item_places(PET, [0x5e; 16])
                .item_not_a_prop(PET)
                .prop(PET_ENTITY, PET, [60.0, 40.0, 60.0]),
        );
        assert_eq!(
            world.transaction().place_prop(PET, here()).commit(),
            Err(WorldError::NotAProp)
        );
        for discard in [false, true] {
            let transaction = world.transaction();
            let transaction = if discard {
                transaction.remove_prop_discarding_state(PET_ENTITY)
            } else {
                transaction.remove_prop(PET_ENTITY)
            };
            let id = transaction.commit().unwrap();
            loader.run_transactions();
            assert_eq!(
                world.transaction_status(id),
                Some(TransactionStatus::Failed(WorldError::NotAProp)),
                "discarding state: {discard}"
            );
        }
        assert_eq!(loader.props().len(), 2, "the pet and the chest stay");
    }

    #[test]
    fn a_placed_crop_goes_on_undo_until_it_grows() {
        let host = enshwick::test::FakeHost::new()
            .item(SEED, 1, 0)
            .item_places(SEED, [0x5d; 16])
            .item_grows(SEED);
        let (loader, world) = started_from(host);
        let young = world
            .transaction()
            .place_prop(SEED, here())
            .commit()
            .unwrap();
        let grown = world
            .transaction()
            .place_prop(SEED, WorldTransform::at([200.0, 40.0, 200.0]))
            .commit()
            .unwrap();
        loader.run_transactions();
        loader.grow_plant(world.transaction_entities(grown)[0]);
        world.undo(young).unwrap();
        world.undo(grown).unwrap();
        loader.run_transactions();
        assert_eq!(
            world.transaction_status(young),
            Some(TransactionStatus::Undone)
        );
        match world.transaction_status(grown) {
            Some(TransactionStatus::UndoRefused(WorldError::HoldsState(state))) => {
                assert!(state.holds(HeldState::Growth));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_crop_the_transaction_did_not_place_is_not_removed() {
        let host = enshwick::test::FakeHost::new()
            .item(SEED, 1, 0)
            .item_places(SEED, [0x5d; 16])
            .item_grows(SEED)
            .prop(CROP, SEED, [70.0, 40.0, 70.0]);
        let (loader, world) = started_from(host);
        let id = world.transaction().remove_prop(CROP).commit().unwrap();
        loader.run_transactions();
        match world.transaction_status(id) {
            Some(TransactionStatus::Failed(WorldError::HoldsState(state))) => {
                assert!(state.holds(HeldState::Growth));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_removed_prop_whose_place_a_like_one_took_does_not_come_back() {
        let (loader, world) = started();
        let removal = world.transaction().remove_prop(OLD_CHEST).commit().unwrap();
        loader.run_transactions();
        let again = world
            .transaction()
            .place_prop(CHEST, WorldTransform::at([50.0, 40.0, 50.0]))
            .commit()
            .unwrap();
        loader.run_transactions();
        assert_eq!(
            world.transaction_status(again),
            Some(TransactionStatus::Applied)
        );
        world.undo(removal).unwrap();
        loader.run_transactions();
        assert_eq!(
            world.transaction_status(removal),
            Some(TransactionStatus::UndoRefused(WorldError::Refused))
        );
    }

    #[test]
    fn a_player_s_rights_are_checked_for_the_turned_placement_box() {
        const BUILDER: u32 = 2;
        let host = enshwick::test::FakeHost::new()
            .placement_box(CHEST, [-0.5, 0.0, -5.0], [0.5, 2.0, 5.0])
            .player(PlayerInfo {
                header: Header::of::<PlayerInfo>(),
                slot: BUILDER,
                entity: Entity(BUILDER),
                key: enshwick::PlayerKey::new(u64::from(BUILDER)),
                permissions: PlayerInfo::CAN_EDIT_WORLD,
                name_len: 0,
                name: [0; 64],
            })
            .building_area([0.0; 3], [1000.0; 3])
            .no_build_zone([104.0, 0.0, 0.0], [106.0, 1000.0, 1000.0], true);
        let (loader, world) = started_from(host);
        let builder = world.player(Entity(BUILDER)).unwrap();
        let along = world
            .transaction_for(&builder)
            .place_prop(CHEST, here())
            .commit()
            .unwrap();
        let quarter = core::f32::consts::FRAC_1_SQRT_2;
        let across = world
            .transaction_for(&builder)
            .place_prop(
                CHEST,
                WorldTransform {
                    position: [100.0, 40.0, 100.0],
                    orientation: [0.0, quarter, 0.0, quarter],
                },
            )
            .commit()
            .unwrap();
        loader.run_transactions();
        assert_eq!(
            world.transaction_status(along),
            Some(TransactionStatus::Applied)
        );
        assert!(
            matches!(
                world.transaction_status(across),
                Some(TransactionStatus::NotAllowed(_))
            ),
            "turned, its box reaches the zone"
        );
    }

    #[test]
    fn placing_a_flame_altar_founds_a_base_and_is_refused() {
        const ALTAR: ItemId = ItemId::new(34);
        let (loader, world) = started_from(
            enshwick::test::FakeHost::new()
                .item(ALTAR, 1, 0)
                .item_places(ALTAR, [0xa1; 16])
                .item_not_a_prop(ALTAR),
        );
        assert_eq!(
            world.transaction().place_prop(ALTAR, here()).commit(),
            Err(WorldError::NotAProp)
        );
        assert!(loader.transactions().is_empty(), "nothing is queued");
    }

    #[test]
    fn only_a_box_inside_a_base_persists() {
        let (_loader, world) =
            started_from(enshwick::test::FakeHost::new().building_zone([0.0; 3], [40.0; 3]));
        assert_eq!(world.persists([1.0; 3], [5.0; 3]), Ok(true));
        assert_eq!(
            world.persists([30.0; 3], [50.0; 3]),
            Ok(false),
            "partly out"
        );
        assert_eq!(world.persists([100.0; 3], [101.0; 3]), Ok(false));
        assert_eq!(
            world.persists([f64::NAN; 3], [1.0; 3]),
            Err(WorldError::Refused)
        );
    }
}
