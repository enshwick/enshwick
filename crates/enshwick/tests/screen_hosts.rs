//! A panel shows only for the screen hosts made from the templates its mod
//! names, and a frame reads its host's template and position.

#[cfg(test)]
mod tests {
    use core::ffi::c_void;
    use core::ptr::null_mut;
    use std::sync::Mutex;

    use enshwick::Entity;
    use enshwick::abi::{Header, Side, Status, Str, UiExtension, UiFrame};
    use enshwick::content::Guid;
    use enshwick::test::Drawn;
    use enshwick::ui::{Edge, HudScreenType};
    use enshwick::{Load, Mod, Off};

    const OURS: Guid = Guid([7; 16]);
    const THEIRS: Guid = Guid([8; 16]);

    /// A frame's host template and position.
    type Read = (Option<Guid>, Option<[f64; 3]>);

    static READS: Mutex<Vec<Read>> = Mutex::new(Vec::new());
    /// What the loader answered too many hosts, then a null host list with
    /// a count.
    static REFUSED: Mutex<Vec<Result<(), Status>>> = Mutex::new(Vec::new());

    struct Safes;

    impl Mod for Safes {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.extend_screen_for(HudScreenType::StorageChest, Edge::Right, &[OURS], |panel| {
                panel.label("Ours");
            })?;
            load.on_frame(|frame| {
                let read = (frame.screen_host_template(), frame.screen_host_position());
                READS.lock().unwrap().push(read);
            })?;
            let many = vec![OURS; UiExtension::MAX_HOSTS as usize + 1];
            let too_many = load
                .extend_screen_for(HudScreenType::StorageChest, Edge::Left, &many, |_| {})
                .map_err(|_| Status::BAD_ARGS);
            let host = load.host();
            let spec = UiExtension {
                header: Header::of::<UiExtension>(),
                screen: Str::new("StorageChest"),
                edge: UiExtension::LEFT,
                reserved: 0,
                hosts: core::ptr::null(),
                host_count: 1,
                reserved_hosts: 0,
            };
            let status =
                (host.api().ui_extension)(host.handle(), &raw const spec, nothing, null_mut());
            let null = if status == Status::OK {
                Ok(())
            } else {
                Err(status)
            };
            *REFUSED.lock().unwrap() = vec![too_many, null];
            Ok(Self)
        }
    }

    unsafe extern "system" fn nothing(_frame: *const UiFrame, _user: *mut c_void) {}

    enshwick::test::declare_mod!(
        Safes,
        r#"
        id = "you.safes"
        side = "both"
        "#
    );

    #[test]
    fn a_panel_shows_only_for_its_mod_s_hosts_and_a_frame_reads_where_the_host_stands() {
        let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Safes)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(
            *REFUSED.lock().unwrap(),
            [Err(Status::BAD_ARGS), Err(Status::BAD_ARGS)]
        );
        let ours = || {
            vec![(
                UiExtension::RIGHT,
                vec![Drawn::Label {
                    text: "Ours".into(),
                    style: 0,
                }],
            )]
        };

        assert!(
            loader.draw_screen(HudScreenType::StorageChest).is_empty(),
            "no host"
        );
        loader.host_screen(Some(Entity(512)));
        assert!(
            loader.draw_screen(HudScreenType::StorageChest).is_empty(),
            "a host no step read yet"
        );
        loader.read_screen_host(THEIRS, [1.0, 2.0, 3.0]);
        assert!(
            loader.draw_screen(HudScreenType::StorageChest).is_empty(),
            "another mod's host"
        );
        loader.read_screen_host(OURS, [10.5, -2.0, 300.25]);
        assert_eq!(loader.draw_screen(HudScreenType::StorageChest), ours());

        let _drawn = loader.draw_frame();
        loader.host_screen(None);
        let _drawn = loader.draw_frame();
        assert_eq!(
            *READS.lock().unwrap(),
            [(Some(OURS), Some([10.5, -2.0, 300.25])), (None, None)]
        );
    }
}
