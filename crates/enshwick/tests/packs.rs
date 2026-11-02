//! A data pack through the fake loader: its items, texts and recipes
//! added with the calls code uses, under the pack's content ids, and its
//! changes of vanilla values declared.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::content::changes::{Change, Field, Value};
    use enshwick::content::{EngineFacts, Guid, ResourceType, types};
    use enshwick::packs::{Pack, PackFiles};
    use enshwick::test::FakeHost;
    use enshwick::{Load, Mod, Off};

    const CHEST: &str = "6943053a-32c1-4718-ae22-e61427ddf54d";
    const RECIPE: &str = "b2c1d2e3-4f50-4a61-8b72-c3d4e5f60718";

    struct Lanterns;

    impl Mod for Lanterns {
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
                    format!("[[recipe]]\nid = \"lantern.recipe\"\nlike = \"{CHEST}\"\noutput = \"lantern\"\ncost = 3\n\n[[change]]\nrecipes = [\"{RECIPE}\"]\ntime = 0\ningredients = {{ \"{CHEST}\" = 2 }}\n"),
                )
                .with("texts.toml", "[en]\n\"lantern.name\" = \"Lantern\"\n[uk]\n\"lantern.name\" = \"Ліхтар\"\n");
            load.add_pack(&Pack::parse("you.lanterns", &files)?)?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Lanterns,
        r#"
        id = "you.lanterns"
        side = "both"
        "#
    );

    fn host() -> FakeHost {
        FakeHost::new().items().recipes()
    }

    /// The hash `host` gives the resource type `ty`.
    fn hash(ty: ResourceType) -> u32 {
        host().hash(ty.id()).unwrap()
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
        let item = hash(types::ITEM_INFO);
        assert!(served.contains(&(
            lantern.key_by_hash(item),
            Guid::parse(CHEST).key_by_hash(item)
        )));
        let recipes = hash(types::RECIPE_REGISTRY_RESOURCE);
        assert!(served.iter().any(|(key, _)| key.type_hash == recipes));
    }

    #[test]
    fn the_pack_s_changes_reach_the_loader_with_their_sources() {
        let loader = start();
        assert_eq!(
            loader.changes(),
            [
                (
                    Change::new(Guid::parse(CHEST), Field::MaxStackSize, Value::Set(50.0)),
                    "items.toml:6".to_owned()
                ),
                (
                    Change::new(
                        Guid::parse(RECIPE),
                        Field::CraftingDuration,
                        Value::Set(0.0)
                    ),
                    "recipes.toml:7".to_owned()
                ),
                (
                    Change::ingredient(Guid::parse(RECIPE), Guid::parse(CHEST), Value::Set(2.0)),
                    "recipes.toml:7".to_owned()
                ),
            ]
        );
    }

    #[test]
    fn the_name_has_its_texts_in_every_language() {
        let loader = start();
        let tag = Guid::named("you.lanterns:text/lantern.name").fnv1a();
        let texts: Vec<String> = loader
            .texts()
            .into_iter()
            .filter(|(t, _, _)| *t == tag)
            .map(|(_, _, text)| text)
            .collect();
        assert_eq!(texts.len(), 2, "{texts:?}");
        assert!(texts.iter().any(|t| t == "Lantern") && texts.iter().any(|t| t == "Ліхтар"));
    }

    /// A pack that changes another mod's item, as the loader checks it
    /// against that mod's exports.
    mod named {
        use enshwick::abi::{Level, Status};
        use enshwick::packs::{Pack, PackFiles};
        use enshwick::test::FakeHost;
        use enshwick::{Load, Mod, Off};

        struct Stacks;

        impl Mod for Stacks {
            fn init(load: &mut Load<'_>) -> Result<Self, Off> {
                let files = PackFiles::new().with(
                    "items.toml",
                    "[[change]]\nitems = [\"you.lockers:key\"]\nmax_stack = 50\n",
                );
                load.add_pack(&Pack::parse("you.stacks", &files)?)?;
                Ok(Self)
            }
        }

        enshwick::test::declare_mod!(
            Stacks,
            r#"
            id = "you.stacks"
            side = "server"
            "#
        );

        fn start(exports: &[&str]) -> (Status, Vec<(Level, String)>) {
            let loader = FakeHost::new().exports("you.lockers", exports).install();
            // SAFETY: both functions are the exports of this module's `declare_mod!(Stacks)`.
            let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
            (status, loader.logs())
        }

        #[test]
        fn another_mod_s_item_it_exports_takes_the_change() {
            let (status, logs) = start(&["content:key"]);
            assert_eq!(status, Status::OK, "{logs:?}");
        }

        #[test]
        fn another_mod_s_item_it_does_not_export_turns_the_pack_off() {
            let (status, logs) = start(&["content:locker", "point:key"]);
            assert_ne!(status, Status::OK);
            assert!(
                logs.iter().any(|(level, line)| *level == Level::WARN
                    && line == "content id you.lockers:key: you.lockers does not export key; it exports content:locker, point:key"),
                "{logs:?}"
            );
        }
    }
}
