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
use std::sync::{Arc, Mutex};

use crate::ecs::{Components, Entity};
use crate::fresh::Fresh;
use crate::inventory::Fill;
use crate::locks::Relock;
use crate::raw::{read_at, read_ptr_at, write_at};
use crate::{Load, Next, Off};

/// What a container is, for [`Source::kinds`].
///
/// # Examples
///
/// ```
/// use enshwick::loot::Kinds;
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
    /// use enshwick::loot::Kinds;
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
    /// use enshwick::loot::Kinds;
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
    /// The container; `None` for a single-item roll.
    pub entity: Option<Entity>,
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
/// use enshwick::ecs::Entity;
/// use enshwick::loot::{Source, Kinds, Roll};
///
/// let boss = Roll { kind: Kinds::CORPSES, entity: Some(Entity(7)), node: 0, labels: 9, boss: true, locked: false };
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
/// use enshwick::content::Id;
/// use enshwick::loot::{Drop, Source};
///
/// const STAFF: Id = Id::new("my-mod:staff");
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
    /// How full each dropped item's meter starts (see
    /// [`Meter`](crate::content::items::Meter)); `None` for its template's
    /// start.
    pub meter: Option<MeterRoll>,
    /// Where it may drop.
    pub from: Source,
}

/// How full a dropped item's meter starts, for [`Drop::meter`]: `min` to
/// `max` of `of`, each count in between as likely.
///
/// # Examples
///
/// ```
/// use enshwick::loot::MeterRoll;
///
/// let charges = MeterRoll { min: 1, max: 5, of: 5 };
/// assert!(charges.min <= charges.max && charges.max <= charges.of);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MeterRoll {
    /// The fewest.
    pub min: u16,
    /// The most.
    pub max: u16,
    /// The meter's maximum, as its [`Meter`](crate::content::items::Meter)
    /// says.
    pub of: u16,
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
            meter: None,
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

    /// The same with each dropped item's meter starting at `min` to `max`
    /// of `of`, each count as likely: charges rolled as the item drops.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::loot::{Drop, MeterRoll, Source};
    ///
    /// let drop = Drop::new(0x5e01, 0.1, Source::bosses()).meter(1, 5, 5);
    /// assert_eq!(drop.meter, Some(MeterRoll { min: 1, max: 5, of: 5 }));
    /// ```
    #[must_use]
    pub const fn meter(mut self, min: u16, max: u16, of: u16) -> Self {
        self.meter = Some(MeterRoll { min, max, of });
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
        if let Some(MeterRoll { min, max, of }) = self.meter
            && (min > max || max > of || of == 0)
        {
            return Err(Off::new(format!(
                "loot drop {:?}: meter {min}..{max} of {of} does not fit",
                self.items
            )));
        }
        Ok(())
    }

    /// The item, how many to add and their meter's fill when `dice` draws
    /// this roll; `None` when it misses.
    fn draw(&self, dice: &Dice) -> Option<(u32, u16, Option<Fill>)> {
        if dice.unit() >= self.chance {
            return None;
        }
        let pick = |n: u64| usize::try_from(dice.next() % n.max(1)).unwrap_or(0);
        let between = |min: u16, max: u16| {
            min + u16::try_from(pick(u64::from(max.saturating_sub(min)) + 1)).unwrap_or(0)
        };
        let item = *self.items.get(pick(self.items.len() as u64))?;
        let [min, max] = self.count;
        let fill = self.meter.map(|m| Fill::of(between(m.min, m.max), m.of));
        Some((item, between(min, max), fill))
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

static ENGINE: Fresh<Engine> = Fresh::new();
/// Every drop, with the group it belongs to: 0 for a mod's own, a content
/// pack's hash for the pack's, which a reload replaces whole.
static DROPS: Fresh<Mutex<Vec<(u64, Drop)>>> = Fresh::new();
/// Whether the single-item roll is linked.
static ONE_LINKED: Fresh<std::sync::atomic::AtomicBool> = Fresh::new();
/// The queues of the rolls watchers asked to hear of.
static WATCHERS: Fresh<Mutex<Vec<Watcher>>> = Fresh::new();

/// The rolls one watcher collects.
type Watcher = Arc<Mutex<Vec<Roll>>>;
static DICE: Fresh<Dice> = Fresh::new();

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

/// Turns a resolved address into a function pointer of type `F`.
///
/// # Safety
///
/// - `address` is the entry of a function with the signature and ABI of `F`.
unsafe fn function<F: crate::FnPtr>(address: u64) -> F {
    let address = usize::try_from(address).unwrap_or(0);
    // SAFETY: `F: FnPtr` is a function pointer the size of `usize`, and the
    // caller guarantees `address` is a function of type `F`.
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
            // SAFETY: the build table's `inventory.give` is the game's add of
            // items to a transaction, with the signature of `GiveFn`.
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
    /// - `txn` and `request` are the non-null arguments the game passed to
    ///   the container roll, used on its thread inside the hook.
    unsafe fn roll(&self, txn: *mut u8, request: *const u8) -> (Roll, *mut u64) {
        // SAFETY: the caller guarantees the roll's live transaction, which
        // keeps its system context pointer at the build table's
        // `loot.txn_context`.
        let ctx = unsafe { read_ptr_at::<u64>(txn, self.txn_context) };
        // SAFETY: the caller guarantees the roll's live transaction, which
        // keeps the u32 entity it fills at the build table's
        // `loot.txn_entity`.
        let entity = Entity(unsafe { read_at(txn, self.txn_entity) });
        // SAFETY: the caller guarantees the roll's live request, which keeps
        // its u32 node id at the build table's `loot.request_node`.
        let node: u32 = unsafe { read_at(request, self.request_node) };
        // SAFETY: the caller guarantees the roll's live request, which keeps
        // its u32 label ids at the build table's `loot.request_labels`.
        let labels: u32 = unsafe { read_at(request, self.request_labels) };
        // SAFETY: `ctx` is the live system context the transaction holds,
        // used on the roll's thread inside the hook.
        let has = |hash| unsafe { self.components.component(ctx, entity, hash) };
        let corpse = has(self.destroy_on_loot).is_some() || node == 0;
        let locked = has(self.lock).is_some_and(|data| {
            // SAFETY: `data` is the entity's live InteractionLock, whose
            // `requiredItem` u32 sits at the reflected offset `lock_item`.
            unsafe { read_at::<u32>(data, self.lock_item) != 0 }
        });
        let roll = Roll {
            kind: if corpse {
                Kinds::CORPSES
            } else {
                Kinds::CHESTS
            },
            entity: Some(entity),
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
    // SAFETY: these are the arguments the game called the roll with on this
    // thread, passed on unchanged, so the rest of the chain gets what the
    // game's own call gave it.
    let out = unsafe { next.get()(res, txn, generation, labels, request) };
    // Guarded here: the detour's own guard would roll again.
    crate::host::guard(
        "panic in a loot roll",
        || (),
        // SAFETY: `res`, `txn` and `request` are the arguments the game passed
        // to the roll that just returned, on its thread inside the hook.
        || unsafe { add_drops(res, txn, request) },
    );
    out
}

/// Adds the mods' drops to a container roll the game committed.
///
/// # Safety
///
/// - `res`, `txn` and `request` are null or the arguments the game passed to
///   the container roll that just returned, used on its thread inside the
///   hook.
unsafe fn add_drops(res: *mut u8, txn: *mut u8, request: *const u8) {
    let Some(engine) = ENGINE.get() else {
        return;
    };
    if res.is_null() || txn.is_null() || request.is_null() {
        return;
    }
    // SAFETY: `res` is not null, as the early return ensures, and the caller
    // guarantees the roll's status byte there; 0 means the game commits `txn`.
    if unsafe { read_at::<u8>(res, 0) } != 0 {
        return;
    }
    // SAFETY: `txn` and `request` are not null, as the early return ensures,
    // and the caller guarantees they are the roll's live arguments on its
    // thread.
    let (roll, _) = unsafe { engine.roll(txn, request) };
    for watcher in WATCHERS.get_or_init(Mutex::default).relock().iter() {
        watcher.relock().push(roll);
    }
    let Some(container) = roll.entity else {
        return;
    };
    let drops = DROPS.get_or_init(Mutex::default).relock();
    let absent = [0u8; 16];
    for drop in drops
        .iter()
        .map(|(_, d)| d)
        .filter(|d| d.from.matches(&roll))
    {
        let Some((item, count, fill)) = drop.draw(dice()) else {
            continue;
        };
        let mut extra = absent;
        if let Some(fill) = fill {
            extra[..4].copy_from_slice(&fill.fraction().to_le_bytes());
            extra[4] = 1;
        }
        let mut added = [0u64; 2];
        // SAFETY: `txn` is the live transaction the roll filled, as the caller
        // guarantees, on its thread; the build table's `inventory.give` takes
        // it with three 16-byte options, here locals: no level, no rarity, and
        // the new items' fuel fill. A full container keeps what fits.
        unsafe {
            (engine.give)(
                &raw mut added,
                txn,
                container.0,
                0,
                item,
                i32::from(count),
                &raw const absent,
                &raw const absent,
                &raw const extra,
            )
        };
        crate::debug!(
            "loot: {count} of item {item} into {:?} ({:?}), status {}",
            roll.entity,
            roll.kind,
            added[0] & 0xff
        );
    }
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
    // SAFETY: these are the arguments the game called the single roll with
    // on this thread, passed on unchanged, so the rest of the chain gets what
    // the game's own call gave it.
    let ret = unsafe { next.get()(res, out, generation, labels, request) };
    // Guarded here: the detour's own guard would roll again.
    crate::host::guard(
        "panic in a loot roll",
        || (),
        // SAFETY: `res` and `out` are the arguments the game passed to the
        // single roll that just returned, on its thread inside the hook.
        || unsafe { add_one(res, out) },
    );
    ret
}

/// Puts a mod's drop into a single roll that came out empty.
///
/// # Safety
///
/// - `res` is null or the single roll's status byte, writable.
/// - `out` is null or the single roll's output, writable for `ONE_SIZE`
///   bytes.
/// - Both are the arguments the game passed to the single roll that just
///   returned, used on its thread inside the hook.
unsafe fn add_one(res: *mut u8, out: *mut u8) {
    if ENGINE.get().is_none() || res.is_null() || out.is_null() {
        return;
    }
    let roll = Roll {
        kind: Kinds::DESTROYED,
        entity: None,
        node: 0,
        labels: 0,
        boss: false,
        locked: false,
    };
    let drops = DROPS.get_or_init(Mutex::default).relock();
    let Some((item, count, _)) = drops
        .iter()
        .map(|(_, d)| d)
        .filter(|d| d.from.matches(&roll))
        .find_map(|d| d.draw(dice()))
    else {
        return;
    };
    let mut stack = [0u8; ONE_SIZE];
    stack[ONE_ITEM..ONE_ITEM + 4].copy_from_slice(&item.to_le_bytes());
    stack[ONE_COUNT..ONE_COUNT + 2].copy_from_slice(&count.to_le_bytes());
    // SAFETY: `out` is not null, as the early return ensures, and the caller
    // guarantees the single roll's `ONE_SIZE` writable output bytes; the game's
    // callers drop the stack when the status is 0 and the item and count are
    // set.
    unsafe { write_at(out, 0, stack) };
    // SAFETY: `res` is not null, as the early return ensures, and the caller
    // guarantees the single roll's writable status byte.
    unsafe { write_at(res, 0, 0u8) };
    crate::debug!("loot: {count} of item {item} from a single roll");
}

/// Links the roll of containers' loot once.
fn link_roll(load: &mut Load<'_>) -> Result<(), Off> {
    if ENGINE.get().is_none() {
        let engine = Engine::resolve(load)?;
        let roll = load.symbol("loot.roll")?;
        // SAFETY: the build table's `loot.roll` is the roll `ROLL` declares.
        unsafe { load.hook(&ROLL, roll) }?;
        let _first = ENGINE.set(engine);
    }
    Ok(())
}

/// A queue that gets every container roll the game commits from now on, on
/// the roll's worker thread; the caller drains it on the game thread, where
/// the rolled items have their entities.
pub(crate) fn watch_rolls(load: &mut Load<'_>) -> Result<Arc<Mutex<Vec<Roll>>>, Off> {
    link_roll(load)?;
    let queue = Arc::new(Mutex::new(Vec::new()));
    WATCHERS
        .get_or_init(Mutex::default)
        .relock()
        .push(Arc::clone(&queue));
    Ok(queue)
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
    /// use enshwick::content::Id;
    /// use enshwick::loot::{Drop, Source, Kinds};
    ///
    /// const STAFF: Id = Id::new("my-mod:staff");
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.add_loot(Drop::new(STAFF.id(), 0.10, Source::bosses()))?;
    ///     load.add_loot(Drop::new(STAFF.id(), 0.02, Source::locked_chests()))?;
    ///     load.add_loot(Drop::new(STAFF.id(), 0.001, Source::kinds(Kinds::CORPSES)).count(1, 3))
    /// }
    /// ```
    pub fn add_loot(&mut self, drop: Drop) -> Result<(), Off> {
        self.add_loot_in(0, drop)
    }

    /// [`Load::add_loot`] for the drops of group `group`.
    pub(crate) fn add_loot_in(&mut self, group: u64, drop: Drop) -> Result<(), Off> {
        drop.check()?;
        link_roll(self)?;
        if drop.from.kinds.contains(Kinds::DESTROYED)
            && !ONE_LINKED
                .get_or_init(Default::default)
                .load(Ordering::Acquire)
        {
            let roll_one = self.symbol("loot.roll_one")?;
            // SAFETY: the build table's `loot.roll_one` is the single roll
            // `ROLL_ONE` declares.
            unsafe { self.hook(&ROLL_ONE, roll_one) }?;
            ONE_LINKED
                .get_or_init(Default::default)
                .store(true, Ordering::Release);
        }
        DROPS
            .get_or_init(Mutex::default)
            .relock()
            .push((group, drop));
        Ok(())
    }
}

/// Replaces the drops the mod added with [`Load::add_loot`] with `drops`, from
/// any thread, such as when a setting turns a drop off or changes its chance;
/// no drops stops them all.
///
/// A kind of roll nothing linked at start stays off until a restart, so a mod
/// that may turn its drops on later adds them at init and replaces them with
/// none at once.
///
/// # Errors
///
/// [`Off`] when a drop is not valid or needs a roll no drop linked at start.
///
/// # Examples
///
/// ```
/// use enshwick::content::Id;
/// use enshwick::loot::{Drop, Source, replace_drops};
///
/// const STAFF: Id = Id::new("my-mod:staff");
///
/// fn set(on: bool) -> Result<(), enshwick::Off> {
///     let drops = if on {
///         vec![Drop::new(STAFF.id(), 0.10, Source::bosses())]
///     } else {
///         Vec::new()
///     };
///     replace_drops(drops)
/// }
/// ```
pub fn replace_drops(drops: Vec<Drop>) -> Result<(), Off> {
    replace_group(0, drops).map_err(Off::new)
}

/// Replaces every drop of group `group` with `drops`, as a content pack's
/// reload does; no drops forgets the group.
///
/// # Errors
///
/// Why the drops cannot take effect before a restart: a drop that is not
/// valid, or one of a kind of roll nothing linked at start.
pub(crate) fn replace_group(group: u64, drops: Vec<Drop>) -> Result<(), String> {
    for drop in &drops {
        drop.check().map_err(|off| off.to_string())?;
        if ENGINE.get().is_none() {
            return Err("no loot was linked at start; it takes a restart".to_owned());
        }
        if drop.from.kinds.contains(Kinds::DESTROYED)
            && !ONE_LINKED
                .get_or_init(Default::default)
                .load(Ordering::Acquire)
        {
            return Err(
                "single-item rolls were not linked at start; they take a restart".to_owned(),
            );
        }
    }
    let mut all = DROPS.get_or_init(Mutex::default).relock();
    all.retain(|(g, _)| *g != group);
    all.extend(drops.into_iter().map(|drop| (group, drop)));
    Ok(())
}

#[cfg(test)]
mod tests;
