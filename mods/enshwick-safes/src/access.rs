//! Who may use a Safe: the player who placed it, the players they allowed,
//! and the server's administrators when the settings allow.
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
use enshwick_loader_api::ecs::{Components, Ecs, Entity, Worlds};

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

/// The centre of `cell`, in metres.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "world coordinates fit in f64 exactly enough"
)]
pub fn centre(cell: Cell) -> [f64; 3] {
    [
        cell.0 as f64 / 2.0,
        cell.1 as f64 / 2.0,
        cell.2 as f64 / 2.0,
    ]
}

/// The record at `cell` or a neighbouring one, with the cell it is kept
/// at: a placement and the stored transform may round to adjacent cells.
#[must_use]
pub fn owner_near<V, S: std::hash::BuildHasher>(
    owners: &HashMap<Cell, V, S>,
    at: Cell,
) -> Option<(Cell, &V)> {
    if let Some(found) = owners.get(&at) {
        return Some((at, found));
    }
    (-1..=1)
        .flat_map(|x| (-1..=1).flat_map(move |y| (-1..=1).map(move |z| (x, y, z))))
        .map(|(x, y, z)| (at.0 + x, at.1 + y, at.2 + z))
        .find_map(|near| owners.get(&near).map(|found| (near, found)))
}

/// The cell of `key`'s Safe nearest to `position`, within `max` metres.
#[must_use]
pub fn nearest_own<S: std::hash::BuildHasher>(
    owners: &HashMap<Cell, Safe, S>,
    key: u64,
    position: [f64; 3],
    max: f64,
) -> Option<Cell> {
    owners
        .iter()
        .filter(|(_, safe)| safe.owner == key)
        .map(|(&at, _)| {
            let c = centre(at);
            let d = (0..3).map(|i| (c[i] - position[i]).powi(2)).sum::<f64>();
            (at, d.sqrt())
        })
        .filter(|&(_, distance)| distance <= max)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(at, _)| at)
}

/// A player allowed at a Safe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Allowed {
    /// Their account key.
    pub key: u64,
    /// Their name when they were allowed.
    pub name: String,
}

/// One Safe's owner and the players they allowed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Safe {
    /// The owner's account key.
    pub owner: u64,
    /// The owner's name, as last seen; empty when unknown.
    pub owner_name: String,
    /// Players who may use the Safe as the owner does, but never remove it
    /// or change who may.
    pub allowed: Vec<Allowed>,
}

/// What allowing a player did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AllowOutcome {
    /// They may now use the Safe.
    Allowed,
    /// They already could.
    Already,
    /// They are the owner.
    Owner,
}

impl Safe {
    /// A Safe of `owner`, named `owner_name`, no one else allowed.
    #[must_use]
    pub fn new(owner: u64, owner_name: &str) -> Self {
        Self {
            owner,
            owner_name: owner_name.to_owned(),
            allowed: Vec::new(),
        }
    }

    /// Lets the player with `key`, named `name`, use the Safe.
    pub fn allow(&mut self, key: u64, name: &str) -> AllowOutcome {
        if key == self.owner {
            return AllowOutcome::Owner;
        }
        if let Some(known) = self.allowed.iter_mut().find(|a| a.key == key) {
            name.clone_into(&mut known.name);
            return AllowOutcome::Already;
        }
        self.allowed.push(Allowed {
            key,
            name: name.to_owned(),
        });
        AllowOutcome::Allowed
    }

    /// Takes back the use of the Safe from the player named `name`, any
    /// case; who that was, or `None` when no one by that name was allowed.
    pub fn deny(&mut self, name: &str) -> Option<Allowed> {
        let index = self
            .allowed
            .iter()
            .position(|a| a.name.to_lowercase() == name.to_lowercase())?;
        Some(self.allowed.remove(index))
    }
}

/// What a player tries to do with a Safe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Open it, take, put or sort, craft and build from it.
    Use,
    /// Pick it up or dismantle it.
    Remove,
    /// Rename it or change who may use it.
    Manage,
}

/// A connected player as the checks see them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seen {
    /// The account key.
    pub key: u64,
    /// Whether the server makes them an administrator.
    pub admin: bool,
}

/// Whether `player` may do `action` with `safe`: the owner may do all, the
/// players they allowed only use it, administrators all when `admins_open`.
/// What is not a known player is no person and may.
#[must_use]
pub fn permits(safe: &Safe, player: Option<Seen>, admins_open: bool, action: Action) -> bool {
    let Some(player) = player else {
        return true;
    };
    player.key == safe.owner
        || (player.admin && admins_open)
        || (action == Action::Use && safe.allowed.iter().any(|a| a.key == player.key))
}

/// What the checks read.
#[derive(Debug)]
pub struct Access {
    /// Safes by cell; the persistent truth.
    pub owners: ArcSwap<HashMap<Cell, Safe>>,
    /// Safes seen this session: entity to owner and cell.
    pub safes: ArcSwap<HashMap<u32, (u64, Cell)>>,
    /// Connected players by character entity.
    pub players: ArcSwap<HashMap<u32, Seen>>,
    /// Administrators may use, remove and rename every Safe.
    pub admins_open: AtomicBool,
}

