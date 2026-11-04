//! Panels on the game's screens: drawn only while their screen is open, at
//! their edge, inside the window the loader opens for them.

#[cfg(test)]
mod tests {
    use core::ffi::c_void;
    use core::ptr::null_mut;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    use enshwick::abi::{Header, Side, Status, Str, UiExtension, UiFrame, UiWidget};
    use enshwick::test::{Drawn, Input};
    use enshwick::ui::{Edge, HudScreenType, Label, Style};
    use enshwick::{Load, Mod, Off};

    static SORTED: AtomicU32 = AtomicU32::new(0);
    static WINDOW_DRAWN: AtomicBool = AtomicBool::new(false);
    static REFUSED: AtomicBool = AtomicBool::new(false);

    struct Chests;

    impl Mod for Chests {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let added = load.extend_screen(HudScreenType::StorageChest, Edge::Right, |panel| {
                panel.label(Label::new("Shared").style(Style::Secondary));
                if panel.button("Sort") {
                    SORTED.fetch_add(1, Ordering::Relaxed);
                }
                if panel.window("nested", "Nested", |_| {}) {
                    WINDOW_DRAWN.store(true, Ordering::Relaxed);
                }
            });
            REFUSED.store(added.is_err(), Ordering::Relaxed);
            let host = load.host();
            let raw = |screen: &str, edge: u32| {
                let spec = UiExtension {
                    header: Header::of::<UiExtension>(),
                    screen: Str::new(screen),
                    edge,
                    reserved: 0,
                };
                (host.api().ui_extension)(host.handle(), &raw const spec, nothing, null_mut())
            };
            *MALFORMED.lock().unwrap() = vec![
                raw("Map", UiExtension::BOTTOM),
                raw("StorageChest", UiExtension::BOTTOM + 1),
                raw("", UiExtension::LEFT),
            ];
            Ok(Self)
        }
    }

    /// What the loader answered the raw extensions, the last two malformed.
    static MALFORMED: Mutex<Vec<Status>> = Mutex::new(Vec::new());

    unsafe extern "system" fn nothing(_frame: *const UiFrame, _user: *mut c_void) {}

    enshwick::test::declare_mod!(
        Chests,
        r#"
        id = "you.chests"
        side = "both"
        "#
    );

    #[test]
    fn a_panel_draws_with_its_screen_only_and_takes_no_window_of_its_own() {
        let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Chests)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert!(!REFUSED.load(Ordering::Relaxed));
        assert_eq!(
            *MALFORMED.lock().unwrap(),
            [Status::OK, Status::BAD_ARGS, Status::BAD_ARGS],
            "the bottom edge, an unknown edge and an empty screen name"
        );

        assert!(loader.draw_screen(HudScreenType::Backpack).is_empty());
        assert!(loader.draw_frame().is_empty());

        loader.input("Sort", Input::Click);
        assert_eq!(
            loader.draw_screen(HudScreenType::StorageChest),
            [(
                UiExtension::RIGHT,
                vec![
                    Drawn::Label {
                        text: "Shared".into(),
                        style: UiWidget::STYLE_SECONDARY,
                    },
                    Drawn::Button {
                        text: "Sort".into(),
                        enabled: true,
                    },
                ]
            )]
        );
        assert_eq!(SORTED.load(Ordering::Relaxed), 1);
        assert!(!WINDOW_DRAWN.load(Ordering::Relaxed));
    }

    #[test]
    fn a_server_turns_a_panel_away() {
        let loader = enshwick::test::FakeHost::new().side(Side::SERVER).install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Chests)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert!(REFUSED.load(Ordering::Relaxed));
        assert!(loader.draw_screen(HudScreenType::StorageChest).is_empty());
    }
}
