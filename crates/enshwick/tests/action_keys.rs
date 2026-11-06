//! An action on a key in one call: the control shows the action's label in
//! the player's language and runs the action at each press.

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use enshwick::abi::{Side, Status};
    use enshwick::actions::{Action, Arg, Outcome};
    use enshwick::controls::Key;
    use enshwick::i18n::{Language, Texts};
    use enshwick::{Load, Mod, Off};

    const TEXTS: &str = "[en]\n\"day\" = \"Make it day\"\n\n[uk]\n\"day\" = \"Зробити день\"\n";

    struct Weather;

    impl Mod for Weather {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            // Given to the actions alone, never handed to the loader by the mod.
            let actions = load.actions(Texts::parse(TEXTS)?)?;
            actions.add(Action::new("day", "day").admins(), |_| {
                Outcome::done("Morning")
            })?;
            actions.add(
                Action::new("rain", "rain").arg(Arg::number("Minutes", 1.0..=60.0, 1.0)),
                |_| Outcome::done("Rain"),
            )?;
            actions.control("day", Key::parse("F8")?)?;
            let refused = [
                actions.control("night", Key::parse("F9")?),
                actions.control("rain", Key::parse("F10")?),
            ];
            if refused.iter().any(Result::is_ok) {
                return Err(Off::new("a key took an undeclared action or arguments"));
            }
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Weather,
        r#"
        id = "you.weather"
        side = "server"
        "#
    );

    #[test]
    fn the_key_shows_the_action_s_label_in_the_player_s_language_and_runs_it() {
        let loader = enshwick::test::FakeHost::new()
            .side(Side::CLIENT)
            .hosting(true)
            .language(Language::UK_UA)
            .install();
        // SAFETY: the exports of this file's `declare_mod!(Weather)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let f8 = Key::parse("F8").unwrap();
        assert_eq!(
            loader.controls(),
            [(
                "day".to_owned(),
                "Зробити день".to_owned(),
                [Some(f8), None]
            )]
        );
        assert_eq!(loader.keys(&[f8], Duration::from_millis(16)), 1);
        assert_eq!(loader.action_runs(), [("day".to_owned(), String::new())]);
    }
}
