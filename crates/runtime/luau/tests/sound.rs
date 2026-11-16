//! `enshwick.sound`: a script changes a sound container and adds a sound,
//! with the bounds and refusals of a Rust mod's, and raises and hears
//! notifiers as a Rust mod does.

#[cfg(test)]
mod tests {
    use enshwick::abi::Side;
    use enshwick::content::EngineFacts;
    use enshwick::test::{FakeHost, facts};

    use crate::harness::{logged, run};

    #[test]
    fn a_script_raises_a_notifier_and_hears_it_as_a_rust_mod_does() {
        let fake = FakeHost::new().world().effect_events();
        let ty = fake.hash(facts::NOTIFY_EVENT.id()).unwrap();
        let loader = run(
            fake,
            r#"
            enshwick.sound.on_notify(function(notify)
                enshwick.log.info(`heard {notify.type} {notify.type_guid} {notify.sound_volume_hint} {notify.position.x} {notify.source_entity_id}`)
            end)
            enshwick.effects.notify({
                notifier = "07070707-0707-0707-0707-070707070707",
                at = { Position = vector.create(2, 0, 0) },
                sound_volume_hint = "Enemy",
            })
            enshwick.effects.notify({ notifier = "07070707-0707-0707-0707-070707070707", at = { Entity = 9 } })
            "#,
        );
        let emits = loader.emits();
        assert_eq!(emits.len(), 2, "{:?}", loader.logs());
        for (raised, bytes) in &emits {
            assert_eq!(*raised, ty);
            let _heard = loader.raise_game_event(ty, bytes);
        }
        assert!(
            logged(
                &loader,
                "heard Normal 07070707-0707-0707-0707-070707070707 Enemy 2 nil"
            ),
            "{:?}",
            loader.logs()
        );
        assert!(logged(&loader, "GenericSfx 0 9"), "{:?}", loader.logs());
    }

    const MAIN: &str = r#"
        local chest = "6943053a-32c1-4718-ae22-e61427ddf54d"
        enshwick.sound.change(chest, { volume = 0.5, looped = true })
        local loud = pcall(enshwick.sound.change, chest, { volume = 9 })
        enshwick.log.info(`loud refused {not loud}`)
        local ok, why = pcall(enshwick.sound.add, { id = "you.horns:horn", file = "../horn.wav" })
        enshwick.log.info(`outside refused {not ok}: {why}`)
    "#;

    #[test]
    fn a_script_s_sound_changes_and_files_are_checked_as_a_rust_mod_s() {
        let loader = run(FakeHost::new().side(Side::SERVER), MAIN);
        assert!(logged(&loader, "loud refused true"), "{:?}", loader.logs());
        assert!(
            logged(&loader, "outside refused true") && logged(&loader, "inside the mod's folder"),
            "{:?}",
            loader.logs()
        );
    }
}