/// The one set of snapshots.
pub static ACCESS: LazyLock<Access> = LazyLock::new(Access::new);

/// Component hashes and reads the checks use.
#[derive(Clone, Copy, Debug)]
pub struct Reads {
    /// The current row's entity: the acting player.
    pub ecs: Ecs,
    /// Component reads inside systems.
    pub components: Components,
    /// Tells the server's world from a hosting client's own.
    pub worlds: Worlds,
    /// The `UsedItem` component's hash.
    pub used_item: u32,
    /// The `CurrentTransform` component's hash.
    pub transform: u32,
    /// The Safe item's id.
    pub safe_item: u32,
}

impl Access {
    /// No Safes, no players; administrators may open.
    #[must_use]
    pub fn new() -> Self {
        Self {
            owners: ArcSwap::from_pointee(HashMap::new()),
            safes: ArcSwap::from_pointee(HashMap::new()),
            players: ArcSwap::from_pointee(HashMap::new()),
            admins_open: AtomicBool::new(true),
        }
    }

    /// Whether `player` may do `action` with the Safe of `owner` kept at
    /// `at`.
    #[must_use]
    pub fn may(&self, owner: u64, at: Cell, player: Entity, action: Action) -> bool {
        let seen = self.players.load().get(&player.0).copied();
        let admins_open = self.admins_open.load(Ordering::Relaxed);
        match self.owners.load().get(&at) {
            Some(safe) => permits(safe, seen, admins_open, action),
            None => permits(&Safe::new(owner, ""), seen, admins_open, action),
        }
    }

    /// The owner of `chest` and the cell it is kept at, when it is a Safe
    /// with a known owner, learning it on first sight.
    ///
    /// # Safety
    ///
    /// `ctx` is the live context of the hooked system, used on its thread.
    #[must_use]
    pub unsafe fn owner(&self, reads: &Reads, ctx: *mut u64, chest: Entity) -> Option<(u64, Cell)> {
        if chest.is_none() {
            return None;
        }
        if let Some(&known) = self.safes.load().get(&chest.0) {
            return Some(known);
        }
        // SAFETY: forwarded from the caller.
        let item = unsafe { reads.components.component(ctx, chest, reads.used_item) }?;
        // SAFETY: UsedItem is the item id alone.
        if unsafe { item.cast::<u32>().read_unaligned() } != reads.safe_item {
            return None;
        }
        // SAFETY: as above.
        let transform = unsafe { reads.components.component(ctx, chest, reads.transform) }?;
        let found = self.seen(chest.0, cell(position(transform)));
        if found.is_none() {
            enshwick_loader_api::warn_every!(
                std::time::Duration::from_secs(60),
                "a Safe has no recorded owner and stays open to all"
            );
        }
        found
    }

    /// The owner recorded near `at` for the Safe `entity`, remembered for
    /// the session.
    pub fn seen(&self, entity: u32, at: Cell) -> Option<(u64, Cell)> {
        let found = owner_near(&self.owners.load(), at).map(|(at, safe)| (safe.owner, at))?;
        self.safes.rcu(|safes| {
            let mut safes = HashMap::clone(safes);
            safes.insert(entity, found);
            safes
        });
        Some(found)
    }

