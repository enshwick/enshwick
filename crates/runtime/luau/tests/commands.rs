//! `enshwick.commands`: a script names its actions as commands, limits one
//! to a permission, and runs a command line for a remote caller, as a Rust
//! mod does.

#[cfg(test)]
mod tests {
    use enshwick::abi::{ActionCall, Header, PlayerInfo};
    use enshwick::test::FakeHost;

    use crate::harness::{logged, run};

    fn player(entity: u32, key: u64, name: &str, permissions: u8) -> PlayerInfo {
        let mut info = PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: 0,
            entity: enshwick::Entity(entity),
            key: enshwick::PlayerKey::new(key),
            permissions,
            name_len: u8::try_from(name.len()).unwrap(),
            name: [0; 64],
        };
        info.name[..name.len()].copy_from_slice(name.as_bytes());
        info
    }

    #[test]
    fn a_script_s_actions_run_as_commands_with_each_caller_s_permissions() {
        let guest = player(3, 7, "Guest", 0);
        let friend = player(4, 8, "Friend", PlayerInfo::CAN_EDIT_BASE);
        let loader = run(
            FakeHost::new()
                .player(guest)
                .player(friend)
                .other_mod_commands("you.weather", "wx"),
            r#"
            local actions = enshwick.actions.actions(enshwick.i18n.parse(""))
            actions:add({ id = "online", label = "Players online" }, function(request)
                enshwick.log.info(`online {request:via()} {request:player().name}`)
                return enshwick.actions.done("2 online")
            end)
            actions:add({ id = "repair", label = "Repair", needs = { "CanEditBase" } }, function()
                return enshwick.actions.done("repaired")
            end)
            local ok = pcall(actions.commands, actions, "luau-long")
            enshwick.log.info(`longer alias {ok}`)
            actions:commands("lk")
            actions:rules_changed()
            local reply = enshwick.commands.run("/lk repair", "rcon", { "CanEditBase" })
            enshwick.log.info(`remote {reply.done} {reply.text}`)
            local refused = enshwick.commands.run("/lk repair", "rcon", {})
            enshwick.log.info(`remote without {refused.done}`)
            for _, set in enshwick.commands.list() do
                enshwick.log.info(`listed {set.name} {set.alias} {set.mod_id}`)
            end
            "#,
        );
        assert!(logged(&loader, "longer alias false"), "{:?}", loader.logs());
        assert!(
            logged(&loader, "remote true repaired"),
            "{:?}",
            loader.logs()
        );
        assert!(
            logged(&loader, "remote without false"),
            "{:?}",
            loader.logs()
        );
        assert!(
            logged(&loader, "listed luau lk you.luau"),
            "{:?}",
            loader.logs()
        );
        assert!(
            logged(&loader, "listed weather wx you.weather"),
            "{:?}",
            loader.logs()
        );
        assert_eq!(loader.rules_changed(), 1);

        assert_eq!(
            loader.type_in_chat(&guest, 0, "/luau online").as_deref(),
            Some("12 online")
        );
        assert!(logged(&loader, "online Chat Guest"), "{:?}", loader.logs());
        assert!(
            loader
                .type_in_chat(&guest, 0, "/lk repair")
                .unwrap()
                .starts_with('0')
        );
        assert_eq!(
            loader.type_in_chat(&friend, 0, "/lk repair").as_deref(),
            Some("1repaired")
        );
        assert!(
            loader
                .ask_actions(ActionCall::RUN, &guest, 0, "repair", "")
                .unwrap()
                .starts_with('0')
        );
    }

    #[test]
    fn a_script_s_action_has_a_group_optional_arguments_and_the_player_who_looks() {
        let guest = player(3, 7, "Guest", 0);
        let friend = player(4, 8, "Friend", 0);
        let loader = run(
            FakeHost::new().player(guest).player(friend),
            r#"
            local actions = enshwick.actions.actions(enshwick.i18n.parse(""))
            actions:add({ id = "online", label = "Players online" }, function()
                return enshwick.actions.done("2 online")
            end)
            actions:add({
                id = "give",
                label = "Give",
                help = "Gives the item.",
                group = "Items",
                with_self = true,
                args = {
                    enshwick.actions.player("Player"),
                    enshwick.actions.text("Item", 40),
                    enshwick.actions.number("Count", 1, 99, 1):optional({ Number = 1 }),
                },
            }, function(request)
                local args = request:args()
                return enshwick.actions.done(`{args[2].Text} x{args[3].Number}`)
            end)
            local early = pcall(actions.add, actions, {
                id = "early",
                label = "Early",
                args = {
                    enshwick.actions.toggle("On"):optional({ Toggle = true }),
                    enshwick.actions.text("Item", 40),
                },
            }, function() end)
            enshwick.log.info(`optional first {early}`)
            actions:commands("lk")
            "#,
        );
        assert!(
            logged(&loader, "optional first false"),
            "{:?}",
            loader.logs()
        );
        let chat = |line: &str| loader.type_in_chat(&guest, 0, line);
        assert_eq!(
            chat("/lk give Friend Stone Axe").as_deref(),
            Some("1Stone Axe x1"),
            "an optional argument left out takes its default"
        );
        assert_eq!(
            chat("/lk give Friend Stone Axe 5").as_deref(),
            Some("1Stone Axe x5")
        );
        assert_eq!(
            chat("/lk help").as_deref(),
            Some(
                "1you.luau: /lk <action> runs one, /lk help <action> explains one.\n\
                 online: Players online\n\
                 Items:\n\
                 give <player> <Item> [1-99]: Give"
            ),
            "a group heads its actions"
        );
        assert_eq!(
            chat("/lk help give").as_deref(),
            Some("1give <player> <Item> [1-99]: Give\nGives the item.")
        );
        let sheet = enshwick::actions::sheet::decode(
            &loader
                .ask_actions(ActionCall::SHEET, &guest, 0, "", "")
                .unwrap(),
        );
        let give = sheet.iter().find(|entry| entry.id == "give").unwrap();
        assert_eq!(
            give.args[0].players.first().map(|(_, name)| name.as_str()),
            Some("Guest"),
            "with_self offers the player who looks"
        );
    }
}
