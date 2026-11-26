//! Waits for a transaction's end: the step that ends it runs the handler once.

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex, OnceLock};
    use std::time::Duration;

    use enshwick::abi::{Status, TerrainMaterialInfo};
    use enshwick::world::{Settled, TransactionId, TransactionStatus, World, WorldTransform};
    use enshwick::{Entity, ItemId, Load, Mod, Off, WorldError};

    static WORLD: OnceLock<World> = OnceLock::new();

    struct Builder;

    impl Mod for Builder {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = WORLD.set(load.world()?);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Builder,
        r#"
        id = "you.builder"
        side = "server"
        "#
    );

    const CHEST: ItemId = ItemId::new(30);

    fn started() -> (enshwick::test::Installed, World) {
        let loader = enshwick::test::FakeHost::new()
            .world()
            .item(CHEST, 1, 0)
            .item_places(CHEST, enshwick::content::Guid([0x3a; 16]))
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Builder)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        loader.set_terrain(&[enshwick::abi::headed!(TerrainMaterialInfo {
            id: 9,
            flags: 0,
            hardness: 3,
            reserved: 0,
            item: ItemId::new(77),
            health: 500,
        })]);
        (loader, *WORLD.get().unwrap())
    }

    fn chest(world: World) -> TransactionId {
        world
            .transaction()
            .place_prop(CHEST, WorldTransform::at([100.0, 40.0, 100.0]))
            .commit()
            .unwrap()
    }

    /// Where the handler of a wait on `id` within `within` puts what it gets.
    fn settled_into(
        world: World,
        id: TransactionId,
        within: Duration,
    ) -> Arc<Mutex<Option<Settled>>> {
        let seen = Arc::new(Mutex::new(None));
        let kept = Arc::clone(&seen);
        world
            .when_settled(id, within, move |settled| {
                *kept.lock().unwrap() = Some(settled.clone());
            })
            .unwrap();
        seen
    }

    /// One server step.
    fn step(loader: &enshwick::test::Installed) -> usize {
        loader.tick(Duration::from_secs(1))
    }

    #[test]
    fn opening_the_world_subscribes_no_tick() {
        let (loader, _world) = started();
        assert_eq!(step(&loader), 0);
    }

    #[test]
    fn a_wait_for_a_placement_hands_over_the_prop_after_the_step_that_applies_it() {
        let (loader, world) = started();
        let id = chest(world);
        let seen = settled_into(world, id, Duration::from_secs(60));
        step(&loader);
        assert!(seen.lock().unwrap().is_none(), "pending");
        loader.run_transactions();
        step(&loader);
        let settled = seen.lock().unwrap().take().unwrap();
        assert_eq!(settled.status, Some(TransactionStatus::Applied));
        assert_eq!(settled.props, world.transaction_entities(id));
        assert_eq!(settled.props.len(), 1);
        assert_eq!(settled.standing(), Some(settled.props[0]));
        step(&loader);
        assert!(seen.lock().unwrap().is_none(), "the handler ran once");
    }

    #[test]
    fn a_wait_for_a_failed_transaction_hands_over_its_failure() {
        let (loader, world) = started();
        let id = world
            .transaction()
            .remove_prop(Entity::from_engine(999))
            .commit()
            .unwrap();
        let seen = settled_into(world, id, Duration::from_secs(60));
        loader.run_transactions();
        step(&loader);
        let settled = seen.lock().unwrap().take().unwrap();
        assert_eq!(
            settled.status,
            Some(TransactionStatus::Failed(WorldError::NotAProp))
        );
        assert!(settled.props.is_empty());
        assert_eq!(settled.standing(), None);
    }

    #[test]
    fn a_wait_past_its_bound_hands_over_the_transaction_still_pending() {
        let (loader, world) = started();
        let id = chest(world);
        let seen = settled_into(world, id, Duration::ZERO);
        step(&loader);
        let settled = seen.lock().unwrap().take().unwrap();
        assert_eq!(settled.status, Some(TransactionStatus::Pending));
        assert!(settled.props.is_empty());
        assert_eq!(settled.standing(), None);
    }

    #[test]
    fn a_wait_on_a_transaction_being_undone_ends_with_the_undo_running() {
        let (loader, world) = started();
        let id = chest(world);
        loader.run_transactions();
        world.undo(id).unwrap();
        let seen = settled_into(world, id, Duration::from_secs(60));
        step(&loader);
        let settled = seen.lock().unwrap().take().unwrap();
        assert_eq!(settled.status, Some(TransactionStatus::Undoing));
        assert_eq!(settled.standing(), None);
    }

    #[test]
    fn a_handler_that_panics_turns_its_mod_off_and_no_later_handler_runs() {
        let (loader, world) = started();
        let id = chest(world);
        world
            .when_settled(id, Duration::from_secs(60), |_| panic!("a broken handler"))
            .unwrap();
        let seen = settled_into(world, id, Duration::from_secs(60));
        loader.run_transactions();
        step(&loader);
        assert!(seen.lock().unwrap().is_none(), "the mod is off");
    }

    #[test]
    fn a_wait_for_a_placement_that_expires_hands_over_the_prop_it_placed() {
        let (loader, world) = started();
        let id = chest(world);
        let seen = settled_into(world, id, Duration::from_secs(60));
        loader.run_transactions();
        let placed = world.transaction_entities(id);
        assert_eq!(placed.len(), 1);
        loader.expire_transactions();
        step(&loader);
        let settled = seen.lock().unwrap().take().unwrap();
        assert_eq!(settled.status, Some(TransactionStatus::Expired));
        assert_eq!(settled.props, placed);
        assert_eq!(settled.standing(), Some(placed[0]));
    }

    #[test]
    fn a_hosted_game_that_ends_runs_its_waits_and_the_next_game_runs_none() {
        let (loader, world) = started();
        let applied = chest(world);
        loader.run_transactions();
        let pending = chest(world);
        let on_pending = settled_into(world, pending, Duration::from_secs(60));
        let on_applied = settled_into(world, applied, Duration::from_secs(60));
        loader.end_game();
        let ended = on_pending.lock().unwrap().take().unwrap();
        assert_eq!(
            ended.status,
            Some(TransactionStatus::Failed(WorldError::Unavailable))
        );
        let ended = on_applied.lock().unwrap().take().unwrap();
        assert_eq!(ended.status, Some(TransactionStatus::Expired));
        assert!(ended.props.is_empty(), "its props went with the world");
        assert_eq!(ended.standing(), None);
        step(&loader);
        assert!(
            on_pending.lock().unwrap().is_none(),
            "nothing in the next game"
        );
        assert!(on_applied.lock().unwrap().is_none());
    }

    #[test]
    fn a_world_s_load_runs_no_wait_and_the_next_step_does() {
        let (loader, world) = started();
        let id = chest(world);
        let seen = settled_into(world, id, Duration::ZERO);
        let _handled = loader.raise::<enshwick::events::WorldLoaded>(&());
        assert!(seen.lock().unwrap().is_none(), "a world's load is no step");
        step(&loader);
        assert!(seen.lock().unwrap().is_some());
    }

    #[test]
    fn a_wait_on_a_transaction_the_loader_forgot_is_refused_and_its_handler_dropped() {
        let (loader, world) = started();
        let id = chest(world);
        loader.forget_transactions();
        let held = Arc::new(());
        let kept = Arc::clone(&held);
        let asked = world.when_settled(id, Duration::from_secs(60), move |_| drop(kept));
        assert_eq!(asked, Err(WorldError::Refused));
        assert_eq!(Arc::strong_count(&held), 1, "the handler was freed unrun");
        assert_ne!(chest(world), id, "a new transaction never takes its id");
    }

    #[test]
    fn a_wait_whose_transaction_the_loader_forgets_meanwhile_ends_with_no_status() {
        let (loader, world) = started();
        let id = chest(world);
        let seen = settled_into(world, id, Duration::from_secs(60));
        loader.forget_transactions();
        step(&loader);
        let settled = seen.lock().unwrap().take().unwrap();
        assert_eq!(settled.status, None);
        assert!(settled.props.is_empty());
    }

    #[test]
    fn a_handler_may_wait_for_the_next_transaction() {
        let (loader, world) = started();
        let first = chest(world);
        let second = Arc::new(Mutex::new(None));
        let next = Arc::clone(&second);
        world
            .when_settled(first, Duration::from_secs(60), move |_| {
                let id = chest(world);
                *next.lock().unwrap() = Some(settled_into(world, id, Duration::from_secs(60)));
            })
            .unwrap();
        loader.run_transactions();
        step(&loader);
        let seen = second
            .lock()
            .unwrap()
            .take()
            .expect("the first handler ran");
        assert!(seen.lock().unwrap().is_none(), "the second is pending");
        loader.run_transactions();
        step(&loader);
        let settled = seen.lock().unwrap().take().unwrap();
        assert_eq!(settled.status, Some(TransactionStatus::Applied));
    }
}
