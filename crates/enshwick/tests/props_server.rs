//! A server-only mod adds props but no crafting prop: the server sends
//! clients its crafting prop bits, so one clients lack would shift theirs.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::Status;
    use enshwick::content::Id;
    use enshwick::content::items::NewItem;
    use enshwick::content::props::{CraftingPropSetup, NewProp};
    use enshwick::test::{CRAFTING_PROP_ITEM, FakeHost, PROP_ITEM};
    use enshwick::{Load, Mod, Off};

    static OUTCOMES: Mutex<Vec<Result<(), String>>> = Mutex::new(Vec::new());

    struct ServerProps;

    impl Mod for ServerProps {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let plain = load.add_prop(NewProp::new(NewItem::new(
                Id::new("you.server-props:crate"),
                PROP_ITEM,
            )));
            let crafting = load.add_prop(NewProp {
                crafting_prop: Some(CraftingPropSetup::default()),
                ..NewProp::new(NewItem::new(
                    Id::new("you.server-props:anvil"),
                    CRAFTING_PROP_ITEM,
                ))
            });
            OUTCOMES.lock().unwrap().extend(
                [plain.map(|_| ()), crafting.map(|_| ())]
                    .map(|outcome| outcome.map_err(|off| off.to_string())),
            );
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        ServerProps,
        r#"
        id = "you.server-props"
        side = "server"
        "#
    );

    #[test]
    fn a_server_only_mod_adds_a_prop_but_no_crafting_prop() {
        let loader = FakeHost::new().props().install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(ServerProps)`.
        let _status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        let outcomes = OUTCOMES.lock().unwrap().clone();
        assert_eq!(outcomes.len(), 2, "{:?}", loader.logs());
        assert_eq!(outcomes[0], Ok(()), "a plain prop");
        assert!(outcomes[1].is_err(), "a crafting prop: {:?}", outcomes[1]);
    }
}
