//! An example mod for the game's own item widgets: three item slots in a
//! panel beside the backpack, the backpack slot under the cursor, and lines
//! in the game's item tooltips, one from the mod's item data and one on a
//! vanilla sword.

use enshwick::ItemId;
use enshwick::content::vanilla;
use enshwick::item_data::ItemData;
use enshwick::ui::{Edge, HudScreenType, Label, Style};
use enshwick::{Load, Mod, Off};

/// The vanilla items the panel shows, by debug name, with a count each.
const SHOWN: [(&str, u32); 3] = [
    ("Tool_Torch01", 3),
    ("Material_T1_Wood_Logs", 20),
    ("Prop_Storage_24_T1_Box", 1),
];
/// The vanilla sword whose tooltip gets a line.
const SWORD: &str = "Weapon_Old_Sword";

/// The mod: everything happens in its handlers.
#[derive(Debug)]
pub struct ItemUi;

impl Mod for ItemUi {
    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        // The server keeps and sends the uses; a client reads what it got.
        let uses: ItemData<u16> = load.item_data("uses")?;
        if load.host().build().side != enshwick::abi::Side::CLIENT {
            return Ok(Self);
        }
        let item = |name: &str| vanilla::item(name).map(ItemId::from_guid);
        let shown: Vec<(ItemId, u32)> = SHOWN
            .iter()
            .filter_map(|&(name, count)| item(name).ok().map(|id| (id, count)))
            .collect();
        load.extend_screen(HudScreenType::Backpack, Edge::Right, move |panel| {
            for &(item, count) in &shown {
                if panel.item_slot(item, count).clicked {
                    enshwick::info!("slot of item {} clicked", item.get());
                }
            }
            if let Some(slot) = panel.hovered_slot() {
                let line = format!("Slot {} holds {}", slot.at.index(), slot.count);
                panel.label(Label::new(&line).style(Style::Secondary));
            }
        })?;
        let sword = item(SWORD).ok();
        load.extend_item_tooltip(move |item, lines| {
            if let Some(left) = uses.get(item) {
                let line = format!("Uses left: {left}");
                let _shown = lines.push(Label::new(&line).style(Style::Positive));
            }
            if Some(item.item) == sword {
                let _shown =
                    lines.push(Label::new("Kept sharp by item-ui").style(Style::Secondary));
            }
        })?;
        Ok(Self)
    }
}

enshwick::declare_mod!(ItemUi);

#[cfg(test)]
mod tests {
    use enshwick::abi::{Side, Status, UiExtension};
    use enshwick::content::Guid;
    use enshwick::inventory::SlotRef;
    use enshwick::test::Drawn;
    use enshwick::{Entity, ItemId};

    use super::*;

    const TORCH: Guid = Guid([1; 16]);
    const LOGS: Guid = Guid([2; 16]);
    const BOX: Guid = Guid([3; 16]);
    const OLD_SWORD: Guid = Guid([4; 16]);

    fn start(side: Side) -> enshwick::test::Installed {
        let mut host = enshwick::test::FakeHost::new()
            .side(side)
            .items()
            .workshops()
            .vanilla_items(&[
                (TORCH, "Tool_Torch01"),
                (LOGS, "Material_T1_Wood_Logs"),
                (BOX, "Prop_Storage_24_T1_Box"),
                (OLD_SWORD, SWORD),
            ]);
        for guid in [TORCH, LOGS, BOX, OLD_SWORD] {
            host = host.item(ItemId::from_guid(guid), 20, 0);
        }
        let loader = host.install();
        // SAFETY: the exports of this crate's `declare_mod!(ItemUi)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        loader
    }

    #[test]
    fn the_panel_beside_the_backpack_shows_three_slots_and_the_slot_under_the_cursor() {
        let loader = start(Side::CLIENT);
        let at = SlotRef::new(Entity(41), 3);
        loader.hover_slot(Some((at.0, ItemId::from_guid(LOGS), 7, Entity(0))));
        let drawn = loader.draw_screen(HudScreenType::Backpack);
        assert_eq!(drawn.len(), 1);
        assert_eq!(drawn[0].0, UiExtension::RIGHT);
        let slots: Vec<_> = drawn[0]
            .1
            .iter()
            .filter(|d| matches!(d, Drawn::ItemSlot { .. }))
            .collect();
        assert_eq!(slots.len(), 3);
        assert!(drawn[0].1.contains(&Drawn::Label {
            text: "Slot 3 holds 7".into(),
            style: enshwick::abi::UiWidget::STYLE_SECONDARY,
        }));
    }

    #[test]
    fn the_vanilla_sword_s_tooltip_gets_a_line_and_another_item_none() {
        let loader = start(Side::CLIENT);
        let sword = loader.open_tooltip(ItemId::from_guid(OLD_SWORD), Entity(900));
        assert!(
            matches!(&sword[..], [Drawn::Label { text, .. }] if text == "Kept sharp by item-ui")
        );
        assert!(
            loader
                .open_tooltip(ItemId::from_guid(TORCH), Entity(901))
                .is_empty()
        );
    }

    #[test]
    fn a_server_keeps_the_uses_and_draws_nothing() {
        let loader = start(Side::SERVER);
        assert!(loader.draw_screen(HudScreenType::Backpack).is_empty());
    }
}
