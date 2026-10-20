//! Mod items in random loot, each with an exact chance.
//!
//! The game rolls a container's random loot the first time a player opens it:
//! world chests and the corpses enemies leave. A link after that roll gives
//! each [`Drop`] whose filter matches its own chance to add a stack into the
//! same transaction, so the drop is saved and replicated with the vanilla
//! loot, and a container is never rolled twice. Destroyed objects and pickup
//! spawners roll one item instead; a drop for [`Kinds::DESTROYED`] takes that
//! item's place at its chance.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};

use crate::ecs::{Components, Entity};
use crate::raw::read_at;
use crate::{Load, Next, Off};

/// What a container is, for [`Source::kinds`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::loot::Kinds;
///
/// let containers = Kinds::CORPSES.with(Kinds::CHESTS);
/// assert!(containers.contains(Kinds::CORPSES) && !containers.contains(Kinds::DESTROYED));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Kinds(u8);

impl Kinds {
    /// The corpses and loot bags enemies leave.
    pub const CORPSES: Self = Self(1);
    /// World chests and every other container that rolls on first opening.
    pub const CHESTS: Self = Self(2);
    /// Destroyed objects and pickup spawners, which roll a single item.
    pub const DESTROYED: Self = Self(4);
    /// Corpses and chests.
    pub const CONTAINERS: Self = Self(1 | 2);

    /// Whether every kind of `other` is in `self`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::loot::Kinds;
    ///
    /// assert!(Kinds::CONTAINERS.contains(Kinds::CHESTS));
    /// assert!(!Kinds::CHESTS.contains(Kinds::CORPSES));
    /// ```
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Both sets together.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::loot::Kinds;
    ///
    /// assert_eq!(Kinds::CORPSES.with(Kinds::CHESTS), Kinds::CONTAINERS);
    /// ```
    #[must_use]
    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// One roll of a container's loot, as the filters see it.
///
/// # Examples
///
/// See [`Source`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Roll {
    /// What the container is.
    pub kind: Kinds,
    /// The container; [`Entity::NONE`] for a single-item roll.
    pub entity: Entity,
    /// The world node a chest is placed at; 0 for a corpse.
    pub node: u32,
    /// The loot labels of the enemy or chest type: the FNV-1a hash of the
    /// label collection's GUID; 0 without one.
    pub labels: u32,
    /// Whether the corpse is a boss's.
    pub boss: bool,
    /// Whether the chest needs a key or a lockpick to open.
    pub locked: bool,
}

/// Where a [`Drop`] may come from.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::ecs::Entity;
/// use enshwick_loader_api::loot::{Source, Kinds, Roll};
///
/// let boss = Roll { kind: Kinds::CORPSES, entity: Entity(7), node: 0, labels: 9, boss: true, locked: false };
/// assert!(Source::bosses().matches(&boss));
/// assert!(!Source::locked_chests().matches(&boss));
/// assert!(Source::kinds(Kinds::CORPSES).with_labels(&[9]).matches(&boss));
/// assert!(!Source::kinds(Kinds::CONTAINERS).with_nodes(&[3]).matches(&boss));
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Source {
    /// The kinds of container.
    pub kinds: Kinds,
    /// Only bosses' corpses.
    pub bosses_only: bool,
    /// Only chests that need a key or a lockpick.
    pub locked_only: bool,
    /// Only these loot label ids ([`Roll::labels`]); empty for any.
    pub labels: Vec<u32>,
    /// Only these world nodes ([`Roll::node`]); empty for any.
    pub nodes: Vec<u32>,
}

impl Source {
    /// Every container of `kinds`.
    ///
    /// # Examples
    ///
    /// See [`Source`].
    #[must_use]
    pub const fn kinds(kinds: Kinds) -> Self {
        Self {
            kinds,
            bosses_only: false,
            locked_only: false,
            labels: Vec::new(),
            nodes: Vec::new(),
        }
    }

    /// Every boss's corpse.
    ///
    /// # Examples
    ///
    /// See [`Source`].
    #[must_use]
    pub const fn bosses() -> Self {
        let mut source = Self::kinds(Kinds::CORPSES);
        source.bosses_only = true;
        source
    }

    /// Every chest that needs a key or a lockpick.
    ///
    /// # Examples
    ///
    /// See [`Source`].
    #[must_use]
    pub const fn locked_chests() -> Self {
        let mut source = Self::kinds(Kinds::CHESTS);
        source.locked_only = true;
        source
    }

    /// The same, limited to the loot label ids `labels`.
    ///
    /// # Examples
    ///
    /// See [`Source`].
    #[must_use]
    pub fn with_labels(mut self, labels: &[u32]) -> Self {
        self.labels.extend_from_slice(labels);
        self
    }

