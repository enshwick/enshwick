//! Notifiers: the ones a mod raises, as the loader receives them, and the
//! ones the server raises, as a mod hears them.

#[cfg(test)]
mod tests {
    use core::time::Duration;
    use std::sync::{Mutex, OnceLock};

    use enshwick::abi::Status;
    use enshwick::content::{EngineFacts, Guid};
    use enshwick::effects::{Effects, MAX_NOTIFIES_PER_STEP, Notify, NotifyAt};
    use enshwick::game_events::{NotifyEvent, NotifyEventType, SoundVolumeHint};
    use enshwick::test::FakeHost;
    use enshwick::test::facts::{self, Fact};
    use enshwick::{Entity, Load, Mod, Off, WorldError};

    static EFFECTS: OnceLock<Effects> = OnceLock::new();
    static HEARD: Mutex<Vec<NotifyEvent>> = Mutex::new(Vec::new());

    struct Chimes;

    impl Mod for Chimes {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = EFFECTS.set(load.effects());
            load.on_notify(|notify| HEARD.lock().unwrap().push(*notify))?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Chimes,
        r#"
        id = "you.chimes"
        side = "server"
        "#
    );

    const BELL: Guid = Guid([7; 16]);

    fn word<const N: usize>(bytes: &[u8], at: usize) -> [u8; N] {
        bytes[at..at + N].try_into().unwrap()
    }

