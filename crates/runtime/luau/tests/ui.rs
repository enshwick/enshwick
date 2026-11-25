//! `enshwick.ui`: a script's window drawn each frame, inputs reaching its
//! widgets, a HUD, a panel on a game screen, dialogs and map markers, as a
//! Rust mod's.

#[cfg(test)]
mod tests {
    use enshwick::abi::{MapMarkerTypeInfo, Side, UiDialog, UiMarker, UiWidget};
    use enshwick::content::{Guid, types};
    use enshwick::test::{Drawn, FakeHost, Input};
    use enshwick::ui::{HudScreenType, Response};

    use crate::harness::{logged, run, start};

    const MAIN: &str = r#"
        local open, fast, volume, mode = true, false, 0.5, 0
        local kept = nil
        enshwick.ui.on_frame(function(frame)
            if not open then
                return
            end
            local same = kept == nil or rawequal(kept, frame)
            kept = frame
            frame:window("main", "Panel", function(w)
                if w:back() then
                    enshwick.log.info(`back {frame:back()}`)
                end
                w:label({ text = "Settings", style = "H2" })
                fast = w:toggle({ label = "Fast" }, fast)
                volume = w:slider({ label = "Volume", min = 0, max = 1, steps = 11 }, volume)
                mode = w:dropdown({ label = "Mode", options = { "Off", "On" } }, mode)
                w:toggle({ label = "Synced", read_only = true }, fast)
                w:progress(volume)
                if w:button({ text = "Close" }) then
                    w:sound("Confirm")
                    open = false
                end
            end)
            enshwick.log.info(`frame {frame:screen()} same {same} fast {fast} mode {mode}`)
        end)
        enshwick.ui.on_hud(function(hud)
            hud:text("TopRight", { text = "Raid soon", style = "Warning" })
            hud:progress("BottomLeft", 0.25)
        end)
        enshwick.ui.extend_screen("StorageChest", { edge = "Right" }, function(panel)
            panel:label({ text = "Shared with your guild", style = "Secondary" })
            if panel:button({ text = "Sort", enabled = false }) then
                enshwick.log.info("sorted")
            end
        end)
        local ok, why = pcall(enshwick.ui.extend_screen, "InGame", { edge = "Left" }, function() end)
        enshwick.log.info(`in game {ok}`)
        enshwick.ui.dialog({
            title = "Updates",
            message = "Download them now?",
            detail = "two mods",
            buttons = { "Ok", "Cancel" },
        }, function(answer)
            enshwick.log.info(`answer {answer}`)
        end)
        enshwick.ui.set_markers({
            { position = vector.create(1, 2, 3), label = "Locker", color = "Green" },
            { position = vector.create(0, 0, 0) },
        })
    "#;

    #[test]
    fn a_script_s_window_draws_each_frame_takes_inputs_and_closes() {
        let loader = run(FakeHost::new().side(Side::CLIENT), MAIN);
        assert!(logged(&loader, "in game false"), "{:?}", loader.logs());

        let first = loader.draw_frame();
        assert_eq!(
            first.first(),
            Some(&Drawn::Window {
                id: "main".into(),
                title: "Panel".into()
            }),
            "{:?}",
            loader.logs()
        );
        assert_eq!(
            first[1],
            Drawn::Label {
                text: "Settings".into(),
                style: UiWidget::STYLE_H2
            }
        );
        assert_eq!(first.last(), Some(&Drawn::End));

        loader.input("Fast", Input::Toggle(true));
        loader.input("Volume", Input::Value(0.8));
        loader.input("Mode", Input::Pick(1));
        let second = loader.draw_frame();
        assert!(second.contains(&Drawn::Toggle {
            label: "Synced".into(),
            on: true,
            read_only: true
        }));
        assert!(second.iter().any(|d| matches!(
            d,
            Drawn::Slider { label, value, steps: 11, .. } if label == "Volume" && (*value - 0.8).abs() < 1e-6
        )));
        assert!(
            second
                .iter()
                .any(|d| matches!(d, Drawn::Dropdown { label, index: 1, .. } if label == "Mode"))
        );
        assert!(
            logged(&loader, "frame InGame same true fast true mode 1"),
            "one frame object each frame: {:?}",
            loader.logs()
        );

        assert!(second.contains(&Drawn::Progress { fraction: 0.8 }));

        // The script's dialog shows over the window and would take Back.
        assert!(loader.answer(None));
        loader.input("main", Input::Back);
        let _backed = loader.draw_frame();
        assert!(
            logged(&loader, "back false"),
            "Back reaches the window's body, not the frame around it: {:?}",
            loader.logs()
        );

        loader.input("Close", Input::Click);
        let closing = loader.draw_frame();
        assert!(
            closing.contains(&Drawn::Sound(UiWidget::SOUND_CONFIRM)),
            "{closing:?}"
        );
        assert!(loader.draw_frame().is_empty(), "closed");
    }

