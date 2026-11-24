//! Client commands through the safe layer on the fake host.
#[cfg(test)]
mod tests {
    use enshwick::abi::{ClientCommand as Raw, ClientCommandStatus as Status, Side};
    use enshwick::controls::GameAction;
    use enshwick::input::{
        CancelReason, ClientCommand, CommandAction, CommandState, Input, JoinFailure, JoinPhase,
        Password,
    };
    use enshwick::{Load, Mod, Off, WorldError};
    use std::sync::Mutex;

    static INPUT: Mutex<Option<Input>> = Mutex::new(None);

    struct Driver;

    impl Mod for Driver {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            *INPUT.lock().unwrap() = Some(load.input()?);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Driver,
        r#"id = "you.client-commands"
side = "both""#
    );

    fn input(side: Side) -> (enshwick::test::Installed, Input) {
        let loader = enshwick::test::FakeHost::new()
            .side(side)
            .player_input()
            .install();
        // SAFETY: these are this file's generated exports.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, enshwick::abi::Status::OK);
        (loader, INPUT.lock().unwrap().unwrap())
    }

    fn open_backpack() -> ClientCommand {
        ClientCommand::new(CommandAction::OpenScreen("Backpack".to_owned()), 1).on_screen("InGame")
    }

    #[test]
    fn a_command_goes_from_queued_to_applied_to_observed() {
        let (loader, input) = input(Side::CLIENT);
        let id = input.client_command(&open_backpack()).unwrap();
        assert_eq!(
            loader.client_commands(),
            [(1, Raw::OPEN_SCREEN, "Backpack".to_owned())]
        );
        assert_eq!(
            input.command_status(id).unwrap().state,
            CommandState::Pending
        );
        loader.set_command_status(1, Status::APPLIED, 0);
        assert_eq!(
            input.command_status(id).unwrap().state,
            CommandState::Applied
        );
        loader.set_command_status(1, Status::OBSERVED, 0);
        let observed = input.command_status(id).unwrap().state;
        assert_eq!(observed, CommandState::Observed);
    }

    #[test]
    fn one_command_of_a_kind_waits_and_a_cancelled_one_frees_its_kind() {
        let (_loader, input) = input(Side::CLIENT);
        let id = input.client_command(&open_backpack()).unwrap();
        assert!(matches!(
            input.client_command(&open_backpack()),
            Err(WorldError::Refused)
        ));
        let tab = ClientCommand::new(CommandAction::ChooseTab("Character".to_owned()), 1);
        assert!(input.client_command(&tab).is_ok(), "another kind");
        input.cancel_command(id).unwrap();
        assert_eq!(
            input.command_status(id).unwrap().state,
            CommandState::Cancelled(CancelReason::ByMod)
        );
        assert!(input.client_command(&open_backpack()).is_ok());
    }

    #[test]
    fn each_cancel_reason_and_unavailable_reach_the_mod() {
        let (loader, input) = input(Side::CLIENT);
        for (reason, expected) in [
            (Status::SESSION_CHANGED, CancelReason::SessionChanged),
            (Status::OTHER_SCREEN, CancelReason::OtherScreen),
            (Status::PLAYER_INPUT, CancelReason::PlayerInput),
            (Status::TIMED_OUT, CancelReason::TimedOut),
            (Status::MOD_OFF, CancelReason::ModOff),
        ] {
            let id = input.client_command(&open_backpack()).unwrap();
            let raw = loader.client_commands().last().unwrap().0;
            loader.set_command_status(raw, Status::CANCELLED, reason);
            assert_eq!(
                input.command_status(id).unwrap().state,
                CommandState::Cancelled(expected)
            );
        }
        let id = input.client_command(&open_backpack()).unwrap();
        let raw = loader.client_commands().last().unwrap().0;
        loader.set_command_status(raw, Status::UNAVAILABLE, 0);
        assert_eq!(
            input.command_status(id).unwrap().state,
            CommandState::Unavailable
        );
    }

    fn walk(hold_ms: u32) -> ClientCommand {
        let action = CommandAction::Move {
            direction: [0.0, 1.0],
            hold_ms,
        };
        ClientCommand::new(action, 1)
    }

