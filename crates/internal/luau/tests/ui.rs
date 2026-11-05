//! `enshwick.ui`: a script's window drawn each frame, inputs reaching its
//! widgets, a HUD, a panel on a game screen, dialogs and map markers, as a
//! Rust mod's.

#[cfg(test)]
mod tests {
    use enshwick::abi::{MapMarkerTypeInfo, Side, UiDialog, UiMarker, UiWidget};
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
        enshwick.ui.extend_screen("StorageChest", "Right", function(panel)
            panel:label({ text = "Shared with your guild", style = "Secondary" })
            if panel:button({ text = "Sort", enabled = false }) then
                enshwick.log.info("sorted")
            end
        end)
        local ok, why = pcall(enshwick.ui.extend_screen, "InGame", "Left", function() end)
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

        loader.input("Close", Input::Click);
        let closing = loader.draw_frame();
        assert!(
            closing.contains(&Drawn::Sound(UiWidget::SOUND_CONFIRM)),
            "{closing:?}"
        );
        assert!(loader.draw_frame().is_empty(), "closed");
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
        assert_eq!(markers[0].position, [1 << 32, 2 << 32, 3 << 32]);
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
    fn a_server_script_is_refused_the_interface() {
        let (loader, status) = start(FakeHost::new(), "enshwick.ui.on_frame(function() end)");
        assert!(!status.is_ok());
        assert!(
            loader
                .logs()
                .iter()
                .any(|(_, line)| line.contains("on clients only")),
            "{:?}",
            loader.logs()
        );
    }
}
