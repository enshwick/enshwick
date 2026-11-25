//! Sounds a client mod plays and steers: an unknown container is refused,
//! a frame starts at most so many per mod, a handle of a sound that ended
//! changes nothing, and a mod turned off ends its sounds.

#[cfg(test)]
mod tests {
    use core::sync::atomic::{AtomicBool, Ordering};
    use core::time::Duration;

    use enshwick::abi::{Side, Status};
    use enshwick::content::{Guid, types};
    use enshwick::sound::{self, Place, Play, SoundContainerId, SoundState};
    use enshwick::test::{FakeHost, Installed};
    use enshwick::{Entity, Load, Mod, Off};

    const BELL: SoundContainerId = SoundContainerId::from_guid(Guid::named("you.bells:bell"));
    const TYPO: SoundContainerId = SoundContainerId::from_guid(Guid::named("you.bells:bel"));
    const HASH: u32 = 0xf257_640c;

    static FAULT: AtomicBool = AtomicBool::new(false);

    struct Bells;

    impl Mod for Bells {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            if load.host().build().side == Side::CLIENT {
                load.on_frame(|_| {
                    assert!(!FAULT.load(Ordering::Relaxed), "the bell mod faults");
                })?;
            }
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Bells,
        r#"
        id = "you.bells"
        side = "both"
        "#
    );

    fn client() -> Installed {
        installed(Side::CLIENT)
    }

    fn installed(side: Side) -> Installed {
        let loader = FakeHost::new()
            .side(side)
            .type_hash(types::SOUND_CONTAINER_RESOURCE.id(), HASH)
            .resource(BELL.guid().key_by_hash(HASH), Vec::new())
            .install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Bells)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        loader
    }

    #[test]
    fn a_known_container_plays_at_the_next_frame_and_an_unknown_one_is_refused() {
        let loader = client();
        let bell = sound::play(Play::new(BELL, Place::At([10.0, 0.0, 0.0])).volume(0.5)).unwrap();
        assert_eq!(bell.state(), SoundState::Pending);
        let refused = sound::play(Play::new(TYPO, Place::Screen)).unwrap_err();
        assert!(
            refused.to_string().contains(&TYPO.guid().to_string()),
            "{refused}"
        );
        let _drawn = loader.draw_frame();
        assert_eq!(bell.state(), SoundState::Playing);
        let [played] = loader.sounds()[..] else {
            panic!("one sound plays");
        };
        assert_eq!(played.place, Place::At([10.0, 0.0, 0.0]));
        assert_eq!((played.container, played.volume), (BELL.guid(), 0.5));
    }

    #[test]
    fn a_frame_starts_at_most_the_bound_of_one_mod_s_sounds() {
        let loader = client();
        for _ in 0..sound::MAX_STARTS {
            sound::play(Play::new(BELL, Place::Screen)).unwrap();
        }
        assert!(sound::play(Play::new(BELL, Place::Screen)).is_err());
        let _drawn = loader.draw_frame();
        assert!(sound::play(Play::new(BELL, Place::Screen)).is_ok());
    }

    #[test]
    fn out_of_bounds_arguments_are_refused() {
        let _loader = client();
        assert!(sound::play(Play::new(BELL, Place::Screen).volume(1.5)).is_err());
        assert!(sound::play(Play::new(BELL, Place::On(Entity::NONE))).is_err());
        for far in [
            [f64::NAN; 3],
            [1e300, 0.0, 0.0],
            [0.0, -2_147_483_648.0, 0.0],
        ] {
            assert!(
                sound::play(Play::new(BELL, Place::At(far))).is_err(),
                "{far:?}"
            );
        }
        let edge = sound::play(Play::new(BELL, Place::At([2_147_483_647.0, 0.0, 0.0]))).unwrap();
        assert!(edge.set_position([0.0, 0.0, 2_147_483_648.0]).is_err());
        assert!(edge.set_position([0.0, 0.0, -2_147_483_647.0]).is_ok());
        let long =
            Play::new(BELL, Place::Music).fade_in(sound::MAX_FADE + Duration::from_millis(1));
        assert!(sound::play(long).is_err());
    }

    #[test]
    fn a_sound_on_the_screen_or_of_music_stays_there_when_moved() {
        let loader = client();
        let bell = sound::play(Play::new(BELL, Place::Screen)).unwrap();
        let tune = sound::play(Play::new(BELL, Place::Music)).unwrap();
        let _drawn = loader.draw_frame();
        bell.set_position([1.0, 2.0, 3.0]).unwrap();
        tune.set_position([1.0, 2.0, 3.0]).unwrap();
        let places: Vec<Place> = loader.sounds().iter().map(|sound| sound.place).collect();
        assert_eq!(places, [Place::Screen, Place::Music]);
    }

    #[test]
    fn a_stopped_handle_reads_ended_and_changes_nothing_after() {
        let loader = client();
        let bell = sound::play(Play::new(BELL, Place::Screen)).unwrap();
        let _drawn = loader.draw_frame();
        bell.stop(Duration::from_millis(200)).unwrap();
        assert_eq!(bell.state(), SoundState::Ended);
        assert!(!bell.is_playing());
        bell.set_volume(0.1, Duration::ZERO).unwrap();
        bell.set_position([1.0, 2.0, 3.0]).unwrap();
        let [ended] = loader.sounds()[..] else {
            panic!("one sound was started");
        };
        assert!(!ended.playing);
        assert_eq!((ended.place, ended.volume), (Place::Screen, 1.0));
    }

    #[test]
    fn stop_all_ends_every_sound_of_the_mod() {
        let loader = client();
        let first = sound::play(Play::new(BELL, Place::Music)).unwrap();
        let second = sound::play(Play::new(BELL, Place::Screen)).unwrap();
        let _drawn = loader.draw_frame();
        sound::stop_all(Duration::ZERO).unwrap();
        assert_eq!(
            (first.state(), second.state()),
            (SoundState::Ended, SoundState::Ended)
        );
    }

    #[test]
    fn a_mod_turned_off_ends_its_sounds() {
        let loader = client();
        let bell = sound::play(Play::new(BELL, Place::Music)).unwrap();
        let _drawn = loader.draw_frame();
        FAULT.store(true, Ordering::Relaxed);
        let _faulted = loader.draw_frame();
        assert!(loader.is_off());
        assert_eq!(bell.state(), SoundState::Ended);
    }

    #[test]
    fn a_server_plays_no_sound() {
        let _loader = installed(Side::SERVER);
        assert!(sound::play(Play::new(BELL, Place::Screen)).is_err());
        assert!(sound::stop_all(Duration::ZERO).is_err());
    }
}
