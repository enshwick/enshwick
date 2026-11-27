//! Client commands for the loader's Mods screen and the main menu through
//! the safe layer on the fake host: a control rebound, a setting set and
//! the game quit, each refused where the loader refuses it.
#[cfg(test)]
mod tests {
    use enshwick::abi::{ClientCommand as Raw, ClientCommandStatus as Status, Side};
    use enshwick::controls::{Control, Key};
    use enshwick::input::{CancelReason, ClientCommand, CommandAction, CommandState, Input};
    use enshwick::{Load, Mod, Off, WorldError};
    use std::sync::Mutex;

    static INPUT: Mutex<Option<Input>> = Mutex::new(None);

    /// This file's mod's id, as its manifest names it.
    const ID: &str = "you.lights";

    struct Lights;

    impl Mod for Lights {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.control(
                Control::new("toggle", "Toggle the lights").key(Key::parse("L")?),
                |_| {},
            )?;
            *INPUT.lock().unwrap() = Some(load.input()?);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Lights,
        r#"id = "you.lights"
side = "both""#
    );

    /// A mod the fake loaded beside this file's, whose settings a command
    /// sets.
    const TRAIL: &str = "you.trail";

    /// The mod started on a client that also loaded [`TRAIL`], and its
    /// input.
    fn started() -> (enshwick::test::Installed, Input) {
        let loader = enshwick::test::FakeHost::new()
            .side(Side::CLIENT)
            .player_input()
            .loaded(enshwick::abi::headed!(enshwick::abi::ModInfo {
                id: enshwick::abi::Str::new(TRAIL),
                ..enshwick::abi::ModInfo::EMPTY
            }))
            .install();
        // SAFETY: these are this file's generated exports.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, enshwick::abi::Status::OK, "{:?}", loader.logs());
        (loader, INPUT.lock().unwrap().unwrap())
    }

    fn rebind(control: &str, key: Option<Key>) -> ClientCommand {
        ClientCommand::new(
            CommandAction::Rebind {
                mod_id: ID.to_owned(),
                control: control.to_owned(),
                second: false,
                key,
            },
            0,
        )
    }

    fn set(mod_id: &str, value: &str) -> ClientCommand {
        ClientCommand::new(
            CommandAction::SetSetting {
                mod_id: mod_id.to_owned(),
                key: "range".to_owned(),
                value: value.to_owned(),
            },
            0,
        )
    }

    #[test]
    fn a_rebind_of_a_declared_control_queues_and_ends_as_the_mods_screen_answered() {
        let (loader, input) = started();
        let id = input
            .client_command(&rebind("toggle", Some(Key::parse("Alt+L").unwrap())))
            .unwrap();
        assert_eq!(loader.client_commands(), [(1, Raw::REBIND, ID.to_owned())]);
        assert!(
            matches!(
                input.client_command(&rebind("toggle", None)),
                Err(WorldError::Refused)
            ),
            "one rebind waits per mod"
        );
        loader.set_command_status(1, Status::OBSERVED, 0);
        assert_eq!(
            input.command_status(id).unwrap().state,
            CommandState::Observed
        );
        let cleared = input.client_command(&rebind("toggle", None)).unwrap();
        loader.set_command_status(2, Status::CANCELLED, Status::CHANGE_REFUSED);
        assert_eq!(
            input.command_status(cleared).unwrap().state,
            CommandState::Cancelled(CancelReason::ChangeRefused),
            "the file did not write"
        );
    }

    #[test]
    fn a_rebind_of_a_control_no_mod_declared_is_refused_at_the_call() {
        let (_loader, input) = started();
        assert!(matches!(
            input.client_command(&rebind("horn", None)),
            Err(WorldError::BadArgs)
        ));
    }

    #[test]
    fn a_setting_names_a_loaded_mod_and_a_value_within_its_bound() {
        let (loader, input) = started();
        for (refused, why) in [
            (set("you.other", "12"), "a mod no one loaded"),
            (set(TRAIL, ""), "no value"),
            (
                set(TRAIL, &"1".repeat(CommandAction::MAX_SETTING + 1)),
                "past the bound",
            ),
        ] {
            assert!(
                matches!(input.client_command(&refused), Err(WorldError::BadArgs)),
                "{why}"
            );
        }
        let id = input.client_command(&set(TRAIL, "12")).unwrap();
        assert_eq!(
            loader.client_commands(),
            [(1, Raw::SET_SETTING, TRAIL.to_owned())]
        );
        loader.set_command_status(1, Status::UNAVAILABLE, 0);
        assert_eq!(
            input.command_status(id).unwrap().state,
            CommandState::Unavailable,
            "the Mods screen does not show the setting"
        );
    }

    #[test]
    fn a_raw_setting_whose_key_or_value_is_not_utf_8_is_refused_as_the_loader_refuses_it() {
        use enshwick::abi::{Status as Code, Str};
        /// `Str`'s layout, to view bytes that are not UTF-8 as a C caller can.
        #[repr(C)]
        struct Bytes(*const u8, usize);
        const NOT_TEXT: &[u8] = b"\xff\xfe";
        let (loader, _input) = started();
        // SAFETY: `Str` is `repr(C)` with a pointer and a length, as `Bytes`
        // is; the pointer views a `'static` slice of that length.
        let not_text: Str =
            unsafe { core::mem::transmute(Bytes(NOT_TEXT.as_ptr(), NOT_TEXT.len())) };
        // SAFETY: the fake's table is a static.
        let api = unsafe { &*loader.api() };
        for (item, text, why) in [
            (not_text, Str::new("12"), "the key"),
            (Str::new("range"), not_text, "the value"),
        ] {
            let spec = enshwick::abi::headed!(Raw {
                kind: Raw::SET_SETTING,
                target: Str::new(TRAIL),
                item,
                text,
                ..Raw::EMPTY
            });
            let mut id = 0;
            assert_eq!(
                (api.client_command)(loader.handle(), &raw const spec, &raw mut id),
                Code::BAD_ARGS,
                "{why}"
            );
        }
        assert!(loader.client_commands().is_empty());
    }

    #[test]
    fn a_quit_queues_at_the_main_menu_and_is_refused_in_a_world_or_another_session() {
        let (loader, input) = started();
        let quit = ClientCommand::new(CommandAction::Quit, 0);
        assert!(matches!(
            input.client_command(&ClientCommand::new(CommandAction::Quit, 2)),
            Err(WorldError::BadArgs)
        ));
        assert!(matches!(
            input.client_command(&quit.clone().on_screen("InGame")),
            Err(WorldError::BadArgs)
        ));
        let id = input.client_command(&quit).unwrap();
        loader.set_command_status(1, Status::OBSERVED, 0);
        assert_eq!(
            input.command_status(id).unwrap().state,
            CommandState::Observed
        );
        loader.set_client_snapshot(enshwick::abi::headed!(enshwick::abi::ClientSnapshot {
            session: 4,
            ..enshwick::abi::ClientSnapshot::EMPTY
        }));
        assert!(
            matches!(input.client_command(&quit), Err(WorldError::Refused)),
            "in a world"
        );
    }
}
