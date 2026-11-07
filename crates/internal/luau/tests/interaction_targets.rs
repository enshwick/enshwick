//! `enshwick.interactions.target`: the entity a player's prompt offers, as
//! a Rust mod reads it.

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use enshwick::Entity;
    use enshwick::content::EngineFacts;
    use enshwick::test::{FakeHost, facts};

    use crate::harness::{logged, run};

    #[test]
    fn a_script_reads_the_prompt_s_target_and_nil_for_a_stranger() {
        let fake = FakeHost::new().world().interaction_targets();
        let ty = fake.hash(facts::SELECTED_TARGETS.id()).unwrap();
        let info = fake.size(facts::TARGET_INFO.id()).unwrap();
        let at = 2 * info + fake.offset(facts::TARGET_INFO_ENTITY.id()).unwrap();
        let mut bytes = vec![0u8; fake.size(facts::SELECTED_TARGETS.id()).unwrap()];
        bytes[at..][..4].copy_from_slice(&512u32.to_le_bytes());
        let loader = run(
            fake.component(Entity(40), ty, &bytes),
            r"
            enshwick.on_tick(0.1, function()
                local target = enshwick.interactions.target(40)
                enshwick.log.info(`target {target} {enshwick.interactions.target(99) == nil}`)
            end)
            ",
        );
        let _ran = loader.tick(Duration::from_millis(100));
        assert!(logged(&loader, "target 512 true"), "{:?}", loader.logs());
    }
}