    /// The same, limited to the world nodes `nodes`.
    ///
    /// # Examples
    ///
    /// See [`Source`].
    #[must_use]
    pub fn with_nodes(mut self, nodes: &[u32]) -> Self {
        self.nodes.extend_from_slice(nodes);
        self
    }

    /// Whether `roll` comes from here.
    ///
    /// # Examples
    ///
    /// See [`Source`].
    #[must_use]
    pub fn matches(&self, roll: &Roll) -> bool {
        self.kinds.contains(roll.kind)
            && (!self.bosses_only || roll.boss)
            && (!self.locked_only || roll.locked)
            && (self.labels.is_empty() || self.labels.contains(&roll.labels))
            && (self.nodes.is_empty() || self.nodes.contains(&roll.node))
    }
}

/// A mod item in random loot, for [`Load::add_loot`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::content::Guid;
/// use enshwick_loader_api::loot::{Drop, Source};
///
/// const STAFF: Guid = Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31");
///
/// let drop = Drop::new(STAFF.id(), 0.1, Source::bosses()).count(1, 2);
/// assert_eq!(drop.count, [1, 2]);
/// let one_of = Drop::one_of(&[STAFF.id(), 7], 0.1, Source::bosses());
/// assert_eq!(one_of.items.len(), 2);
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct Drop {
    /// The items' game ids, [`Guid::id`](crate::content::Guid::id) of their
    /// GUIDs; a drop that hits gives one of them, each as likely.
    pub items: Vec<u32>,
    /// The chance per roll, from 0 to 1.
    pub chance: f64,
    /// How many, at least and at most; each count in between is as likely.
    pub count: [u16; 2],
    /// Where it may drop.
    pub from: Source,
}

impl Drop {
    /// One `item` at `chance` per roll of a container `from` matches.
    ///
    /// # Examples
    ///
    /// See [`Drop`].
    #[must_use]
    pub fn new(item: u32, chance: f64, from: Source) -> Self {
        Self::one_of(&[item], chance, from)
    }

    /// One of `items`, each as likely, at `chance` per roll of a container
    /// `from` matches: the chance is that of any of them.
    ///
    /// # Examples
    ///
    /// See [`Drop`].
    #[must_use]
    pub fn one_of(items: &[u32], chance: f64, from: Source) -> Self {
        Self {
            items: items.to_vec(),
            chance,
            count: [1, 1],
            from,
        }
    }

    /// The same with `min` to `max` items per drop.
    ///
    /// # Examples
    ///
    /// See [`Drop`].
    #[must_use]
    pub const fn count(mut self, min: u16, max: u16) -> Self {
        self.count = [min, max];
        self
    }

    /// Why the game could not take it, if so.
    fn check(&self) -> Result<(), Off> {
        if self.items.is_empty() || self.items.contains(&0) {
            return Err(Off::new(format!("loot drop {:?}: no item", self.items)));
        }
        if !(0.0..=1.0).contains(&self.chance) {
            return Err(Off::new(format!(
                "loot drop {:?}: chance {} is not between 0 and 1",
                self.items, self.chance
            )));
        }
        let [min, max] = self.count;
        if min == 0 || min > max {
            return Err(Off::new(format!(
                "loot drop {:?}: count {min}..{max} is empty",
                self.items
            )));
        }
        Ok(())
    }

    /// The item and how many to add when `dice` draws this roll; `None`
    /// when it misses.
    fn draw(&self, dice: &Dice) -> Option<(u32, u16)> {
        if dice.unit() >= self.chance {
            return None;
        }
        let pick = |n: u64| usize::try_from(dice.next() % n.max(1)).unwrap_or(0);
        let item = *self.items.get(pick(self.items.len() as u64))?;
        let [min, max] = self.count;
        let extra = u16::try_from(pick(u64::from(max - min) + 1)).unwrap_or(0);
        Some((item, min + extra))
    }
}

/// A splitmix64 generator shared by every roll; loot needs no more.
#[derive(Debug)]
struct Dice(AtomicU64);

impl Dice {
    const GAMMA: u64 = 0x9e37_79b9_7f4a_7c15;

