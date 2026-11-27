//! What a player's move carries into the interaction filters' calls: the
//! moved stack, the action type, the transfer kind and both containers.

use enshwick_core::abi::Interaction;
use enshwick_core::raw::read_at;

use super::Layout;
use crate::engine::ecs::Components;
use crate::engine::loader::Loader;
use crate::inventory::stacks::{Stacks, slot_id, split};

/// Where the player's transfer action keeps what it moves, and the
/// interaction constants of the game's action types and transfer flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Moves {
    /// The input's `u8` action type, `u8` transfer flags and `u16` amount.
    action: usize,
    flags: usize,
    amount: usize,
    /// [`Interaction`]'s `ACTION_` constant of each `InventoryActionType`
    /// value, 0 for none.
    actions: [u8; 8],
    /// [`Interaction`]'s `TRANSFER_` bit of each `TransferItemToInventoryFlag`
    /// bit index, 0 for none.
    transfers: [u8; 8],
    stacks: Stacks,
}

impl Moves {
    /// The `InventoryActionType` values by name, with their constants.
    const ACTIONS: [(&str, u8); 5] = [
        ("One", Interaction::ACTION_ONE),
        ("Half", Interaction::ACTION_HALF),
        ("All", Interaction::ACTION_ALL),
        ("CustomAmount", Interaction::ACTION_AMOUNT),
        ("Destroy", Interaction::ACTION_DESTROY),
    ];
    /// The `TransferItemToInventoryFlag` bits by name, with their constants.
    const TRANSFERS: [(&str, u8); 5] = [
        ("Gifting", Interaction::TRANSFER_GIFT),
        ("RefillStacks", Interaction::TRANSFER_REFILL_STACKS),
        ("MoveAllStacks", Interaction::TRANSFER_MOVE_ALL_STACKS),
        ("ToActionbar", Interaction::TRANSFER_TO_ACTIONBAR),
        ("ToBackpack", Interaction::TRANSFER_TO_BACKPACK),
    ];

    /// Where the input's transfer action starting at `start` keeps its
    /// fields, with the constants and stacks of the running build.
    pub(super) fn resolve(loader: &Loader, source: usize, target: usize) -> Option<Self> {
        let reflection = &loader.reflection;
        let action = "keen::ecs::InventoryTransferAction";
        let field = |name: &str| {
            reflection
                .field_offset(action, name)
                .ok()
                .and_then(|v| usize::try_from(v).ok())
        };
        // The table's source and target must sit where reflection puts
        // them in the action, or its other fields are not where we read.
        let start = source.checked_sub(field("sourceEntityId")?)?;
        if start.checked_add(field("targetEntityId")?)? != target {
            return None;
        }
        let values = |ty: &str, names: &[(&str, u8)]| {
            constants(
                |name| {
                    reflection
                        .enum_value(ty, name)
                        .ok()
                        .and_then(|v| usize::try_from(v).ok())
                },
                names,
            )
        };
        Some(Self {
            action: start + field("type")?,
            flags: start + field("flags")?,
            amount: start + field("amount")?,
            actions: values("keen::ecs::InventoryActionType", &Self::ACTIONS)?,
            transfers: values("keen::ecs::TransferItemToInventoryFlag", &Self::TRANSFERS)?,
            stacks: Stacks::resolve(loader)?,
        })
    }

    /// The `ACTION_` constant of the game's action type `raw`; 0 for one
    /// this build does not name.
    fn action_of(&self, raw: u8) -> u8 {
        self.actions.get(usize::from(raw)).copied().unwrap_or(0)
    }

    /// The `TRANSFER_` mask of the game's transfer flags `raw`.
    fn transfer_of(&self, raw: u8) -> u8 {
        self.transfers
            .iter()
            .enumerate()
            .filter(|&(bit, _)| raw & (1 << bit) != 0)
            .fold(0, |mask, (_, &ours)| mask | ours)
    }
}

/// What one move carries into each of its calls.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Moved {
    item: u32,
    stack: u32,
    amount: u32,
    from: u32,
    to: u32,
    action: u8,
    transfer: u8,
    from_slot: u64,
    to_slot: u64,
}

impl Moved {
    /// Writes the move into `call`.
    pub(super) fn fill(&self, call: &mut Interaction) {
        call.moved = enshwick_core::ItemId::new(self.item);
        call.stack = self.stack;
        call.amount = self.amount;
        call.from = enshwick_core::Entity::from_engine(self.from);
        call.to = enshwick_core::Entity::from_engine(self.to);
        call.action = self.action;
        call.transfer = self.transfer;
        call.moving = 1;
        call.from_slot = self.from_slot;
        call.to_slot = self.to_slot;
    }

    /// Whether the item filters let `player` make the move: only a
    /// `Destroy` asks them, as a trash of the source slot's stack.
    pub(super) fn items_allow(
        &self,
        player: u32,
        allows: impl FnOnce(u32, u32, u32, u32, u32) -> bool,
    ) -> bool {
        let Some((inventory, index)) = split(self.from_slot) else {
            return true;
        };
        self.action != Interaction::ACTION_DESTROY
            || allows(player, self.item, self.stack, inventory, index.into())
    }
}

