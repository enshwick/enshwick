//! Explicit client event subscriptions never receive hosted server events.

#[cfg(test)]
mod tests {
    use enshwick::abi::{Side, Status};
    use enshwick::resources::{Constant, Field, Kind, Type, fnv1a32};
    use enshwick::typed::{
        EventType,
        ecs::{OpenHudScreenEvent, UiSelectCraftingStationRecipeEvent},
    };
    use enshwick::{Load, Mod, Off};
    use std::sync::Mutex;

    static CLIENT: Mutex<Vec<u32>> = Mutex::new(Vec::new());
    static SERVER: Mutex<Vec<u32>> = Mutex::new(Vec::new());
    struct Watch;
    impl Mod for Watch {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let open = EventType::<OpenHudScreenEvent>::resolve(load)?;
            open.on_client(load, |event| {
                CLIENT.lock().unwrap().push(event.interaction_host_id.id);
            })?;
            EventType::<UiSelectCraftingStationRecipeEvent>::resolve(load)?.on_client(
                load,
                |event| {
                    CLIENT.lock().unwrap().push(event.recipe_id.value);
                },
            )?;
            let raw = load.game_event_type("ecs::OpenHudScreenEvent")?;
            load.on_raw_game_event(raw, |event| {
                SERVER.lock().unwrap().push(event.type_hash());
            })?;
            Ok(Self)
        }
    }
    enshwick::test::declare_mod!(
        Watch,
        r#"id = "test.client-events"
side = "both""#
    );

    fn types() -> Vec<Type<'static>> {
        let field = |name, offset, ty| Field::new(name, offset, ty);
        let mut out = vec![
            Type::new("keen::uint8", Kind::Uint8, 1, 1),
            Type::new("keen::uint32", Kind::Uint32, 4, 4),
            Type::new("keen::sint64", Kind::Sint64, 8, 8),
            Type::new("keen::Time", Kind::Struct, 8, 8).with_fields(vec![field(
                "value",
                0,
                "keen::sint64",
            )]),
            Type::new("keen::ecs::Event", Kind::Struct, 1, 1),
            Type::new("keen::ecs::GameEvent", Kind::Struct, 8, 8)
                .with_inner("keen::ecs::Event")
                .with_fields(vec![field("timeStamp", 0, "keen::Time")]),
            Type::new("keen::HudScreenType", Kind::Enum, 1, 1)
                .with_inner("keen::uint8")
                .with_constants(vec![Constant::new("InGameMenu", 4)]),
            Type::new("keen::RecipeInputCategorySelection", Kind::Struct, 8, 4).with_fields(vec![
                field("id", 0, "keen::HashKey32"),
                field("selectedIndex", 4, "keen::uint8"),
            ]),
            Type::new(
                "keen::StaticArray<keen::RecipeInputCategorySelection,4>",
                Kind::StaticArray,
                32,
                4,
            )
            .with_inner("keen::RecipeInputCategorySelection")
            .with_count(4),
            Type::new("keen::RecipeInputCategorySelections", Kind::Struct, 32, 4).with_fields(
                vec![field(
                    "selections",
                    0,
                    "keen::StaticArray<keen::RecipeInputCategorySelection,4>",
                )],
            ),
            Type::new("keen::ecs::OpenHudScreenEvent", Kind::Struct, 24, 8)
                .with_inner("keen::ecs::GameEvent")
                .with_fields(vec![
                    field("playerEntityId", 8, "keen::ecs::EntityId"),
                    field("interactionHostId", 12, "keen::ecs::EntityId"),
                    field("screen", 16, "keen::HudScreenType"),
                ]),
            Type::new(
                "keen::ecs::UiSelectCraftingStationRecipeEvent",
                Kind::Struct,
                56,
                8,
            )
            .with_inner("keen::ecs::GameEvent")
            .with_fields(vec![
                field("craftingOperatorId", 8, "keen::ecs::EntityId"),
                field("craftingStationId", 12, "keen::ecs::EntityId"),
                field("recipeId", 16, "keen::RecipeId"),
                field(
                    "inputCategorySelections",
                    20,
                    "keen::RecipeInputCategorySelections",
                ),
            ]),
        ];
        for (name, member) in [
            ("keen::ecs::EntityId", "id"),
            ("keen::HashKey32", "value"),
            ("keen::RecipeId", "value"),
        ] {
            out.push(Type::new(name, Kind::Struct, 4, 4).with_fields(vec![field(
                member,
                0,
                "keen::uint32",
            )]));
        }
        out
    }

    fn hash(name: &str) -> u32 {
        fnv1a32(name.as_bytes())
    }

    /// A fake game of `side` that reflects the two client events the mod reads.
    fn host(side: Side) -> enshwick::test::FakeHost {
        enshwick::test::FakeHost::new()
            .side(side)
            .types(types())
            .game_event_type(
                "ecs::OpenHudScreenEvent",
                hash("keen::ecs::OpenHudScreenEvent"),
                24,
            )
            .game_event_type(
                "ecs::UiSelectCraftingStationRecipeEvent",
                hash("keen::ecs::UiSelectCraftingStationRecipeEvent"),
                56,
            )
    }

    #[test]
    fn typed_client_events_and_server_events_have_independent_delivery() {
        let loader = host(Side::CLIENT).install();
        // SAFETY: these exports are generated for this module's test mod.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        // SAFETY: the fake table remains valid throughout its installation.
        let register = unsafe { &*loader.api() }.client_game_event;
        assert_eq!(
            register(
                enshwick::abi::ModHandle(0),
                42,
                unused,
                core::ptr::null_mut()
            ),
            Status::TOO_LATE,
            "client subscriptions close when initialization ends"
        );
        let mut open = [0; 24];
        open[12..16].copy_from_slice(&42u32.to_le_bytes());
        open[16] = 4;
        let open_hash = hash("keen::ecs::OpenHudScreenEvent");
        assert_eq!(loader.raise_game_event(open_hash, &open), 1);
        assert!(CLIENT.lock().unwrap().is_empty());
        assert_eq!(loader.raise_client_game_event(open_hash, &open), 1);
        let mut recipe = [0; 56];
        recipe[16..20].copy_from_slice(&99u32.to_le_bytes());
        assert_eq!(
            loader.raise_client_game_event(
                hash("keen::ecs::UiSelectCraftingStationRecipeEvent"),
                &recipe
            ),
            1
        );
        assert_eq!(*CLIENT.lock().unwrap(), [42, 99]);
        assert_eq!(*SERVER.lock().unwrap(), [open_hash]);
    }

    unsafe extern "system" fn unused(
        _event: *const enshwick::abi::GameEventData,
        _user: *mut core::ffi::c_void,
    ) {
    }

    #[test]
    fn a_dedicated_server_keeps_a_client_subscription_that_never_runs() {
        use enshwick::abi::ModHandle;
        let loader = enshwick::test::FakeHost::new().side(Side::SERVER).install();
        // SAFETY: the fake's table is readable and lives for the installation.
        let register = unsafe { &*loader.api() }.client_game_event;
        assert_eq!(
            register(ModHandle(0), 42, unused, core::ptr::null_mut()),
            Status::OK
        );
        assert_eq!(loader.raise_client_game_event(42, &[0; 8]), 0);
    }

    #[test]
    fn a_client_takes_every_event_type_and_any_number_of_subscriptions() {
        use enshwick::abi::ModHandle;
        let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).install();
        // SAFETY: the fake's table is readable and lives for the installation.
        let register = unsafe { &*loader.api() }.client_game_event;
        assert_eq!(
            register(ModHandle(0), 0, unused, core::ptr::null_mut()),
            Status::OK
        );
        for _ in 0..100 {
            assert_eq!(
                register(ModHandle(0), 42, unused, core::ptr::null_mut()),
                Status::OK
            );
        }
        assert_eq!(loader.raise_client_game_event(7, &[0; 8]), 1);
        assert_eq!(loader.raise_client_game_event(42, &[0; 8]), 101);
    }

    #[test]
    fn a_client_subscription_after_init_is_too_late_on_either_side() {
        use enshwick::abi::ModHandle;
        for side in [Side::SERVER, Side::CLIENT] {
            let loader = host(side).install();
            // SAFETY: these exports are generated for this module's test mod.
            let status =
                unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
            assert_eq!(status, Status::OK, "{:?}", loader.logs());
            // SAFETY: the fake's table is readable and lives for the installation.
            let register = unsafe { &*loader.api() }.client_game_event;
            assert_eq!(
                register(ModHandle(0), 42, unused, core::ptr::null_mut()),
                Status::TOO_LATE,
                "{side:?}"
            );
        }
    }

    #[test]
    fn client_event_api_refuses_an_older_or_short_loader_table() {
        use enshwick::abi::{ABI_MINOR, HostApi, ModHandle};
        let loader = enshwick::test::FakeHost::new().install();
        for minor in [ABI_MINOR - 1, ABI_MINOR] {
            // SAFETY: the fake table is readable for its full size.
            let mut old = unsafe { *loader.api() };
            old.header.abi_minor = minor;
            old.header.size =
                u32::try_from(core::mem::offset_of!(HostApi, client_game_event)).unwrap();
            let old = Box::leak(Box::new(old));
            // SAFETY: this aligned table is readable for its declared prefix
            // and remains valid and unchanged for the rest of the process.
            assert!(unsafe { enshwick::Host::new(old, ModHandle(1)) }.is_none());
        }
    }
}

