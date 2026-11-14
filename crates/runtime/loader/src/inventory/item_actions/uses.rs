//! The uses every filter allowed, watched until the game takes their item.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use enshwick_core::abi::ItemAction;
use enshwick_core::raw::read_at;

use super::Layout;
use enshwick_base::Relock;

/// How long a use every filter allowed waits for the game to take its item
/// before the loader takes it as refused.
const USE_WAIT: Duration = Duration::from_secs(5);
/// Uses watched at most at once, and of one player; a use past them raises
/// no `CONSUMED`.
const MAX_WATCHED: usize = 64;
const MAX_WATCHED_PER_PLAYER: usize = 8;

/// A use every filter allowed, watched until the game takes its item from
/// the slot: the game refuses some only after the filters and keeps the
/// item, as a healing potion at full health on client 26430625 and server
/// 26430253.
#[derive(Clone, Copy, Debug)]
struct Use {
    call: ItemAction,
    /// The use's version in the player's input, so a use is watched once.
    version: u32,
    /// What the slot held when the use came.
    item: u32,
    count: u32,
    /// The player's other handled actions then, as [`moves`] gives them.
    moves: u64,
    until: Instant,
}

/// A hash of every handled version in `consumed` but the item action's;
/// `None` on a build that does not reflect where they sit.
///
/// # Safety
///
/// - `consumed` is a player's live `ServerConsumedPlayerInput` component,
///   read on the game thread.
pub(super) unsafe fn moves(layout: &Layout, consumed: *const u8) -> Option<u64> {
    use core::hash::Hasher;
    let (start, end) = layout.moves?;
    let mut hash = rustc_hash::FxHasher::default();
    for at in (start..end.saturating_sub(3))
        .step_by(4)
        .filter(|&at| at != layout.item.consumed)
    {
        // SAFETY: `consumed` is the live component (the caller's contract),
        // reflected as `end` bytes, and `at + 4` is at most `end`.
        hash.write_u32(unsafe { read_at(consumed, at) });
    }
    Some(hash.finish())
}

/// What a watched use's slot shows now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Seen {
    /// The game took the item: its stack went down or left the slot.
    Taken,
    /// Nothing yet.
    Waiting,
    /// The time ran out, the inventory is gone or the player did something
    /// else: the game refused the use, or the slot can no longer prove it
    /// took the item.
    Refused,
}

impl Use {
    /// Reads the slot's `now`, `None` when it proves nothing, at `at`; a
    /// stack that grew, as by a pickup, is the new count to go down from.
    fn see(&mut self, now: Option<(u32, u32)>, at: Instant) -> Seen {
        let Some((item, count)) = now else {
            return Seen::Refused;
        };
        if item != self.item || count < self.count {
            return Seen::Taken;
        }
        self.count = count;
        if at >= self.until {
            Seen::Refused
        } else {
            Seen::Waiting
        }
    }

    fn same_slot(&self, other: &Self) -> bool {
        self.call.inventory == other.call.inventory && self.call.slot == other.call.slot
    }
}

/// The uses the loader watches, whether it watches any, so a step without
/// one takes no lock, and whether it said it watches too many.
pub(super) struct Uses {
    watched: Mutex<Vec<Use>>,
    any: AtomicBool,
    full: AtomicBool,
}

impl Uses {
    const fn new() -> Self {
        Self {
            watched: Mutex::new(Vec::new()),
            any: AtomicBool::new(false),
            full: AtomicBool::new(false),
        }
    }

    /// Starts watching `call`, the use of `version` whose slot holds
    /// `count` of `item`, while the player's other handled actions are
    /// `moves`; once per use, and not at all without `moves`.
    pub(super) fn watch(
        &self,
        call: ItemAction,
        version: u32,
        item: u32,
        count: u32,
        moves: Option<u64>,
        at: Instant,
    ) {
        let Some(moves) = moves else {
            return;
        };
        let mut watched = self.watched.relock();
        let of_player = watched.iter().filter(|u| u.call.player == call.player);
        if of_player.clone().any(|u| u.version == version) {
            return;
        }
        if watched.len() >= MAX_WATCHED || of_player.count() >= MAX_WATCHED_PER_PLAYER {
            if !self.full.swap(true, Ordering::Relaxed) {
                crate::log!(
                    WARN,
                    "too many item uses wait for the game: those past them raise no consumed"
                );
            }
            return;
        }
        watched.push(Use {
            call,
            version,
            item,
            count,
            moves,
            until: at + USE_WAIT,
        });
        self.any.store(true, Ordering::Release);
    }

