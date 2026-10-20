//! Who may use a Safe: the player who placed it, and the server's
//! administrators when the settings allow.
//!
//! Owners are kept by position, in half-metre cells, since a placed chest
//! keeps no owner of its own; the item it was placed from, kept as its
//! `UsedItem`, tells a Safe from a chest. Every check runs inside the game's
//! systems on worker threads, so it reads only lock-free snapshots. A Safe
//! whose owner is unknown behaves as an ordinary chest: nothing is ever
//! locked away for good.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};

use arc_swap::ArcSwap;
use enshwick_api::ecs::{Components, Ecs, Entity};

/// A position in half-metre cells.
pub type Cell = (i64, i64, i64);

/// The cell holding `position`, in metres.
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    reason = "world coordinates fit in i64 half-metres"
)]
pub fn cell(position: [f64; 3]) -> Cell {
    let half = |v: f64| (v * 2.0).round() as i64;
    (half(position[0]), half(position[1]), half(position[2]))
}

/// The owner recorded at `cell` or a neighbouring one: a placement and the
/// stored transform may round to adjacent cells.
#[must_use]
pub fn owner_near<S: std::hash::BuildHasher>(
    owners: &HashMap<Cell, u64, S>,
    at: Cell,
) -> Option<u64> {
    if let Some(&owner) = owners.get(&at) {
        return Some(owner);
    }
    (-1..=1)
        .flat_map(|x| (-1..=1).flat_map(move |y| (-1..=1).map(move |z| (x, y, z))))
        .find_map(|(x, y, z)| owners.get(&(at.0 + x, at.1 + y, at.2 + z)).copied())
}

/// A connected player as the checks see them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seen {
    /// The account key.
    pub key: u64,
    /// Whether the server makes them an administrator.
    pub admin: bool,
}

/// What the checks read.
#[derive(Debug)]
pub struct Access {
    /// Owners by cell; the persistent truth.
    pub owners: ArcSwap<HashMap<Cell, u64>>,
    /// Safes seen this session: entity to owner and cell.
    pub safes: ArcSwap<HashMap<u32, (u64, Cell)>>,
    /// Connected players by character entity.
    pub players: ArcSwap<HashMap<u32, Seen>>,
    /// Administrators may use every Safe.
    pub admins_open: AtomicBool,
}

/// The one set of snapshots.
pub static ACCESS: LazyLock<Access> = LazyLock::new(|| Access {
    owners: ArcSwap::from_pointee(HashMap::new()),
    safes: ArcSwap::from_pointee(HashMap::new()),
    players: ArcSwap::from_pointee(HashMap::new()),
    admins_open: AtomicBool::new(true),
});

/// Component hashes and reads the checks use.
#[derive(Clone, Copy, Debug)]
pub struct Reads {
    /// The current row's entity: the acting player.
    pub ecs: Ecs,
    /// Component reads inside systems.
    pub components: Components,
    /// The `UsedItem` component's hash.
    pub used_item: u32,
    /// The `CurrentTransform` component's hash.
    pub transform: u32,
    /// The Safe item's id.
    pub safe_item: u32,
}

impl Access {
    /// Whether `player` may use Safe `owner`.
    #[must_use]
    pub fn may_use(&self, owner: u64, player: Entity) -> bool {
        match self.players.load().get(&player.0) {
            Some(seen) => {
                seen.key == owner || (seen.admin && self.admins_open.load(Ordering::Relaxed))
            }
            // Not a known player: no person acts there.
            None => true,
        }
    }

    /// The owner of `chest` when it is a Safe with a known owner, learning
    /// it on first sight.
    ///
    /// # Safety
    ///
    /// `ctx` is the live context of the hooked system, used on its thread.
    #[must_use]
    pub unsafe fn owner(&self, reads: &Reads, ctx: *mut u64, chest: Entity) -> Option<u64> {
        if chest.is_none() {
            return None;
        }
        if let Some(&(owner, _)) = self.safes.load().get(&chest.0) {
            return Some(owner);
        }
        // SAFETY: forwarded from the caller.
        let item = unsafe { reads.components.component(ctx, chest, reads.used_item) }?;
        // SAFETY: UsedItem is the item id alone.
        if unsafe { item.cast::<u32>().read_unaligned() } != reads.safe_item {
            return None;
        }
        // SAFETY: as above.
        let transform = unsafe { reads.components.component(ctx, chest, reads.transform) }?;
        let at = cell(position(transform));
        let Some(owner) = owner_near(&self.owners.load(), at) else {
            enshwick_api::warn_every!(
                std::time::Duration::from_secs(60),
                "a Safe at {at:?} has no recorded owner and stays open to all"
            );
            return None;
        };
        self.safes.rcu(|safes| {
            let mut safes = HashMap::clone(safes);
            safes.insert(chest.0, (owner, at));
            safes
        });
        Some(owner)
    }