    fn look(yaw: f32) -> ClientCommand {
        ClientCommand::new(CommandAction::Look { yaw, pitch: 0.0 }, 1)
    }

    #[test]
    fn a_move_holds_until_observed_and_frees_its_kind_once_it_ends() {
        let (loader, input) = input(Side::CLIENT);
        let id = input.client_command(&walk(250)).unwrap();
        assert_eq!(loader.client_commands(), [(1, Raw::MOVE, String::new())]);
        loader.set_command_status(1, Status::APPLIED, 0);
        assert_eq!(
            input.command_status(id).unwrap().state,
            CommandState::Applied,
            "the builder holds the stick"
        );
        assert!(
            matches!(input.client_command(&walk(100)), Err(WorldError::Refused)),
            "one move at a time"
        );
        loader.set_command_status(1, Status::OBSERVED, 0);
        assert_eq!(
            input.command_status(id).unwrap().state,
            CommandState::Observed,
            "the hold ran out"
        );
        assert!(input.client_command(&walk(100)).is_ok(), "released");
    }

    #[test]
    fn a_look_or_move_past_its_bounds_is_refused() {
        let (_loader, input) = input(Side::CLIENT);
        for refused in [
            walk(251),
            walk(0),
            look(Raw::MAX_LOOK * 2.0),
            look(f32::NAN),
            ClientCommand::new(
                CommandAction::Move {
                    direction: [1.0, 1.0],
                    hold_ms: 100,
                },
                1,
            ),
        ] {
            assert!(
                matches!(input.client_command(&refused), Err(WorldError::Refused)),
                "{refused:?}"
            );
        }
        assert!(input.client_command(&look(Raw::MAX_LOOK)).is_ok());
    }

    #[test]
    fn a_screen_tab_or_expected_screen_the_loader_refuses_the_fake_refuses_too() {
        let (_loader, input) = input(Side::CLIENT);
        for refused in [
            ClientCommand::new(CommandAction::OpenScreen("StorageChest".to_owned()), 1),
            ClientCommand::new(CommandAction::ChooseTab("Inventory".to_owned()), 1),
            look(0.5).on_screen("Backpack"),
        ] {
            assert!(
                matches!(input.client_command(&refused), Err(WorldError::Refused)),
                "{refused:?}"
            );
        }
        assert!(input.client_command(&look(0.5).on_screen("InGame")).is_ok());
    }

    #[test]
    fn the_player_s_input_and_a_block_cancel_a_look_or_move() {
        let (loader, input) = input(Side::CLIENT);
        for (command, reason, expected) in [
            (walk(250), Status::PLAYER_INPUT, CancelReason::PlayerInput),
            (look(0.5), Status::PLAYER_INPUT, CancelReason::PlayerInput),
            (walk(250), Status::INPUT_BLOCKED, CancelReason::InputBlocked),
            (look(0.5), Status::OTHER_SCREEN, CancelReason::OtherScreen),
            (
                walk(250),
                Status::SESSION_CHANGED,
                CancelReason::SessionChanged,
            ),
            (look(0.5), Status::MOD_OFF, CancelReason::ModOff),
        ] {
            let id = input.client_command(&command).unwrap();
            let raw = loader.client_commands().last().unwrap().0;
            loader.set_command_status(raw, Status::CANCELLED, reason);
            assert_eq!(
                input.command_status(id).unwrap().state,
                CommandState::Cancelled(expected),
                "{command:?}"
            );
        }
    }

    fn press(action: GameAction, hold_ms: u32) -> ClientCommand {
        ClientCommand::new(CommandAction::Press { action, hold_ms }, 1)
    }