    fn next(&self) -> u64 {
        let mut z = self
            .0
            .fetch_add(Self::GAMMA, Ordering::Relaxed)
            .wrapping_add(Self::GAMMA);
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A number from 0 up to 1, 1 excluded.
    #[expect(clippy::cast_precision_loss, reason = "53 bits fit an f64 exactly")]
    fn unit(&self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

type RollFn = unsafe extern "system" fn(
    res: *mut u8,
    txn: *mut u8,
    generation: *mut u8,
    labels: *mut u8,
    request: *const u8,
) -> *mut u8;
type GiveFn = unsafe extern "system" fn(
    res: *mut [u64; 2],
    txn: *mut u8,
    inventory: u32,
    hint: u64,
    item: u32,
    count: i32,
    level: *const [u8; 16],
    rarity: *const [u8; 16],
    extra: *const [u8; 16],
) -> *mut [u64; 2];

crate::detour! {
    static ROLL: unsafe extern "system" fn(
        res: *mut u8,
        txn: *mut u8,
        generation: *mut u8,
        labels: *mut u8,
        request: *const u8,
    ) -> *mut u8 = after_roll;
}
crate::detour! {
    static ROLL_ONE: unsafe extern "system" fn(
        res: *mut u8,
        out: *mut u8,
        generation: *mut u8,
        labels: *mut u8,
        request: *const u8,
    ) -> *mut u8 = after_roll_one;
}

/// Offsets and component types the links read.
#[derive(Debug)]
struct Engine {
    give: GiveFn,
    components: Components,
    txn_context: usize,
    txn_entity: usize,
    request_node: usize,
    request_labels: usize,
    destroy_on_loot: u32,
    boss: u32,
    lock: u32,
    lock_item: usize,
}

static ENGINE: OnceLock<Engine> = OnceLock::new();
static DROPS: Mutex<Vec<Drop>> = Mutex::new(Vec::new());
static DICE: OnceLock<Dice> = OnceLock::new();

fn dice() -> &'static Dice {
    DICE.get_or_init(|| {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        #[expect(clippy::cast_possible_truncation, reason = "the low bits seed")]
        Dice(AtomicU64::new(
            now as u64 ^ u64::from(std::process::id()).rotate_left(32),
        ))
    })
}

/// # Safety
///
/// The loader pins `address` to a function of type `F`.
unsafe fn function<F: Copy>(address: u64) -> F {
    let address = usize::try_from(address).unwrap_or(0);
    // SAFETY: forwarded from the caller; `F` is a function pointer.
    unsafe { core::mem::transmute_copy(&address) }
}

impl Engine {
    fn resolve(load: &mut Load<'_>) -> Result<Self, Off> {
        let world = load.world()?;
        let ty = |name: &str| {
            world
                .component_type(load, name)
                .map(crate::world::ComponentType::hash)
        };
        let size = |name: &str| {
            load.constant(name)
                .map(|v| usize::try_from(v).unwrap_or(usize::MAX))
        };
        let give = load.symbol("inventory.give")?;
        Ok(Self {
            // SAFETY: the loader pins this id to the game's add to a
            // transaction.
            give: unsafe { function(give) },
            components: Components::resolve(load)?,
            txn_context: size("loot.txn_context")?,
            txn_entity: size("loot.txn_entity")?,
            request_node: size("loot.request_node")?,
            request_labels: size("loot.request_labels")?,
            destroy_on_loot: ty("keen::ecs::DestroyOnLoot")?,
            boss: ty("keen::ecs::Boss")?,
            lock: ty("keen::ecs::InteractionLock")?,
            lock_item: load
                .field_offset("keen::ecs::InteractionLock", "requiredItem")
                .and_then(|v| usize::try_from(v).map_err(|_| Off::new("offset")))?,
        })
    }

    /// The roll the game just made into `txn` for `request`.
    ///
    /// # Safety
    ///
    /// `txn` and `request` are the live arguments of the roll, on its thread.
    unsafe fn roll(&self, txn: *mut u8, request: *const u8) -> (Roll, *mut u64) {
        // SAFETY: the transaction keeps its system context and the
        // container it fills at these offsets.
        let ctx: *mut u64 = unsafe { read_at(txn, self.txn_context) };
        // SAFETY: as above.
        let entity = Entity(unsafe { read_at(txn, self.txn_entity) });
        // SAFETY: the request holds the node and label ids here.
        let node: u32 = unsafe { read_at(request, self.request_node) };
        // SAFETY: as above.
        let labels: u32 = unsafe { read_at(request, self.request_labels) };
        // SAFETY: the roll's system context, on its thread.
        let has = |hash| unsafe { self.components.component(ctx, entity, hash) };
        let corpse = has(self.destroy_on_loot).is_some() || node == 0;
        let locked = has(self.lock).is_some_and(|data| {
            // SAFETY: a live InteractionLock holds the item it asks for here.
            unsafe { read_at::<u32>(data, self.lock_item) != 0 }
        });
        let roll = Roll {
            kind: if corpse {
                Kinds::CORPSES
            } else {
                Kinds::CHESTS
            },
            entity,
            node,
            labels,
            boss: corpse && has(self.boss).is_some(),
            locked: !corpse && locked,
        };
        (roll, ctx)
    }
}

fn after_roll(
    next: Next<RollFn>,
    res: *mut u8,
    txn: *mut u8,
    generation: *mut u8,
    labels: *mut u8,
    request: *const u8,
) -> *mut u8 {
    // SAFETY: the rest of the chain takes the arguments the game passed.
    let out = unsafe { next.get()(res, txn, generation, labels, request) };
    let Some(engine) = ENGINE.get() else {
        return out;
    };
    // SAFETY: the roll's status byte; 0 means the game commits `txn`.
    if res.is_null() || txn.is_null() || request.is_null() || unsafe { res.read() } != 0 {
        return out;
    }
    // SAFETY: the live arguments of the roll, on its thread.
    let (roll, _) = unsafe { engine.roll(txn, request) };
    let drops = DROPS.lock().unwrap_or_else(PoisonError::into_inner);
    let absent = [0u8; 16];
    for drop in drops.iter().filter(|d| d.from.matches(&roll)) {
        let Some((item, count)) = drop.draw(dice()) else {
            continue;
        };
        let mut added = [0u64; 2];
        // SAFETY: the game's add into the transaction the roll filled, with
        // no level, rarity or extra; a full container keeps what fits.
        unsafe {
            (engine.give)(
                &raw mut added,
                txn,
                roll.entity.0,
                0,
                item,
                i32::from(count),
                &raw const absent,
                &raw const absent,
                &raw const absent,
            )
        };
        crate::debug!(
            "loot: {count} of item {item} into {:?} ({:?}), status {}",
            roll.entity,
            roll.kind,
            added[0] & 0xff
        );
    }
    out
}

/// Item id, count, rarity, level and value: the single roll's output.
const ONE_ITEM: usize = 0;
const ONE_COUNT: usize = 4;
const ONE_SIZE: usize = 16;

fn after_roll_one(
    next: Next<RollFn>,
    res: *mut u8,
    out: *mut u8,
    generation: *mut u8,
    labels: *mut u8,
    request: *const u8,
) -> *mut u8 {
    // SAFETY: the rest of the chain takes the arguments the game passed.
    let ret = unsafe { next.get()(res, out, generation, labels, request) };
    if ENGINE.get().is_none() || res.is_null() || out.is_null() {
        return ret;
    }
    let roll = Roll {
        kind: Kinds::DESTROYED,
        entity: Entity::NONE,
        node: 0,
        labels: 0,
        boss: false,
        locked: false,
    };
    let drops = DROPS.lock().unwrap_or_else(PoisonError::into_inner);
    let Some((item, count)) = drops
        .iter()
        .filter(|d| d.from.matches(&roll))
        .find_map(|d| d.draw(dice()))
    else {
        return ret;
    };
    let mut stack = [0u8; ONE_SIZE];
    stack[ONE_ITEM..ONE_ITEM + 4].copy_from_slice(&item.to_le_bytes());
    stack[ONE_COUNT..ONE_COUNT + 2].copy_from_slice(&count.to_le_bytes());
    // SAFETY: the roll's 16-byte output; the callers drop the stack when
    // the status is 0 and the item and count are set.
    unsafe { out.cast::<[u8; ONE_SIZE]>().write_unaligned(stack) };
    // SAFETY: the roll's status byte.
    unsafe { res.write(0) };
    crate::debug!("loot: {count} of item {item} from a single roll");
    ret
}

impl Load<'_> {
    /// Adds a mod item to random loot on the server, the dedicated one or a
    /// client's while it hosts: each roll of a container `drop.from` matches
    /// adds the item at `drop.chance`, on top of the vanilla loot. Containers
    /// roll once, when first opened, so loot already rolled in a save stays
    /// as it was.
    ///
    /// The item must be in the game's item registry, a vanilla item or one
    /// from [`Load::add_item`]; unknown items are skipped by the game.
    ///
    /// # Errors
    ///
    /// [`Off`] when the drop is not valid or the build lacks a piece.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::content::Guid;
    /// use enshwick_loader_api::loot::{Drop, Source, Kinds};
    ///
    /// const STAFF: Guid = Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31");
    ///
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     load.add_loot(Drop::new(STAFF.id(), 0.10, Source::bosses()))?;
    ///     load.add_loot(Drop::new(STAFF.id(), 0.02, Source::locked_chests()))?;
    ///     load.add_loot(Drop::new(STAFF.id(), 0.001, Source::kinds(Kinds::CORPSES)).count(1, 3))
    /// }
    /// ```
    pub fn add_loot(&mut self, drop: Drop) -> Result<(), Off> {
        drop.check()?;
        if ENGINE.get().is_none() {
            let engine = Engine::resolve(self)?;
            self.hook(&ROLL, self.symbol("loot.roll")?)?;
            let _first = ENGINE.set(engine);
        }
        let single = drop.from.kinds.contains(Kinds::DESTROYED);
        let mut drops = DROPS.lock().unwrap_or_else(PoisonError::into_inner);
        let linked = drops
            .iter()
            .any(|d| d.from.kinds.contains(Kinds::DESTROYED));
        if single && !linked {
            self.hook(&ROLL_ONE, self.symbol("loot.roll_one")?)?;
        }
        drops.push(drop);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roll(kind: Kinds) -> Roll {
        Roll {
            kind,
            entity: Entity(5),
            node: 0,
            labels: 0,
            boss: false,
            locked: false,
        }
    }

    #[test]
    fn filters_keep_their_kind_and_markers() {
        let corpse = roll(Kinds::CORPSES);
        let boss = Roll {
            boss: true,
            labels: 3,
            ..corpse
        };
        let chest = Roll {
            node: 9,
            ..roll(Kinds::CHESTS)
        };
        let locked = Roll {
            locked: true,
            ..chest
        };
        assert!(Source::kinds(Kinds::CONTAINERS).matches(&corpse));
        assert!(Source::kinds(Kinds::CONTAINERS).matches(&chest));
        assert!(!Source::kinds(Kinds::CONTAINERS).matches(&roll(Kinds::DESTROYED)));
        assert!(Source::bosses().matches(&boss) && !Source::bosses().matches(&corpse));
        assert!(!Source::bosses().matches(&locked));
        assert!(Source::locked_chests().matches(&locked));
        assert!(!Source::locked_chests().matches(&chest));
        assert!(Source::bosses().with_labels(&[3]).matches(&boss));
        assert!(!Source::bosses().with_labels(&[4]).matches(&boss));
        assert!(
            Source::kinds(Kinds::CHESTS)
                .with_nodes(&[9])
                .matches(&chest)
        );
        assert!(
            !Source::kinds(Kinds::CHESTS)
                .with_nodes(&[8])
                .matches(&chest)
        );
    }

    #[test]
    fn invalid_drops_are_refused() {
        let from = Source::bosses();
        assert!(Drop::new(7, 0.5, from.clone()).check().is_ok());
        assert!(Drop::new(0, 0.5, from.clone()).check().is_err());
        assert!(Drop::new(7, 1.5, from.clone()).check().is_err());
        assert!(Drop::new(7, f64::NAN, from.clone()).check().is_err());
        assert!(Drop::new(7, 0.5, from.clone()).count(0, 1).check().is_err());
        assert!(Drop::new(7, 0.5, from).count(3, 2).check().is_err());
    }

    #[test]
    fn the_chance_holds_over_many_rolls() {
        let dice = Dice(AtomicU64::new(42));
        let drop = Drop::new(7, 0.1, Source::bosses()).count(2, 4);
        let mut hits = 0;
        for _ in 0..100_000 {
            if let Some((item, n)) = drop.draw(&dice) {
                assert_eq!(item, 7);
                assert!((2..=4).contains(&n));
                hits += 1;
            }
        }
        assert!((9_000..11_000).contains(&hits), "{hits}");
        let never = Drop::new(7, 0.0, Source::bosses());
        let always = Drop::new(7, 1.0, Source::bosses());
        for _ in 0..1000 {
            assert_eq!(never.draw(&dice), None);
            assert_eq!(always.draw(&dice), Some((7, 1)));
        }
    }

    #[test]
    fn one_of_spreads_over_its_items() {
        let dice = Dice(AtomicU64::new(7));
        let drop = Drop::one_of(&[1, 2, 3], 1.0, Source::bosses());
        let mut seen = [0usize; 3];
        for _ in 0..30_000 {
            let (item, _) = drop.draw(&dice).unwrap();
            seen[item as usize - 1] += 1;
        }
        assert!(
            seen.iter().all(|&n| (9_000..11_000).contains(&n)),
            "{seen:?}"
        );
        assert!(Drop::one_of(&[], 0.5, Source::bosses()).check().is_err());
        assert!(
            Drop::one_of(&[1, 0], 0.5, Source::bosses())
                .check()
                .is_err()
        );
    }
}
