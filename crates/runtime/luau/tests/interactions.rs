//! `enshwick.interactions`: the interaction filter a script links at start.

#[cfg(test)]
mod tests {
    use enshwick::abi::Interaction;
    use enshwick::test::FakeHost;
    use enshwick::{Entity, ItemId};

    use crate::harness::run;

    fn opening(item: u32) -> Interaction {
        enshwick::abi::headed!(Interaction {
            item: ItemId::new(item),
            known: 1,
            position: [1.0, 2.0, 3.0],
            ..Interaction::new(
                Interaction::OPEN,
                Entity::from_engine(2),
                Entity::from_engine(9),
            )
        })
    }

    #[test]
    fn a_script_refuses_opening_its_locked_chest_and_allows_the_rest() {
        let loader = run(
            FakeHost::new(),
            r#"
            enshwick.interactions.on_interaction(function(i)
                return not (i.item == 0x5e01 and i.kind == "Open")
            end)
            "#,
        );
        assert!(!loader.interaction(&opening(0x5e01)));
        assert!(loader.interaction(&opening(0x5e02)));
    }

    #[test]
    fn a_script_gives_its_item_a_game_verb_its_text_and_the_empty_action() {
        use enshwick::abi::InteractionPrompt;
        use enshwick::interactions::Verb;

        let loader = run(
            FakeHost::new(),
            r#"
            local pray = enshwick.resources.declare("you.luau:pray")
            enshwick.interactions.prompt(0x5e01, { verb = "Use", text = pray, replace_action = true })
            enshwick.interactions.prompt(0x5e02, { verb = "Activate" })
            "#,
        );
        let altar = loader.prompt(ItemId::new(0x5e01)).unwrap();
        assert_eq!(
            (altar.verb, altar.flags),
            (Verb::Use.id(), InteractionPrompt::REPLACE_ACTION)
        );
        assert_ne!(altar.text, 0);
        let lamp = loader.prompt(ItemId::new(0x5e02)).unwrap();
        assert_eq!(
            (lamp.verb, lamp.text, lamp.flags),
            (Verb::Activate.id(), 0, 0)
        );
    }
}
