//! `enshwick.actions`: a script declares player actions as tables, and the
//! loader's sheet, runs and console reach its functions as a Rust mod's.

#[cfg(test)]
mod tests {
    use enshwick::abi::{ActionCall, Header, PlayerInfo, Side};
    use enshwick::actions::sheet::{ArgView, decode};
    use enshwick::console;
    use enshwick::i18n::Language;
    use enshwick::test::FakeHost;

    use crate::harness::{logged, run};

    const MAIN: &str = r#"
        local texts = enshwick.i18n.parse([[
[en]
"access" = "Who may use it"
"locker" = "Your locker"
"none" = "No locker of yours here"
[uk]
"access" = "Хто може користуватися"
"locker" = "Твоя шафка"
]])
        local actions = enshwick.actions.actions(texts)
        local allowed = {}
        local first = nil
        actions:add({
            id = "greet",
            label = "Greet",
            allow = function(player)
                return player.name ~= "Ann"
            end,
        }, function(request)
            return enshwick.actions.done(request:say("locker", {}))
        end)
        actions:add({
            id = "access",
            label = "access",
            help = "Let players open the locker",
            at = function(ask)
                if ask:player().entity == 0 then
                    return { Err = ask:say("none", {}) }
                end
                return { Ok = enshwick.actions.subject("42", ask:say("locker", {})) }
            end,
            per_player = function(ask, subject)
                assert(subject.id == "42", "the list gets the subject")
                return allowed
            end,
        }, function(request)
            local args = request:args()
            local who, on = args[1].Player, args[2].Toggle
            if who == nil or on == nil then
                return enshwick.actions.refused("bad")
            end
            if on then
                table.insert(allowed, { key = who.key, name = who.name })
            end
            enshwick.log.info(`access {request:via()} {request:subject().id}`)
            return enshwick.actions.done(`{who.name} {on}`)
        end)
        actions:add({
            id = "wipe",
            label = "Wipe",
            admins = true,
            args = {
                enshwick.actions.choice("What", { { "all", "Everything" }, { "half", "Half" } }),
                enshwick.actions.number("Minutes", 1, 10, 1),
            },
        }, function(request)
            -- A request is no lent object: the one kept stays its own.
            if first ~= nil then
                enshwick.log.info(`kept {rawequal(first, request)} {first:args()[1].Choice}`)
            end
            first = request
            local args = request:args()
            return enshwick.actions.done(`wipe {args[1].Choice} in {args[2].Number}`)
        end)
        local ok, why = pcall(actions.add, actions, { id = "access", label = "Taken" }, function() end)
        enshwick.log.info(`taken {ok} {why}`)
    "#;

