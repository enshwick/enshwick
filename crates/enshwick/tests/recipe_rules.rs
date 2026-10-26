//! A mod disables recipes and takes its rules back; it asks whether a
//! recipe is disabled.

#[cfg(test)]
mod tests {
    use enshwick::abi::{RecipeRuleKind, Status};
    use enshwick::content::recipes::{RecipeRule, disable, enable, is_disabled};
    use enshwick::content::{DebugName, Guid, Id};
    use enshwick::{Load, Mod, Off};

    const TORCH: Guid = Guid::parse("7fa62007-a38e-4790-8591-873055fc3f34");
    const STRING: Guid = Guid::parse("4ebbe3ea-3b30-4c3c-9291-2c8155f7dc26");
    const SAFE_RECIPE: Id = Id::new("safes:safe.recipe");

    struct Rules;

    impl Mod for Rules {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "rules", side: BOTH)
        }

        fn init(_load: &mut Load<'_>) -> Result<Self, Off> {
            disable(RecipeRule::output(DebugName("Tool_Torch01_T0")))?;
            disable(RecipeRule::recipe(SAFE_RECIPE))?;
            disable(RecipeRule::workshop(STRING))?;
            disable(RecipeRule::Hand)?;
            enable(RecipeRule::Hand)?;
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Rules);

    #[test]
    fn rules_are_kept_until_taken_back_and_names_must_not_be_empty() {
        let loader = enshwick::test::FakeHost::new()
            .disabled_recipe(TORCH)
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(
            loader.recipe_rules(),
            [
                (RecipeRuleKind::OUTPUT, "Tool_Torch01_T0".to_owned()),
                (RecipeRuleKind::RECIPE, "safes:safe.recipe".to_owned()),
                (
                    RecipeRuleKind::WORKSHOP,
                    "4ebbe3ea-3b30-4c3c-9291-2c8155f7dc26".to_owned()
                ),
            ]
        );
        assert!(is_disabled(TORCH));
        assert!(!is_disabled(STRING));
        assert!(!is_disabled(SAFE_RECIPE));
        assert!(disable(RecipeRule::recipe(DebugName(" "))).is_err());
    }
}
