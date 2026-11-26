//! Craft and build filters, as the loader asks them.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicU8, Ordering};

    use enshwick::abi::{BuildCheck, CraftCheck, PlayerInfo, Status};
    use enshwick::vetoes::Verdict;
    use enshwick::world::World;
    use enshwick::{Entity, Load, Mod, Off, PlayerKey};

    struct Picky;

    static WORLD: OnceLock<World> = OnceLock::new();
    /// What the last build filter's world read found: 0 none yet, 1 the
    /// builder, 2 nothing.
    static FOUND_IN_FILTER: AtomicU8 = AtomicU8::new(0);
    const BANNED: PlayerKey = PlayerKey::new(7);

    impl Mod for Picky {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            if let Ok(world) = load.world() {
                let _first = WORLD.set(world);
            }
            load.on_build(|placement| {
                let found = WORLD.get().and_then(|world| world.player(placement.player));
                FOUND_IN_FILTER.store(if found.is_some() { 1 } else { 2 }, Ordering::Relaxed);
                true
            })?;
            load.on_build(|placement| placement.account != Some(BANNED))?;
            load.on_craft(|craft| craft.crafting_station_id.is_some() || craft.recipe_amount < 10)?;
            load.on_build(|placement| {
                if placement.position[1] < 0.0 {
                    Verdict::refuse_because("picky.below_sea")
                } else {
                    (placement.item != enshwick::ItemId::new(13)).into()
                }
            })?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Picky,
        r#"
        id = "you.picky"
        side = "both"
        "#
    );

    #[test]
    fn filters_see_typed_requests_and_refuse_by_returning_false() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Picky)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let craft = |crafting_station_id, recipe_amount| {
            enshwick::abi::headed!(CraftCheck {
                crafting_operator_id: Entity::from_engine(2),
                crafting_station_id,
                recipe_id: enshwick::RecipeId::new(5),
                recipe_amount,
                reserved: 0,
            })
        };
        assert!(loader.craft(&craft(Entity::from_engine(0), 3)));
        assert!(!loader.craft(&craft(Entity::from_engine(0), 12)));
        assert!(loader.craft(&craft(Entity::from_engine(40), 12)));
        let build = |item, y: i32| {
            enshwick::abi::headed!(BuildCheck {
                character: Entity::from_engine(2),
                item,
                position: [0.0, f64::from(y), 0.0],
                reason: core::ptr::null_mut(),
                account: PlayerKey::default(),
            })
        };
        assert_eq!(loader.build(&build(enshwick::ItemId::new(7), 5)), Ok(()));
        assert_eq!(
            loader.build(&build(enshwick::ItemId::new(7), -1)),
            Err(Some("picky.below_sea".to_owned()))
        );
        assert_eq!(
            loader.build(&build(enshwick::ItemId::new(13), 5)),
            Err(None)
        );
    }

    fn wall(character: u32) -> BuildCheck {
        enshwick::abi::headed!(BuildCheck {
            character: Entity::from_engine(character),
            item: enshwick::ItemId::new(7),
            position: [0.0; 3],
            reason: core::ptr::null_mut(),
            account: PlayerKey::default(),
        })
    }

    #[test]
    fn a_build_filter_judges_a_respawned_builder_by_account_at_once() {
        let ann = PlayerInfo::with_name(Entity::from_engine(2), BANNED, 0, "Ann");
        let bob = PlayerInfo::with_name(Entity::from_engine(3), PlayerKey::new(8), 0, "Bob");
        let loader = enshwick::test::FakeHost::new()
            .player(ann)
            .player(bob)
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Picky)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(loader.build(&wall(2)), Err(None));
        assert_eq!(loader.build(&wall(3)), Ok(()));
        assert!(loader.respawn(BANNED, Entity::from_engine(5)));
        assert_eq!(loader.build(&wall(5)), Err(None));
        assert_eq!(loader.build(&wall(9)), Ok(()));
    }

    #[test]
    fn a_client_s_own_prediction_gives_build_filters_no_account() {
        let ann = PlayerInfo::with_name(Entity::from_engine(2), BANNED, 0, "Ann");
        let loader = enshwick::test::FakeHost::new()
            .side(enshwick::abi::Side::CLIENT)
            .player(ann)
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Picky)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(loader.build(&wall(2)), Ok(()));
    }

    #[test]
    fn a_hosting_client_s_own_prediction_gives_build_filters_the_host_s_account() {
        let guest = PlayerInfo::with_name(Entity::from_engine(3), PlayerKey::new(8), 0, "Bob");
        let loader = enshwick::test::FakeHost::new()
            .side(enshwick::abi::Side::CLIENT)
            .hosting(true)
            .host_key(BANNED)
            .player(guest)
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Picky)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(
            loader.build(&wall(40)),
            Err(None),
            "the host's character in its own world"
        );
        assert_eq!(loader.build(&wall(3)), Ok(()), "a guest keeps their own");
    }

    #[test]
    fn a_build_filter_runs_off_the_game_thread_where_world_reads_find_nothing() {
        let ann = PlayerInfo::with_name(Entity::from_engine(2), BANNED, 0, "Ann");
        let loader = enshwick::test::FakeHost::new()
            .world()
            .player(ann)
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Picky)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let world = WORLD.get().expect("init kept the world");
        assert!(
            world.player(Entity::from_engine(2)).is_some(),
            "found on the game thread"
        );
        assert_eq!(
            loader.build(&wall(2)),
            Err(None),
            "the verdict still applies"
        );
        assert_eq!(FOUND_IN_FILTER.load(Ordering::Relaxed), 2);
    }
}