    fn player(entity: u32, key: u64, name: &str, admin: bool) -> PlayerInfo {
        let mut info = PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: 0,
            entity: enshwick::Entity(entity),
            key: enshwick::PlayerKey::new(key),
            permissions: if admin { PlayerInfo::CAN_KICK_BAN } else { 0 },
            name_len: u8::try_from(name.len()).unwrap(),
            name: [0; 64],
        };
        info.name[..name.len()].copy_from_slice(name.as_bytes());
        info
    }

    #[test]
    fn a_script_declares_actions_that_run_and_refuse_as_a_rust_mods() {
        let owner = player(2, 1, "Owl", false);
        let ann = player(3, 7, "Ann", false);
        let admin = player(4, 9, "Root", true);
        let loader = run(
            FakeHost::new().player(owner).player(ann).player(admin),
            MAIN,
        );
        assert!(logged(&loader, "taken false"), "{:?}", loader.logs());

        let sheet = decode(
            &loader
                .ask_actions(ActionCall::SHEET, &owner, Language::UK_UA.0, "", "")
                .unwrap(),
        );
        let sheet: Vec<_> = sheet.into_iter().filter(|e| e.id == "access").collect();
        assert_eq!(sheet.len(), 1, "the administrators' action is hidden");
        assert_eq!(sheet[0].label, "Хто може користуватися");
        assert_eq!(sheet[0].help, "Let players open the locker");
        assert_eq!(sheet[0].subject, "Твоя шафка", "{:?}", loader.logs());
        assert!(sheet[0].per_player && sheet[0].offered);

        let reply = loader.ask_actions(
            ActionCall::RUN,
            &owner,
            0,
            "access",
            "p0000000000000007\tb1",
        );
        assert_eq!(reply.as_deref(), Some("1Ann true"), "{:?}", loader.logs());
        assert!(
            logged(&loader, "access Interface 42"),
            "{:?}",
            loader.logs()
        );
        let sheet = decode(
            &loader
                .ask_actions(ActionCall::SHEET, &owner, 0, "", "")
                .unwrap(),
        );
        let access = sheet.iter().find(|e| e.id == "access").unwrap();
        assert!(
            access
                .players
                .iter()
                .any(|p| p.0 == enshwick::PlayerKey::new(7) && p.2),
            "the list shows the switch on"
        );

        let ghost = player(0, 5, "Ghost", false);
        assert_eq!(
            loader
                .ask_actions(
                    ActionCall::RUN,
                    &ghost,
                    0,
                    "access",
                    "p0000000000000007\tb1"
                )
                .as_deref(),
            Some("0No locker of yours here")
        );

        let wipe = decode(
            &loader
                .ask_actions(ActionCall::SHEET, &admin, 0, "", "")
                .unwrap(),
        );
        let args = &wipe.iter().find(|e| e.id == "wipe").unwrap().args;
        assert_eq!(
            (args[0].kind, args[1].kind),
            (ArgView::CHOICE, ArgView::NUMBER)
        );
        assert_eq!(
            loader
                .ask_actions(ActionCall::RUN, &ann, 0, "wipe", "c0\tn2")
                .as_deref(),
            Some("0You may not do that.")
        );
        assert_eq!(
            console::run("you.luau wipe half 3").as_deref(),
            Ok("wipe 1 in 3")
        );
        assert_eq!(
            console::run("you.luau wipe all 2").as_deref(),
            Ok("wipe 0 in 2")
        );
        assert!(logged(&loader, "kept false 1"), "{:?}", loader.logs());
        assert_eq!(
            loader
                .ask_actions(ActionCall::RUN, &owner, 0, "greet", "")
                .as_deref(),
            Some("1Your locker")
        );
        assert_eq!(
            loader
                .ask_actions(ActionCall::RUN, &ann, 0, "greet", "")
                .as_deref(),
            Some("0You may not do that.")
        );
    }

    #[test]
    fn a_script_s_gate_holds_admins_and_allow_together_refuses_nil_and_a_handler_may_add() {
        let ann = player(3, 7, "Ann", true);
        let bob = player(4, 8, "Bob", false);
        let loader = run(
            FakeHost::new().player(ann).player(bob),
            r#"
            local actions = enshwick.actions.actions(enshwick.i18n.parse(""))
            actions:add({
                id = "wipe",
                label = "Wipe",
                admins = true,
                allow = function(player) return player.name ~= "Ann" end,
            }, function() return enshwick.actions.done("wiped") end)
            actions:add({
                id = "quiet",
                label = "Quiet",
                allow = function(player) if player.name == "Ann" then return true end end,
            }, function() return enshwick.actions.done("quiet") end)
            actions:add({ id = "grow", label = "Grow" }, function()
                actions:add({ id = "grown", label = "Grown" }, function()
                    return enshwick.actions.done("grown")
                end)
                return enshwick.actions.done("grew")
            end)
            "#,
        );
        let refused = Some("0You may not do that.");
        let run = |who: &PlayerInfo, id: &str| loader.ask_actions(ActionCall::RUN, who, 0, id, "");
        assert_eq!(
            run(&ann, "wipe").as_deref(),
            refused,
            "allow refuses the admin"
        );
        assert_eq!(run(&bob, "wipe").as_deref(), refused, "admins refuses Bob");
        assert_eq!(run(&ann, "quiet").as_deref(), Some("1quiet"));
        assert_eq!(run(&bob, "quiet").as_deref(), refused, "nil refuses");
        assert_eq!(
            run(&bob, "grow").as_deref(),
            Some("1grew"),
            "{:?}",
            loader.logs()
        );
        assert_eq!(run(&bob, "grown").as_deref(), Some("1grown"));
    }

    #[test]
    fn a_client_script_asks_the_server_to_run_its_action() {
        let loader = run(
            FakeHost::new().side(Side::CLIENT),
            r#"
            local actions = enshwick.actions.actions(enshwick.i18n.parse(""))
            actions:add({ id = "honk", label = "Honk" }, function() return enshwick.actions.done("Honk!") end)
            actions:run("honk", { { Toggle = true }, { Number = 2 } })
            "#,
        );
        assert_eq!(
            loader.action_runs(),
            [("honk".to_owned(), "b1\tn2".to_owned())],
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn a_script_runs_its_action_on_a_named_object_and_its_at_reads_it() {
        let client = run(
            FakeHost::new().side(Side::CLIENT),
            r#"
            local actions = enshwick.actions.actions(enshwick.i18n.parse(""))
            actions:add({ id = "switch", label = "Switch" }, function() return enshwick.actions.done("") end)
            actions:run_at("switch", {}, { position = vector.create(1, 2, 3) })
            "#,
        );
        assert_eq!(
            client.action_targets(),
            [Some(enshwick::actions::ActionTarget::at([1.0, 2.0, 3.0]))],
            "{:?}",
            client.logs()
        );
    }

    #[test]
    fn a_script_s_at_and_handler_get_the_object_the_client_named() {
        let owner = player(2, 1, "Owl", false);
        let loader = run(
            FakeHost::new().player(owner),
            r#"
            local actions = enshwick.actions.actions(enshwick.i18n.parse(""))
            actions:add({
                id = "switch",
                label = "Switch",
                at = function(ask)
                    local target = ask:target()
                    if target == nil then
                        return { Err = "Open a safe first" }
                    end
                    return { Ok = enshwick.actions.subject("7", "safe at " .. target.position.x) }
                end,
            }, function(request)
                return enshwick.actions.done("entity " .. tostring(request:target().entity))
            end)
            "#,
        );
        let chest = enshwick::actions::ActionTarget::entity(enshwick::Entity(4096))
            .with_position([5.0, 0.0, 0.0]);
        assert_eq!(
            loader
                .ask_action_at(&owner, 0, "switch", "", chest)
                .as_deref(),
            Some("1entity 4096"),
            "{:?}",
            loader.logs()
        );
        assert_eq!(
            loader
                .ask_actions(ActionCall::RUN, &owner, 0, "switch", "")
                .as_deref(),
            Some("0Open a safe first")
        );
    }
}
