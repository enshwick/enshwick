//! Props in transactions: placing, removing, the state a player would lose, and undo.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::{Header, Status, TerrainMaterialInfo};
    use enshwick::ecs::Entity;
    use enshwick::world::{HeldState, TransactionStatus, World, WorldError, WorldTransform};
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
        let loader = enshwick::test::FakeHost::new()
            .type_hash("current_transform", 1)
            .type_hash("used_item", 2)
            .item(CHEST.get(), 1, 0)
            .item_places(CHEST.get(), [0x3a; 16])
            .item(PEBBLE.get(), 99, 0)
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
            item: 77,
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
            Some(TransactionStatus::Failed(WorldError::Refused))
        );
    }
}