    #[test]
    fn a_script_plays_a_held_sound_container_and_hears_why_another_is_refused() {
        let held = Guid::named("you.bells:bell");
        let typo = Guid::named("you.bells:bel");
        let host = FakeHost::new()
            .side(Side::CLIENT)
            .type_hash(types::SOUND_CONTAINER_RESOURCE.id(), 7)
            .resource(held.key_by_hash(7), Vec::new());
        let main = format!(
            r#"
            enshwick.ui.on_frame(function(frame)
                frame:play("{held}")
                local ok, why = pcall(frame.play, frame, "{typo}")
                enshwick.log.info(`refused {{not ok}} {{why}}`)
            end)
            "#
        );
        let loader = run(host, &main);
        assert_eq!(loader.draw_frame(), [Drawn::Play(held)]);
        assert!(
            logged(&loader, "refused true") && logged(&loader, &typo.to_string()),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn a_script_draws_its_hud_and_a_panel_on_a_game_screen() {
        let loader = run(FakeHost::new().side(Side::CLIENT), MAIN);
        let hud = loader.draw_hud();
        assert_eq!(
            hud,
            [
                (
                    UiWidget::TOP_RIGHT,
                    Drawn::Label {
                        text: "Raid soon".into(),
                        style: UiWidget::STYLE_WARNING
                    }
                ),
                (UiWidget::BOTTOM_LEFT, Drawn::Progress { fraction: 0.25 }),
            ]
        );
        let panels = loader.draw_screen(HudScreenType::StorageChest);
        assert_eq!(panels.len(), 1, "{:?}", loader.logs());
        assert!(panels[0].1.contains(&Drawn::Label {
            text: "Shared with your guild".into(),
            style: UiWidget::STYLE_SECONDARY
        }));
        assert!(!logged(&loader, "sorted"));
    }

    #[test]
    fn a_script_s_dialog_is_answered_and_its_markers_show_on_the_map() {
        let loader = run(FakeHost::new().side(Side::CLIENT), MAIN);
        let waiting = loader.dialogs();
        assert_eq!(waiting.len(), 1, "{:?}", loader.logs());
        assert_eq!(waiting[0].detail, "two mods");
        assert_eq!(
            waiting[0].buttons,
            1 << UiDialog::OK | 1 << UiDialog::CANCEL
        );
        assert!(loader.answer(Some(Response::Cancel)));
        assert!(logged(&loader, "answer Cancel"), "{:?}", loader.logs());

        let markers = loader.markers();
        assert_eq!(markers.len(), 2);
        assert_eq!(
            (markers[0].label.as_str(), markers[0].color),
            ("Locker", UiMarker::GREEN)
        );
        assert_eq!(markers[0].position, [1.0, 2.0, 3.0]);
        assert_eq!(markers[1].color, UiMarker::WHITE);
    }

    #[test]
    fn a_script_reads_the_marker_types_and_clears_its_markers() {
        let altar = MapMarkerTypeInfo {
            marker_id: 0x0a0b_0c0d,
            ..MapMarkerTypeInfo::EMPTY
        };
        let loader = run(
            FakeHost::new()
                .side(Side::CLIENT)
                .map_marker_types(vec![altar]),
            r"
            local types = enshwick.ui.map_marker_types()
            enshwick.log.info(`types {#types}`)
            enshwick.ui.set_markers({ { position = vector.create(0, 0, 0), marker_type = types[1].marker_type } })
            ",
        );
        assert!(logged(&loader, "types 1"), "{:?}", loader.logs());
        assert_eq!(loader.markers()[0].marker_type, 0x0a0b_0c0d);
        drop(loader);
        let loader = run(
            FakeHost::new().side(Side::CLIENT),
            r"
            enshwick.ui.set_markers({ { position = vector.create(0, 0, 0) } })
            enshwick.ui.clear_markers()
            ",
        );
        assert!(loader.markers().is_empty(), "{:?}", loader.logs());
    }

    #[test]
    fn a_script_reads_the_open_screen_and_hears_it_change() {
        let loader = run(
            FakeHost::new().side(Side::CLIENT),
            r"
            enshwick.ui.on_frame(function(frame)
                enshwick.log.info(`open {frame:open_screen()}`)
            end)
            enshwick.ui.on_screen_changed(function(change)
                enshwick.log.info(`changed {change.before} to {change.now}`)
            end)
            ",
        );
        loader.open_screen(Some(HudScreenType::Map));
        let _drawn = loader.draw_frame();
        loader.open_screen(None);
        let _drawn = loader.draw_frame();
        for line in [
            "open Map",
            "changed nil to Map",
            "open nil",
            "changed Map to nil",
        ] {
            assert!(logged(&loader, line), "{line}: {:?}", loader.logs());
        }
    }

    #[test]
    fn a_script_observes_cached_sampler_flags_and_nil_zero_or_stale_age() {
        use enshwick::abi::{Header, LocalInputInfo as Info};
        let loader = run(
            FakeHost::new().side(Side::CLIENT).player_input(),
            r"
            enshwick.ui.on_frame(function(frame)
                local status = enshwick.input.local_status()
                assert(status.age_ms == nil or type(status.age_ms) == 'string')
                if status.fresh then assert(status.age_ms == '0') end
                enshwick.log.info(`sample {status.sample_present} age {status.age_ms} fresh {status.fresh}`)
            end)
            ",
        );
        let _drawn = loader.draw_frame();
        loader.set_local_input_status(Info {
            header: Header::of::<Info>(),
            flags: Info::SAMPLE_PRESENT | Info::FRESH,
            reserved: 0,
            age_ms: 0,
        });
        let _drawn = loader.draw_frame();
        loader.set_local_input_status(Info {
            header: Header::of::<Info>(),
            flags: Info::SAMPLE_PRESENT,
            reserved: 0,
            age_ms: 1001,
        });
        let _drawn = loader.draw_frame();
        for line in [
            "sample false age nil fresh false",
            "sample true age 0 fresh true",
            "sample true age 1001 fresh false",
        ] {
            assert!(logged(&loader, line), "{line}: {:?}", loader.logs());
        }
    }

    #[test]
    fn sampler_age_keeps_full_u64_precision_and_the_never_sampled_sentinel() {
        use enshwick::abi::{Header, LocalInputInfo as Info};
        let loader = run(
            FakeHost::new().side(Side::CLIENT).player_input(),
            r"
            enshwick.ui.on_frame(function(frame)
                local status = enshwick.input.local_status()
                enshwick.log.info(`boundary age {status.age_ms}`)
            end)
            ",
        );
        for age_ms in [u64::MAX - 1, u64::MAX] {
            loader.set_local_input_status(Info {
                header: Header::of::<Info>(),
                flags: Info::SAMPLE_PRESENT | (1 << 31),
                reserved: 0,
                age_ms,
            });
            let _drawn = loader.draw_frame();
        }
        for line in ["boundary age 18446744073709551614", "boundary age nil"] {
            assert!(logged(&loader, line), "{line}: {:?}", loader.logs());
        }
    }

    #[test]
    fn a_script_cannot_read_client_sampler_status_on_a_server() {
        let loader = run(
            FakeHost::new().player_input(),
            r"
            assert(type(enshwick.input.local_status) == 'function')
            local ok = pcall(enshwick.input.local_status)
            enshwick.log.info(`sampler available {ok}`)
        ",
        );
        assert!(
            logged(&loader, "sampler available false"),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn a_script_reads_the_client_snapshot_s_session_screen_and_tab() {
        use enshwick::abi::{ClientSnapshot, Str};
        let loader = run(
            FakeHost::new().side(Side::CLIENT).player_input(),
            r"
            enshwick.ui.on_frame(function(frame)
                local s = enshwick.input.client_snapshot()
                enshwick.log.info(`{s.state} {s.session} {s.hud_screen} {s.menu_tab} {s.age_ms} {#s.reads}`)
            end)
            ",
        );
        let _drawn = loader.draw_frame();
        loader.set_client_snapshot(ClientSnapshot {
            state: ClientSnapshot::FRESH,
            available: ClientSnapshot::SCREENS | ClientSnapshot::MENU_TAB,
            session: 2,
            age_ms: 16,
            hud_screen: Str::new("InGameMenu"),
            menu_tab: Str::new("Character"),
            ..ClientSnapshot::EMPTY
        });
        let _drawn = loader.draw_frame();
        for line in ["never 0 nil nil nil 1", "fresh 2 InGameMenu Character 16 3"] {
            assert!(logged(&loader, line), "{line}: {:?}", loader.logs());
        }
    }

    #[test]
    fn a_script_reads_the_prompt_s_target_and_verb_and_the_selected_character() {
        use enshwick::abi::ClientSnapshot;
        let loader = run(
            FakeHost::new().side(Side::CLIENT).player_input(),
            r"
            enshwick.ui.on_frame(function(frame)
                local s = enshwick.input.client_snapshot()
                enshwick.log.info(`prompt {s.prompt_target} {s.prompt_verb} character {s.selected_character}`)
            end)
            ",
        );
        let _drawn = loader.draw_frame();
        loader.set_client_snapshot(ClientSnapshot {
            state: ClientSnapshot::FRESH,
            available: ClientSnapshot::PROMPT | ClientSnapshot::CHARACTER,
            prompt_target: enshwick::Entity(1052),
            prompt_verb: 7,
            character: 1,
            ..ClientSnapshot::EMPTY
        });
        let _drawn = loader.draw_frame();
        assert!(
            logged(&loader, "prompt 1052 7 character 1"),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn a_script_reads_a_fresh_snapshot_in_its_first_drawn_frame() {
        use enshwick::abi::{ClientSnapshot, Str};
        let loader = run(
            FakeHost::new().side(Side::CLIENT).player_input(),
            r"
            enshwick.ui.on_frame(function(frame)
                local s = enshwick.input.client_snapshot()
                enshwick.log.info(`first frame {s.state} {s.hud_screen}`)
            end)
            ",
        );
        loader.set_client_snapshot(ClientSnapshot {
            state: ClientSnapshot::FRESH,
            available: ClientSnapshot::SCREENS,
            session: 1,
            age_ms: 16,
            hud_screen: Str::new("Backpack"),
            ..ClientSnapshot::EMPTY
        });
        let _drawn = loader.draw_frame();
        assert!(
            logged(&loader, "first frame fresh Backpack"),
            "the start opened the input: {:?}",
            loader.logs()
        );
    }

    #[test]
    fn a_script_moves_looks_and_highlights_a_recipe_within_their_bounds() {
        use enshwick::abi::ClientCommandStatus as Status;
        let loader = run(
            FakeHost::new().side(Side::CLIENT).player_input(),
            r#"
            local walk = enshwick.input.client_command({ kind = "move", session = "1", direction = { 0, 1 }, hold_ms = 250 })
            local far = pcall(enshwick.input.client_command, { kind = "move", session = "1", direction = { 0, 1 }, hold_ms = 251 })
            local spin = pcall(enshwick.input.client_command, { kind = "look", session = "1", look = { 4, 0 } })
            local torch = enshwick.input.client_command({ kind = "highlight_recipe", session = "1", recipe = 0x7cd80a04 })
            enshwick.log.info(`queued {walk} {torch} far {far} spin {spin}`)
            enshwick.ui.on_frame(function()
                local s = enshwick.input.command_status(walk)
                enshwick.log.info(`move {s.state} {s.reason}`)
            end)
            "#,
        );
        loader.set_command_status(1, Status::CANCELLED, Status::INPUT_BLOCKED);
        let _drawn = loader.draw_frame();
        for line in [
            "queued 1 2 far false spin false",
            "move cancelled input_blocked",
        ] {
            assert!(logged(&loader, line), "{line}: {:?}", loader.logs());
        }
    }

    #[test]
    fn a_script_presses_a_game_action_by_its_game_name_within_its_bound() {
        use enshwick::abi::ClientCommandStatus as Status;
        let loader = run(
            FakeHost::new().side(Side::CLIENT).player_input(),
            r#"
            local interact = enshwick.input.client_command({ kind = "press", session = "1", action = "ContextualAction", hold_ms = 50 })
            local block = enshwick.input.client_command({ kind = "press", session = "1", action = "Block", hold_ms = 2000 })
            local long = pcall(enshwick.input.client_command, { kind = "press", session = "1", action = "Block", hold_ms = 2001 })
            local dance = pcall(enshwick.input.client_command, { kind = "press", session = "1", action = "Dance", hold_ms = 50 })
            enshwick.log.info(`queued {interact} {block} long {long} dance {dance}`)
            enshwick.ui.on_frame(function()
                local s = enshwick.input.command_status(interact)
                enshwick.log.info(`press {s.state} {s.reason}`)
            end)
            "#,
        );
        assert_eq!(
            loader.client_commands(),
            [
                (
                    1,
                    enshwick::abi::ClientCommand::PRESS,
                    "ContextualAction".to_owned()
                ),
                (2, enshwick::abi::ClientCommand::PRESS, "Block".to_owned()),
            ]
        );
        loader.set_command_status(1, Status::CANCELLED, Status::PLAYER_INPUT);
        let _drawn = loader.draw_frame();
        for line in [
            "queued 1 2 long false dance false",
            "press cancelled player_input",
        ] {
            assert!(logged(&loader, line), "{line}: {:?}", loader.logs());
        }
    }

    #[test]
    fn a_script_opens_a_mod_s_page_and_reads_it_from_the_snapshot() {
        let trail = enshwick::abi::ModInfo {
            header: enshwick::abi::Header::of::<enshwick::abi::ModInfo>(),
            id: enshwick::abi::Str::new("you.trail"),
            ..enshwick::abi::ModInfo::EMPTY
        };
        let loader = run(
            FakeHost::new()
                .side(Side::CLIENT)
                .player_input()
                .loaded(trail),
            r#"
            local id = enshwick.input.client_command({ kind = "open_mod", session = "1", target = "you.trail" })
            local bare = pcall(enshwick.input.client_command, { kind = "open_mod", session = "1" })
            enshwick.log.info(`queued {id} bare {bare}`)
            enshwick.ui.on_frame(function()
                local s = enshwick.input.client_snapshot()
                enshwick.log.info(`page {s.loader_screen} of {s.loader_screen_mod}`)
            end)
            "#,
        );
        assert_eq!(
            loader.client_commands(),
            [(
                1,
                enshwick::abi::ClientCommand::OPEN_MOD,
                "you.trail".to_owned()
            )]
        );
        loader.set_client_snapshot(enshwick::abi::ClientSnapshot {
            available: enshwick::abi::ClientSnapshot::LOADER_SCREEN,
            loader_screen: enshwick::abi::ClientSnapshot::MOD_PAGE,
            loader_screen_mod: enshwick::abi::Str::new("you.trail"),
            ..enshwick::abi::ClientSnapshot::EMPTY
        });
        let _first = loader.draw_frame();
        let _second = loader.draw_frame();
        for line in ["queued 1 bare false", "page mod_page of you.trail"] {
            assert!(logged(&loader, line), "{line}: {:?}", loader.logs());
        }
    }

    #[test]
    fn a_script_presses_and_types_in_a_carried_window_by_the_ids_it_lists() {
        use enshwick::abi::ClientWindowItem;
        let loader = run(
            FakeHost::new().side(Side::CLIENT).player_input(),
            r#"
            local typed = enshwick.input.client_command({
                kind = "set_window_text", session = "1",
                window = "you.admin:panel", item = "say/0", text = "Restart soon",
            })
            local bare = pcall(enshwick.input.client_command, {
                kind = "press_window_item", session = "1", window = "you.admin:panel",
            })
            enshwick.log.info(`queued {typed} bare {bare}`)
            enshwick.ui.on_frame(function()
                for _, item in enshwick.input.client_windows() do
                    enshwick.log.info(`item {item.item} {item.kind} {item.line} shown {item.shown}`)
                end
            end)
            "#,
        );
        assert_eq!(
            loader.client_commands(),
            [(
                1,
                enshwick::abi::ClientCommand::WINDOW_TEXT,
                "you.admin:panel".to_owned()
            )]
        );
        loader.set_client_windows(vec![ClientWindowItem {
            kind: ClientWindowItem::FIELD,
            field: ClientWindowItem::TEXT,
            flags: ClientWindowItem::SHOWN,
            window: enshwick::abi::Str::new("you.admin:panel"),
            item: enshwick::abi::Str::new("say/0"),
            label: enshwick::abi::Str::new("Message"),
            line: enshwick::abi::Str::new("Announce"),
            ..ClientWindowItem::EMPTY
        }]);
        let _drawn = loader.draw_frame();
        for line in [
            "queued 1 bare false",
            "item say/0 text_field Announce shown true",
        ] {
            assert!(logged(&loader, line), "{line}: {:?}", loader.logs());
        }
    }

    #[test]
    fn a_script_reads_the_local_player_s_state_and_slots_and_crafts_once_at_a_time() {
        use enshwick::abi::{ClientSlot, ClientSnapshot};
        let loader = run(
            FakeHost::new().side(Side::CLIENT).player_input(),
            r#"
            local craft = enshwick.input.client_command({ kind = "craft", session = "1", recipe = 7, count = 2 })
            local twice = pcall(enshwick.input.client_command, {
                kind = "trash", session = "1", slot = "4294967361",
            })
            local bare = pcall(enshwick.input.client_command, { kind = "move_item", session = "1", slot = "65" })
            enshwick.log.info(`queued {craft} twice {twice} bare {bare}`)
            enshwick.ui.on_frame(function()
                local s = enshwick.input.client_snapshot()
                enshwick.log.info(`health {s.health.current}/{s.health.max} bar {s.actionbar_slot}`)
                local slots = enshwick.input.client_slots()
                if slots then
                    for _, slot in slots do
                        enshwick.log.info(`slot {slot.slot.item} x{slot.slot.count} bar {slot.actionbar}`)
                    end
                end
            end)
            "#,
        );
        assert_eq!(
            loader.client_commands(),
            [(1, enshwick::abi::ClientCommand::CRAFT, String::new())]
        );
        loader.set_client_snapshot(ClientSnapshot {
            state: ClientSnapshot::FRESH,
            available: ClientSnapshot::VITALS | ClientSnapshot::ACTIONBAR,
            session: 1,
            age_ms: 16,
            health: 90,
            health_max: 120,
            actionbar_slot: 2,
            ..ClientSnapshot::EMPTY
        });
        loader.set_client_slots(Some(vec![ClientSlot {
            slot: 0x41,
            item: 7,
            count: 3,
            category: 4,
            flags: ClientSlot::ACTIONBAR,
            ..ClientSlot::EMPTY
        }]));
        let _drawn = loader.draw_frame();
        for line in [
            "queued 1 twice false bare false",
            "health 90/120 bar 2",
            "slot 7 x3 bar true",
        ] {
            assert!(logged(&loader, line), "{line}: {:?}", loader.logs());
        }
    }

    #[test]
    fn a_script_hosts_a_world_by_its_list_place_and_reads_the_phase() {
        use enshwick::abi::ClientCommandStatus as Status;
        let loader = run(
            FakeHost::new().side(Side::CLIENT).player_input(),
            r#"
            local id = enshwick.input.client_command({ kind = "host", session = "0", world = 2, character = 0 })
            local bare = pcall(enshwick.input.client_command, { kind = "host", session = "0", world = 1 })
            enshwick.log.info(`queued {id} bare {bare}`)
            enshwick.ui.on_frame(function()
                local s = enshwick.input.command_status(id)
                enshwick.log.info(`host {s.state} {s.phase}`)
            end)
            "#,
        );
        assert_eq!(
            loader.client_commands(),
            [(1, enshwick::abi::ClientCommand::HOST, String::new())]
        );
        loader.set_command_phase(1, Status::PHASE_LOADING, 0);
        let _drawn = loader.draw_frame();
        for line in ["queued 1 bare false", "host pending loading"] {
            assert!(logged(&loader, line), "{line}: {:?}", loader.logs());
        }
    }

    #[test]
    fn a_script_sets_its_waypoint_then_pings_once_the_waypoint_is_observed() {
        use enshwick::abi::ClientCommandStatus as Status;
        let loader = run(
            FakeHost::new().side(Side::CLIENT).player_input(),
            r#"
            local set = enshwick.input.client_command({ kind = "set_waypoint", session = "1", position = vector.create(10, 5, 20) })
            local busy = pcall(enshwick.input.client_command, { kind = "ping", session = "1", position = vector.create(10, 5, 20) })
            local bare = pcall(enshwick.input.client_command, { kind = "ping", session = "1" })
            enshwick.log.info(`queued {set} busy {busy} bare {bare}`)
            enshwick.ui.on_frame(function()
                local s = enshwick.input.command_status(set)
                enshwick.log.info(`waypoint {s.state}`)
            end)
            "#,
        );
        assert_eq!(
            loader.client_commands(),
            [(1, enshwick::abi::ClientCommand::SET_WAYPOINT, String::new())]
        );
        loader.set_command_status(1, Status::OBSERVED, 0);
        let _drawn = loader.draw_frame();
        for line in ["queued 1 busy false bare false", "waypoint observed"] {
            assert!(logged(&loader, line), "{line}: {:?}", loader.logs());
        }
    }

    #[test]
    fn a_script_joins_a_server_and_reads_the_join_s_phase_and_failure() {
        use enshwick::abi::ClientCommandStatus as Status;
        let loader = run(
            FakeHost::new().side(Side::CLIENT).player_input(),
            r#"
            local join = enshwick.input.client_command({ kind = "join", session = "0", target = "127.0.0.1:15637", password = "hunter2" })
            local bare = pcall(enshwick.input.client_command, { kind = "join", session = "0" })
            enshwick.log.info(`queued {join} bare {bare}`)
            enshwick.ui.on_frame(function()
                local s = enshwick.input.command_status(join)
                enshwick.log.info(`join {s.state} {s.reason} {s.phase}`)
            end)
            "#,
        );
        assert_eq!(
            loader.client_commands(),
            [(
                1,
                enshwick::abi::ClientCommand::JOIN,
                "127.0.0.1:15637".to_owned()
            )]
        );
        loader.set_command_status(1, Status::FAILED, 0);
        loader.set_command_phase(1, Status::PHASE_MENU, Status::FULL);
        let _drawn = loader.draw_frame();
        for line in ["queued 1 bare false", "join failed full menu"] {
            assert!(logged(&loader, line), "{line}: {:?}", loader.logs());
        }
        assert!(
            !loader
                .logs()
                .iter()
                .any(|(_, line)| line.contains("hunter2")),
            "the password is never logged"
        );
    }

    #[test]
    fn a_script_reads_the_prompt_the_screen_s_host_and_the_local_held_controls() {
        let loader = run(
            FakeHost::new().side(Side::CLIENT).player_input(),
            r"
            enshwick.ui.on_frame(function(frame)
                local prompt = frame:prompt()
                local held = enshwick.input.local_held()
                enshwick.log.info(`prompt {prompt and prompt.title} {prompt and prompt.verb}`)
                enshwick.log.info(`host {frame:screen_host()} held {held and table.concat(held, ',')}`)
            end)
            enshwick.ui.on_screen_changed(function(change)
                enshwick.log.info(`changed to {change.now} of {change.host}`)
            end)
            ",
        );
        let _nothing_held = loader.draw_frame();
        loader.show_prompt(Some(("Chest", 7)));
        loader.hold_locally(Some(1 << 30));
        let _drawn = loader.draw_frame();
        loader.open_screen(Some(HudScreenType::StorageChest));
        loader.host_screen(Some(enshwick::Entity(512)));
        let _drawn = loader.draw_frame();
        for line in [
            "host nil held nil",
            "prompt Chest 7",
            "host nil held Block",
            "host 512 held Block",
            "changed to StorageChest of 512",
        ] {
            assert!(logged(&loader, line), "{line}: {:?}", loader.logs());
        }
    }

    #[test]
    fn a_script_s_first_frame_reads_the_held_controls_as_its_input_opened_at_start() {
        let loader = run(
            FakeHost::new().side(Side::CLIENT).player_input(),
            r"
            enshwick.ui.on_frame(function()
                local held = enshwick.input.local_held()
                enshwick.log.info(`first frame held {held and table.concat(held, ',')}`)
            end)
            ",
        );
        loader.hold_locally(Some(1 << 30));
        let _first = loader.draw_frame();
        assert!(
            logged(&loader, "first frame held Block"),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn a_script_draws_a_key_hint_and_a_panel_for_its_own_hosts_and_reads_where_the_host_stands() {
        let loader = run(
            FakeHost::new().side(Side::CLIENT),
            r#"
            local guard = enshwick.content.guid("you.guards:guard")
            enshwick.ui.extend_screen_for("StorageChest", { edge = "Right" }, { guard }, function(panel)
                panel:label({ text = "Ours" })
            end)
            enshwick.ui.on_frame(function(frame)
                local at = frame:screen_host_position()
                enshwick.log.info(`ours {frame:screen_host_template() == guard} x {at and at.x}`)
                frame:window("help", "Help", function(w)
                    enshwick.log.info(`hint {w:input_hint("ContextualAction", "Open")}`)
                end)
            end)
            "#,
        );
        loader.game_action("ContextualAction", true);
        loader.open_screen(Some(HudScreenType::StorageChest));
        loader.host_screen(Some(enshwick::Entity(512)));
        assert!(
            loader.draw_screen(HudScreenType::StorageChest).is_empty(),
            "no read yet"
        );
        loader.read_screen_host(Guid::named("you.guards:guard"), [12.5, 0.0, 3.0]);
        assert_eq!(loader.draw_screen(HudScreenType::StorageChest).len(), 1);
        let _drawn = loader.draw_frame();
        loader.read_screen_host(Guid::named("you.other:chest"), [1.0, 0.0, 0.0]);
        assert!(
            loader.draw_screen(HudScreenType::StorageChest).is_empty(),
            "another host"
        );
        for line in ["ours true x 12.5", "hint true"] {
            assert!(logged(&loader, line), "{line}: {:?}", loader.logs());
        }
    }

    #[test]
    fn a_script_draws_item_slots_reads_the_hovered_slot_and_adds_tooltip_lines() {
        let torch = enshwick::ItemId(0x5e01);
        let loader = run(
            FakeHost::new().side(Side::CLIENT).item(torch, 20, 0),
            r#"
            enshwick.ui.extend_screen("Backpack", { edge = "Left", align = "Start", shift = 362, inset = 125 }, function(panel)
                local input = panel:item_slot(0x5e01, 3)
                local slot = panel:hovered_slot()
                enshwick.log.info(`clicked {input.clicked} hovered {slot and slot.count}`)
            end)
            enshwick.ui.extend_item_tooltip(function(item, lines)
                lines:push({ text = `Uses left: {item.item}`, style = "Positive" })
            end)
            "#,
        );
        loader.slot_input(torch, Input::Click);
        loader.hover_slot(Some((0x3_0000_0041, torch, 2, enshwick::Entity(0))));
        let under_slots = enshwick::ui::PanelPlacement::new(enshwick::ui::Edge::Left)
            .align(enshwick::ui::Align::Start)
            .shift(362.0)
            .inset(125.0);
        assert_eq!(
            loader.screen_placements(HudScreenType::Backpack),
            [under_slots],
            "the script's placement table"
        );
        let drawn = loader.draw_screen(HudScreenType::Backpack);
        assert_eq!(
            drawn[0].1,
            [Drawn::ItemSlot {
                item: torch,
                count: 3
            }]
        );
        assert!(
            logged(&loader, "clicked true hovered 2"),
            "{:?}",
            loader.logs()
        );
        assert_eq!(
            loader.open_tooltip(torch, enshwick::Entity(900)),
            [Drawn::Label {
                text: format!("Uses left: {}", 0x5e01),
                style: UiWidget::STYLE_POSITIVE
            }]
        );
    }

    #[test]
    fn a_server_script_is_refused_the_interface() {
        let (loader, status) = start(FakeHost::new(), "enshwick.ui.on_frame(function() end)");
        assert!(!status.is_ok());
        assert!(
            loader
                .logs()
                .iter()
                .any(|(_, line)| line.contains("enshwick.ui is off here")
                    && line.contains("on clients only")
                    && !line.contains("game build")),
            "{:?}",
            loader.logs()
        );
    }
}