    /// Whether `player` may do `action` with `chest`: true for anything but a
    /// Safe they may not.
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
        action: Action,
    ) -> bool {
        // SAFETY: forwarded from the caller.
        match unsafe { self.owner(reads, ctx, chest) } {
            Some((owner, at)) => self.may(owner, at, player, action),
            None => true,
        }
    }

    /// Whether `chest` is a Safe `player` may not use, from what this
    /// session has seen; for checks without a system context.
    #[must_use]
    pub fn refused_seen(&self, chest: u32, player: Entity) -> bool {
        self.safes
            .load()
            .get(&chest)
            .is_some_and(|&(owner, at)| !self.may(owner, at, player, Action::Use))
    }

    /// Forgets the Safes and players of a hosted game that stopped; their
    /// entities mean nothing in the next. Owners stay.
    pub fn forget_session(&self) {
        self.safes.store(Arc::new(HashMap::new()));
        self.players.store(Arc::new(HashMap::new()));
    }

    /// Records a Safe just placed by the player with `key`, named `name`.
    pub fn placed(&self, entity: u32, at: Cell, key: u64, name: &str) {
        self.owners.rcu(|owners| {
            let mut owners = HashMap::clone(owners);
            owners.insert(at, Safe::new(key, name));
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

    /// Changes the Safe kept at `at` with `change`; what it returns, or
    /// `None` when no Safe is kept there.
    pub fn change<R>(&self, at: Cell, mut change: impl FnMut(&mut Safe) -> R) -> Option<R> {
        let mut outcome = None;
        self.owners.rcu(|owners| {
            let mut owners = HashMap::clone(owners);
            outcome = owners.get_mut(&at).map(&mut change);
            owners
        });
        outcome
    }

    /// Replaces the Safes, as loaded from disk.
    pub fn load_owners(&self, owners: HashMap<Cell, Safe>) {
        self.owners.store(Arc::new(owners));
    }
}

impl Default for Access {
    fn default() -> Self {
        Self::new()
    }
}

/// A transform's position: x, y and z in 32.32 fixed point first.
fn position(transform: *mut u8) -> [f64; 3] {
    // SAFETY: a live CurrentTransform.
    unsafe { enshwick_loader_api::ecs::transform_position(transform) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNER: Seen = Seen {
        key: 1,
        admin: false,
    };
    const FRIEND: Seen = Seen {
        key: 2,
        admin: false,
    };
    const STRANGER: Seen = Seen {
        key: 3,
        admin: false,
    };
    const ADMIN: Seen = Seen {
        key: 4,
        admin: true,
    };

    #[test]
    fn cells_are_half_metres_and_neighbours_count() {
        assert_eq!(cell([1.0, 2.24, -0.26]), (2, 4, -1));
        let owners = HashMap::from([((2, 4, -1), 7)]);
        assert_eq!(owner_near(&owners, (2, 4, -1)), Some(((2, 4, -1), &7)));
        assert_eq!(owner_near(&owners, (3, 5, 0)), Some(((2, 4, -1), &7)));
        assert_eq!(owner_near(&owners, (4, 4, -1)), None);
        assert!(
            centre((3, -2, 0))
                .iter()
                .zip([1.5, -1.0, 0.0])
                .all(|(a, b)| (a - b).abs() < 1e-9)
        );
    }

    #[test]
    fn allowed_players_use_but_never_remove_or_manage() {
        let mut safe = Safe::new(OWNER.key, "Ann");
        assert_eq!(safe.allow(FRIEND.key, "Bob"), AllowOutcome::Allowed);
        assert_eq!(safe.allow(FRIEND.key, "Bobby"), AllowOutcome::Already);
        assert_eq!(safe.allow(OWNER.key, "Ann"), AllowOutcome::Owner);
        for action in [Action::Use, Action::Remove, Action::Manage] {
            assert!(permits(&safe, Some(OWNER), false, action));
            assert!(!permits(&safe, Some(STRANGER), true, action));
            assert!(permits(&safe, Some(ADMIN), true, action));
            assert!(!permits(&safe, Some(ADMIN), false, action));
            assert!(permits(&safe, None, false, action));
        }
        assert!(permits(&safe, Some(FRIEND), false, Action::Use));
        assert!(!permits(&safe, Some(FRIEND), false, Action::Remove));
        assert!(!permits(&safe, Some(FRIEND), false, Action::Manage));
        assert_eq!(safe.deny("BOBBY").map(|a| a.key), Some(FRIEND.key));
        assert_eq!(safe.deny("Bobby"), None);
        assert!(!permits(&safe, Some(FRIEND), false, Action::Use));
    }

    #[test]
    fn the_nearest_own_safe_is_found_within_reach() {
        let owners = HashMap::from([
            ((0, 0, 0), Safe::new(1, "Ann")),
            ((4, 0, 0), Safe::new(1, "Ann")),
            ((2, 0, 0), Safe::new(2, "Bob")),
            ((40, 0, 0), Safe::new(1, "Ann")),
        ]);
        assert_eq!(
            nearest_own(&owners, 1, [1.6, 0.0, 0.0], 4.0),
            Some((4, 0, 0))
        );
        assert_eq!(nearest_own(&owners, 1, [10.0, 0.0, 0.0], 4.0), None);
        assert_eq!(nearest_own(&owners, 3, [0.0, 0.0, 0.0], 4.0), None);
    }

    #[test]
    fn owners_friends_and_admins_through_the_snapshots() {
        let access = Access::new();
        access.players.store(Arc::new(HashMap::from([
            (10, OWNER),
            (11, FRIEND),
            (12, STRANGER),
            (13, ADMIN),
        ])));
        access.placed(500, (0, 0, 0), OWNER.key, "Ann");
        assert_eq!(
            access.change((0, 0, 0), |safe| safe.allow(FRIEND.key, "Bob")),
            Some(AllowOutcome::Allowed)
        );
        assert!(!access.refused_seen(500, Entity(10)));
        assert!(!access.refused_seen(500, Entity(11)));
        assert!(access.refused_seen(500, Entity(12)));
        assert!(!access.refused_seen(500, Entity(13)));
        assert!(!access.may(OWNER.key, (0, 0, 0), Entity(11), Action::Remove));
        access.admins_open.store(false, Ordering::Relaxed);
        assert!(access.refused_seen(500, Entity(13)));
        assert!(access.removed(500));
        assert!(!access.refused_seen(500, Entity(12)));
        assert!(access.owners.load().is_empty());
        assert_eq!(access.change((0, 0, 0), |_| ()), None);
    }
}
