//! `enshwick.sound`: a script changes a sound container and adds a sound,
//! with the bounds and refusals of a Rust mod's, and raises and hears
//! notifiers as a Rust mod does.

#[cfg(test)]
mod tests {
    use enshwick::abi::Side;
    use enshwick::content::EngineFacts;
    use enshwick::events::WorldLoaded;
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
            enshwick.on("WorldLoaded", function()
                enshwick.effects.notify({
                    notifier = "07070707-0707-0707-0707-070707070707",
                    at = { Position = vector.create(2, 0, 0) },
                    sound_volume_hint = "Enemy",
                })
                enshwick.effects.notify({ notifier = "07070707-0707-0707-0707-070707070707", at = { Entity = 9 } })
                local refused = pcall(enshwick.effects.notify, { notifier = "07070707-0707-0707-0707-070707070707", at = { Entity = 0 } })
                enshwick.log.info(`entity 0 raised: {refused}`)
            end)
            "#,
        );
        assert!(loader.raise::<WorldLoaded>(&()) > 0, "{:?}", loader.logs());
        assert!(
            logged(&loader, "entity 0 raised: false"),
            "{:?}",
            loader.logs()
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

    #[test]
    fn a_script_plays_a_sound_and_steers_it_through_its_handle() {
        use enshwick::content::{Guid, types};
        let bell = Guid::named("you.bells:bell");
        let fake = FakeHost::new()
            .side(Side::CLIENT)
            .type_hash(types::SOUND_CONTAINER_RESOURCE.id(), 0xf257_640c)
            .resource(bell.key_by_hash(0xf257_640c), Vec::new());
        let loader = run(
            fake,
            &format!(
                r#"
                local bell = enshwick.sound.play({{ container = "{bell}", place = {{ At = vector.create(10, 0, 0) }}, volume = 0.5, fade_in = 0.3 }})
                enshwick.log.info(`queued {{bell:state()}} {{bell:is_playing()}}`)
                bell:set_volume(0.2, 1)
                bell:stop(0.5)
                enshwick.log.info(`stopped {{bell:state()}}`)
                local typo = pcall(enshwick.sound.play, {{ container = "07070707-0707-0707-0707-070707070707" }})
                enshwick.log.info(`typo refused {{not typo}}`)
                enshwick.sound.stop_all(0)
                "#
            ),
        );
        for line in ["queued Pending true", "stopped Ended", "typo refused true"] {
            assert!(logged(&loader, line), "{line}: {:?}", loader.logs());
        }
        let [played] = loader.sounds()[..] else {
            panic!("one sound was started: {:?}", loader.logs());
        };
        assert_eq!((played.volume, played.playing), (0.2, false));
    }

    #[test]
    fn a_script_plays_a_sound_on_a_player_s_client_and_steers_it() {
        use enshwick::content::Guid;
        use enshwick::net::Peer;
        use enshwick::{Entity, PlayerKey};
        let horn = Guid::named("you.horns:horn");
        let fake = FakeHost::new()
            .side(Side::SERVER)
            .player(enshwick::abi::headed!(enshwick::abi::PlayerInfo {
                slot: 0,
                entity: Entity::from_engine(10),
                key: PlayerKey::new(7),
                permissions: 0,
                name_len: 0,
                name: [0; 128],
            }))
            .net_peer(Peer::Player(PlayerKey::new(7)));
        let loader = run(
            fake,
            &format!(
                r#"
                local horn = enshwick.sound.play_for({{ Player = "7" }}, {{ container = "{horn}" }})
                horn:set_volume(0.5, 0)
                horn:stop(0.2)
                local wide = pcall(enshwick.sound.play_for, {{ Near = {{ at = vector.create(0, 0, 0), radius = 900 }} }}, {{ container = "{horn}" }})
                enshwick.log.info(`too wide refused {{not wide}}`)
                "#
            ),
        );
        assert!(
            logged(&loader, "too wide refused true"),
            "{:?}",
            loader.logs()
        );
        let [(to, played)] = &loader.sent_sounds()[..] else {
            panic!("one sound went: {:?}", loader.logs());
        };
        assert_eq!(to.as_slice(), [PlayerKey::new(7)]);
        assert_eq!((played.volume, played.playing), (0.5, false));
    }

    const MAIN: &str = r#"
        local chest = "6943053a-32c1-4718-ae22-e61427ddf54d"
        enshwick.sound.change(chest, { volume = 0.5, looped = true })
        local loud = pcall(enshwick.sound.change, chest, { volume = 9 })
        enshwick.log.info(`loud refused {not loud}`)
        local ok, why = pcall(enshwick.sound.add, { id = "you.horns:horn", takes = { { file = "../horn.wav" } } })
        enshwick.log.info(`outside refused {not ok}: {why}`)
        enshwick.sound.change(chest, {
            sleep = 0.25,
            max_count = 3,
            stealing_mode = "Furthest",
            random_position = { left_right = 2, front_back = 0 },
            takes = { Add = { { sound = chest, chance = 0.5 } } },
        })
        local none = pcall(enshwick.sound.change, chest, { takes = { Replace = {} } })
        enshwick.log.info(`no takes refused {not none}`)
        local away = pcall(enshwick.sound.replace, chest, "../creak.wav")
        enshwick.log.info(`replace outside refused {not away}`)
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
        for refused in ["no takes refused true", "replace outside refused true"] {
            assert!(logged(&loader, refused), "{refused}: {:?}", loader.logs());
        }
    }
}
