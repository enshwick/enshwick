//! A content pack through the fake loader: its items, texts and recipes
//! added with the calls code uses, under the pack's content ids.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::content::{Guid, types};
    use enshwick::packs::{Pack, PackFiles};
    use enshwick::test::FakeHost;
    use enshwick::{Load, Mod, Off};

    const CHEST: &str = "6943053a-32c1-4718-ae22-e61427ddf54d";

    struct Lanterns;

    impl Mod for Lanterns {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "lanterns", side: BOTH)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let files = PackFiles::new()
                .with(
                    "items.toml",
                    format!(
                        "[[item]]\nid = \"lantern\"\nlike = \"{CHEST}\"\nname = \"lantern.name\"\n"
                    ),
                )
                .with(
                    "recipes.toml",
                    format!("[[recipe]]\nid = \"lantern.recipe\"\nlike = \"{CHEST}\"\noutput = \"lantern\"\ncost = 3\n"),
                )
                .with("texts.toml", "[en]\n\"lantern.name\" = \"Lantern\"\n[uk]\n\"lantern.name\" = \"Ліхтар\"\n");
            load.add_pack(&Pack::parse("lanterns", &files)?)?;
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Lanterns);

    fn host() -> FakeHost {
        let mut host = FakeHost::new()
            .type_size("keen::ImpactUiValue", 0x28)
            .type_size("keen::RecipeInfo", 0xa8)
            .type_size("keen::RecipeInputResource", 0x18)
            .type_size("keen::RecipeItemStackResource", 0x18);
        for (field, at) in [
            ("itemId", 0),
            ("objectId", 4),
            ("name", 0x48),
            ("iconImage", 0x58),
            ("description", 0x68),
            ("flags", 0x78),
            ("pidEntity", 0x80),
            ("uiValues", 0x90),
        ] {
            host = host.field_offset("keen::ItemInfo", field, at);
        }
        for (field, at) in [
            ("locaId", 0),
            ("valueFormat", 4),
            ("value", 8),
            ("isSigned", 0x24),
        ] {
            host = host.field_offset("keen::ImpactUiValue", field, at);
        }
        for (field, at) in [
            ("recipeGuid", 0),
            ("recipeId", 0x10),
            ("workshopGuid", 0x14),
            ("workshopId", 0x24),
            ("input", 0x28),
            ("output", 0x30),
            ("completionRequirementQuery", 0x5c),
            ("requiredProps", 0x38),
            ("debugName", 0x40),
        ] {
            host = host.field_offset("keen::RecipeInfo", field, at);
        }
        host.field_offset("keen::RecipeRegistryResource", "recipes", 0)
            .field_offset("keen::RecipeItemStackResource", "item", 0x10)
            .field_offset("keen::RecipeItemStackResource", "count", 0x14)
            .field_offset("keen::ItemRegistryResource", "itemRefs", 0)
            .field_offset("keen::ItemRegistryResource", "dbgNames", 0x38)
    }

    fn start() -> enshwick::test::Installed {
        let loader = host().install();
        // SAFETY: the exports of `declare_mod!` above.
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
                "lanterns:text/lantern.name",
                "lanterns:lantern",
                "lanterns:lantern.recipe",
            ]
        );
    }

    #[test]
    fn the_item_is_served_beside_the_vanilla_one() {
        let loader = start();
        let served: Vec<_> = loader.resources().iter().map(|r| (r.key, r.base)).collect();
        let lantern = Guid::named("lanterns:lantern");
        assert!(served.contains(&(
            lantern.key(types::ITEM_INFO),
            Guid::parse(CHEST).key(types::ITEM_INFO)
        )));
        assert!(
            served
                .iter()
                .any(|(key, _)| key.type_hash == types::RECIPE_REGISTRY)
        );
    }

    #[test]
    fn the_name_has_its_texts_in_every_language() {
        let loader = start();
        let tag = Guid::named("lanterns:text/lantern.name").id();
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