    /// Whether `player` may use `chest`: true for anything but a Safe of
    /// someone else.
    ///
    /// # Safety
    ///
    /// As for [`Access::owner`].
    #[must_use]
    pub unsafe fn allowed(
        &self,
        reads: &Reads,
        ctx: *mut u64,
        chest: Entity,
        player: Entity,
    ) -> bool {
        // SAFETY: forwarded from the caller.
        match unsafe { self.owner(reads, ctx, chest) } {
            Some(owner) => self.may_use(owner, player),
            None => true,
        }
    }

    /// Whether `chest` is a Safe of someone other than `player`, from what
    /// this session has seen; for checks without a system context.
    #[must_use]
    pub fn refused_seen(&self, chest: u32, player: Entity) -> bool {
        self.safes
            .load()
            .get(&chest)
            .is_some_and(|&(owner, _)| !self.may_use(owner, player))
    }

    /// Records a Safe just placed by the player with `key`.
    pub fn placed(&self, entity: u32, at: Cell, key: u64) {
        self.owners.rcu(|owners| {
            let mut owners = HashMap::clone(owners);
            owners.insert(at, key);
            owners
        });
        self.safes.rcu(|safes| {
            let mut safes = HashMap::clone(safes);
            safes.insert(entity, (key, at));
            safes
        });
    }

    /// Forgets a Safe picked up or torn down; false when `entity` was none.
    pub fn removed(&self, entity: u32) -> bool {
        let Some(&(_, at)) = self.safes.load().get(&entity) else {
            return false;
        };
        self.safes.rcu(|safes| {
            let mut safes = HashMap::clone(safes);
            safes.remove(&entity);
            safes
        });
        self.owners.rcu(|owners| {
            let mut owners = HashMap::clone(owners);
            owners.remove(&at);
            owners
        });
        true
    }

    /// Replaces the owners, as loaded from disk.
    pub fn load_owners(&self, owners: HashMap<Cell, u64>) {
        self.owners.store(Arc::new(owners));
    }
}

/// A transform's position: x, y and z in 32.32 fixed point first.
fn position(transform: *mut u8) -> [f64; 3] {
    // SAFETY: a live CurrentTransform holds three i64 first.
    let fixed = unsafe { transform.cast::<[i64; 3]>().read_unaligned() };
    enshwick_api::ecs::metres(fixed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_are_half_metres_and_neighbours_count() {
        assert_eq!(cell([1.0, 2.24, -0.26]), (2, 4, -1));
        let owners = HashMap::from([((2, 4, -1), 7)]);
        assert_eq!(owner_near(&owners, (2, 4, -1)), Some(7));
        assert_eq!(owner_near(&owners, (3, 5, 0)), Some(7));
        assert_eq!(owner_near(&owners, (4, 4, -1)), None);
    }

    #[test]
    fn owners_and_admins_may_use_a_safe_and_others_may_not() {
        let access = Access {
            owners: ArcSwap::from_pointee(HashMap::new()),
            safes: ArcSwap::from_pointee(HashMap::new()),
            players: ArcSwap::from_pointee(HashMap::from([
                (
                    10,
                    Seen {
                        key: 1,
                        admin: false,
                    },
                ),
                (
                    11,
                    Seen {
                        key: 2,
                        admin: false,
                    },
                ),
                (
                    12,
                    Seen {
                        key: 3,
                        admin: true,
                    },
                ),
            ])),
            admins_open: AtomicBool::new(true),
        };
        access.placed(500, (0, 0, 0), 1);
        assert!(!access.refused_seen(500, Entity(10)));
        assert!(access.refused_seen(500, Entity(11)));
        assert!(!access.refused_seen(500, Entity(12)));
        access.admins_open.store(false, Ordering::Relaxed);
        assert!(access.refused_seen(500, Entity(12)));
        assert!(access.removed(500));
        assert!(!access.refused_seen(500, Entity(11)));
        assert!(access.owners.load().is_empty());
    }
}
