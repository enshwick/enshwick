//! `enshwick.items`: handlers a script sets on its items' actions, in
//! `main.luau` or through the `on` tables of `items.toml`.

#[cfg(test)]
mod tests {
    use enshwick::abi::{Header, ItemAction, Status};
    use enshwick::packs::{Pack, PackFiles};
    use enshwick::test::FakeHost;
    use enshwick::{Entity, ItemId};
    use enshwick_luau::Scripts;

    use crate::harness::{logged, run, start_with};

    const TONIC: u32 = 0x5e01;
    const LANTERN: u32 = 0x5e02;

    fn action(kind: u32, item: u32) -> ItemAction {
        ItemAction {
            header: Header::of::<ItemAction>(),
            kind,
            player: Entity(4096),
            item: ItemId::new(item),
            count: 1,
            inventory: Entity(4100),
            slot: 2,
        }
    }

    #[test]
    fn a_handler_hears_its_item_refuses_with_false_and_ignores_others() {
        let loader = run(
            FakeHost::new(),
            &format!(
                r#"
                enshwick.items.on_item({TONIC}, "Consume", function(used)
                    enshwick.log.info(`drunk by {{used.player}} from a slot {{used.slot ~= nil}}`)
                end)
                enshwick.items.on_item({TONIC}, "Trash", function() return false end)
                "#
            ),
        );
        assert!(loader.item_action(&action(ItemAction::CONSUME, TONIC)));
        assert!(
            logged(&loader, "drunk by 4096 from a slot true"),
            "{:?}",
            loader.logs()
        );
        assert!(!loader.item_action(&action(ItemAction::TRASH, TONIC)));
        assert!(loader.item_action(&action(ItemAction::TRASH, LANTERN)));
        assert!(loader.faults().is_empty(), "{:?}", loader.faults());
    }

    #[test]
    fn a_content_id_gives_the_item_id_rust_names_it_by() {
        let loader = run(
            FakeHost::new(),
            r#"enshwick.log.info(`id {enshwick.content.item_id("you.luau:tonic")}`)"#,
        );
        let id = ItemId::named("you.luau:tonic").get();
        assert!(logged(&loader, &format!("id {id}")), "{:?}", loader.logs());
    }

    #[test]
    fn an_error_in_a_handler_allows_the_action_and_turns_the_mod_off() {
        let loader = run(
            FakeHost::new(),
            &format!(
                r#"enshwick.items.on_item({TONIC}, "Equip", function()
                    error("boom")
                end)"#
            ),
        );
        assert!(loader.item_action(&action(ItemAction::EQUIP, TONIC)));
        let faults = loader.faults();
        assert_eq!(faults.len(), 1, "{faults:?}");
        assert!(faults[0].contains("boom"), "{faults:?}");
    }

    fn pack(main: &str, on: &str) -> (Pack, Scripts) {
        let items = format!(
            "[[item]]\nid = \"tonic\"\nlike = \"6943053a-32c1-4718-ae22-e61427ddf54d\"\non = {on}\n"
        );
        let files = PackFiles::new()
            .with("items.toml", items)
            .with("main.luau", main);
        let pack = Pack::parse_with("you.luau", &files, None).unwrap();
        let scripts = Scripts::new().with_items(&pack.item_bindings());
        (pack, scripts)
    }

    #[test]
    fn items_toml_binds_a_global_function_of_main_luau_to_its_item() {
        let main = r"function drink(used) enshwick.log.info(`drank {used.count}`) return false end";
        let (pack, scripts) = pack(main, r#"{ consume = "drink" }"#);
        let item = pack.item_bindings()[0].item;
        let (loader, status) = start_with(FakeHost::new(), main, scripts, None);
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert!(!loader.item_action(&action(ItemAction::CONSUME, item.get())));
        assert!(logged(&loader, "drank 1"), "{:?}", loader.logs());
    }

    #[test]
    fn a_binding_to_a_missing_function_stops_the_start_naming_it() {
        let main = "local function drink() end";
        let (_, scripts) = pack(main, r#"{ consume = "drink" }"#);
        let (loader, status) = start_with(FakeHost::new(), main, scripts, None);
        assert_ne!(status, Status::OK);
        assert!(
            logged(&loader, "item you.luau:tonic: on.consume names drink"),
            "{:?}",
            loader.logs()
        );
    }
}