    #[test]
    fn a_notifier_past_the_fixed_point_is_refused_and_one_at_the_bound_is_raised() {
        let loader = FakeHost::new().world().effect_events().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Chimes)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let effects = EFFECTS.get().unwrap();
        let at = |position| Notify::new(BELL, NotifyAt::Position(position));
        for far in [[0.0, 1e300, 0.0], [0.0, 0.0, -2_147_483_648.0]] {
            assert_eq!(
                effects.notify(&at(far)),
                Err(WorldError::BadArgument),
                "{far:?}"
            );
        }
        assert!(loader.emits().is_empty());
        effects.notify(&at([2_147_483_647.0, 0.0, 0.0])).unwrap();
        assert_eq!(loader.emits().len(), 1);
    }

    #[test]
    fn a_notifier_lays_out_its_event_is_bounded_each_step_and_heard_without_effects() {
        let host = FakeHost::new().world().effect_events();
        let at = |fact: Fact| host.offset(fact.id()).unwrap();
        let transform = at(facts::NOTIFY_EVENT_TRANSFORM);
        let (position, orientation, scale) = (
            transform + at(facts::WORLD_TRANSFORM_POSITION),
            transform + at(facts::WORLD_TRANSFORM_ORIENTATION),
            transform + at(facts::WORLD_TRANSFORM_SCALE),
        );
        let turn = at(facts::NOTIFY_EVENT_OFFSET) + at(facts::OFFSET_ORIENTATION_OFFSET);
        let (source, guid, kind, hint) = (
            at(facts::NOTIFY_EVENT_SOURCE_ENTITY_ID),
            at(facts::NOTIFY_EVENT_TYPE_GUID),
            at(facts::NOTIFY_EVENT_TYPE),
            at(facts::NOTIFY_EVENT_SOUND_VOLUME_HINT),
        );
        let notify_event = host.hash(facts::NOTIFY_EVENT.id()).unwrap();
        let loader = host.install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Chimes)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let effects = EFFECTS.get().unwrap();

        let here = Notify::new(BELL, NotifyAt::Position([1.0, 2.0, -3.0]))
            .sound_volume_hint(SoundVolumeHint::FriendlyNpc);
        effects.notify(&here).unwrap();
        effects
            .notify(&Notify::new(BELL, NotifyAt::Entity(Entity::from_engine(9))))
            .unwrap();
        let emits = loader.emits();
        let (ty, placed) = &emits[0];
        assert_eq!((*ty, placed.len()), (notify_event, 224));
        assert_eq!(word::<16>(placed, guid), BELL.0);
        assert_eq!((placed[kind], placed[hint]), (0, 4), "Normal, FriendlyNpc");
        assert_eq!(i64::from_le_bytes(word(placed, position)), 1 << 32);
        assert_eq!(i64::from_le_bytes(word(placed, position + 16)), -3 << 32);
        assert_eq!(u32::from_le_bytes(word(placed, source)), 0, "no source");
        for w in [turn + 12, orientation + 12, scale, scale + 4, scale + 8] {
            assert_eq!(word::<4>(placed, w), 1.0f32.to_le_bytes(), "at {w}");
        }
        let (_, on_entity) = &emits[1];
        assert_eq!(u32::from_le_bytes(word(on_entity, source)), 9);
        assert_eq!(i64::from_le_bytes(word(on_entity, position)), 0);

        assert_eq!(
            effects.notify(&Notify::new(BELL, NotifyAt::Entity(Entity::NONE))),
            Err(WorldError::BadArgument),
            "entity 0 names none"
        );
        assert_eq!(
            effects.notify(&Notify::new(BELL, NotifyAt::Position([f64::NAN, 0.0, 0.0]))),
            Err(WorldError::BadArgument),
            "a position that is not finite"
        );
        let off_thread = std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    (0..=MAX_NOTIFIES_PER_STEP)
                        .map(|_| effects.notify(&here))
                        .collect::<Vec<_>>()
                })
                .join()
                .unwrap()
        });
        assert!(
            off_thread
                .iter()
                .all(|r| *r == Err(WorldError::WrongThread)),
            "{off_thread:?}"
        );
        assert_eq!(loader.emits().len(), 2, "nothing refused was raised");

        for _ in 2..MAX_NOTIFIES_PER_STEP {
            effects.notify(&here).unwrap();
        }
        assert_eq!(
            effects.notify(&here),
            Err(WorldError::Refused),
            "one past the step's bound"
        );
        let _ran = loader.tick(Duration::from_millis(50));
        effects.notify(&here).unwrap();

        let mut vfx = placed.clone();
        vfx[kind] = 2;
        let mut sfx = on_entity.clone();
        sfx[kind] = 1;
        let mut unnamed = placed.clone();
        unnamed[kind] = 9;
        for bytes in [placed, &vfx, &sfx, &unnamed] {
            let _heard = loader.raise_game_event(notify_event, bytes);
        }
        loader.stop_world();
        assert_eq!(effects.notify(&here), Err(WorldError::Unavailable));
        let heard = HEARD.lock().unwrap();
        assert_eq!(heard.len(), 3, "the visual effect is not handed on");
        assert_eq!(heard[2].r#type, NotifyEventType::Other(9));
        assert_eq!(heard[0].r#type, NotifyEventType::Normal);
        assert_eq!(heard[0].type_guid, BELL);
        assert_eq!(heard[0].position, [1.0, 2.0, -3.0]);
        assert_eq!(heard[0].source_entity_id, None);
        assert_eq!(heard[0].sound_volume_hint, SoundVolumeHint::FriendlyNpc);
        assert_eq!(heard[1].r#type, NotifyEventType::Sfx);
        assert_eq!(heard[1].source_entity_id, Some(Entity::from_engine(9)));
        assert_eq!(heard[1].sound_volume_hint, SoundVolumeHint::GenericSfx);
    }

    #[test]
    fn a_build_without_what_raising_writes_still_hears_notifiers() {
        let host = FakeHost::new()
            .world()
            .effect_events()
            .without_fact(facts::OFFSET_ORIENTATION_OFFSET);
        let notify_event = host.hash(facts::NOTIFY_EVENT.id()).unwrap();
        let guid = host.offset(facts::NOTIFY_EVENT_TYPE_GUID.id()).unwrap();
        let loader = host.install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Chimes)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let effects = EFFECTS.get().unwrap();
        assert_eq!(
            effects.notify(&Notify::new(BELL, NotifyAt::Entity(Entity::from_engine(9)))),
            Err(WorldError::Unavailable),
            "raising needs the turn"
        );

        let mut bytes = vec![0u8; 224];
        bytes[guid..guid + 16].copy_from_slice(&BELL.0);
        let _heard = loader.raise_game_event(notify_event, &bytes);
        let heard = HEARD.lock().unwrap();
        assert_eq!(heard.len(), 1, "hearing needs no turn");
        assert_eq!(heard[0].type_guid, BELL);
    }
}