/// What the transfer in `input` moves, read in `ctx`.
///
/// # Safety
///
/// - `ctx` is the live system context of the running transfer, used on its
///   thread, and `state` its live action state.
/// - `input` is the player's live input the transfer reads.
#[expect(
    clippy::too_many_arguments,
    reason = "the transfer's own reads, passed as read"
)]
pub(super) unsafe fn moved(
    moves: &Moves,
    layout: &Layout,
    components: &Components,
    ctx: *mut u64,
    state: *mut u8,
    input: *const u8,
    ends: [u32; 2],
    slots: [u64; 2],
) -> Moved {
    // SAFETY: `state` is the transfer's live action state, by the caller's
    // contract; its `OwnerRelationship` index sits at the build table's
    // `access.state_owner_index` offset.
    let owner_index: u64 = unsafe { read_at(state, layout.state_owner_index) };
    let container = |end: u32, slot: u64| {
        if end != 0 || slot == 0 {
            return end;
        }
        // SAFETY: `ctx` is the transfer's live context on its thread and
        // `owner_index` comes from the same system's action state.
        unsafe { components.slot_root(ctx, slot, owner_index) }.unwrap_or(0)
    };
    // SAFETY: `ctx` is the transfer's live context on its thread.
    let (item, stack) =
        unsafe { moves.stacks.in_slot(components, ctx, slots[0]) }.unwrap_or((0, 0));
    // SAFETY: `input` is the player's live input; the transfer action's u8
    // type sits at the reflected `moves.action`.
    let action: u8 = unsafe { read_at(input, moves.action) };
    // SAFETY: `input` is the player's live input; the action's u8 flags sit
    // at the reflected `moves.flags`.
    let flags: u8 = unsafe { read_at(input, moves.flags) };
    // SAFETY: `input` is the player's live input; the action's u16 amount
    // sits at the reflected `moves.amount`.
    let amount: u16 = unsafe { read_at(input, moves.amount) };
    Moved {
        item,
        stack,
        amount: amount.into(),
        from: container(ends[0], slots[0]),
        to: container(ends[1], slots[1]),
        action: moves.action_of(action),
        transfer: moves.transfer_of(flags),
        from_slot: slot_id(slots[0]),
        to_slot: slot_id(slots[1]),
    }
}

/// Each of `names`' constants at the index `index` gives its name, 0 at the
/// others; `None` when a name has no index below 8, so a build that renames
/// one reads as a build without moves, never as moves of the wrong kind.
fn constants(index: impl Fn(&str) -> Option<usize>, names: &[(&str, u8)]) -> Option<[u8; 8]> {
    let mut map = [0u8; 8];
    for &(name, ours) in names {
        *map.get_mut(index(name)?)? = ours;
    }
    Some(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The game's indexes on server 26430253: action types by value,
    /// transfer flags by bit.
    fn game(names: &[(&str, u8)]) -> impl Fn(&str) -> Option<usize> {
        let order: Vec<String> = names.iter().map(|(name, _)| (*name).to_owned()).collect();
        move |name| order.iter().position(|n| n == name)
    }

    fn moves() -> Moves {
        Moves {
            action: 0,
            flags: 1,
            amount: 2,
            actions: constants(game(&Moves::ACTIONS), &Moves::ACTIONS).unwrap(),
            transfers: constants(game(&Moves::TRANSFERS), &Moves::TRANSFERS).unwrap(),
            stacks: Stacks::new(3, 12, 0, 4, 8).unwrap(),
        }
    }

    #[test]
    fn each_game_action_type_maps_to_its_constant_and_an_unknown_one_to_none() {
        let moves = moves();
        assert_eq!(moves.action_of(0), Interaction::ACTION_ONE);
        assert_eq!(moves.action_of(1), Interaction::ACTION_HALF);
        assert_eq!(moves.action_of(2), Interaction::ACTION_ALL);
        assert_eq!(moves.action_of(3), Interaction::ACTION_AMOUNT);
        assert_eq!(moves.action_of(4), Interaction::ACTION_DESTROY);
        assert_eq!(moves.action_of(5), 0);
        assert_eq!(moves.action_of(200), 0);
    }

    #[test]
    fn transfer_flags_map_by_bit_index_to_their_constants() {
        let moves = moves();
        assert_eq!(moves.transfer_of(0), 0, "a plain move");
        assert_eq!(moves.transfer_of(1), Interaction::TRANSFER_GIFT);
        assert_eq!(moves.transfer_of(2), Interaction::TRANSFER_REFILL_STACKS);
        assert_eq!(moves.transfer_of(4), Interaction::TRANSFER_MOVE_ALL_STACKS);
        assert_eq!(moves.transfer_of(8), Interaction::TRANSFER_TO_ACTIONBAR);
        assert_eq!(moves.transfer_of(0x10), Interaction::TRANSFER_TO_BACKPACK);
        assert_eq!(moves.transfer_of(0x80), 0, "a bit the build does not name");
    }

    #[test]
    fn only_a_destroy_asks_the_item_filters_about_the_source_stack() {
        let destroy = Moved {
            item: 0x0e17,
            stack: 7,
            action: Interaction::ACTION_DESTROY,
            from_slot: slot_id(0x0000_0003_0000_1234),
            ..Moved::default()
        };
        let asked = core::cell::Cell::new(None);
        let refuse = |player, item, count, inventory, index| {
            asked.set(Some((player, item, count, inventory, index)));
            false
        };
        assert!(!destroy.items_allow(10, refuse), "a refused trash keeps it");
        assert_eq!(asked.get(), Some((10, 0x0e17, 7, 0x1234, 3)));
        let all = Moved {
            action: Interaction::ACTION_ALL,
            ..destroy
        };
        assert!(
            all.items_allow(10, |_, _, _, _, _| false),
            "a plain move is not a trash"
        );
        let nothing = Moved {
            from_slot: 0,
            ..destroy
        };
        assert!(
            nothing.items_allow(10, |_, _, _, _, _| false),
            "no source, nothing destroyed"
        );
    }

    #[test]
    fn a_name_the_build_lacks_or_places_past_eight_resolves_no_constants() {
        let lacks = |name: &str| (name != "Half").then_some(0);
        assert_eq!(constants(lacks, &Moves::ACTIONS), None);
        assert_eq!(constants(|_| Some(8), &Moves::ACTIONS), None);
    }
}
