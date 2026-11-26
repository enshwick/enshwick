//! The quest reward filter: what the game pays a player for a quest, as the
//! mods' filters leave it, at the game's own payment.
//!
//! `sys.trigger_quest_rewards` looks at up to 50 quests of each player with
//! the receive-experience permission a step, and pays one whose own bool is
//! unset and whose last step holds through the payment
//! `(out, iteration, player, rewards, quest)`: it resolves the experience
//! value of each of the quest's `QuestRewardResource`s that names one, at
//! most [`PAID`] of them since a fourth ends the process, queues the quest's
//! bool and adds the experience; a status of 1 to 3 in `out` puts the quest
//! off to a later step (server 26430253 `0x1401bbbb0`, client 26430625
//! `0x14041b7e0`, read in the decompiler). The items the journal draws as
//! rewards it never pays. The detour hands the mods the quest's experience
//! values and calls the payment with the list they leave, an empty one for
//! a refusal, which still sets the bool. Until a mod filters, the detour is
//! not linked.

use core::ffi::c_void;
use std::sync::OnceLock;

use enshwick_core::Entity;
use enshwick_core::abi::{
    FILTER_REFUSE, FILTER_YES, HookFlags, KnowledgeId, QuestPayment, QuestRewardFilterFn,
};

use crate::engine::filters::Filters;
use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, Target};
use crate::engine::memory::read;
use crate::engine::mod_index::ModIndex;
use crate::engine::pinned;
use crate::engine::profile::Feature;
use crate::engine::registry::JoinFeatureError;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[pinned::knowledge::QUEST_PAY.id()];

/// The most experience values the payment takes: it keeps them in a small
/// array of three and ends the process at a fourth (server 26430253).
const PAID: usize = 3;
/// The most rewards a quest of the journal holds that the detour reads.
const ROWS_READ: u32 = 64;
/// The most bytes of one reward row the detour writes.
const ROW_MAX: usize = 64;

static FILTERS: Filters<QuestRewardFilterFn> = Filters::new(Feature::Progress);
static ROWS: OnceLock<Rows> = OnceLock::new();
static FUSE: Fuse = Fuse::new("quest reward filter");

/// What the detour reads and writes of a `QuestRewardResource`, from the
/// build's reflection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rows {
    size: usize,
    experience: usize,
}

/// The rewards the payment takes: the rows and their count, as the system
/// passes them in a 16-byte slot (server 26430253).
#[repr(C)]
struct List {
    start: *const u8,
    count: u64,
}

/// The reward rows of the build, `None` where its reflection does not hold
/// them in the form the detour writes.
fn rows(loader: &Loader) -> Option<Rows> {
    let ty = "keen::QuestRewardResource";
    let reflection = &loader.reflection;
    let size = usize::try_from(reflection.type_info(ty).ok()?.size()).ok()?;
    let experience =
        usize::try_from(reflection.field_offset(ty, "experienceValueId").ok()?).ok()?;
    let rows = Rows { size, experience };
    rows.fit().then_some(rows)
}

impl Rows {
    /// Whether the experience id lies inside a row the detour can write.
    const fn fit(self) -> bool {
        self.size <= ROW_MAX && self.size >= 4 && self.experience <= self.size - 4
    }
}

/// Registers mod `owner`'s filter.
///
/// # Errors
///
/// [`JoinFeatureError`] after init, or on a build without the payment or
/// its rows.
pub(crate) fn register(
    loader: &Loader,
    owner: ModIndex,
    callback: QuestRewardFilterFn,
    user: *mut c_void,
) -> Result<(), JoinFeatureError> {
    let available = || rows(loader).is_some() && loader.code_holds(pinned::knowledge::QUEST_PAY);
    FILTERS.register_when(available, owner, callback, user)
}

/// Ends the filters mods registered during init.
pub(crate) fn seal() {
    let _filters = FILTERS.seal();
}

/// Links the payment's detour once a mod filters it.
pub(crate) fn link(loader: &Loader) {
    if !FILTERS.has_sealed() {
        return;
    }
    let Some(rows) = rows(loader) else {
        let _off = loader.fuse_off(&FUSE, "the quest reward rows", "do not read on this build");
        return;
    };
    // In before the hooks arm, so the detour never runs without it.
    let _first = ROWS.set(rows);
    let target = Target::Pinned(pinned::knowledge::QUEST_PAY);
    if loader.link_feature(&FUSE, target, &NEXT, quest_pay, HookFlags::NONE) {
        crate::log!(INFO, "quest reward filter on");
    }
}

