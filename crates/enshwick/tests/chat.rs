//! Chat filters and the lines mods hear, as the loader hands them over.

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};
    use std::time::Duration;

    use enshwick::abi::Status;
    use enshwick::chat::{ChatKind, ChatVerdict};
    use enshwick::world::World;
    use enshwick::{Load, Mod, Off, PlayerKey};

    static WORLD: OnceLock<World> = OnceLock::new();
    /// What the second filter saw.
    static SEEN: Mutex<Vec<String>> = Mutex::new(Vec::new());
    /// What the handler heard.
    static HEARD: Mutex<Vec<(ChatKind, String)>> = Mutex::new(Vec::new());

    struct Moderator;

    impl Mod for Moderator {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = WORLD.set(load.world()?);
            load.on_chat(|line| match line.text.as_str() {
                "drop me" => ChatVerdict::Drop,
                "erase me" => ChatVerdict::Rewrite(String::new()),
                "panic" => panic!("a broken filter"),
                text => ChatVerdict::Rewrite(text.replace("bad", "good")),
            })?;
            load.on_chat(|line| {
                SEEN.lock().unwrap().push(line.text.clone());
                ChatVerdict::Keep
            })?;
            load.on_chat_said(|line| HEARD.lock().unwrap().push((line.kind, line.text.clone())))?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Moderator,
        r#"
        id = "you.moderator"
        side = "server"
        "#
    );

    #[test]
    fn filters_drop_rewrite_and_chain_and_a_muted_or_broken_line_is_handled_as_the_loader_does() {
        let loader = enshwick::test::FakeHost::new()
            .world()
            .world_key("world0")
            .install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Moderator)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let ann = PlayerKey::new(7);

        assert_eq!(loader.say(ann, "a bad day").as_deref(), Some("a good day"));
        assert_eq!(
            *SEEN.lock().unwrap(),
            ["a good day"],
            "the next filter sees the rewrite"
        );
        assert_eq!(
            *HEARD.lock().unwrap(),
            [(ChatKind::Player, "a good day".to_owned())],
            "the handler hears the line as the filters left it"
        );

        assert_eq!(loader.say(ann, "drop me"), None);
        assert_eq!(
            SEEN.lock().unwrap().len(),
            1,
            "a drop stops the filters after it"
        );
        assert_eq!(HEARD.lock().unwrap().len(), 1, "and no handler hears it");
        assert_eq!(loader.say(ann, "erase me"), None, "an empty rewrite drops");
        assert_eq!(SEEN.lock().unwrap().len(), 1);
        assert_eq!(HEARD.lock().unwrap().len(), 1);

        WORLD.get().unwrap().set_muted(ann, true).unwrap();
        assert_eq!(loader.say(ann, "a bad day"), None);
        assert_eq!(
            SEEN.lock().unwrap().len(),
            1,
            "a muted line reaches no filter"
        );
        WORLD.get().unwrap().set_muted(ann, false).unwrap();

        assert_eq!(
            loader.say(ann, "panic").as_deref(),
            Some("panic"),
            "the line passes"
        );
        assert_eq!(
            loader.say(ann, "bad").as_deref(),
            Some("bad"),
            "the panicking filter is off, and only it"
        );
        assert_eq!(SEEN.lock().unwrap().last().map(String::as_str), Some("bad"));
    }

    #[test]
    fn lines_past_eight_a_second_reach_the_filters_but_no_handler_until_the_next_second() {
        let loader = enshwick::test::FakeHost::new().world().install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Moderator)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let (ann, bob) = (PlayerKey::new(7), PlayerKey::new(8));
        let (seen, heard) = (SEEN.lock().unwrap().len(), HEARD.lock().unwrap().len());

        for _ in 0..9 {
            assert_eq!(loader.say(ann, "hi").as_deref(), Some("hi"));
        }
        assert_eq!(
            SEEN.lock().unwrap().len(),
            seen + 9,
            "every line is filtered"
        );
        assert_eq!(
            HEARD.lock().unwrap().len(),
            heard + 8,
            "the ninth is not heard"
        );

        let _ran = loader.tick(Duration::from_millis(999));
        let _said = loader.say(ann, "hi");
        let _said = loader.say(bob, "hi");
        assert_eq!(
            HEARD.lock().unwrap().len(),
            heard + 9,
            "another account has a rate of its own"
        );
        let _ran = loader.tick(Duration::from_millis(1));
        let _said = loader.say(ann, "hi");
        assert_eq!(
            HEARD.lock().unwrap().len(),
            heard + 10,
            "a new second starts afresh"
        );
    }
}