    #[test]
    fn a_press_holds_until_observed_beside_a_move_and_another_press() {
        let (loader, input) = input(Side::CLIENT);
        let block = input
            .client_command(&press(GameAction::Block, 2000))
            .unwrap();
        let attack = input
            .client_command(&press(GameAction::MainhandAction, 16))
            .unwrap();
        let walking = input.client_command(&walk(250)).unwrap();
        assert_eq!(
            loader.client_commands(),
            [
                (1, Raw::PRESS, "Block".to_owned()),
                (2, Raw::PRESS, "MainhandAction".to_owned()),
                (3, Raw::MOVE, String::new()),
            ]
        );
        assert!(
            matches!(
                input.client_command(&press(GameAction::Block, 100)),
                Err(WorldError::Refused)
            ),
            "one press of an action at a time"
        );
        loader.set_command_status(1, Status::APPLIED, 0);
        assert_eq!(
            input.command_status(block).unwrap().state,
            CommandState::Applied,
            "the builder holds the bit"
        );
        loader.set_command_status(1, Status::OBSERVED, 0);
        assert_eq!(
            input.command_status(block).unwrap().state,
            CommandState::Observed,
            "the hold ran out"
        );
        assert!(
            input.client_command(&press(GameAction::Block, 100)).is_ok(),
            "released"
        );
        for id in [attack, walking] {
            assert_eq!(
                input.command_status(id).unwrap().state,
                CommandState::Pending
            );
        }
    }

    #[test]
    fn a_press_past_its_bounds_or_off_the_world_screen_is_refused() {
        let (_loader, input) = input(Side::CLIENT);
        for refused in [
            press(GameAction::Block, 0),
            press(GameAction::Block, Raw::MAX_PRESS_MS + 1),
            press(GameAction::Block, 16).on_screen("Backpack"),
        ] {
            assert!(
                matches!(input.client_command(&refused), Err(WorldError::Refused)),
                "{refused:?}"
            );
        }
        assert!(
            input
                .client_command(&press(GameAction::Block, Raw::MAX_PRESS_MS).on_screen("InGame"))
                .is_ok()
        );
    }

    #[test]
    fn a_press_of_an_action_the_build_lacks_is_unavailable() {
        let (_loader, input) = input(Side::CLIENT);
        assert!(matches!(
            input.client_command(&press(GameAction::Jump, 16)),
            Err(WorldError::Unavailable)
        ));
    }

    #[test]
    fn each_cancel_reason_ends_a_press() {
        let (loader, input) = input(Side::CLIENT);
        for (reason, expected) in [
            (Status::PLAYER_INPUT, CancelReason::PlayerInput),
            (Status::OTHER_SCREEN, CancelReason::OtherScreen),
            (Status::SESSION_CHANGED, CancelReason::SessionChanged),
            (Status::INPUT_BLOCKED, CancelReason::InputBlocked),
            (Status::MOD_OFF, CancelReason::ModOff),
            (Status::TIMED_OUT, CancelReason::TimedOut),
        ] {
            let id = input
                .client_command(&press(GameAction::ContextualAction, 500))
                .unwrap();
            let raw = loader.client_commands().last().unwrap().0;
            loader.set_command_status(raw, Status::CANCELLED, reason);
            assert_eq!(
                input.command_status(id).unwrap().state,
                CommandState::Cancelled(expected),
                "{reason}"
            );
        }
        let id = input
            .client_command(&press(GameAction::ContextualAction, 500))
            .unwrap();
        input.cancel_command(id).unwrap();
        assert_eq!(
            input.command_status(id).unwrap().state,
            CommandState::Cancelled(CancelReason::ByMod)
        );
    }

    #[test]
    fn a_recipe_is_highlighted_or_unavailable_when_the_menu_lacks_it() {
        let (loader, input) = input(Side::CLIENT);
        let torch = ClientCommand::new(
            CommandAction::HighlightRecipe(enshwick::RecipeId(0x7cd8_0a04)),
            1,
        );
        let id = input.client_command(&torch).unwrap();
        loader.set_command_status(1, Status::OBSERVED, 0);
        assert_eq!(
            input.command_status(id).unwrap().state,
            CommandState::Observed
        );
        let other = input.client_command(&torch).unwrap();
        loader.set_command_status(2, Status::UNAVAILABLE, 0);
        assert_eq!(
            input.command_status(other).unwrap().state,
            CommandState::Unavailable,
            "another workshop's recipe"
        );
        let none = ClientCommand::new(CommandAction::HighlightRecipe(enshwick::RecipeId(0)), 1);
        assert!(matches!(
            input.client_command(&none),
            Err(WorldError::Refused)
        ));
    }

