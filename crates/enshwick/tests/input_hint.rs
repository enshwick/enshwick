//! A client mod draws the key the player bound to a game action, and the
//! loader refuses an action the build lacks and a hint outside a window.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{Side, Status};
    use enshwick::test::Drawn;
    use enshwick::typed::input_mapping::runtime::GameplayAction;
    use enshwick::{Load, Mod, Off};

    /// Each frame's answers: the bound hint, the unbound one, the one the
    /// build lacks and the one outside the window.
    type Answers = [Result<bool, ()>; 4];

    static ANSWERS: Mutex<Vec<Answers>> = Mutex::new(Vec::new());

    struct Hinter;

    impl Mod for Hinter {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.on_frame(|frame| {
                let mut answers = [Err(()); 4];
                frame.window("help", "Help", |frame| {
                    answers[0] = frame
                        .input_hint(&GameplayAction::ContextualAction, "Open")
                        .map_err(drop);
                    answers[1] = frame
                        .input_hint(&GameplayAction::DropItem, "")
                        .map_err(drop);
                    answers[2] = frame.input_hint(&GameplayAction::Sneak, "").map_err(drop);
                });
                answers[3] = frame
                    .input_hint(&GameplayAction::ContextualAction, "")
                    .map_err(drop);
                ANSWERS.lock().unwrap().push(answers);
            })?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Hinter,
        r#"
        id = "you.hinter"
        side = "both"
        "#
    );

    #[test]
    fn a_hint_draws_a_bound_key_reports_an_unbound_one_and_refuses_what_the_build_lacks() {
        let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Hinter)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        loader.game_action("ContextualAction", true);
        loader.game_action("DropItem", false);
        let drawn = loader.draw_frame();
        assert_eq!(
            ANSWERS.lock().unwrap().as_slice(),
            [[Ok(true), Ok(false), Err(()), Err(())]],
            "bound, unbound, unnamed on this build, outside the window"
        );
        let hints: Vec<&Drawn> = drawn
            .iter()
            .filter(|d| matches!(d, Drawn::InputHint { .. }))
            .collect();
        assert_eq!(
            hints,
            [
                &Drawn::InputHint {
                    action: "ContextualAction".into(),
                    label: "Open".into(),
                    bound: true,
                },
                &Drawn::InputHint {
                    action: "DropItem".into(),
                    label: String::new(),
                    bound: false,
                },
            ]
        );
    }
}