    /// The watched uses whose item the game took, read through `slot_now`
    /// with the player's other handled actions, each item gone from a stack
    /// paying one use of its slot, oldest first; refused ones are let go,
    /// and so are those whose player did anything else since: a stack
    /// dragged or stashed out of its slot looks like one the game took.
    pub(super) fn taken(
        &self,
        at: Instant,
        mut slot_now: impl FnMut(&ItemAction) -> Option<((u32, u32), Option<u64>)>,
    ) -> Vec<ItemAction> {
        if !self.any.load(Ordering::Acquire) {
            return Vec::new();
        }
        let mut watched = self.watched.relock();
        let mut done = Vec::new();
        // Per slot that went down this pass: what it holds now, and how many
        // more of its watched uses the items gone from it pay.
        let mut paying: Vec<(Use, (u32, u32), u32)> = Vec::new();
        watched.retain_mut(|used| {
            if let Some((_, (item, count), more)) =
                paying.iter_mut().find(|(paid, ..)| paid.same_slot(used))
            {
                if *more > 0 {
                    *more -= 1;
                    done.push(used.call);
                    return false;
                }
                used.item = *item;
                used.count = *count;
                return true;
            }
            let now = slot_now(&used.call)
                .filter(|&(_, moves)| moves == Some(used.moves))
                .map(|(stack, _)| stack);
            match (used.see(now, at), now) {
                (Seen::Waiting, _) => true,
                (Seen::Taken, Some((item, count))) => {
                    let gone = if item == used.item {
                        used.count - count
                    } else {
                        used.count
                    };
                    paying.push((*used, (item, count), gone.saturating_sub(1)));
                    done.push(used.call);
                    false
                }
                _ => false,
            }
        });
        self.any.store(!watched.is_empty(), Ordering::Release);
        done
    }

    /// Forgets the watched uses, when a world ends.
    fn forget(&self) {
        self.watched.relock().clear();
        self.any.store(false, Ordering::Release);
        self.full.store(false, Ordering::Relaxed);
    }
}

pub(super) static USES: Uses = Uses::new();

/// Forgets the watched uses, when a world ends.
pub(crate) fn forget() {
    USES.forget();
}

#[cfg(test)]
mod tests {
    use enshwick_core::abi::Header;

    use super::super::tests::LAYOUT;
    use super::*;

    fn drink(inventory: u32, slot: u32) -> ItemAction {
        ItemAction {
            header: Header::of::<ItemAction>(),
            kind: ItemAction::CONSUMED,
            player: enshwick_core::Entity(1),
            item: enshwick_core::ItemId::new(7),
            count: 1,
            inventory: enshwick_core::Entity(inventory),
            slot,
        }
    }

    #[test]
    fn a_use_is_consumed_only_once_the_game_takes_its_item() {
        let at = Instant::now();
        let mut used = Use {
            call: drink(100, 0),
            version: 1,
            item: 7,
            count: 5,
            moves: STILL,
            until: at + USE_WAIT,
        };
        assert_eq!(used.see(Some((7, 5)), at), Seen::Waiting, "kept so far");
        assert_eq!(used.see(Some((7, 6)), at), Seen::Waiting, "a pickup");
        assert_eq!(used.see(Some((7, 5)), at), Seen::Taken, "down from six");
        assert_eq!(used.see(Some((0, 0)), at), Seen::Taken, "the last one");
        assert_eq!(
            used.see(Some((9, 1)), at),
            Seen::Taken,
            "another item there"
        );
        assert_eq!(used.see(None, at), Seen::Refused, "the inventory went");
        assert_eq!(
            used.see(Some((7, 6)), at + USE_WAIT),
            Seen::Refused,
            "the game kept it"
        );
    }

    /// The player's other handled actions while none ran.
    const STILL: u64 = 3;

    /// Watches use `version` of slot `slot` holding `count` of item 7.
    fn watch_use(uses: &Uses, version: u32, slot: u32, count: u32, at: Instant) {
        uses.watch(drink(100, slot), version, 7, count, Some(STILL), at);
    }

