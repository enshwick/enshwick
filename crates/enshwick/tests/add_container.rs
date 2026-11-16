//! Mods add container types: a prop copied from a vanilla container whose
//! template holds the slot count the mod asks, in whole rows of 8.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::Status;
    use enshwick::content::items::NewItem;
    use enshwick::content::props::{NewProp, PropId};
    use enshwick::content::{EngineFacts, Guid, Id, types};
    use enshwick::inventory::containers::{MAX_SLOTS, NewContainer};
    use enshwick::resources::{Registry, Value};
    use enshwick::test::{CHEST_ITEM, CHEST_SLOTS, FakeHost, PROP_ITEM};
    use enshwick::{Load, Mod, Off};

    const BIN: Id = Id::new("you.containers:ore-bin");

    static ADDED: Mutex<Vec<PropId>> = Mutex::new(Vec::new());
    static REFUSED: Mutex<Vec<String>> = Mutex::new(Vec::new());

    fn container(name: &'static str, like: Guid, slots: u8) -> NewContainer {
        NewContainer::new(NewProp::new(NewItem::new(Id::new(name), like)), slots)
    }

    struct Containers;

    impl Mod for Containers {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let bin = load.add_container(NewContainer::new(
                NewProp::new(NewItem::new(BIN, CHEST_ITEM)),
                48,
            ))?;
            ADDED.lock().unwrap().push(bin);
            for refused in [
                container("you.containers:none", CHEST_ITEM, 0),
                container("you.containers:part-row", CHEST_ITEM, 12),
                container("you.containers:past-links", CHEST_ITEM, MAX_SLOTS + 8),
                container("you.containers:no-chest", PROP_ITEM, 16),
            ] {
                if let Err(off) = load.add_container(refused) {
                    REFUSED.lock().unwrap().push(off.to_string());
                }
            }
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Containers,
        r#"
        id = "you.containers"
        side = "both"
        "#
    );

    fn start() -> enshwick::test::Installed {
        let loader = FakeHost::new().containers().install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Containers)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    /// The generic slots the inventory setup of `template` holds.
    fn slots(template: &Value) -> Option<u64> {
        let Some(Value::List(components)) = template.field("components") else {
            return None;
        };
        components.iter().find_map(|held| match held {
            Value::Variant(held) if held.type_name() == "demo::ecs::InventorySetup" => {
                match held.value().field("genericSlotCount") {
                    Some(Value::Uint(slots)) => Some(*slots),
                    _ => None,
                }
            }
            _ => None,
        })
    }

    #[test]
    fn a_container_s_template_holds_the_slots_the_mod_asks() {
        let loader = start();
        let bin = ADDED.lock().unwrap()[0];
        assert_eq!(bin.template.guid(), BIN.part("template").guid());

        let host = enshwick::host().unwrap();
        let placed_at = host.offset("item_info.placed_entity").unwrap();
        let chest = host
            .game_resource(types::ITEM_INFO.key(CHEST_ITEM).unwrap())
            .unwrap();
        let vanilla = Guid(chest[placed_at..placed_at + 16].try_into().unwrap());
        let template_hash = host.hash("ecs::TemplateResource").unwrap();
        let base = host
            .game_resource(vanilla.key_by_hash(template_hash))
            .unwrap();
        let registry = Registry::game().unwrap();
        let decode = |bytes: &[u8]| {
            registry
                .decode("demo::ecs::TemplateResource", bytes)
                .unwrap()
        };
        assert_eq!(slots(&decode(&base)), Some(CHEST_SLOTS.into()));
        let built = loader
            .build_resource(&bin.template.guid().key_by_hash(template_hash), &base)
            .unwrap();
        assert_eq!(slots(&decode(&built)), Some(48));
    }

    #[test]
    fn a_count_the_game_cannot_hold_and_an_item_that_places_no_container_are_refused() {
        let _loader = start();
        let refused = REFUSED.lock().unwrap().clone();
        assert_eq!(refused.len(), 4, "{refused:?}");
        for (off, said) in
            refused
                .iter()
                .zip(["0 slots", "12 slots", "136 slots", "places no container"])
        {
            assert!(off.contains(said), "{off}");
        }
    }
}
