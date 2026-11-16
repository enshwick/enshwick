//! `enshwick.inventory.container_rule`, the move a script's interaction
//! filter sees, and `enshwick.inventory.show_pickup`.

#[cfg(test)]
mod tests {
    use enshwick::abi::{ContainerPut, Header, Interaction, PlayerInfo};
    use enshwick::events::WorldSaved;
    use enshwick::inventory::SlotRef;
    use enshwick::test::FakeHost;
    use enshwick::{Entity, ItemId};

    use crate::harness::{logged, run};

    const PLAYER: u32 = 2;
    const CHEST: u32 = 50;

    fn host() -> FakeHost {
        FakeHost::new()
            .world()
            .inventory_requests()
            .player(PlayerInfo {
                header: Header::of::<PlayerInfo>(),
                slot: 1,
                entity: Entity(PLAYER),
                key: enshwick::PlayerKey::new(7),
                permissions: 0,
                name_len: 0,
                name: [0; PlayerInfo::NAME_MAX],
            })
    }

    fn put(item: u32) -> ContainerPut {
        ContainerPut {
            header: Header::of::<ContainerPut>(),
            container: Entity(CHEST),
            container_item: ItemId::new(0x5e01),
            item: ItemId::new(item),
            count: 4,
            slot: SlotRef::new(Entity(CHEST + 1), 2).0,
        }
    }

    #[test]
    fn a_script_rules_which_items_its_container_takes() {
        let loader = run(
            host(),
            r"
            enshwick.inventory.container_rule(0x5e01, function(put)
                return put.item == 0x0e17 and put.count <= 10
            end)
            ",
        );
        assert!(loader.container_put(&put(0x0e17)));
        assert!(!loader.container_put(&put(0x0d01)));
    }

    #[test]
    fn a_script_refuses_take_all_by_the_kind_of_the_move() {
        let loader = run(
            host(),
            r#"
            enshwick.interactions.on_interaction(function(i)
                return not (i.moved and i.moved.transfer == "TakeAll")
            end)
            "#,
        );
        let take_all = Interaction {
            from: Entity(CHEST),
            to: Entity(PLAYER),
            action: Interaction::ACTION_ALL,
            transfer: Interaction::TRANSFER_MOVE_ALL_STACKS,
            moving: 1,
            ..Interaction::new(Interaction::MOVE, Entity(PLAYER), Entity(CHEST))
        };
        let deposit_all = Interaction {
            from: Entity(PLAYER),
            to: Entity(CHEST),
            ..take_all
        };
        assert!(!loader.interaction(&take_all));
        assert!(loader.interaction(&deposit_all));
    }

    #[test]
    fn a_script_shows_a_pickup_message_without_a_give() {
        let loader = run(
            host(),
            r#"enshwick.on("WorldSaved", function()
                local ok = pcall(enshwick.inventory.show_pickup, 2, 0x5e01, 3)
                print(`shown {ok}`)
            end)"#,
        );
        let _handled = loader.raise::<WorldSaved>(&());
        assert!(logged(&loader, "shown true"), "{:?}", loader.logs());
        assert_eq!(
            loader.pickups_shown(),
            [(Entity(PLAYER), ItemId::new(0x5e01), 3)]
        );
    }
}
