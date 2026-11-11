//! `enshwick.interactions`: the interaction filter a script links at start.

#[cfg(test)]
mod tests {
    use enshwick::abi::{Header, Interaction};
    use enshwick::test::FakeHost;
    use enshwick::{Entity, ItemId};

    use crate::harness::run;

    fn opening(item: u32) -> Interaction {
        Interaction {
            header: Header::of::<Interaction>(),
            kind: Interaction::OPEN,
            player: Entity(2),
            target: Entity(9),
            item: ItemId::new(item),
            known: 1,
            reserved: [0; 3],
            position: [1.0, 2.0, 3.0],
        }
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
}
