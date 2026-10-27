//! The props world transactions keep players from before they destroy them,
//! and the quick answer every interaction detour asks first: is there
//! anything to check at all.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use arc_swap::ArcSwapOption;
use enshwick::abi::Status;

use crate::locks::Relock;

/// Steps after its request by which a destroy has landed: the request
/// plays back at the end of the next step.
const DESTROY_STEPS: u64 = 2;

/// What a hold changed to keep Quick Stash out of a magic chest, as it
/// was before, for its release to put back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stash {
    /// The chest's switch a player sets: whether a player set it, and to
    /// what.
    Switch([u8; 2]),
    /// The chest's own Quick Stash byte, where no player set the switch
    /// yet and the chest has none.
    Stock(u8),
}

/// A prop held against players.
#[derive(Debug, Clone, Copy)]
struct Hold {
    entity: u32,
    /// The world it was held in, so an id the next world gives again never
    /// meets it.
    world: u64,
    /// The step by which its destroy has landed; `None` while only held.
    landed: Option<u64>,
    /// What the hold changed to keep Quick Stash out, as it was.
    deposit: Option<Stash>,
}

/// The props held, and whether the interaction filter that keeps players
/// from them is in place.
#[derive(Debug)]
pub struct PropHolds {
    /// Live interaction filters plus props held: while 0, every detour
    /// hands its call to the game at once.
    active: AtomicUsize,
    /// The interaction filters mods registered.
    filters: AtomicUsize,
    /// Whether every hook the hold relies on linked.
    linked: AtomicBool,
    /// The running world's number, one up at each world end.
    world: AtomicU64,
    /// The last step the loader saw.
    step: AtomicU64,
    held: Mutex<Vec<Hold>>,
    /// The entities held, for the filter's lock-free check.
    ids: ArcSwapOption<Vec<u32>>,
}

impl Default for PropHolds {
    fn default() -> Self {
        Self::new()
    }
}

