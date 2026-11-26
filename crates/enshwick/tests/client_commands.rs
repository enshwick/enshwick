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
    static UI: Mutex<Option<enshwick::ui::Ui>> = Mutex::new(None);

    struct Driver;

    impl Mod for Driver {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            *INPUT.lock().unwrap() = Some(load.input()?);
            *UI.lock().unwrap() = load.ui().ok();
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Driver,
        r#"id = "you.client-commands"
side = "both""#
    );

    fn input(side: Side) -> (enshwick::test::Installed, Input) {
        started(enshwick::test::FakeHost::new().side(side).player_input())
    }

    /// The mod started on `host`, and the input it opened.
    fn started(host: enshwick::test::FakeHost) -> (enshwick::test::Installed, Input) {
        let loader = host.install();
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
    fn a_chat_line_queues_within_the_game_s_bounds_and_an_empty_or_longer_one_is_refused() {
        let (loader, input) = input(Side::CLIENT);
        let chat = |text: String| ClientCommand::new(CommandAction::Chat(text), 1);
        for refused in [String::new(), "x".repeat(CommandAction::MAX_CHAT + 1)] {
            assert!(
                matches!(
                    input.client_command(&chat(refused)),
                    Err(WorldError::Refused)
                ),
                "refused at the call"
            );
        }
        let id = input
            .client_command(&chat("x".repeat(CommandAction::MAX_CHAT)))
            .unwrap();
        assert_eq!(loader.client_commands(), [(1, Raw::CHAT, String::new())]);
        loader.set_command_status(1, Status::OBSERVED, 0);
        assert_eq!(
            input.command_status(id).unwrap().state,
            CommandState::Observed
        );
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
    fn a_mod_holds_at_most_four_presses_at_once() {
        let (loader, input) = input(Side::CLIENT);
        for action in [
            GameAction::MainhandAction,
            GameAction::ContextualAction,
            GameAction::QuickUse0,
            GameAction::QuickUse1,
        ] {
            assert!(
                input.client_command(&press(action, 2000)).is_ok(),
                "{action:?}"
            );
        }
        assert!(
            matches!(
                input.client_command(&press(GameAction::Block, 16)),
                Err(WorldError::Refused)
            ),
            "its fifth"
        );
        loader.set_command_status(1, Status::OBSERVED, 0);
        assert!(
            input.client_command(&press(GameAction::Block, 16)).is_ok(),
            "one ended"
        );
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
        assert!(
            matches!(
                input.client_command(&press(GameAction::Jump, 16).on_screen("Backpack")),
                Err(WorldError::Unavailable)
            ),
            "the loader finds the action's bit before it checks the screen"
        );
    }

    #[test]
    fn an_equip_names_a_member_of_the_build_s_equipment_slots() {
        let (loader, input) = input(Side::CLIENT);
        let equip = |equipment: &str| {
            ClientCommand::new(
                CommandAction::Equip {
                    slot: enshwick::inventory::SlotRef::new(enshwick::Entity::from_engine(41), 0),
                    equipment: equipment.to_owned(),
                },
                1,
            )
        };
        assert!(
            matches!(
                input.client_command(&equip("RangedWeapn")),
                Err(WorldError::Refused)
            ),
            "a typo the game's build lacks"
        );
        assert!(input.client_command(&equip("RangedWeapon")).is_ok());
        assert_eq!(loader.client_commands().len(), 1);
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
        loader.set_client_snapshot(enshwick::abi::headed!(enshwick::abi::ClientSnapshot {
            session: 2,
            ..enshwick::abi::ClientSnapshot::EMPTY
        }));
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

    fn host(world: u16, character: u16) -> ClientCommand {
        let action = CommandAction::Host {
            world,
            character,
            online: false,
        };
        ClientCommand::new(action, 0)
    }

    #[test]
    fn a_host_follows_the_phases_of_play_into_the_world() {
        let (loader, input) = input(Side::CLIENT);
        let id = input.client_command(&host(2, 0)).unwrap();
        assert_eq!(loader.client_commands(), [(1, Raw::HOST, String::new())]);
        let queued = input.command_status(id).unwrap();
        assert_eq!(
            (queued.state, queued.phase),
            (CommandState::Pending, Some(JoinPhase::Menu)),
            "a host starts at the menu"
        );
        assert!(
            matches!(input.client_command(&host(1, 0)), Err(WorldError::Refused)),
            "one host at a time"
        );
        loader.set_command_phase(1, Status::PHASE_IN_WORLD, 0);
        loader.set_command_status(1, Status::OBSERVED, 0);
        let status = input.command_status(id).unwrap();
        assert_eq!(
            (status.state, status.phase),
            (CommandState::Observed, Some(JoinPhase::InWorld))
        );
    }

    #[test]
    fn a_host_is_refused_past_the_menu_s_lists_in_a_session_or_in_a_world() {
        let (loader, input) = input(Side::CLIENT);
        for bad in [host(Raw::MAX_SLOTS, 0), host(0, Raw::MAX_SLOTS)] {
            assert!(
                matches!(input.client_command(&bad), Err(WorldError::Refused)),
                "{bad:?}"
            );
        }
        let in_a_session = ClientCommand::new(
            CommandAction::Host {
                world: 0,
                character: 0,
                online: true,
            },
            3,
        );
        assert!(matches!(
            input.client_command(&in_a_session),
            Err(WorldError::Refused)
        ));
        let on_a_screen = host(0, 0).on_screen("InGame");
        assert!(matches!(
            input.client_command(&on_a_screen),
            Err(WorldError::Refused)
        ));
        loader.set_client_snapshot(enshwick::abi::headed!(enshwick::abi::ClientSnapshot {
            session: 2,
            ..enshwick::abi::ClientSnapshot::EMPTY
        }));
        assert!(
            matches!(input.client_command(&host(0, 0)), Err(WorldError::Refused)),
            "in a world"
        );
    }

    #[test]
    fn the_mods_screen_and_a_mod_s_page_open_by_command_and_the_snapshot_names_them() {
        use enshwick::abi::{ModInfo, Str};
        use enshwick::input::LoaderPage;
        let (loader, input) = started(
            enshwick::test::FakeHost::new()
                .side(Side::CLIENT)
                .player_input()
                .loaded(enshwick::abi::headed!(ModInfo {
                    id: Str::new("you.trail"),
                    ..ModInfo::EMPTY
                })),
        );
        let open = ClientCommand::new(CommandAction::OpenMods, 1);
        let id = input.client_command(&open).unwrap();
        let page = ClientCommand::new(CommandAction::OpenMod("you.trail".to_owned()), 1);
        assert!(input.client_command(&page).is_ok(), "another kind");
        for refused in [String::new(), "you.trial".to_owned()] {
            let unknown = ClientCommand::new(CommandAction::OpenModSettings(refused.clone()), 1);
            assert!(
                matches!(input.client_command(&unknown), Err(WorldError::Refused)),
                "{refused:?}: no loaded mod has it, as the loader refuses"
            );
        }
        assert_eq!(
            loader.client_commands(),
            [
                (1, Raw::OPEN_MODS, String::new()),
                (2, Raw::OPEN_MOD, "you.trail".to_owned())
            ]
        );
        loader.set_command_status(1, Status::OBSERVED, 0);
        assert_eq!(
            input.command_status(id).unwrap().state,
            CommandState::Observed
        );
        loader.set_client_snapshot(enshwick::abi::headed!(enshwick::abi::ClientSnapshot {
            available: enshwick::abi::ClientSnapshot::LOADER_SCREEN,
            loader_screen: enshwick::abi::ClientSnapshot::MOD_PAGE,
            loader_screen_mod: Str::new("you.trail"),
            ..enshwick::abi::ClientSnapshot::EMPTY
        }));
        let _asked = input.client_snapshot().unwrap();
        let _drawn = loader.draw_frame();
        let snapshot = input.client_snapshot().unwrap();
        assert_eq!(snapshot.loader_screen(), Some(LoaderPage::ModPage));
        assert_eq!(snapshot.loader_screen_mod(), Some("you.trail"));
    }

    #[test]
    fn another_mod_queues_a_kind_this_mod_holds_and_neither_sees_or_cancels_the_other_s() {
        use enshwick::abi::{ModHandle, Status as Code, Str};
        const OTHER: ModHandle = ModHandle(2);
        let (loader, input) = started(
            enshwick::test::FakeHost::new()
                .side(Side::CLIENT)
                .player_input()
                .other_mod(OTHER),
        );
        let mine = input.client_command(&open_backpack()).unwrap();
        let waypoint = ClientCommand::new(CommandAction::SetWaypoint([10.0, 5.0, 20.0]), 1);
        let my_waypoint = input.client_command(&waypoint).unwrap();
        // SAFETY: the fake's table is a static.
        let api = unsafe { &*loader.api() };
        let queue = |spec: Raw| {
            let mut id = 0;
            (
                (api.client_command)(OTHER, &raw const spec, &raw mut id),
                id,
            )
        };
        let open = enshwick::abi::headed!(Raw {
            kind: Raw::OPEN_SCREEN,
            session: 1,
            target: Str::new("Backpack"),
            expected_screen: Str::new("InGame"),
            ..Raw::EMPTY
        });
        let (status, theirs) = queue(open);
        assert_eq!(status, Code::OK, "one of a kind per mod, as the loader");
        let ping = enshwick::abi::headed!(Raw {
            kind: Raw::PING,
            session: 1,
            position: [10.0, 5.0, 20.0],
            ..Raw::EMPTY
        });
        assert_eq!(queue(ping).0, Code::OK, "a waypoint or ping per mod");
        assert_eq!(queue(open).0, Code::REFUSED, "its own still waits");
        assert!(
            matches!(
                input.client_command(&open_backpack()),
                Err(WorldError::Refused)
            ),
            "and so does this mod's"
        );
        let mut status = Status::EMPTY;
        assert_eq!(
            (api.client_command_status)(OTHER, mine.as_u64(), &raw mut status),
            Code::UNAVAILABLE,
            "another mod's command is not its to read"
        );
        assert!(
            input
                .command_status(enshwick::input::CommandId::new(theirs))
                .is_err(),
            "nor this mod the other's"
        );
        let cancel = enshwick::abi::headed!(Raw {
            kind: Raw::CANCEL,
            cancels: mine.as_u64(),
            ..Raw::EMPTY
        });
        let _cancelled = queue(cancel);
        assert_eq!(
            input.command_status(mine).unwrap().state,
            CommandState::Pending,
            "nor to cancel"
        );
        assert_eq!(
            input.command_status(my_waypoint).unwrap().state,
            CommandState::Pending
        );
    }

    #[test]
    fn a_waypoint_and_a_ping_wait_one_at_a_time_and_end_observed() {
        let (loader, input) = input(Side::CLIENT);
        let set = ClientCommand::new(CommandAction::SetWaypoint([10.0, 5.0, 20.0]), 1);
        let id = input.client_command(&set).unwrap();
        assert_eq!(
            loader.client_commands(),
            [(1, Raw::SET_WAYPOINT, String::new())]
        );
        let ping = ClientCommand::new(CommandAction::Ping([10.0, 5.0, 20.0]), 1);
        assert!(
            matches!(input.client_command(&ping), Err(WorldError::Refused)),
            "one waypoint or ping at a time"
        );
        loader.set_command_status(1, Status::OBSERVED, 0);
        assert_eq!(
            input.command_status(id).unwrap().state,
            CommandState::Observed
        );
        assert!(input.client_command(&ping).is_ok());
        assert_eq!(id.as_u64(), 1);
    }

    #[test]
    fn a_waypoint_off_the_map_or_not_finite_is_refused_and_clear_needs_no_position() {
        let loader = enshwick::test::FakeHost::new()
            .side(Side::CLIENT)
            .player_input()
            .map_frame(enshwick::abi::headed!(enshwick::abi::MapFrameInfo {
                size: [10240.0; 2],
                metres_per_cell: 8.0,
                ..enshwick::abi::MapFrameInfo::EMPTY
            }))
            .install();
        // SAFETY: these are this file's generated exports.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, enshwick::abi::Status::OK);
        let input = INPUT.lock().unwrap().unwrap();
        for refused in [[-1.0, 0.0, 100.0], [f64::NAN, 0.0, 100.0]] {
            let set = ClientCommand::new(CommandAction::SetWaypoint(refused), 1);
            assert!(
                matches!(input.client_command(&set), Err(WorldError::Refused)),
                "{refused:?}"
            );
        }
        let clear = ClientCommand::new(CommandAction::ClearWaypoint, 1);
        assert!(input.client_command(&clear).is_ok());
    }

    #[test]
    fn a_dialog_answer_goes_to_the_dialog_it_names_by_a_button_it_has() {
        use enshwick::ui::{Dialog, Response};

        let (loader, input) = input(Side::CLIENT);
        let answer = |dialog, response| {
            ClientCommand::new(CommandAction::AnswerDialog { dialog, response }, 1)
        };
        let unavailable = |command: &ClientCommand| {
            matches!(input.client_command(command), Err(WorldError::Unavailable))
        };
        assert!(unavailable(&answer(1, Response::Ok)), "no dialog shows");
        let snapshot = input.client_snapshot().unwrap();
        assert_eq!(
            (snapshot.dialog_buttons(), snapshot.dialog()),
            (Some(vec![]), 0)
        );
        let ui = UI.lock().unwrap().unwrap();
        let ask =
            Dialog::new("Reset", "Reset the world?").buttons(&[Response::Ok, Response::Cancel]);
        ui.dialog(&ask, |_| {}).unwrap();
        ui.dialog(&ask, |_| {}).unwrap();
        let snapshot = input.client_snapshot().unwrap();
        let first = snapshot.dialog();
        assert_ne!(first, 0);
        assert_eq!(
            snapshot.dialog_buttons(),
            Some(vec![Response::Cancel, Response::Ok])
        );
        assert_eq!(snapshot.dialog_mod(), Some("you.client-commands"));
        assert!(
            matches!(
                input.client_command(&answer(first, Response::Retry)),
                Err(WorldError::Refused)
            ),
            "the dialog has no Retry"
        );
        let id = input.client_command(&answer(first, Response::Ok)).unwrap();
        assert_eq!(
            loader.client_commands(),
            [(1, Raw::ANSWER_DIALOG, String::new())]
        );
        assert!(loader.answer(Some(Response::Ok)));
        loader.set_command_status(1, Status::OBSERVED, 0);
        assert_eq!(
            input.command_status(id).unwrap().state,
            CommandState::Observed
        );
        let next = input.client_snapshot().unwrap().dialog();
        assert_ne!(next, first, "the next dialog has a number of its own");
        assert!(
            unavailable(&answer(first, Response::Ok)),
            "an answer from an old snapshot never reaches the next dialog"
        );
    }

    #[test]
    fn dialogs_do_not_show_in_game_before_the_mod_s_init_returned() {
        use enshwick::abi::ClientSnapshot;

        let loader = enshwick::test::FakeHost::new()
            .side(Side::CLIENT)
            .player_input()
            .install();
        let mut snapshot = enshwick::abi::headed!(ClientSnapshot {
            ..ClientSnapshot::EMPTY
        });
        // SAFETY: the fake's table is a static; `snapshot` is writable and
        // headed for the call.
        let status =
            unsafe { ((*loader.api()).client_snapshot)(loader.handle(), &raw mut snapshot) };
        assert_eq!(status, enshwick::abi::Status::OK);
        assert_eq!(
            snapshot.available & ClientSnapshot::DIALOG,
            0,
            "as the loader before arming"
        );
    }

    #[test]
    fn a_joined_client_refuses_a_waypoint_or_ping_past_the_fixed_point_range() {
        let (loader, input) = input(Side::CLIENT);
        for action in [
            CommandAction::SetWaypoint([1e300, 0.0, 0.0]),
            CommandAction::Ping([0.0, -2_147_483_648.0, 0.0]),
        ] {
            assert!(
                matches!(
                    input.client_command(&ClientCommand::new(action.clone(), 1)),
                    Err(WorldError::Refused)
                ),
                "{action:?}"
            );
        }
        let far = ClientCommand::new(CommandAction::SetWaypoint([2_147_483_647.0, 0.0, 0.0]), 1);
        assert!(input.client_command(&far).is_ok());
        assert_eq!(
            loader.client_commands(),
            [(1, Raw::SET_WAYPOINT, String::new())]
        );
    }

    fn press_item(window: &str, item: &str) -> ClientCommand {
        let action = CommandAction::PressWindowItem {
            window: window.to_owned(),
            item: item.to_owned(),
        };
        ClientCommand::new(action, 1)
    }

    #[test]
    fn a_window_command_names_its_window_and_item_and_ends_as_the_window_shows() {
        let (loader, input) = input(Side::CLIENT);
        let id = input
            .client_command(&press_item("you.admin:panel", "ban#0"))
            .unwrap();
        assert_eq!(
            loader.client_commands(),
            [(1, Raw::WINDOW_PRESS, "you.admin:panel".to_owned())]
        );
        loader.set_command_status(1, Status::CANCELLED, Status::ITEM_OFF);
        assert_eq!(
            input.command_status(id).unwrap().state,
            CommandState::Cancelled(CancelReason::ItemOff)
        );
        let typed = CommandAction::SetWindowText {
            window: "you.admin:panel".to_owned(),
            item: "say/0".to_owned(),
            text: "Restart soon".to_owned(),
        };
        assert!(input.client_command(&ClientCommand::new(typed, 1)).is_ok());
    }

    #[test]
    fn a_window_command_with_ids_no_window_holds_is_refused_as_the_loader_refuses_it() {
        let (_loader, input) = input(Side::CLIENT);
        for (window, item) in [
            ("panel", "ban#0"),
            ("you.admin:panel", "Ban"),
            ("you.admin:panel", "ban/x"),
        ] {
            assert!(
                matches!(
                    input.client_command(&press_item(window, item)),
                    Err(WorldError::Refused)
                ),
                "{window} {item}"
            );
        }
        let no_option = CommandAction::PickWindowItem {
            window: "you.admin:panel".to_owned(),
            item: "ban/0".to_owned(),
            option: String::new(),
        };
        assert!(matches!(
            input.client_command(&ClientCommand::new(no_option, 1)),
            Err(WorldError::Refused)
        ));
    }

    #[test]
    fn the_client_lists_its_windows_items_by_the_ids_commands_name_them() {
        use enshwick::abi::{ClientWindowItem, Str};
        use enshwick::input::WindowItemKind;
        let (loader, input) = input(Side::CLIENT);
        assert_eq!(input.client_windows().unwrap(), []);
        loader.set_client_windows(vec![enshwick::abi::headed!(ClientWindowItem {
            kind: ClientWindowItem::FIELD,
            field: ClientWindowItem::TEXT,
            flags: ClientWindowItem::SHOWN,
            window: Str::new("you.admin:panel"),
            item: Str::new("say/0"),
            label: Str::new("Message"),
            line: Str::new("Announce"),
            ..ClientWindowItem::EMPTY
        })]);
        let items = input.client_windows().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(
            (items[0].item.as_str(), items[0].kind),
            ("say/0", WindowItemKind::TextField)
        );
        assert!(items[0].shown);
        drop(loader);
        let (_server, server_input) = super::tests::input(Side::SERVER);
        assert!(matches!(
            server_input.client_windows(),
            Err(WorldError::Unavailable)
        ));
    }
}
