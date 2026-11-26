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
            fake.component(Entity::from_engine(40), ty, &bytes),
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

    #[test]
    fn a_script_reads_a_game_binding_and_rebinds_it_by_the_action_s_name() {
        use enshwick::abi::Side;
        use enshwick::controls::Key;
        let loader = run(
            FakeHost::new().side(Side::CLIENT).player_input(),
            r#"
            enshwick.on_tick(0.1, function()
                local jump = enshwick.input.binding("Jump")
                enshwick.log.info(`jump {jump.first} {jump.second == nil}`)
                enshwick.input.rebind("Jump", "Shift+J")
                local ok, why = pcall(enshwick.input.binding, "Sneak")
                enshwick.log.info(`sneak {ok} {why}`)
            end)
            "#,
        );
        loader.bind_game_action("Jump", Some(Key::parse("Space").unwrap()), None);
        let _drawn = loader.draw_frame();
        let _ran = loader.tick(Duration::from_millis(100));
        assert!(logged(&loader, "jump Space true"), "{:?}", loader.logs());
        assert!(
            logged(&loader, "sneak false") && logged(&loader, "no action Sneak"),
            "{:?}",
            loader.logs()
        );
        assert_eq!(loader.game_rebind("Jump"), Key::parse("Shift+J").ok());
    }

    #[test]
    fn a_namespace_whose_open_costs_nothing_opens_though_no_script_names_it() {
        let loader = run(
            FakeHost::new().world(),
            r#"
            local name = "wor" .. "ld"
            enshwick.log.info(`reached {enshwick[name] ~= nil}`)
            "#,
        );
        assert!(logged(&loader, "reached true"), "{:?}", loader.logs());
    }

    #[test]
    fn a_mod_whose_scripts_never_name_input_starts_no_input_reads() {
        use enshwick::abi::{LocalInputInfo, Side};
        let loader = run(
            FakeHost::new().side(Side::CLIENT).player_input(),
            "enshwick.log.info('no controls read here')",
        );
        let _drawn = loader.draw_frame();
        let mut info = enshwick::abi::headed!(LocalInputInfo {
            flags: 0,
            reserved: 0,
            age_ms: 0,
        });
        // SAFETY: the fake's table lives for the process; `info` is writable.
        let status = (unsafe { &*loader.api() }.local_input_status)(loader.handle(), &raw mut info);
        assert!(status.is_ok());
        assert_eq!(
            info.flags & LocalInputInfo::REQUESTED,
            0,
            "nothing asked for the held controls"
        );
    }
}
