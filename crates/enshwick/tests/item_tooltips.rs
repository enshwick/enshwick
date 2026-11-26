//! Lines a mod adds to the game's item tooltips, per item.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{Side, Status, UiTooltipItem, UiWidget};
    use enshwick::inventory::ItemInstance;
    use enshwick::test::Drawn;
    use enshwick::ui::{Label, Style};
    use enshwick::{Entity, ItemId, Load, Mod, Off};

    const STAFF: ItemId = ItemId(0x5e01);
    const SWORD: ItemId = ItemId(0x5e02);

    /// Each item a handler heard, and whether its last push past the most
    /// lines was kept.
    static HEARD: Mutex<Vec<(ItemInstance, bool)>> = Mutex::new(Vec::new());

    struct Tooltips;

    impl Mod for Tooltips {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.extend_item_tooltip(|item, lines| {
                if item.item == STAFF {
                    let _shown = lines.push(Label::new("Uses left: 3").style(Style::Positive));
                }
                let mut past_most = true;
                if item.item == SWORD {
                    for _ in 0..UiTooltipItem::MAX_LINES {
                        let _shown = lines.push("Sharp");
                    }
                    past_most = lines.push("One too many");
                }
                HEARD.lock().unwrap().push((item, past_most));
            })?;
            load.extend_item_tooltip(|_, lines| {
                let _shown = lines.push("Second handler's line");
            })?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Tooltips,
        r#"
        id = "you.tooltips"
        side = "both"
        "#
    );

    fn started(side: Side) -> (enshwick::test::Installed, Status) {
        let loader = enshwick::test::FakeHost::new().side(side).install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Tooltips)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        (loader, status)
    }

    #[test]
    fn each_handler_adds_its_lines_to_the_item_s_tooltip_in_registration_order() {
        let (loader, status) = started(Side::CLIENT);
        assert_eq!(status, Status::OK);
        let staff = loader.open_tooltip(STAFF, Entity::from_engine(900));
        assert_eq!(
            staff,
            [
                Drawn::Label {
                    text: "Uses left: 3".into(),
                    style: UiWidget::STYLE_POSITIVE,
                },
                Drawn::Label {
                    text: "Second handler's line".into(),
                    style: UiWidget::STYLE_BODY,
                },
            ]
        );
        assert_eq!(
            *HEARD.lock().unwrap(),
            [(
                ItemInstance {
                    pide: Entity::from_engine(900),
                    item: STAFF,
                },
                true
            )]
        );
    }

    #[test]
    fn a_handler_s_lines_stop_at_the_most_one_tooltip_takes() {
        let (loader, status) = started(Side::CLIENT);
        assert_eq!(status, Status::OK);
        let sword = loader.open_tooltip(SWORD, Entity::from_engine(901));
        assert_eq!(sword.len(), UiTooltipItem::MAX_LINES as usize + 1);
        assert_eq!(
            HEARD.lock().unwrap().last().map(|&(_, kept)| kept),
            Some(false),
            "the push past the most is refused"
        );
    }

    #[test]
    fn an_item_with_no_entity_of_its_own_gets_no_lines() {
        let (loader, status) = started(Side::CLIENT);
        assert_eq!(status, Status::OK);
        assert!(
            loader
                .open_tooltip(STAFF, Entity::from_engine(0))
                .is_empty()
        );
        assert!(HEARD.lock().unwrap().is_empty(), "no handler ran");
    }

    #[test]
    fn a_server_turns_item_tooltips_away() {
        let (loader, status) = started(Side::SERVER);
        assert_ne!(status, Status::OK);
        assert!(
            loader
                .open_tooltip(STAFF, Entity::from_engine(900))
                .is_empty()
        );
    }
}
