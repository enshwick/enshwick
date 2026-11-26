//! The prompt a mod gives the objects placed from its item: a game verb,
//! its own text and the replaced action.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{InteractionPrompt, Status};
    use enshwick::interactions::{ObjectPrompt, Verb};
    use enshwick::test::FakeHost;
    use enshwick::{ItemId, Load, Mod, Off};

    const ALTAR: ItemId = ItemId::new(0x5e01);
    const LAMP: ItemId = ItemId::new(0x5e02);

    /// What each ask after the first answered: a second prompt on the altar,
    /// and a prompt that changes nothing.
    static REFUSED: Mutex<Vec<bool>> = Mutex::new(Vec::new());

    struct Shrine;

    impl Mod for Shrine {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let pray = load.declare(enshwick::content::id!("pray"))?;
            load.prompt(
                ALTAR,
                ObjectPrompt::new()
                    .verb(Verb::Use)
                    .text(pray)
                    .replace_action(),
            )?;
            load.prompt(LAMP, ObjectPrompt::new().verb(Verb::Activate))?;
            let again = load.prompt(ALTAR, ObjectPrompt::new().verb(Verb::Open));
            let nothing = load.prompt(ItemId::new(0x5e03), ObjectPrompt::new());
            REFUSED
                .lock()
                .unwrap()
                .extend([again.is_err(), nothing.is_err()]);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Shrine,
        r#"
        id = "you.shrine"
        side = "both"
        "#
    );

    #[test]
    fn a_mod_gives_its_items_a_verb_its_text_and_the_empty_action() {
        let loader = FakeHost::new().install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Shrine)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let altar = loader.prompt(ALTAR).unwrap();
        assert_eq!(altar.verb, Verb::Use.id());
        assert_ne!(altar.text, 0, "the declared tag's id");
        assert_eq!(altar.flags, InteractionPrompt::REPLACE_ACTION);
        let lamp = loader.prompt(LAMP).unwrap();
        assert_eq!(
            (lamp.verb, lamp.text, lamp.flags),
            (Verb::Activate.id(), 0, 0)
        );
        assert_eq!(loader.prompt(ItemId::new(0x5e03)), None);
    }

    #[test]
    fn a_second_prompt_on_an_item_and_an_empty_prompt_are_refused() {
        let loader = FakeHost::new().install();
        // Under `cargo test` the other test's init may have run first, in
        // this process.
        let before = REFUSED.lock().unwrap().len();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Shrine)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(REFUSED.lock().unwrap()[before..], [true, true]);
        assert_eq!(
            loader.prompt(ALTAR).map(|p| p.verb),
            Some(Verb::Use.id()),
            "the first prompt stays"
        );
    }

    #[test]
    fn any_prompt_after_init_is_too_late_even_a_malformed_one() {
        let loader = FakeHost::new().install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Shrine)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        // SAFETY: the fake's table lives for the process.
        let api = unsafe { &*loader.api() };
        let empty = enshwick::abi::headed!(InteractionPrompt {
            item: ItemId::new(0),
            verb: 0,
            text: 0,
            flags: 0,
        });
        assert_eq!(
            (api.interaction_prompt)(loader.handle(), &raw const empty),
            Status::TOO_LATE,
            "as the loader, which checks the time before the spec"
        );
    }
}
