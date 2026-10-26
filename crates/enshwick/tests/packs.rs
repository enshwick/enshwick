//! A data pack through the fake loader: its items, texts and recipes
//! added with the calls code uses, under the pack's content ids, and its
//! changes of vanilla values declared.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::content::changes::{Change, Field, Value};
    use enshwick::content::{Guid, types};
    use enshwick::packs::{Pack, PackFiles};
    use enshwick::test::FakeHost;
    use enshwick::{Load, Mod, Off};

    const CHEST: &str = "6943053a-32c1-4718-ae22-e61427ddf54d";
    const ITEM: u32 = 1;
    const RECIPES: u32 = 3;

    struct Lanterns;

    impl Mod for Lanterns {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.lanterns", side: BOTH)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let files = PackFiles::new()
                .with(
                    "items.toml",
                    format!(
                        "[[item]]\nid = \"lantern\"\nlike = \"{CHEST}\"\nname = \"lantern.name\"\n\n[[change]]\nitems = [\"{CHEST}\"]\nmax_stack = 50\n"
                    ),
                )
                .with(
                    "recipes.toml",
                    format!("[[recipe]]\nid = \"lantern.recipe\"\nlike = \"{CHEST}\"\noutput = \"lantern\"\ncost = 3\n"),
                )
                .with("texts.toml", "[en]\n\"lantern.name\" = \"Lantern\"\n[uk]\n\"lantern.name\" = \"Ліхтар\"\n");
            load.add_pack(&Pack::parse("you.lanterns", &files)?)?;
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Lanterns);

    fn host() -> FakeHost {
        let mut host = FakeHost::new()
            .type_hash(types::ITEM_INFO.id(), ITEM)
            .type_hash(types::ITEM_REGISTRY.id(), 2)
            .type_hash(types::RECIPE_REGISTRY.id(), RECIPES)
            .type_size("item_stat", 0x28)
            .type_size("recipe", 0xa8)
            .type_size("recipe_ingredient", 0x18)
            .type_size("recipe_stack", 0x18);
        for (id, at) in [
            ("item_info.id", 0),
            ("item_info.object", 4),
            ("item_info.name", 0x48),
            ("item_info.icon_image", 0x58),
            ("item_info.description", 0x68),
            ("item_info.flags", 0x78),
            ("item_info.pid_entity", 0x80),
            ("item_info.ui_values", 0x90),
        ] {
            host = host.constant(id, at);
        }
        for (id, at) in [
            ("item_stat.label", 0),
            ("item_stat.format", 4),
            ("item_stat.value", 8),
            ("item_stat.signed", 0x24),
        ] {
            host = host.constant(id, at);
        }
        for (id, at) in [
            ("recipe.guid", 0),
            ("recipe.id", 0x10),
            ("recipe.workshop_guid", 0x14),
            ("recipe.workshop", 0x24),
            ("recipe.input", 0x28),
            ("recipe.output", 0x30),
            ("recipe.completion", 0x5c),
            ("recipe.required_props", 0x38),
            ("recipe.debug_name", 0x40),
        ] {
            host = host.constant(id, at);
        }
        host.constant("recipe_registry.recipes", 0)
            .constant("recipe_stack.item", 0x10)
            .constant("recipe_stack.count", 0x14)
            .constant("item_registry.refs", 0)
            .constant("item_registry.names", 0x38)
    }

    fn start() -> enshwick::test::Installed {
        let loader = host().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Lanterns)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    #[test]
    fn the_pack_s_content_is_declared_under_its_ids() {
        let loader = start();
        assert_eq!(
            loader.content_ids(),
            [
                "you.lanterns:text/lantern.name",
                "you.lanterns:lantern",
                "you.lanterns:lantern.recipe",
            ]
        );
    }

    #[test]
    fn the_item_is_served_beside_the_vanilla_one() {
        let loader = start();
        let served: Vec<_> = loader.resources().iter().map(|r| (r.key, r.base)).collect();
        let lantern = Guid::named("you.lanterns:lantern");
        assert!(served.contains(&(
            lantern.key_by_hash(ITEM),
            Guid::parse(CHEST).key_by_hash(ITEM)
        )));
        assert!(served.iter().any(|(key, _)| key.type_hash == RECIPES));
    }

    #[test]
    fn the_pack_s_changes_reach_the_loader_with_their_sources() {
        let loader = start();
        assert_eq!(
            loader.changes(),
            [(
                Change::new(Guid::parse(CHEST), Field::MaxStack, Value::Set(50.0)),
                "items.toml:6".to_owned()
            )]
        );
    }

    #[test]
    fn the_name_has_its_texts_in_every_language() {
        let loader = start();
        let tag = Guid::named("you.lanterns:text/lantern.name").id();
        let texts: Vec<String> = loader
            .texts()
            .into_iter()
            .filter(|(t, _, _)| *t == tag)
            .map(|(_, _, text)| text)
            .collect();
        assert_eq!(texts.len(), 2, "{texts:?}");
        assert!(texts.iter().any(|t| t == "Lantern") && texts.iter().any(|t| t == "Ліхтар"));
    }
}
