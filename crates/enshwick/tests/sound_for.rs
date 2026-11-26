//! Sounds a server mod plays on clients: the audience picks modded clients
//! by distance, a step sends at most so many per mod, a change reaches the
//! sound's clients, and a client drops a sound whose container it lacks.

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use enshwick::abi::{PlayerInfo, Side, SoundPlay, Status};
    use enshwick::content::{Guid, types};
    use enshwick::net::Peer;
    use enshwick::sound::{self, Audience, MAX_PLAYS, Place, Play, SoundContainerId};
    use enshwick::test::{FakeHost, Installed};
    use enshwick::{Entity, Load, Mod, Off, PlayerKey};

    const HORN: SoundContainerId = SoundContainerId::from_guid(Guid::named("you.horns:horn"));
    const HASH: u32 = 0xf257_640c;
    /// A camp entity, and three players: two with Enshwick near it and far
    /// from it, one vanilla client beside it.
    const CAMP: Entity = Entity::from_engine(4096);
    const NEAR: PlayerKey = PlayerKey::new(1);
    const FAR: PlayerKey = PlayerKey::new(2);
    const VANILLA: PlayerKey = PlayerKey::new(3);
    /// A data pack of side server beside the mod.
    const ONE_SIDED: enshwick::abi::ModHandle = enshwick::abi::ModHandle(99);

    struct Horns;

    impl Mod for Horns {
        fn init(_load: &mut Load<'_>) -> Result<Self, Off> {
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Horns,
        r#"
        id = "you.horns"
        side = "both"
        "#
    );

    fn player(key: PlayerKey, entity: u32) -> PlayerInfo {
        enshwick::abi::headed!(PlayerInfo {
            slot: entity,
            entity: Entity::from_engine(entity),
            key,
            permissions: 0,
            name_len: 0,
            name: [0; 128],
        })
    }

    fn installed(fake: FakeHost) -> Installed {
        let loader = fake
            .type_hash(types::SOUND_CONTAINER_RESOURCE.id(), HASH)
            .resource(HORN.guid().key_by_hash(HASH), Vec::new())
            .install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Horns)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        loader
    }

    fn server() -> Installed {
        installed(
            FakeHost::new()
                .side(Side::SERVER)
                .pack(ONE_SIDED, Side::SERVER)
                .player(player(NEAR, 10))
                .player(player(FAR, 11))
                .player(player(VANILLA, 12))
                .net_peer(Peer::Player(NEAR))
                .net_peer(Peer::Player(FAR))
                .placed(0, Entity::from_engine(10), [3.0, 0.0, 4.0])
                .placed(0, Entity::from_engine(11), [60.0, 0.0, 0.0])
                .placed(0, Entity::from_engine(12), [1.0, 0.0, 0.0])
                .placed(0, CAMP, [0.0, 0.0, 0.0]),
        )
    }

    #[test]
    fn a_near_sound_goes_only_to_modded_players_within_its_radius() -> Result<(), Off> {
        let loader = server();
        let near = Audience::Near {
            at: [0.0; 3],
            radius: Some(5.0),
        };
        sound::play_for(near, Play::new(HORN, Place::On(CAMP)))?;
        sound::play_for(Audience::Everyone, Play::new(HORN, Place::Screen))?;
        sound::play_for(Audience::Player(FAR), Play::new(HORN, Place::Music))?;
        let sent = loader.sent_sounds();
        let to: Vec<&[PlayerKey]> = sent.iter().map(|(to, _)| to.as_slice()).collect();
        assert_eq!(to, [&[NEAR][..], &[NEAR, FAR], &[FAR]]);
        assert_eq!(
            sent[0].1.place,
            Place::At([0.0; 3]),
            "an entity's sound plays where it stood"
        );
        Ok(())
    }

    #[test]
    fn a_near_audience_or_a_place_past_the_fixed_point_is_refused() {
        let loader = server();
        for far in [
            [f64::NAN, 0.0, 0.0],
            [0.0, 1e300, 0.0],
            [2_147_483_648.0, 0.0, 0.0],
        ] {
            let near = Audience::Near {
                at: far,
                radius: Some(5.0),
            };
            assert!(
                sound::play_for(near, Play::new(HORN, Place::Screen)).is_err(),
                "centre {far:?}"
            );
            assert!(
                sound::play_for(Audience::Everyone, Play::new(HORN, Place::At(far))).is_err(),
                "place {far:?}"
            );
        }
        assert!(loader.sent_sounds().is_empty());
    }

    #[test]
    fn a_change_reaches_the_sound_on_its_clients() -> Result<(), Off> {
        let loader = server();
        let horn = sound::play_for(Audience::Everyone, Play::new(HORN, Place::Screen))?;
        horn.set_volume(0.25, Duration::ZERO)?;
        horn.stop(Duration::from_millis(400))?;
        let [(_, played)] = &loader.sent_sounds()[..] else {
            panic!("one sound went");
        };
        assert_eq!((played.volume, played.playing), (0.25, false));
        Ok(())
    }

    #[test]
    fn a_mod_sends_at_most_so_many_sounds_in_one_server_step() -> Result<(), Off> {
        let loader = server();
        let horn = || sound::play_for(Audience::Everyone, Play::new(HORN, Place::Screen));
        for _ in 0..MAX_PLAYS {
            horn()?;
        }
        assert!(horn().is_err(), "past the step's bound");
        let _ran = loader.tick(Duration::from_millis(50));
        horn()?;
        assert_eq!(loader.sent_sounds().len(), MAX_PLAYS as usize + 1);
        Ok(())
    }

    #[test]
    fn a_near_audience_off_the_game_thread_is_refused() {
        let loader = server();
        let near = Audience::Near {
            at: [0.0; 3],
            radius: None,
        };
        let sent =
            std::thread::spawn(move || sound::play_for(near, Play::new(HORN, Place::Screen)))
                .join()
                .expect("the thread ends");
        assert!(sent.is_err());
        assert!(loader.sent_sounds().is_empty());
    }

    #[test]
    fn a_change_from_a_mod_not_on_both_sides_is_refused() {
        let loader = server();
        // SAFETY: the fake's table lives for the process.
        let api = unsafe { &*loader.api() };
        let stop = enshwick::abi::SoundSteer::EMPTY;
        assert_eq!(
            (api.sound_steer_for)(ONE_SIDED, 1, &raw const stop),
            Status::REFUSED
        );
    }

    #[test]
    fn a_client_that_hosts_nothing_refuses_a_sound_for_clients_before_its_bounds() {
        let loader = installed(FakeHost::new().side(Side::CLIENT));
        // SAFETY: the fake's table lives for the process.
        let api = unsafe { &*loader.api() };
        let beyond = enshwick::abi::headed!(enshwick::abi::SoundAudience {
            kind: enshwick::abi::SoundAudience::NEAR,
            radius: f32::MAX,
            ..enshwick::abi::SoundAudience::EMPTY
        });
        let (play, mut id) = (SoundPlay::EMPTY, 0);
        let status = (api.sound_play_for)(
            loader.handle(),
            &raw const beyond,
            &raw const play,
            &raw mut id,
        );
        assert_eq!(status, Status::UNAVAILABLE);
    }

    #[test]
    fn a_client_that_hosts_nothing_sends_no_sound_to_clients() {
        let loader = installed(FakeHost::new().side(Side::CLIENT));
        let sent = sound::play_for(Audience::Everyone, Play::new(HORN, Place::Screen));
        assert!(sent.is_err());
        assert!(loader.sent_sounds().is_empty());
    }

    #[test]
    fn a_client_drops_a_server_sound_whose_container_it_lacks() {
        let loader = installed(FakeHost::new().side(Side::CLIENT));
        let unknown = enshwick::abi::headed!(SoundPlay {
            container: Guid::named("you.horns:hron"),
            ..SoundPlay::EMPTY
        });
        assert!(!loader.hear_sound(unknown));
        assert!(loader.sounds().is_empty());
        let known = enshwick::abi::headed!(SoundPlay {
            container: HORN.guid(),
            ..SoundPlay::EMPTY
        });
        assert!(loader.hear_sound(known));
        assert_eq!(loader.sounds().len(), 1);
    }
}
