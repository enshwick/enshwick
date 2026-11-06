//! `enshwick.input`: the game controls a player holds, as a Rust mod reads
//! them.

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use enshwick::Entity;
    use enshwick::content::EngineFacts;
    use enshwick::test::{FakeHost, facts};

    use crate::harness::{logged, run};

    #[test]
    fn a_script_reads_the_held_controls_by_name_and_nil_for_a_stranger() {
        let fake = FakeHost::new().world().player_input();
        let ty = fake.hash(facts::PLAYER_INPUT.id()).unwrap();
        let at = fake.offset(facts::PLAYER_INPUT_DIGITAL.id()).unwrap();
        let mut bytes = vec![0u8; fake.size(facts::PLAYER_INPUT.id()).unwrap()];
        bytes[at..][..8].copy_from_slice(&(1u64 << 30 | 1).to_le_bytes());
        let loader = run(
            fake.component(Entity(40), ty, &bytes),
            r#"
            enshwick.on_tick(0.1, function()
                local held = enshwick.input.held(40)
                enshwick.log.info(`held {table.concat(held, ",")} {enshwick.input.held(99) == nil}`)
            end)
            "#,
        );
        let _ran = loader.tick(Duration::from_millis(100));
        assert!(
            logged(&loader, "held MainhandAction,Block true"),
            "{:?}",
            loader.logs()
        );
    }
}
