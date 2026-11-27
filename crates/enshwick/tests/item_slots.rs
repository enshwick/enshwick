//! Item slots a mod draws in its panels, and the slot of the game's own
//! inventory screen under the cursor.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU32, Ordering};

    use enshwick::abi::{Side, Status, UiExtension};
    use enshwick::inventory::{InventoryCategory, Slot, SlotRef};
    use enshwick::test::{Drawn, Input};
    use enshwick::ui::{Edge, HudScreenType, Label};
    use enshwick::{Entity, ItemId, Load, Mod, Off};

    const TORCH: ItemId = ItemId(0x5e01);
    const LOG: ItemId = ItemId(0x5e02);
    const UNKNOWN: ItemId = ItemId(0x7777);

    static CLICKED: AtomicU32 = AtomicU32::new(0);
    static HOVERED: AtomicU32 = AtomicU32::new(0);
    /// The slot of the game's screen each panel draw read.
    static UNDER_CURSOR: Mutex<Vec<Option<Slot>>> = Mutex::new(Vec::new());
    /// The slot of the game's screen each frame read.
    static IN_FRAME: Mutex<Vec<Option<Slot>>> = Mutex::new(Vec::new());

    struct Slots;

    impl Mod for Slots {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.extend_screen(HudScreenType::Backpack, Edge::Right, |panel| {
                for (item, count) in [(TORCH, 3), (LOG, 1), (UNKNOWN, 1), (TORCH, 0)] {
                    let input = panel.item_slot(item, count);
                    if input.clicked {
                        CLICKED.fetch_add(1, Ordering::Relaxed);
                    }
                    if input.hovered {
                        HOVERED.fetch_add(1, Ordering::Relaxed);
                    }
                }
                let slot = panel.hovered_slot();
                if let Some(slot) = slot {
                    panel.label(Label::new(&slot.count.to_string()));
                }
                UNDER_CURSOR.lock().unwrap().push(slot);
            })?;
            load.on_frame(|frame| IN_FRAME.lock().unwrap().push(frame.hovered_slot()))?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Slots,
        r#"
        id = "you.slots"
        side = "both"
        "#
    );

    fn started(side: Side) -> enshwick::test::Installed {
        let loader = enshwick::test::FakeHost::new()
            .side(side)
            .item(TORCH, 20, 0)
            .item(LOG, 50, 0)
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Slots)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        loader
    }

    #[test]
    fn a_panel_draws_the_slots_of_known_items_and_hears_their_clicks_and_hovers() {
        let loader = started(Side::CLIENT);
        loader.slot_input(TORCH, Input::Click);
        loader.slot_input(LOG, Input::Hover);
        let drawn = loader.draw_screen(HudScreenType::Backpack);
        assert_eq!(
            drawn,
            [(
                UiExtension::RIGHT,
                vec![
                    Drawn::ItemSlot {
                        item: TORCH,
                        count: 3,
                    },
                    Drawn::ItemSlot {
                        item: LOG,
                        count: 1
                    },
                ]
            )],
            "an item the client does not know and a count of 0 draw nothing"
        );
        assert_eq!(CLICKED.load(Ordering::Relaxed), 1);
        assert_eq!(
            HOVERED.load(Ordering::Relaxed),
            2,
            "a clicked slot is under the cursor too"
        );
    }

    #[test]
    fn a_frame_reads_the_slot_of_the_game_s_screen_under_the_cursor() {
        let loader = started(Side::CLIENT);
        let _nothing = loader.draw_screen(HudScreenType::Backpack);
        let at = SlotRef::new(Entity::from_engine(41), 3);
        loader.hover_slot(Some((at.0, LOG, 7, None)));
        let drawn = loader.draw_screen(HudScreenType::Backpack);
        assert!(drawn[0].1.contains(&Drawn::Label {
            text: "7".into(),
            style: enshwick::abi::UiWidget::STYLE_BODY,
        }));
        loader.hover_slot(Some((at.0, TORCH, 1, Entity::from_raw(900))));
        let _drawn = loader.draw_screen(HudScreenType::Backpack);
        let slot = |item, count, pide| Slot::new(at, InventoryCategory::INVALID, item, count, pide);
        assert_eq!(
            *UNDER_CURSOR.lock().unwrap(),
            [
                None,
                Some(slot(LOG, 7, None)),
                Some(slot(TORCH, 1, Some(Entity::from_engine(900)))),
            ]
        );
    }

    #[test]
    fn a_frame_with_no_game_screen_open_reads_no_slot_under_the_cursor() {
        let loader = started(Side::CLIENT);
        loader.hover_slot(Some((
            SlotRef::new(Entity::from_engine(41), 3).0,
            LOG,
            7,
            None,
        )));
        let _drawn = loader.draw_frame();
        assert_eq!(*IN_FRAME.lock().unwrap(), [None]);
    }

    #[test]
    fn a_server_turns_the_panel_with_its_slots_away() {
        let loader = enshwick::test::FakeHost::new().side(Side::SERVER).install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Slots)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_ne!(status, Status::OK);
    }
}