/// A mod that reads every event of the client's own world, untyped.
#[cfg(test)]
mod every {
    use enshwick::abi::{Side, Status};
    use enshwick::{Load, Mod, Off};
    use std::sync::Mutex;

    static SEEN: Mutex<Vec<u32>> = Mutex::new(Vec::new());
    struct Every;
    impl Mod for Every {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.on_every_client_raw_game_event(|event| {
                SEEN.lock().unwrap().push(event.type_hash());
            })?;
            Ok(Self)
        }
    }
    enshwick::test::declare_mod!(
        Every,
        r#"id = "test.client-every"
side = "both""#
    );

    #[test]
    fn the_every_type_handler_gets_each_client_event_and_no_server_event() {
        let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).install();
        // SAFETY: these exports are generated for this module's test mod.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(loader.raise_client_game_event(7, &[0; 4]), 1);
        assert_eq!(loader.raise_client_game_event(9, &[]), 1);
        assert_eq!(loader.raise_game_event(7, &[0; 4]), 0);
        assert_eq!(*SEEN.lock().unwrap(), [7, 9]);
    }

    #[test]
    fn a_both_mod_asking_for_client_events_starts_on_a_dedicated_server() {
        let loader = enshwick::test::FakeHost::new().side(Side::SERVER).install();
        // SAFETY: these exports are generated for this module's test mod.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(loader.raise_client_game_event(7, &[0; 4]), 0);
        assert!(SEEN.lock().unwrap().is_empty());
    }
}