/// The payment the game makes of `experience`, the experience ids of a
/// quest's rows in order, zero for a row without one; `None` for more than
/// [`QuestPayment::MAX`] of them.
fn payment(
    entity: Entity,
    quest: u32,
    experience: impl Iterator<Item = u32>,
) -> Option<QuestPayment> {
    let mut payment = enshwick_abi::headed!(QuestPayment {
        entity,
        quest: KnowledgeId::new(quest),
        count: 0,
        limit: u32::try_from(PAID).unwrap_or(0),
        allowed: 1,
        reserved: [0; 3],
        experience: [0; QuestPayment::MAX],
    });
    for id in experience.filter(|&id| id != 0) {
        let at = usize::try_from(payment.count).ok()?;
        *payment.experience.get_mut(at)? = id;
        payment.count += 1;
    }
    Some(payment)
}

/// The experience ids `payment` pays, and whether a filter asked for more
/// than the game pays.
fn paid(payment: &QuestPayment) -> (&[u32], bool) {
    if payment.allowed == 0 {
        return (&[], false);
    }
    let asked = usize::try_from(payment.count).unwrap_or(usize::MAX);
    let kept = asked.min(PAID).min(QuestPayment::MAX);
    (&payment.experience[..kept], asked > kept)
}

/// Runs every filter on `call` in load order.
fn decide(call: &mut QuestPayment) {
    FILTERS.each(|filter, user| {
        // SAFETY: a filter's mod runs, so its DLL that holds the callback
        // stays loaded; the callback gets the user pointer its mod
        // registered and `call`, which lives writable for the call.
        match unsafe { filter(&raw mut *call, user) } {
            FILTER_YES => call.allowed = 1,
            FILTER_REFUSE => call.allowed = 0,
            _ => {}
        }
    });
}

/// The rows to pay in place of the game's `rewards` for the player
/// `player` and `quest`, `None` when the filters left the payment as it was
/// or the detour does not read it.
///
/// # Safety
///
/// - `rewards` is the payment's own live list, its rows readable for their
///   count of `rows.size` bytes each, on the system's thread.
/// - `player` points at the paid character's u32 entity id.
#[cold]
unsafe fn filtered(
    rows: Rows,
    player: *const u32,
    rewards: *const List,
    quest: u32,
) -> Option<([u8; PAID * ROW_MAX], u64)> {
    let list = rewards.addr();
    // SAFETY: the caller's list, its row pointer in its first 8 bytes.
    let base: usize = unsafe { read(list, core::mem::offset_of!(List, start)) };
    // SAFETY: the caller's list, its count in the 8 bytes after the pointer.
    let count: u64 = unsafe { read(list, core::mem::offset_of!(List, count)) };
    let count = u32::try_from(count)
        .ok()
        .filter(|&count| count <= ROWS_READ)?;
    if base == 0 && count > 0 {
        return None;
    }
    // SAFETY: the caller's player pointer, a u32 entity id.
    let entity = Entity::from_engine(unsafe { read(player.addr(), 0) });
    let experience = (0..count as usize).map(|i| {
        // SAFETY: `i` is below the list's count, so this is a row of the
        // caller's live list of `rows.size` bytes; `Rows::fit` put the u32
        // experience id inside it.
        unsafe { read::<u32>(base + i * rows.size, rows.experience) }
    });
    let Some(mut call) = payment(entity, quest, experience) else {
        crate::log_once!(
            WARN,
            "quest reward filter: quest {quest:#x} names more experience than a filter sees; paid as the game pays it"
        );
        return None;
    };
    let before = call;
    decide(&mut call);
    if call == before {
        return None;
    }
    let (ids, cut) = paid(&call);
    if cut {
        crate::log_every!(
            WARN,
            core::time::Duration::from_secs(60),
            "quest reward filter: quest {quest:#x} asked to pay {} experience values; the game pays {PAID}",
            call.count
        );
    }
    let mut out = [0u8; PAID * ROW_MAX];
    for (i, id) in ids.iter().enumerate() {
        let at = i * rows.size + rows.experience;
        out[at..at + 4].copy_from_slice(&id.to_le_bytes());
    }
    Some((out, ids.len() as u64))
}

