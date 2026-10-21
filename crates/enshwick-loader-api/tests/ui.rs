//! The interface: a mod's window drawn each frame, inputs reaching its
//! widgets, and dialogs answered.

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Mutex, OnceLock};

    use enshwick_loader_api::abi::{Side, Status, UiDialog, UiWidget};
    use enshwick_loader_api::test::{Drawn, Input};
    use enshwick_loader_api::ui::{Dialog, Dropdown, Label, Response, Slider, Style, Toggle, Ui};
    use enshwick_loader_api::{Load, Mod, Off};

    static UI: OnceLock<Ui> = OnceLock::new();
    static ANSWERS: Mutex<Vec<Option<Response>>> = Mutex::new(Vec::new());
    static CLOSED: AtomicU32 = AtomicU32::new(0);

    struct Panel;

    impl Mod for Panel {
        fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
            enshwick_loader_api::manifest!(id: "panel", side: CLIENT)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = UI.set(load.ui()?);
            let mut open = true;
            let mut fast = false;
            let mut volume = 0.5f32;
            let mut mode = 0usize;
            let mut wild = 0.5f32;
            let mut reversed = f32::NAN;
            load.on_frame(move |frame| {
                if !open {
                    return;
                }
                frame.window("main", "Panel", |w| {
                    w.label(Label::new("Settings").style(Style::H2));
                    w.toggle(Toggle::new("Fast"), &mut fast);
                    w.slider(Slider::new("Volume", 0.0..=1.0).steps(11), &mut volume);
                    w.dropdown(Dropdown::new("Mode", &["Off", "On"]), &mut mode);
                    assert!(!w.slider(Slider::new("Wild", f32::NAN..=1.0), &mut wild));
                    w.slider(Slider::new("Reversed", 10.0..=0.0), &mut reversed);
                    w.progress(f32::NAN);
                    w.toggle(Toggle::new("Synced").read_only(true), &mut fast);
                    w.progress(volume);
                    if w.button("Close") {
                        CLOSED.fetch_add(1, Ordering::Relaxed);
                        open = false;
                    }
                });
            })?;
            Ok(Self)
        }
    }

    enshwick_loader_api::declare_mod!(Panel);

    #[test]
    fn a_window_draws_each_frame_takes_inputs_and_closes() {
        let loader = enshwick_loader_api::test::FakeHost::new()
            .side(Side::CLIENT)
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);

        let first = loader.draw_frame();
        assert_eq!(
            first.first(),
            Some(&Drawn::Window {
                id: "main".into(),
                title: "Panel".into()
            })
        );
        assert_eq!(
            first[1],
            Drawn::Label {
                text: "Settings".into(),
                style: UiWidget::STYLE_H2
            }
        );
        assert_eq!(first.last(), Some(&Drawn::End));
        assert!(
            !first
                .iter()
                .any(|d| matches!(d, Drawn::Slider { label, .. } if label == "Wild"))
        );
        assert!(first.iter().any(|d| matches!(
            d,
            Drawn::Slider { label, value, min, max, .. }
                if label == "Reversed" && [*value, *min, *max].map(f32::to_bits) == [0.0f32, 0.0, 10.0].map(f32::to_bits)
        )));
        assert!(first.contains(&Drawn::Progress { fraction: 0.0 }));

        loader.input("Fast", Input::Toggle(true));
        loader.input("Volume", Input::Value(0.8));
        loader.input("Wild", Input::Value(0.7));
        loader.input("Reversed", Input::Value(12.0));
        loader.input("Mode", Input::Pick(1));
        loader.input("Synced", Input::Toggle(false));
        let second = loader.draw_frame();
        assert!(second.contains(&Drawn::Toggle {
            label: "Fast".into(),
            on: true,
            read_only: false
        }));
        assert!(second.iter().any(
            |d| matches!(d, Drawn::Slider { label, value, steps: 11, .. } if label == "Volume" && (*value - 0.8).abs() < 1e-6)
        ));
        assert!(
            second
                .iter()
                .any(|d| matches!(d, Drawn::Dropdown { label, index: 1, .. } if label == "Mode"))
        );
        assert!(second.iter().any(
            |d| matches!(d, Drawn::Slider { label, value, .. } if label == "Reversed" && value.to_bits() == 10.0f32.to_bits())
        ));

        // A read-only row keeps its value: the mod's `fast` stayed on.
        let third = loader.draw_frame();
        assert!(third.contains(&Drawn::Toggle {
            label: "Synced".into(),
            on: true,
            read_only: true
        }));

        loader.input("Close", Input::Click);
        assert!(!loader.draw_frame().is_empty());
        assert_eq!(CLOSED.load(Ordering::Relaxed), 1);
        assert!(loader.draw_frame().is_empty());

        let ui = UI.get().unwrap();
        let ask = Dialog::new("Updates", "Download them now?")
            .detail("safes 0.1.0 \u{2192} 0.2.0")
            .buttons(&[Response::Ok, Response::Cancel]);
        ui.dialog(&ask, |answer| ANSWERS.lock().unwrap().push(answer))
            .unwrap();
        ui.dialog(&Dialog::new("Note", "Done."), |answer| {
            ANSWERS.lock().unwrap().push(answer);
        })
        .unwrap();
        let waiting = loader.dialogs();
        assert_eq!(waiting.len(), 2);
        assert_eq!(waiting[0].detail, "safes 0.1.0 \u{2192} 0.2.0");
        assert_eq!(
            waiting[0].buttons,
            1 << UiDialog::OK | 1 << UiDialog::CANCEL
        );
        assert_eq!(waiting[1].buttons, 1 << UiDialog::OK);
        assert!(loader.answer(Some(Response::Cancel)));
        assert!(loader.answer(None));
        assert!(!loader.answer(Some(Response::Ok)));
        assert_eq!(*ANSWERS.lock().unwrap(), [Some(Response::Cancel), None]);
    }
}
