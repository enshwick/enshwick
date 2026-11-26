//! Where a panel sits beside its screen: the placement a mod asks for, the
//! offsets the loader refuses, and a spec shorter than every writer's.

#[cfg(test)]
mod tests {
    use core::ffi::c_void;
    use core::ptr::null_mut;
    use std::sync::Mutex;

    use enshwick::abi::{ABI_MAJOR, ABI_MINOR, Header, Side, Status, Str, UiExtension, UiFrame};
    use enshwick::ui::{Align, Edge, HudScreenType, PanelPlacement};
    use enshwick::{Load, Mod, Off};

    /// Whether each placement past the loader's bounds was refused.
    static REFUSED: Mutex<Vec<bool>> = Mutex::new(Vec::new());
    /// What the host answered a spec that ends before its placement.
    static SHORT: Mutex<Option<Status>> = Mutex::new(None);

    fn under_grid() -> PanelPlacement {
        PanelPlacement::new(Edge::Left)
            .align(Align::Start)
            .shift(362.0)
            .inset(125.0)
    }

    unsafe extern "system" fn nothing(_frame: *const UiFrame, _user: *mut c_void) {}

    struct Panels;

    impl Mod for Panels {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.extend_screen(HudScreenType::Backpack, under_grid(), |panel| {
                panel.label("Under the grid");
            })?;
            load.extend_screen(HudScreenType::Backpack, Edge::Right, |_| {})?;
            let max = UiExtension::MAX_OFFSET;
            let refused = [
                PanelPlacement::new(Edge::Top).shift(max + 1.0),
                PanelPlacement::new(Edge::Top).shift(-max - 1.0),
                PanelPlacement::new(Edge::Top).inset(-1.0),
                PanelPlacement::new(Edge::Top).inset(max + 1.0),
                PanelPlacement::new(Edge::Top).shift(f32::NAN),
            ]
            .map(|place| {
                load.extend_screen(HudScreenType::Backpack, place, |_| {})
                    .is_err()
            });
            *REFUSED.lock().unwrap() = refused.to_vec();
            // Its header ends where the placement's fields start, before
            // the end of what every writer of this major sends.
            let short = UiExtension {
                header: Header {
                    size: 48,
                    abi_major: ABI_MAJOR,
                    abi_minor: ABI_MINOR,
                },
                screen: Str::new("Backpack"),
                edge: UiExtension::BOTTOM,
                reserved: 0,
                hosts: core::ptr::null(),
                host_count: 0,
                reserved_hosts: 0,
                align: 9,
                shift: f32::NAN,
                inset: -1.0,
                reserved_place: 0,
            };
            let host = load.host();
            let status = (host.api().ui_extension)(
                host.handle(),
                &raw const short,
                Some(nothing),
                null_mut(),
            );
            *SHORT.lock().unwrap() = Some(status);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Panels,
        r#"
        id = "you.panels"
        side = "both"
        "#
    );

    #[test]
    fn a_panel_sits_where_its_mod_placed_it_and_an_offset_past_the_bounds_is_refused() {
        let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Panels)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(*REFUSED.lock().unwrap(), [true; 5]);
        assert_eq!(*SHORT.lock().unwrap(), Some(Status::BAD_ARGS));
        assert_eq!(
            loader.screen_placements(HudScreenType::Backpack),
            [under_grid(), PanelPlacement::from(Edge::Right)],
            "no panel from the short spec"
        );
        let drawn = loader.draw_screen(HudScreenType::Backpack);
        assert_eq!(drawn.len(), 1, "only the panel that drew");
        assert_eq!(drawn[0].0, UiExtension::LEFT);
    }
}
