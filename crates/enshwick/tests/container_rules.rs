//! A mod's rules over containers and moves against the fake loader: a
//! container rule asked per stack for its own container item only, the kind
//! of every move an interaction filter sees, a give whose pickup message is
//! hidden, and a pickup message without a give.

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use enshwick::abi::{ContainerPut, Interaction, InventoryRequest, PlayerInfo, Status};
    use enshwick::interactions::{InventoryActionType, Kind, Transfer};
    use enshwick::inventory::{Batch, Inventories, Op, SlotRef};
    use enshwick::test::FakeHost;
    use enshwick::{Entity, ItemId, WorldError};

    const ORE_BIN: ItemId = ItemId::new(0x5e01);
    const IRON_ORE: ItemId = ItemId::new(0x0e17);
    const WOOD: ItemId = ItemId::new(0x0d01);
    const PLAYER: u32 = 2;
    const CHEST: u32 = 50;

    static INVENTORIES: OnceLock<Inventories> = OnceLock::new();
    static MOVES: Mutex<Vec<enshwick::interactions::Interaction>> = Mutex::new(Vec::new());
    static REFUSED: Mutex<Vec<String>> = Mutex::new(Vec::new());

    struct Sorter;

    impl enshwick::Mod for Sorter {
        fn init(load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
            load.container_rule(ORE_BIN, |put| put.item == IRON_ORE)?;
            if let Err(off) = load.container_rule(ItemId::new(0), |_| true) {
                REFUSED.lock().unwrap().push(off.to_string());
            }
            load.on_interaction(|i| {
                if i.kind == Kind::Move {
                    MOVES.lock().unwrap().push(*i);
                }
                i.transfer() != Some(Transfer::TakeAll)
            })?;
            let _first = INVENTORIES.set(load.inventories()?);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Sorter,
        r#"
        id = "you.sorter"
        side = "server"
        "#
    );

    fn player() -> PlayerInfo {
        enshwick::abi::headed!(PlayerInfo {
            slot: 1,
            entity: Entity::from_engine(PLAYER),
            key: enshwick::PlayerKey::new(7),
            permissions: 0,
            name_len: 0,
            name: [0; PlayerInfo::NAME_MAX],
        })
    }

    fn start() -> enshwick::test::Installed {
        let loader = FakeHost::new()
            .world()
            .inventory_requests()
            .player(player())
            .install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Sorter)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    fn put(container_item: ItemId, item: ItemId) -> ContainerPut {
        enshwick::abi::headed!(ContainerPut {
            container: Entity::from_engine(CHEST),
            container_item,
            item,
            count: 20,
            slot: SlotRef::new(Entity::from_engine(CHEST + 1), 3).0,
        })
    }

    /// A move by the player from `from` to `to`, with the slots, action and
    /// transfer mask the loader reads.
    fn moving(from: u32, to: u32, slots: [u64; 2], action: u8, transfer: u8) -> Interaction {
        enshwick::abi::headed!(Interaction {
            moved: if slots[0] == 0 {
                ItemId::new(0)
            } else {
                IRON_ORE
            },
            stack: if slots[0] == 0 { 0 } else { 20 },
            amount: 0,
            from: Entity::from_engine(from),
            to: Entity::from_engine(to),
            action,
            transfer,
            moving: 1,
            from_slot: slots[0],
            to_slot: slots[1],
            ..Interaction::new(
                Interaction::MOVE,
                Entity::from_engine(PLAYER),
                Entity::from_engine(CHEST),
            )
        })
    }

    #[test]
    fn a_container_rule_answers_per_stack_for_its_own_container_item_only() {
        let loader = start();
        assert!(loader.container_put(&put(ORE_BIN, IRON_ORE)));
        assert!(!loader.container_put(&put(ORE_BIN, WOOD)));
        assert!(
            loader.container_put(&put(ItemId::new(0x5e02), WOOD)),
            "another container takes anything"
        );
        let refused = REFUSED.lock().unwrap().clone();
        assert_eq!(refused.len(), 1, "item 0 names no container: {refused:?}");
    }

    /// Every kind of move, with the kind a filter sees and whether the
    /// filter of this file lets it through.
    fn kinds(slot: u64, target: u64) -> [(Interaction, Transfer, bool); 9] {
        let all = Interaction::ACTION_ALL;
        [
            (
                moving(PLAYER, CHEST, [slot, target], Interaction::ACTION_HALF, 0),
                Transfer::Slot,
                true,
            ),
            (
                moving(
                    CHEST,
                    PLAYER,
                    [0, 0],
                    all,
                    Interaction::TRANSFER_MOVE_ALL_STACKS,
                ),
                Transfer::TakeAll,
                false,
            ),
            (
                moving(CHEST, PLAYER, [0, 0], all, 0),
                Transfer::TakeAll,
                false,
            ),
            (
                moving(
                    PLAYER,
                    CHEST,
                    [0, 0],
                    all,
                    Interaction::TRANSFER_MOVE_ALL_STACKS,
                ),
                Transfer::DepositAll,
                true,
            ),
            (
                moving(
                    PLAYER,
                    CHEST,
                    [0, 0],
                    all,
                    Interaction::TRANSFER_REFILL_STACKS,
                ),
                Transfer::DepositStacks,
                true,
            ),
            (
                moving(
                    CHEST,
                    PLAYER,
                    [0, 0],
                    all,
                    Interaction::TRANSFER_REFILL_STACKS,
                ),
                Transfer::RefillStacks,
                true,
            ),
            (
                moving(PLAYER, 9, [slot, 0], all, Interaction::TRANSFER_GIFT),
                Transfer::Gift,
                true,
            ),
            (
                moving(
                    PLAYER,
                    PLAYER,
                    [slot, 0],
                    all,
                    Interaction::TRANSFER_TO_ACTIONBAR,
                ),
                Transfer::ToActionbar,
                true,
            ),
            (
                moving(
                    PLAYER,
                    PLAYER,
                    [slot, 0],
                    all,
                    Interaction::TRANSFER_TO_BACKPACK,
                ),
                Transfer::ToBackpack,
                true,
            ),
        ]
    }

    #[test]
    fn a_filter_sees_the_kind_of_every_move_and_refuses_take_all() {
        let loader = start();
        let slot = SlotRef::new(Entity::from_engine(PLAYER + 1), 0).0;
        let target = SlotRef::new(Entity::from_engine(CHEST + 1), 5).0;
        let kinds = kinds(slot, target);
        for (call, transfer, allowed) in kinds {
            assert_eq!(loader.interaction(&call), allowed, "{transfer:?}");
            let seen = MOVES.lock().unwrap().pop().unwrap();
            assert_eq!(seen.transfer(), Some(transfer));
        }

        let half = moving(PLAYER, CHEST, [slot, target], Interaction::ACTION_HALF, 0);
        assert!(loader.interaction(&half));
        let seen = MOVES.lock().unwrap().pop().unwrap().moved.unwrap();
        assert_eq!(seen.action, Some(InventoryActionType::Half));
        assert_eq!((seen.item, seen.stack), (Some(IRON_ORE), 20));
        assert_eq!(
            (seen.from, seen.to),
            (
                Some(Entity::from_engine(PLAYER)),
                Some(Entity::from_engine(CHEST))
            )
        );
        assert_eq!(
            (seen.from_slot, seen.to_slot),
            (Some(SlotRef(slot)), Some(SlotRef(target)))
        );

        let unread = Interaction::new(
            Interaction::MOVE,
            Entity::from_engine(PLAYER),
            Entity::from_engine(CHEST),
        );
        assert!(loader.interaction(&unread));
        assert_eq!(
            MOVES.lock().unwrap().pop().unwrap().moved,
            None,
            "a move the loader did not read"
        );
    }

    #[test]
    fn a_hidden_give_still_notifies_and_asks_the_game_to_hide_the_message() {
        let loader = start();
        let inventories = INVENTORIES.get().unwrap();
        let give = Op::Give {
            to: Entity::from_engine(PLAYER),
            item: IRON_ORE,
            count: 5,
            fill: None,
        };
        let _ticket = inventories.submit(&Batch::new().with(give).hide_in_ui(), |_| {});
        let _ticket = inventories.submit(&Batch::new().with(give), |_| {});
        let flags: Vec<u32> = loader
            .inventory_requests()
            .iter()
            .map(|r| r.flags)
            .collect();
        assert_eq!(
            flags,
            [
                InventoryRequest::NOTIFY | InventoryRequest::HIDE_IN_UI,
                InventoryRequest::NOTIFY
            ]
        );
    }

    #[test]
    fn a_pickup_message_shows_for_a_player_and_holds_the_game_s_most() {
        let loader = start();
        let inventories = INVENTORIES.get().unwrap();
        assert_eq!(
            inventories.show_pickup(Entity::from_engine(PLAYER), IRON_ORE, 3),
            Ok(())
        );
        assert_eq!(
            inventories.show_pickup(Entity::from_engine(PLAYER), WOOD, 70_000),
            Ok(())
        );
        assert_eq!(
            inventories.show_pickup(Entity::from_engine(CHEST), IRON_ORE, 3),
            Err(WorldError::BadArgument),
            "a chest is no player"
        );
        assert_eq!(
            inventories.show_pickup(Entity::from_engine(PLAYER), IRON_ORE, 0),
            Err(WorldError::BadArgument)
        );
        assert_eq!(
            loader.pickups_shown(),
            [
                (Entity::from_engine(PLAYER), IRON_ORE, 3),
                (Entity::from_engine(PLAYER), WOOD, 65_535)
            ]
        );
    }

    #[test]
    fn a_pickup_message_off_the_game_thread_is_refused_as_the_loader_refuses_it() {
        let loader = start();
        let elsewhere = std::thread::spawn(|| {
            INVENTORIES
                .get()
                .unwrap()
                .show_pickup(Entity::from_engine(PLAYER), IRON_ORE, 3)
        })
        .join()
        .unwrap();
        assert_eq!(elsewhere, Err(WorldError::WrongThread));
        assert!(loader.pickups_shown().is_empty());
    }
}
