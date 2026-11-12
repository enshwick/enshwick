//! `enshwick.knowledge`: the input filter a script links at start, whose
//! nil keeps the verdict so far.

#[cfg(test)]
mod tests {
    use enshwick::Entity;
    use enshwick::abi::{Header, KnowledgeInfo, KnowledgeInput};
    use enshwick::knowledge::KnowledgeId;
    use enshwick::test::FakeHost;

    use crate::harness::run;

    fn input(value: u32, allowed: bool) -> KnowledgeInput {
        KnowledgeInput {
            header: Header::of::<KnowledgeInput>(),
            player: Entity(2),
            id: KnowledgeId::new(0x1d),
            kind: KnowledgeInfo::BOOL,
            scope: KnowledgeInfo::PLAYER,
            reserved: 0,
            value,
            refusal: KnowledgeInput::ALLOWED,
            allowed: u32::from(allowed),
        }
    }

    #[test]
    fn a_knowledge_input_filter_allows_refuses_or_keeps_the_verdict_with_nil() {
        let loader = run(
            FakeHost::new(),
            r"
            enshwick.knowledge.on_knowledge_input(function(request)
                if request.value == 1 then
                    return true
                elseif request.value == 2 then
                    return false
                end
                return nil
            end)
            ",
        );
        assert!(loader.knowledge_input(input(1, false)), "true allows");
        assert!(!loader.knowledge_input(input(2, true)), "false refuses");
        assert!(loader.knowledge_input(input(3, true)), "nil keeps an allow");
        assert!(
            !loader.knowledge_input(input(3, false)),
            "nil keeps a refusal"
        );
    }
}