    #[test]
    fn a_refused_use_is_let_go_and_one_stack_down_pays_one_use() {
        let uses = Uses::new();
        let at = Instant::now();
        // A potion at full health: the game keeps all five.
        watch_use(&uses, 1, 0, 5, at);
        assert!(uses.taken(at, |_| Some(((7, 5), Some(STILL)))).is_empty());
        assert!(
            uses.taken(at + USE_WAIT, |_| Some(((7, 5), Some(STILL))))
                .is_empty()
        );
        assert!(!uses.any.load(Ordering::Acquire), "let go");
        // Two uses of one stack, then one more of another slot: the stack
        // goes down once.
        watch_use(&uses, 2, 0, 5, at);
        watch_use(&uses, 3, 0, 5, at);
        watch_use(&uses, 4, 1, 2, at);
        let slots =
            |call: &ItemAction| Some((if call.slot == 0 { (7, 4) } else { (7, 2) }, Some(STILL)));
        assert_eq!(uses.taken(at, slots).len(), 1, "one use paid");
        assert!(
            uses.taken(at, slots).is_empty(),
            "the other waits for the next one"
        );
        let slots =
            |call: &ItemAction| Some((if call.slot == 0 { (7, 3) } else { (7, 1) }, Some(STILL)));
        assert_eq!(uses.taken(at, slots).len(), 2, "both stacks went down");
        assert!(!uses.any.load(Ordering::Acquire));
        // Two uses of one stack that the game ran in one step: down by two.
        for version in 5..8 {
            watch_use(&uses, version, 0, 5, at);
        }
        assert_eq!(
            uses.taken(at, |_| Some(((7, 3), Some(STILL)))).len(),
            2,
            "two items gone, two uses"
        );
        assert_eq!(
            uses.taken(at, |_| Some(((0, 0), Some(STILL)))).len(),
            1,
            "the stack went"
        );
        assert!(!uses.any.load(Ordering::Acquire));
    }

    #[test]
    fn a_refused_use_whose_stack_the_player_drags_away_pays_nothing() {
        let uses = Uses::new();
        let at = Instant::now();
        // A potion at full health, then the stack dragged out of its slot,
        // split in half, stashed or swapped with another item: the player's
        // other handled actions changed.
        for stack in [(0, 0), (7, 2), (0, 0), (9, 1)] {
            watch_use(&uses, 1, 0, 5, at);
            assert!(uses.taken(at, |_| Some(((7, 5), Some(STILL)))).is_empty());
            assert!(
                uses.taken(at, |_| Some((stack, Some(STILL + 1))))
                    .is_empty(),
                "{stack:?} pays no use"
            );
            assert!(!uses.any.load(Ordering::Acquire), "{stack:?}: let go");
        }
        // A move before the game took the item pays nothing either: the
        // slot alone cannot tell the two apart.
        watch_use(&uses, 1, 0, 5, at);
        assert!(
            uses.taken(at, |_| Some(((7, 4), Some(STILL + 1))))
                .is_empty()
        );
        // Nor does a build that does not reflect the handled actions.
        uses.watch(drink(100, 0), 1, 7, 5, None, at);
        assert!(!uses.any.load(Ordering::Acquire), "never watched");
        watch_use(&uses, 1, 0, 5, at);
        assert!(uses.taken(at, |_| Some(((7, 4), None))).is_empty());
    }

    #[test]
    fn every_other_handled_action_but_the_use_s_own_counts_as_a_move() {
        let mut consumed = [0u32; 56];
        let read = |c: &[u32; 56]| {
            // SAFETY: `c` is 224 readable bytes, the test layout's component.
            unsafe { moves(&LAYOUT, c.as_ptr().cast()) }
        };
        let still = read(&consumed);
        assert!(still.is_some());
        consumed[128 / 4] += 1;
        assert_eq!(read(&consumed), still, "another use of the item");
        consumed[0] += 1;
        assert_eq!(read(&consumed), still, "the base before the versions");
        for at in [24, 32, 72, 120, 220] {
            let mut moved = consumed;
            moved[at / 4] += 1;
            assert_ne!(read(&moved), still, "the version at {at}");
        }
        let blind = Layout {
            moves: None,
            ..LAYOUT
        };
        // SAFETY: `consumed` is 224 readable bytes; a blind layout reads none.
        assert_eq!(unsafe { moves(&blind, consumed.as_ptr().cast()) }, None);
    }

    #[test]
    fn a_use_is_watched_once_and_one_player_cannot_fill_the_watch() {
        let uses = Uses::new();
        let at = Instant::now();
        watch_use(&uses, 1, 0, 5, at);
        watch_use(&uses, 1, 0, 5, at);
        assert_eq!(uses.watched.relock().len(), 1, "the same use seen again");
        for version in 2..20 {
            watch_use(&uses, version, 0, 5, at);
        }
        assert_eq!(uses.watched.relock().len(), MAX_WATCHED_PER_PLAYER);
        let other = ItemAction {
            player: enshwick_core::Entity(2),
            ..drink(101, 0)
        };
        uses.watch(other, 1, 7, 5, Some(STILL), at);
        assert_eq!(
            uses.watched.relock().len(),
            MAX_WATCHED_PER_PLAYER + 1,
            "another player's use is still watched"
        );
        uses.full.store(true, Ordering::Relaxed);
        uses.forget();
        assert!(
            uses.watched.relock().is_empty(),
            "a world's end forgets them"
        );
        assert!(!uses.any.load(Ordering::Acquire));
        assert!(
            !uses.full.load(Ordering::Relaxed),
            "the next world's overflow is told again"
        );
    }
}
