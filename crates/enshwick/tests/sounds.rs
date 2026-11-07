//! Sound containers a client plays: one the game's archive holds plays, and
//! any other is refused before it reaches the game.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{Side, Status};
    use enshwick::content::{Guid, types};
    use enshwick::sound::SoundContainerId;
    use enshwick::test::{Drawn, FakeHost};
    use enshwick::{Load, Mod, Off};

    const HELD: Guid = Guid::named("you.bells:bell");
    const TYPO: Guid = Guid::named("you.bells:bel");
    const HASH: u32 = 0xf257_640c;

    static PLAYED: Mutex<Vec<Result<(), String>>> = Mutex::new(Vec::new());

    struct Bells;

    impl Mod for Bells {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.on_frame(|frame| {
                for guid in [HELD, TYPO] {
                    let played = frame.play(SoundContainerId::from_guid(guid));
                    PLAYED
                        .lock()
                        .unwrap()
                        .push(played.map_err(|off| off.to_string()));
                }
            })?;
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

    #[test]
    fn a_container_the_archive_holds_plays_and_any_other_guid_is_refused() {
        let loader = FakeHost::new()
            .side(Side::CLIENT)
            .type_hash(types::SOUND_CONTAINER_RESOURCE.id(), HASH)
            .resource(HELD.key_by_hash(HASH), Vec::new())
            .resource(TYPO.key_by_hash(HASH + 1), Vec::new())
            .install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Bells)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(loader.draw_frame(), [Drawn::Play(HELD)]);
        let played = PLAYED.lock().unwrap();
        assert_eq!(played[0], Ok(()));
        let refused = played[1].as_ref().unwrap_err();
        assert!(
            refused.contains(&TYPO.to_string()),
            "a resource of another type is no sound container: {refused}"
        );
    }
}
