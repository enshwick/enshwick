//! The HUD and map markers: items drawn each frame at their anchors, and a
//! mod's markers replaced whole.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicBool, Ordering};

    use enshwick::abi::{Side, Status, UiMarker, UiWidget};
    use enshwick::test::{Drawn, FakeMarker};
    use enshwick::ui::{Anchor, Label, Marker, MarkerColor, Style, Ui};
    use enshwick::{Load, Mod, Off};

    static UI: OnceLock<Ui> = OnceLock::new();
    static RAID: AtomicBool = AtomicBool::new(true);

    struct Raid;

    impl Mod for Raid {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.raid", side: CLIENT)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = UI.set(load.ui()?);
            load.on_hud(|hud| {
                if RAID.load(Ordering::Relaxed) {
                    hud.text(
                        Anchor::TopRight,
                        Label::new("Raid in 2:00").style(Style::Warning),
                    );
                    hud.progress(Anchor::TopRight, 1.5);
                }
                hud.text(Anchor::BottomLeft, "Day 3");
            })?;
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Raid);

    #[test]
    fn hud_items_show_while_drawn_and_markers_replace_each_other() {
        let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);

        assert_eq!(
            loader.draw_hud(),
            [
                (
                    UiWidget::TOP_RIGHT,
                    Drawn::Label {
                        text: "Raid in 2:00".into(),
                        style: UiWidget::STYLE_WARNING
                    }
                ),
                (UiWidget::TOP_RIGHT, Drawn::Progress { fraction: 1.0 }),
                (
                    UiWidget::BOTTOM_LEFT,
                    Drawn::Label {
                        text: "Day 3".into(),
                        style: UiWidget::STYLE_BODY
                    }
                ),
            ]
        );
        RAID.store(false, Ordering::Relaxed);
        assert_eq!(loader.draw_hud().len(), 1);

        let ui = UI.get().unwrap();
        ui.set_markers(&[
            Marker::new([1.0, 2.0, -3.0])
                .label("Safe")
                .color(MarkerColor::Green),
            Marker::new([0.5, 0.0, 0.0]),
        ])
        .unwrap();
        assert_eq!(
            loader.markers(),
            [
                FakeMarker {
                    position: [1 << 32, 2 << 32, -(3 << 32)],
                    label: "Safe".into(),
                    color: UiMarker::GREEN
                },
                FakeMarker {
                    position: [1 << 31, 0, 0],
                    label: String::new(),
                    color: UiMarker::WHITE
                },
            ]
        );
        ui.clear_markers().unwrap();
        assert!(loader.markers().is_empty());
        let many = vec![Marker::new([0.0; 3]); enshwick::ui::MAX_MARKERS + 1];
        assert!(ui.set_markers(&many).is_err());
    }
}