impl PropHolds {
    /// Nothing held, the filter not linked.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            active: AtomicUsize::new(0),
            filters: AtomicUsize::new(0),
            linked: AtomicBool::new(false),
            world: AtomicU64::new(0),
            step: AtomicU64::new(0),
            held: Mutex::new(Vec::new()),
            ids: ArcSwapOption::const_empty(),
        }
    }

    /// Whether there is nothing to check: no filters and nothing held. One
    /// atomic load, the first thing every interaction detour asks.
    #[inline]
    #[must_use]
    pub fn quiet(&self) -> bool {
        self.active.load(Ordering::Acquire) == 0
    }

    /// Notes the interaction filters mods registered, `filters` of them,
    /// and whether every hook the hold relies on linked.
    pub fn linked(&self, filters: usize, all_linked: bool) {
        self.filters.store(filters, Ordering::Release);
        self.linked.store(all_linked, Ordering::Release);
        self.publish(&self.held.relock());
    }

    /// Holds `entity` against players until [`PropHolds::release`] or its
    /// destroy. [`Status::MISSING`] unless every hook the hold relies on
    /// linked and the filter's fuse is on (`fuse_off` false).
    ///
    /// # Errors
    ///
    /// [`Status::MISSING`] as above.
    pub fn hold(&self, entity: u32, fuse_off: bool) -> Result<(), Status> {
        if fuse_off || !self.linked.load(Ordering::Acquire) {
            return Err(Status::MISSING);
        }
        let world = self.world.load(Ordering::Acquire);
        let mut held = self.held.relock();
        if !held.iter().any(|hold| hold.entity == entity) {
            held.push(Hold {
                entity,
                world,
                landed: None,
                deposit: None,
            });
        }
        self.publish(&held);
        Ok(())
    }

    /// Whether `entity` is held and the hold still keeps players out: every
    /// hook linked and the filter's fuse on (`fuse_off` false).
    #[must_use]
    pub fn still_held(&self, entity: u32, fuse_off: bool) -> bool {
        !fuse_off && self.linked.load(Ordering::Acquire) && self.is_held(entity)
    }

    /// Notes what the hold of `entity` changed to keep Quick Stash out, as
    /// it was, `before`, which its release puts back.
    pub fn note_deposit(&self, entity: u32, before: Stash) {
        let world = self.world.load(Ordering::Acquire);
        let mut held = self.held.relock();
        for hold in held.iter_mut() {
            if hold.entity == entity && hold.world == world {
                hold.deposit = Some(before);
            }
        }
    }

    /// Lets players reach `entity` again; what to put back to let Quick
    /// Stash in again, if the hold changed it in this world.
    pub fn release(&self, entity: u32) -> Option<Stash> {
        let world = self.world.load(Ordering::Acquire);
        let mut held = self.held.relock();
        let index = held.iter().position(|hold| hold.entity == entity)?;
        let gone = held.swap_remove(index);
        self.publish(&held);
        gone.deposit.filter(|_| gone.world == world)
    }

    /// Lets every held prop go; what to put back to let Quick Stash in
    /// again, by entity, for those of this world still standing.
    pub fn release_all(&self) -> Vec<(u32, Stash)> {
        let world = self.world.load(Ordering::Acquire);
        let mut held = self.held.relock();
        let back = held
            .drain(..)
            .filter(|hold| hold.world == world && hold.landed.is_none())
            .filter_map(|hold| hold.deposit.map(|deposit| (hold.entity, deposit)))
            .collect();
        self.publish(&held);
        back
    }

    /// What the holds changed to keep Quick Stash out, by entity, as it
    /// was, for a save to keep it so.
    pub fn deposits(&self) -> Vec<(u32, Stash)> {
        let world = self.world.load(Ordering::Acquire);
        self.held
            .relock()
            .iter()
            .filter(|hold| hold.world == world && hold.landed.is_none())
            .filter_map(|hold| hold.deposit.map(|deposit| (hold.entity, deposit)))
            .collect()
    }

    /// Keeps `entity` held until its destroy, asked for now, has landed;
    /// then its id is free for the game to give again.
    pub fn destroying(&self, entity: u32) {
        let step = self.step.load(Ordering::Acquire);
        let mut held = self.held.relock();
        for hold in held.iter_mut() {
            if hold.entity == entity {
                hold.landed = Some(step + DESTROY_STEPS);
                hold.deposit = None;
            }
        }
    }

    /// Notes `step` and lets go of the props whose destroy has landed by
    /// it.
    pub fn after_step(&self, step: u64) {
        self.step.store(step, Ordering::Release);
        if self.ids.load().is_none() {
            return;
        }
        let mut held = self.held.relock();
        let before = held.len();
        held.retain(|hold| hold.landed.is_none_or(|until| step < until));
        if held.len() != before {
            self.publish(&held);
        }
    }

    /// Whether `entity` is held.
    #[must_use]
    pub fn is_held(&self, entity: u32) -> bool {
        self.ids
            .load()
            .as_deref()
            .is_some_and(|ids| ids.contains(&entity))
    }

    /// Forgets every hold at a world's end, where nothing is left to put
    /// back; the next world's ids never meet them.
    pub fn forget(&self) {
        self.world.fetch_add(1, Ordering::AcqRel);
        let mut held = self.held.relock();
        held.clear();
        self.publish(&held);
    }

    fn publish(&self, held: &[Hold]) {
        let ids: Vec<u32> = held.iter().map(|hold| hold.entity).collect();
        let count = ids.len();
        self.ids.store((count > 0).then(|| Arc::new(ids)));
        self.active.store(
            self.filters.load(Ordering::Acquire) + count,
            Ordering::Release,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linked() -> PropHolds {
        let holds = PropHolds::new();
        holds.linked(0, true);
        holds
    }

    #[test]
    fn a_hold_needs_every_hook_linked() {
        let holds = PropHolds::new();
        holds.linked(0, false);
        assert_eq!(holds.hold(7, false), Err(Status::MISSING));
        assert!(!holds.is_held(7));
        assert!(holds.quiet());
    }

    #[test]
    fn a_hold_needs_the_filter_s_fuse_on() {
        let holds = linked();
        assert_eq!(holds.hold(7, true), Err(Status::MISSING));
        assert!(!holds.is_held(7));
    }

    #[test]
    fn a_held_prop_wakes_the_detours_and_its_release_puts_quick_stash_back() {
        let holds = linked();
        assert!(holds.quiet());
        holds.hold(7, false).unwrap();
        holds.note_deposit(7, Stash::Switch([1, 1]));
        assert!(!holds.quiet());
        assert!(holds.is_held(7));
        assert_eq!(holds.deposits(), [(7, Stash::Switch([1, 1]))]);
        assert_eq!(holds.release(7), Some(Stash::Switch([1, 1])));
        assert!(holds.quiet());
        assert!(!holds.is_held(7));
    }

    #[test]
    fn a_hold_stops_keeping_players_out_when_the_fuse_blows_or_a_link_goes() {
        let holds = linked();
        holds.hold(7, false).unwrap();
        assert!(holds.still_held(7, false));
        assert!(!holds.still_held(7, true), "the fuse blew");
        assert!(!holds.still_held(8, false), "never held");
        holds.linked(0, false);
        assert!(!holds.still_held(7, false));
    }

    #[test]
    fn a_chest_without_a_switch_gets_its_own_byte_back() {
        let holds = linked();
        holds.hold(7, false).unwrap();
        holds.note_deposit(7, Stash::Stock(1));
        assert_eq!(holds.deposits(), [(7, Stash::Stock(1))]);
        assert_eq!(holds.release(7), Some(Stash::Stock(1)));
    }

    #[test]
    fn filters_keep_the_detours_awake() {
        let holds = PropHolds::new();
        holds.linked(2, true);
        assert!(!holds.quiet());
    }

    #[test]
    fn a_destroyed_prop_stays_held_until_its_destroy_has_landed() {
        let holds = linked();
        holds.hold(7, false).unwrap();
        holds.note_deposit(7, Stash::Switch([1, 1]));
        holds.after_step(10);
        holds.destroying(7);
        holds.after_step(11);
        assert!(holds.is_held(7));
        assert!(
            holds.deposits().is_empty(),
            "a destroyed prop keeps nothing"
        );
        holds.after_step(12);
        assert!(!holds.is_held(7));
        assert!(holds.quiet());
    }

    #[test]
    fn a_world_end_forgets_holds_so_a_reused_id_meets_none() {
        let holds = linked();
        holds.hold(7, false).unwrap();
        holds.note_deposit(7, Stash::Switch([1, 1]));
        holds.forget();
        assert!(!holds.is_held(7));
        holds.hold(7, false).unwrap();
        assert_eq!(holds.release(7), None, "nothing to put back from before");
    }

    #[test]
    fn releasing_all_gives_back_what_to_restore() {
        let holds = linked();
        holds.hold(7, false).unwrap();
        holds.note_deposit(7, Stash::Switch([1, 1]));
        holds.hold(8, false).unwrap();
        assert_eq!(holds.release_all(), [(7, Stash::Switch([1, 1]))]);
        assert!(holds.quiet());
    }
}
