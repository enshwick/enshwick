//! Skill nodes: a named skill resolves to the node the loader answers for
//! it, and its override answers that node's checks alone.

#[cfg(test)]
mod tests {
    use enshwick::abi::SkillNode;
    use enshwick::abi::{Header, SkillCheck, Status};
    use enshwick::progress::{Skill, SkillGate};
    use enshwick::{Load, Mod, Off};

    struct Flyer;

    impl Mod for Flyer {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.on_skill(Skill::AirborneArchitect, |check| {
                (check.gate == SkillGate::FlyMode).then_some(true)
            })?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Flyer,
        r#"
        id = "you.flyer"
        side = "both"
        "#
    );

    fn check(node: u32) -> SkillCheck {
        SkillCheck {
            header: Header::of::<SkillCheck>(),
            node: SkillNode::new(node),
            gate: SkillCheck::FLY_MODE,
            unlocked: 0,
            reserved: [0; 3],
        }
    }

    #[test]
    fn a_named_skill_answers_the_checks_of_the_node_the_loader_names() {
        let loader = enshwick::test::FakeHost::new()
            .constant("skill.airborne_architect", 0x51)
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Flyer)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(loader.skill(&check(0x51)), SkillCheck::UNLOCKED);
        assert_eq!(loader.skill(&check(0x52)), SkillCheck::VANILLA);
    }
}