crate::engine::chains::chain_detour! {
    /// The game's quest payment `(out, iteration, player, rewards, quest)`,
    /// called by `sys.trigger_quest_rewards` on the thread of its system
    /// (server 26430253, client 26430625).
    fn quest_pay(out: *mut u8, iteration: usize, player: *const u32, rewards: *const List, quest: u32) -> *mut u8;
    next NEXT;
    fuse &FUSE, "the quest payment";
    |onward| {
        let Some(&rows) = ROWS.get() else {
            return onward.call();
        };
        if player.is_null() || rewards.is_null() {
            return onward.call();
        }
        // SAFETY: the payment's own player and live rewards list, on its
        // system's thread (server 26430253).
        match unsafe { filtered(rows, player, rewards, quest) } {
            None => onward.call(),
            Some((bytes, count)) => {
                let list = List { start: bytes.as_ptr(), count };
                // SAFETY: the game's payment with its own arguments but the
                // list, whose rows of the build's reflected size live on this
                // stack for the call; the payment reads only their count.
                unsafe { NEXT.get()(out, iteration, player, &raw const list, quest) }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_payment_holds_the_rows_that_name_experience_in_order() {
        let paid = payment(Entity::from_engine(2), 0x51, [0, 7, 0, 9].into_iter()).unwrap();
        assert_eq!((paid.count, paid.allowed, paid.limit), (2, 1, 3));
        assert_eq!(&paid.experience[..2], &[7, 9]);
        assert!(
            payment(Entity::from_engine(2), 0x51, core::iter::repeat_n(1, 9)).is_none(),
            "more than a filter sees"
        );
    }

    #[test]
    fn a_refusal_pays_nothing_and_a_long_list_is_cut_to_what_the_game_pays() {
        let mut call = payment(Entity::from_engine(2), 0x51, [7].into_iter()).unwrap();
        call.count = 5;
        call.experience[..5].copy_from_slice(&[7, 7, 8, 8, 9]);
        assert_eq!(paid(&call), (&[7, 7, 8][..], true));
        call.allowed = 0;
        assert_eq!(
            paid(&call),
            (&[][..], false),
            "refused, though the quest completes"
        );
        call.allowed = 1;
        call.count = u32::MAX;
        assert_eq!(paid(&call).0.len(), PAID, "a count past the struct is cut");
    }

    #[test]
    fn rows_fit_only_with_the_experience_id_inside() {
        assert!(
            Rows {
                size: 0x18,
                experience: 0x14
            }
            .fit(),
            "server 26430253"
        );
        assert!(
            !Rows {
                size: 0x18,
                experience: 0x15
            }
            .fit()
        );
        assert!(
            !Rows {
                size: ROW_MAX + 8,
                experience: 0
            }
            .fit()
        );
        assert!(
            !Rows {
                size: 2,
                experience: 0
            }
            .fit()
        );
    }

    #[test]
    fn a_filtered_list_writes_each_experience_id_at_its_row() {
        unsafe extern "system" fn twice(call: *mut QuestPayment, _: *mut c_void) -> u8 {
            // SAFETY: the test's own payment.
            let call = unsafe { &mut *call };
            call.experience[1] = call.experience[0];
            call.count = 2;
            enshwick_core::abi::FILTER_PASS
        }
        let _registered = FILTERS.register(ModIndex::LOADER, twice, core::ptr::null_mut());
        let _sealed = FILTERS.seal();
        let rows = Rows {
            size: 0x18,
            experience: 0x14,
        };
        let mut game = [0u8; 0x18];
        game[0x14..0x18].copy_from_slice(&0x77u32.to_le_bytes());
        let list = List {
            start: game.as_ptr(),
            count: 1,
        };
        let player = 3u32;
        // SAFETY: a one-row list of 0x18 bytes and a player id on this stack.
        let (bytes, count) =
            unsafe { filtered(rows, &raw const player, &raw const list, 0x51) }.unwrap();
        assert_eq!(count, 2, "the same experience paid twice");
        for row in 0..2 {
            let at = row * 0x18 + 0x14;
            assert_eq!(bytes[at..at + 4], 0x77u32.to_le_bytes());
        }
    }
}
