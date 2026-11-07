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

    const TEXTS: &str = "[en]\n\"day\" = \"Make it day\"\n\"dusk\" = \"Dusk\"\n\n[uk]\n\"day\" = \"Зробити день\"\n\"dusk\" = \"Сутінки\"\n\n[pt]\n\"dusk\" = \"Crepúsculo\"\n";

    struct Weather;

    impl Mod for Weather {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let actions = load.actions(Texts::parse(TEXTS)?)?;
            // A text the mod hands after its actions stays over theirs.
            load.texts_from("[uk]\n\"day\" = \"Хай буде день\"\n")?;
            actions.add(Action::new("day", "day").admins(), |_| {
                Outcome::done("Morning")
            })?;
            actions.add(
                Action::new("rain", "rain").arg(Arg::number("Minutes", 1.0..=60.0, 1.0)),
                |_| Outcome::done("Rain"),
            )?;
            actions.add(
                Action::per_player("friends", "friends", |_, _| Vec::new()),
                |_| Outcome::done("Friends"),
            )?;
            // Its label is in the actions' texts alone.
            actions.add(Action::new("dusk", "dusk").admins(), |_| {
                Outcome::done("Evening")
            })?;
            actions.control("day", Key::parse("F8")?)?;
            actions.control("dusk", Key::parse("F7")?)?;
            let refused = [
                actions.control("night", Key::parse("F9")?),
                actions.control("rain", Key::parse("F10")?),
                actions.control("friends", Key::parse("F11")?),
            ];
            if refused.iter().any(Result::is_ok) {
                return Err(Off::new(
                    "a key took an undeclared action, arguments or a list of players",
                ));
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

    fn start(language: Language) -> enshwick::test::Installed {
        let loader = enshwick::test::FakeHost::new()
            .side(Side::CLIENT)
            .hosting(true)
            .language(language)
            .install();
        // SAFETY: the exports of this file's `declare_mod!(Weather)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    #[test]
    fn a_label_only_in_a_parent_or_the_fallback_language_reads_from_there() {
        let labels: Vec<String> = start(Language::PT_BR)
            .controls()
            .into_iter()
            .map(|(_, label, _)| label)
            .collect();
        assert_eq!(labels, ["Make it day", "Crepúsculo"]);
    }

    #[test]
    fn the_key_shows_the_action_s_label_in_the_player_s_language_and_runs_it() {
        let loader = start(Language::UK_UA);
        let f8 = Key::parse("F8").unwrap();
        let f7 = Key::parse("F7").unwrap();
        assert_eq!(
            loader.controls(),
            [
                (
                    "day".to_owned(),
                    "Хай буде день".to_owned(),
                    [Some(f8), None]
                ),
                ("dusk".to_owned(), "Сутінки".to_owned(), [Some(f7), None])
            ]
        );
        assert_eq!(loader.keys(&[f8], Duration::from_millis(16)), 1);
        assert_eq!(loader.action_runs(), [("day".to_owned(), String::new())]);
    }
}
