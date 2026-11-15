//! Client commands through the safe layer on the fake host.
#[cfg(test)]
mod tests {
    use enshwick::abi::{ClientCommand as Raw, ClientCommandStatus as Status, Side};
    use enshwick::input::{CancelReason, ClientCommand, CommandAction, CommandState, Input};
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

    #[test]
    fn a_server_runs_no_client_command() {
        let (_loader, input) = input(Side::SERVER);
        assert!(matches!(
            input.client_command(&open_backpack()),
            Err(WorldError::Unavailable)
        ));
    }
}