    #[test]
    fn a_server_runs_no_client_command() {
        let (_loader, input) = input(Side::SERVER);
        assert!(matches!(
            input.client_command(&open_backpack()),
            Err(WorldError::Unavailable)
        ));
    }

    fn join(server: &str) -> ClientCommand {
        ClientCommand::new(
            CommandAction::Join {
                server: server.to_owned(),
                password: Some(Password::new("hunter2")),
            },
            0,
        )
    }

    #[test]
    fn a_join_walks_its_phases_and_its_status_holds_no_password() {
        let (loader, input) = input(Side::CLIENT);
        let id = input.client_command(&join("127.0.0.1:15637")).unwrap();
        assert_eq!(
            loader.client_commands(),
            [(1, Raw::JOIN, "127.0.0.1:15637".to_owned())],
            "the server is kept, the password is not"
        );
        let status = input.command_status(id).unwrap();
        assert_eq!(
            (status.state, status.phase),
            (CommandState::Pending, Some(JoinPhase::Queued))
        );
        assert!(!format!("{status:?}").contains("hunter2"));
        loader.set_command_status(1, Status::APPLIED, 0);
        loader.set_command_phase(1, Status::PHASE_LOADING, 0);
        assert_eq!(
            input.command_status(id).unwrap().phase,
            Some(JoinPhase::Loading)
        );
        loader.set_command_status(1, Status::OBSERVED, 0);
        loader.set_command_phase(1, Status::PHASE_IN_WORLD, 0);
        let done = input.command_status(id).unwrap();
        assert_eq!(
            (done.state, done.phase),
            (CommandState::Observed, Some(JoinPhase::InWorld))
        );
    }

    #[test]
    fn a_failed_join_names_why_and_a_cancelled_one_frees_the_next() {
        let (loader, input) = input(Side::CLIENT);
        let id = input.client_command(&join("127.0.0.1:15637")).unwrap();
        loader.set_command_status(1, Status::FAILED, 0);
        loader.set_command_phase(1, Status::PHASE_MENU, Status::WRONG_PASSWORD);
        assert_eq!(
            input.command_status(id).unwrap().state,
            CommandState::Failed(JoinFailure::WrongPassword)
        );
        let again = input.client_command(&join("127.0.0.1:15637")).unwrap();
        assert!(
            matches!(
                input.client_command(&join("10.0.0.2:15637")),
                Err(WorldError::Refused)
            ),
            "one join at a time"
        );
        input.cancel_command(again).unwrap();
        assert_eq!(
            input.command_status(again).unwrap().state,
            CommandState::Cancelled(CancelReason::ByMod)
        );
        assert!(input.client_command(&join("10.0.0.2:15637")).is_ok());
    }

    #[test]
    fn a_join_is_refused_in_a_world_or_without_a_host_and_port_and_a_leave_outside_one() {
        let (loader, input) = input(Side::CLIENT);
        for bad in ["127.0.0.1", "a b:15637", "host:0"] {
            assert!(
                matches!(input.client_command(&join(bad)), Err(WorldError::Refused)),
                "{bad}"
            );
        }
        let in_a_session = ClientCommand::new(
            CommandAction::Join {
                server: "127.0.0.1:15637".to_owned(),
                password: None,
            },
            3,
        );
        assert!(matches!(
            input.client_command(&in_a_session),
            Err(WorldError::Refused)
        ));
        let leave = ClientCommand::new(CommandAction::Leave, 0);
        assert!(
            matches!(input.client_command(&leave), Err(WorldError::Refused)),
            "no world to leave"
        );
        loader.set_client_snapshot(enshwick::abi::ClientSnapshot {
            session: 2,
            ..enshwick::abi::ClientSnapshot::EMPTY
        });
        assert!(matches!(
            input.client_command(&join("127.0.0.1:15637")),
            Err(WorldError::Refused)
        ));
        let leave = input
            .client_command(&ClientCommand::new(CommandAction::Leave, 2))
            .unwrap();
        assert_eq!(
            input.command_status(leave).unwrap().phase,
            Some(JoinPhase::InWorld),
            "a leave has a phase from the queue on"
        );
    }
}
