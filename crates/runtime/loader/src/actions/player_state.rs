//! The local player's own state on the client: where it stands and where
//! its camera looks, its health and stamina, its action bar's selected
//! slot and the slots of its inventory, read beside the held controls in
//! `player_ui`'s entry, on the row that read found, and kept for any thread
//! to read.
//!
//! Every value is the client's replicated copy, the one its interface
//! draws: `player_ui` copies the same `NetworkPlayerAttributes` fields into
//! the HUD (client 26430625), so a value may trail the server by a step.

use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};

use enshwick_core::__private::inventory_links::{self, SetupLayout};
use enshwick_core::__private::{INVENTORY_LINKS as LINKS, INVENTORY_SLOTS as SLOTS};
use enshwick_core::Entity;
use enshwick_core::abi::ClientSlot;
use enshwick_core::inventory::InventoryCategory;
use enshwick_gamedata::catalogue;

use crate::engine::copied::Copied;
use crate::engine::ecs::Components;
use crate::engine::fuse::Fuse;
use crate::engine::loader::Loader;

mod equipment;

use equipment::{
    EQUIPMENT_SLOTS, EQUIPMENT_WORDS, EquipmentLayout, equipment_layout, equipment_words,
};
pub(crate) use equipment::{EquipMove, equip_move};

/// The feature's name in the log and on the Mods screen.
pub(crate) const FEATURE: &str = "the local player's position, look, vitals and inventory";
/// The slot read's name in the log and on the Mods screen.
const SLOTS_FEATURE: &str = "the local player's inventory slots and item commands";

/// The switch of these reads alone, so a fault leaves the held controls
/// they run beside read.
pub(crate) static FUSE: Fuse = Fuse::new(FEATURE);
/// The switch of the slot read, off on a build that lacks its layout, so
/// the item commands answer unavailable and the other parts still read.
pub(crate) static SLOTS_FUSE: Fuse = Fuse::new(SLOTS_FEATURE);

/// The most slots a read keeps: twice the slots of a whole inventory,
/// room for the player's links and the virtual ones below them.
pub(crate) const MAX_SLOTS: usize = 2 * LINKS * SLOTS as usize;
/// How deep the slot walk follows virtual links, as the server's reads do.
const MAX_DEPTH: u32 = 4;
/// The watch entry for the local player's own inventory, which the steps
/// know and a command may not yet.
pub(crate) const PLAYER: u32 = u32::MAX;
/// How long after a mod's last ask for the slots the steps keep reading
/// them; past it they cost nothing.
const SLOTS_ASK_MS: u64 = 5000;

/// The parts a step read, a bit each, as the snapshot's groups name them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Parts(u8);

impl Parts {
    pub(crate) const POSITION: Self = Self(1);
    pub(crate) const LOOK: Self = Self(2);
    pub(crate) const VITALS: Self = Self(4);
    pub(crate) const ACTIONBAR: Self = Self(8);

    pub(crate) const fn contains(self, part: Self) -> bool {
        self.0 & part.0 == part.0
    }

    #[must_use]
    pub(crate) const fn with(self, part: Self) -> Self {
        Self(self.0 | part.0)
    }
}

/// One step's read of the local player.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Read {
    /// The parts this read holds.
    pub(crate) parts: Parts,
    /// Its position, three 32.32 fixed-point `i64` metres.
    pub(crate) position: [i64; 3],
    /// Its camera's orientation, a quaternion x, y, z, w.
    pub(crate) look: [f32; 4],
    /// Health, maximum health, stamina and maximum stamina.
    pub(crate) vitals: [u16; 4],
    /// The action bar's selected slot.
    pub(crate) actionbar: u8,
}

impl Read {
    /// The read in words, with the time `at` it was taken, as
    /// [`local_held::now_ms`](super::local_held::now_ms) counts.
    fn pack(&self, at: u64) -> [u64; 8] {
        let [x, y, z] = self.position.map(i64::cast_unsigned);
        let [l0, l1, l2, l3] = self.look.map(|part| u64::from(part.to_bits()));
        let [health, health_max, stamina, stamina_max] = self.vitals.map(u64::from);
        [
            x,
            y,
            z,
            l0 | l1 << 32,
            l2 | l3 << 32,
            health | health_max << 16 | stamina << 32 | stamina_max << 48,
            u64::from(self.actionbar) | u64::from(self.parts.0) << 8,
            at,
        ]
    }

    /// The read packed into `words`, and when it was taken.
    fn unpack(words: [u64; 8]) -> (Self, u64) {
        let [x, y, z, look01, look23, vitals, slot, at] = words;
        let low = |word: u64| f32::from_bits(u32::try_from(word & 0xffff_ffff).unwrap_or(0));
        let half = |shift: u32| u16::try_from(vitals >> shift & 0xffff).unwrap_or(0);
        let read = Self {
            parts: Parts(u8::try_from(slot >> 8 & 0xff).unwrap_or(0)),
            position: [x, y, z].map(u64::cast_signed),
            look: [
                low(look01),
                low(look01 >> 32),
                low(look23),
                low(look23 >> 32),
            ],
            vitals: [half(0), half(16), half(32), half(48)],
            actionbar: u8::try_from(slot & 0xff).unwrap_or(0),
        };
        (read, at)
    }
}

/// A slot in three words: the packed slot with its category and flags
/// above the index, the item with its count, and the item's entity.
fn pack_slot(slot: &ClientSlot) -> [u64; 3] {
    [
        slot.slot & SLOT_ID | u64::from(slot.category) << 40 | u64::from(slot.flags) << 48,
        u64::from(slot.item) | u64::from(slot.count) << 32,
        u64::from(slot.pide.get()),
    ]
}

/// The slot `pack_slot` packed into `words`.
fn unpack_slot(words: [u64; 3]) -> ClientSlot {
    let [at, stack, pide] = words;
    let byte = |shift: u32| u8::try_from(at >> shift & 0xff).unwrap_or(0);
    let low = |word: u64| u32::try_from(word & 0xffff_ffff).unwrap_or(0);
    enshwick_abi::headed!(ClientSlot {
        slot: at & SLOT_ID,
        item: low(stack),
        count: low(stack >> 32),
        pide: Entity::from_engine(low(pide)),
        category: byte(40),
        flags: byte(48),
        ..ClientSlot::EMPTY
    })
}

/// The words of the slot copy: the count, the read's time, then three per
/// slot.
const SLOT_WORDS: usize = 2 + 3 * MAX_SLOTS;

/// What the steps keep, written only on `player_ui`'s thread, but for the
/// watch and the ask a command sets.
struct State {
    /// The last read of the player's parts.
    parts: Copied<8>,
    /// The last read of the inventory's slots.
    slots: Copied<SLOT_WORDS>,
    /// What `SlotSelection` names for each equipment slot, as
    /// [`EQUIPMENT_WORDS`] lays it out.
    equipment: Copied<EQUIPMENT_WORDS>,
    /// When a mod last asked for the slots, as `now_ms` counts; 0 never.
    slots_asked: AtomicU64,
    /// The two inventories a command's outcome is watched in, low and high
    /// word, 0 for none.
    watch: AtomicU64,
    /// Moves on with every new watch, so a read of an old one is no
    /// baseline for it.
    watch_generation: AtomicU16,
    /// The generation and time of the last read of the watched inventories.
    watched: AtomicU64,
    /// That read's fingerprint of their slots.
    watched_print: AtomicU64,
    /// How many reads found the watched slots changed from the read before.
    watched_changes: AtomicU64,
}

static STATE: State = State {
    parts: Copied::new(),
    slots: Copied::new(),
    equipment: Copied::new(),
    slots_asked: AtomicU64::new(0),
    watch: AtomicU64::new(0),
    watch_generation: AtomicU16::new(0),
    watched: AtomicU64::new(0),
    watched_print: AtomicU64::new(0),
    watched_changes: AtomicU64::new(0),
};

/// The bits of `watched` that hold the read's time.
const WATCHED_AT: u64 = (1 << 48) - 1;

/// Watches the two inventories `inventories`, each a root walked as the
/// player's, a sub-inventory read alone, [`PLAYER`] or 0 for none, so a
/// command's outcome counts only their changes; asks the steps to read at
/// `now`.
pub(crate) fn watch(inventories: [u32; 2], now: u64) {
    let [low, high] = inventories.map(u64::from);
    STATE.watch.store(low | high << 32, Ordering::Relaxed);
    let _old = STATE.watch_generation.fetch_add(1, Ordering::Release);
    STATE.slots_asked.store(now, Ordering::Relaxed);
}

/// Whether a step read the current watch within a second of `now`, the
/// baseline a command's change counts against; asks the steps to go on.
pub(crate) fn watch_fresh(now: u64) -> bool {
    STATE.slots_asked.store(now, Ordering::Relaxed);
    let watched = STATE.watched.load(Ordering::Acquire);
    watched >> 48 == u64::from(STATE.watch_generation.load(Ordering::Acquire))
        && super::local_held::fresh(watched & WATCHED_AT, now)
}

/// How many step reads found the watched inventories changed, so a command
/// sees its effect as a later count; asks the steps to read at `now`.
pub(crate) fn watched_changes(now: u64) -> u64 {
    STATE.slots_asked.store(now, Ordering::Relaxed);
    STATE.watched_changes.load(Ordering::Relaxed)
}

/// The last whole read of the player's parts and when it was taken; `None`
/// before a step read one, or while a write stays mid-way through every
/// try.
pub(crate) fn last() -> Option<(Read, u64)> {
    let (read, at) = Read::unpack(STATE.parts.load()?);
    (at != 0).then_some((read, at))
}

/// The inventory's slots a step read within a second of `now`, and asks the
/// steps to go on reading them; `None` while none did.
pub(crate) fn slots(now: u64) -> Option<Vec<ClientSlot>> {
    STATE.slots_asked.store(now, Ordering::Relaxed);
    let words = STATE.slots.load()?;
    if !super::local_held::fresh(words[1], now) {
        return None;
    }
    let count = usize::try_from(words[0]).unwrap_or(0).min(MAX_SLOTS);
    Some(
        words[2..2 + 3 * count]
            .as_chunks::<3>()
            .0
            .iter()
            .map(|words| unpack_slot(*words))
            .collect(),
    )
}

/// Whether a mod asked for the slots recently enough at `now` that a step
/// reads them.
fn slots_wanted(now: u64) -> bool {
    let asked = STATE.slots_asked.load(Ordering::Relaxed);
    asked != 0 && now.saturating_sub(asked) <= SLOTS_ASK_MS
}

/// Where the inventory's slots read on this build.
#[derive(Debug)]
struct SlotLayout {
    setup: u32,
    inventory: u32,
    links: SetupLayout,
    stack: usize,
    stack_count: usize,
    stack_pide: usize,
    /// `SlotSelection`'s action bar entities, two `u32`; without them no
    /// slot is flagged as the action bar's.
    actionbar_entities: Option<usize>,
    /// `SlotSelection`'s `EquipmentSlotData`; without it no equip moves an
    /// item into an equipment inventory.
    equipment: Option<EquipmentLayout>,
}

/// Where each part reads on this build: a component's type hash and its
/// field offsets; `None` for a part the build lacks.
#[derive(Debug)]
pub(crate) struct Layout {
    /// `CurrentTransform`'s hash and the offset of its position.
    position: Option<(u32, usize)>,
    /// The camera orientation's offset inside `ClientPlayerInput`.
    look: Option<usize>,
    /// `NetworkPlayerAttributes`' hash and its four fields' offsets.
    vitals: Option<(u32, [usize; 4])>,
    /// `SlotSelection`'s hash and the action bar index's offset.
    actionbar: Option<(u32, usize)>,
    slots: Option<SlotLayout>,
}

impl Layout {
    /// Whether the build reads the inventory's slots.
    pub(crate) const fn reads_slots(&self) -> bool {
        self.slots.is_some()
    }

    /// The parts `loader`'s build reads, each from its reflected names; a
    /// part the build lacks logs its id once and stays unread, and lacking
    /// the slots turns [`SLOTS_FUSE`] off.
    pub(crate) fn resolve(loader: &Loader) -> Self {
        let at = |id: &'static str| {
            catalogue::constant(id, &loader.reflection).and_then(|at| usize::try_from(at).ok())
        };
        let hash = catalogue::type_hash;
        let [health, health_max, stamina, stamina_max] = [
            catalogue::ids::NETWORK_PLAYER_ATTRIBUTES_HEALTH,
            catalogue::ids::NETWORK_PLAYER_ATTRIBUTES_HEALTH_MAX,
            catalogue::ids::NETWORK_PLAYER_ATTRIBUTES_STAMINA,
            catalogue::ids::NETWORK_PLAYER_ATTRIBUTES_STAMINA_MAX,
        ]
        .map(at);
        let slots = || {
            let hash = |id| hash(id).ok_or(id);
            let at = |id| at(id).ok_or(id);
            Ok::<_, &'static str>(SlotLayout {
                setup: hash(catalogue::ids::INVENTORY_SETUP)?,
                inventory: hash(catalogue::ids::INVENTORY)?,
                links: SetupLayout {
                    links: at(catalogue::ids::INVENTORY_SETUP_LINKS)?,
                    categories: at(catalogue::ids::INVENTORY_SETUP_CATEGORIES)?,
                    initialized: at(catalogue::ids::INVENTORY_SETUP_INITIALIZED)?,
                },
                stack: crate::engine::catalogue::size(loader, catalogue::ids::ITEM_STACK)
                    .ok_or(catalogue::ids::ITEM_STACK)?,
                stack_count: at(catalogue::ids::ITEM_STACK_COUNT)?,
                stack_pide: at(catalogue::ids::ITEM_STACK_PIDE)?,
                actionbar_entities: at(catalogue::ids::SLOT_SELECTION_ACTIONBAR_ENTITIES).ok(),
                equipment: equipment_layout(loader),
            })
        };
        let slots = slots()
            .inspect_err(|&id| {
                let _off = loader.fuse_off(&SLOTS_FUSE, id, "this build lacks it");
            })
            .ok();
        let layout = Self {
            position: hash(catalogue::ids::CURRENT_TRANSFORM).zip(
                at(catalogue::ids::CURRENT_TRANSFORM_TRANSFORM)
                    .zip(at(catalogue::ids::WORLD_TRANSFORM_POSITION))
                    .map(|(transform, position)| transform + position),
            ),
            look: at(catalogue::ids::CLIENT_PLAYER_INPUT_CAMERA_ORIENTATION),
            vitals: hash(catalogue::ids::NETWORK_PLAYER_ATTRIBUTES).zip(
                health.zip(health_max).zip(stamina.zip(stamina_max)).map(
                    |((health, max), (stamina, stamina_max))| [health, max, stamina, stamina_max],
                ),
            ),
            actionbar: hash(catalogue::ids::SLOT_SELECTION)
                .zip(at(catalogue::ids::SLOT_SELECTION_ACTIONBAR)),
            slots,
        };
        for (lacks, part) in [
            (layout.position.is_none(), catalogue::ids::CURRENT_TRANSFORM),
            (
                layout.look.is_none(),
                catalogue::ids::CLIENT_PLAYER_INPUT_CAMERA_ORIENTATION,
            ),
            (
                layout.vitals.is_none(),
                catalogue::ids::NETWORK_PLAYER_ATTRIBUTES,
            ),
            (layout.actionbar.is_none(), catalogue::ids::SLOT_SELECTION),
        ] {
            if lacks {
                crate::log_once!(WARN, "{FEATURE}: this build lacks {part}; it stays unread");
            }
        }
        layout
    }
}

/// Reads the local player `player`'s parts, and its slots while a mod wants
/// them, and keeps them, taken at `at`.
///
/// # Safety
///
/// - `ctx` is the `player_ui` system's live, non-null context, at its entry
///   on its thread.
/// - `input` is `player`'s live `ClientPlayerInput`, unwritten until the
///   system runs.
pub(crate) unsafe fn read(
    ctx: *mut u64,
    components: &Components,
    layout: &Layout,
    player: u32,
    input: *const u8,
    at: u64,
) {
    let component = |entity: u32, hash: u32| {
        // SAFETY: the caller's live context at the system's entry, on its
        // thread.
        unsafe { components.component(ctx, entity, hash) }
    };
    let mut read = Read::default();
    if let Some((hash, position)) = layout.position
        && let Some(data) = component(player, hash)
    {
        // SAFETY: `data` is the player's live `CurrentTransform`, whose
        // position, three fixed-point `i64`, sits at the reflected offset.
        read.position = unsafe { enshwick_core::raw::read_at(data, position) };
        read.parts = read.parts.with(Parts::POSITION);
    }
    if let Some(look) = layout.look {
        // SAFETY: the caller's live `ClientPlayerInput`, whose camera
        // orientation, a reflected `Quaternion` of four `f32`, sits at
        // `look`.
        read.look = unsafe { enshwick_core::raw::read_at(input, look) };
        read.parts = read.parts.with(Parts::LOOK);
    }
    if let Some((hash, fields)) = layout.vitals
        && let Some(data) = component(player, hash)
    {
        // SAFETY: `data` is the player's live `NetworkPlayerAttributes`,
        // whose four fields are reflected `u16` at these offsets.
        read.vitals = fields.map(|at| unsafe { enshwick_core::raw::read_at(data, at) });
        read.parts = read.parts.with(Parts::VITALS);
    }
    let selection = layout.actionbar.and_then(|(hash, index)| {
        component(player, hash).inspect(|&data| {
            // SAFETY: `data` is the player's live `SlotSelection`, whose
            // action bar index is a reflected `u8` at `index`.
            read.actionbar = unsafe { enshwick_core::raw::read_at(data, index) };
            read.parts = read.parts.with(Parts::ACTIONBAR);
        })
    });
    STATE.parts.write(read.pack(at));
    if let Some(slots) = layout.slots.as_ref().filter(|_| !SLOTS_FUSE.is_off()) {
        read_selected(&component, layout, slots, player, selection, at);
    }
}

/// Reads `player`'s equipment slots each step and, while a mod asks, its
/// slots at `at`; `selection` is the `SlotSelection` the action bar's read
/// found, which flags the action bar's slots whatever the equipment's layout.
fn read_selected(
    component: &dyn Fn(u32, u32) -> Option<*const u8>,
    layout: &Layout,
    slots: &SlotLayout,
    player: u32,
    selection: Option<*const u8>,
    at: u64,
) {
    // The action bar's read found the same `SlotSelection` the equipment's
    // layout names.
    let same = selection.filter(|_| {
        layout
            .actionbar
            .zip(slots.equipment.as_ref())
            .is_some_and(|((bar, _), equipment)| bar == equipment.selection)
    });
    let named = equipment_step(component, slots, player, same);
    if slots_wanted(at) {
        let bar = selection
            .zip(slots.actionbar_entities)
            .map_or([0; 2], |(data, entities)| {
                // SAFETY: `data` is the player's live `SlotSelection`, whose
                // two action bar entity ids sit at the reflected offset.
                unsafe { enshwick_core::raw::read_at::<[u32; 2]>(data, entities) }
            });
        read_slots(component, slots, player, bar, named.as_ref(), at);
    }
}

/// Reads the slots `player`'s `SlotSelection` names for its equipment slots,
/// `selection` when the caller found it already, and keeps where one moved
/// ([`equipment::moved_words`]); every step, so allocation-free: one
/// component read, at most [`EQUIPMENT_SLOTS`] words and, for a slot that
/// moved, its item's.
fn equipment_step(
    component: &dyn Fn(u32, u32) -> Option<*const u8>,
    slots: &SlotLayout,
    player: u32,
    selection: Option<*const u8>,
) -> Option<[u64; EQUIPMENT_SLOTS]> {
    let equipment = slots.equipment.as_ref()?;
    let data = selection.or_else(|| component(player, equipment.selection))?;
    let mut named = [0u64; EQUIPMENT_SLOTS];
    for (i, slot) in named[..equipment.count].iter_mut().enumerate() {
        // SAFETY: `data` is the player's live `SlotSelection`, whose
        // `EquipmentSlotData` at the reflected `offset` holds `count`
        // `InventorySlotId`s of the reflected `stride`, at least 8.
        let id: u64 =
            unsafe { enshwick_core::raw::read_at(data, equipment.offset + equipment.stride * i) };
        *slot = id & SLOT_ID;
    }
    let before = STATE.equipment.load();
    if let Some(words) = equipment::moved_words(before.as_ref(), &named, |slot| {
        item_at(component, slots, slot)
    }) {
        STATE.equipment.write(words);
    }
    Some(named)
}

/// The item in `slot`, an `InventorySlotId`'s entity and index; 0 for none
/// or a slot whose inventory does not read.
fn item_at(component: &dyn Fn(u32, u32) -> Option<*const u8>, at: &SlotLayout, slot: u64) -> u64 {
    let (link, index) = (slot & 0xffff_ffff, slot >> 32 & 0xff);
    let (Ok(link), Ok(index)) = (u32::try_from(link), u16::try_from(index)) else {
        return 0;
    };
    if index >= u16::from(SLOTS) {
        return 0;
    }
    component(link, at.inventory).map_or(0, |stacks| {
        // SAFETY: `stacks` is the link's live `Inventory` of `SLOTS`
        // reflected `ItemStack`s, `index < SLOTS`; each stack holds its u32
        // item id first.
        u64::from(unsafe {
            enshwick_core::raw::read_at::<u32>(stacks, usize::from(index) * at.stack)
        })
    })
}

/// The bytes of an `InventorySlotId` that name it: the `u32` entity and the
/// `u8` index after it.
const SLOT_ID: u64 = 0xff_ffff_ffff;

/// Reads and keeps the player `player`'s slots, those of the action bar's
/// two inventories `bar` flagged, with the slots `SlotSelection` named
/// for its equipment slots, then its watched inventories, at `at`.
fn read_slots(
    component: &dyn Fn(u32, u32) -> Option<*const u8>,
    slots: &SlotLayout,
    player: u32,
    bar: [u32; 2],
    named: Option<&[u64; EQUIPMENT_SLOTS]>,
    at: u64,
) {
    let mut words = [0u64; SLOT_WORDS];
    let mut count = 0;
    let mut dropped = false;
    walk(component, slots, player, &mut |slot| {
        let inventory = u32::try_from(slot.slot & 0xffff_ffff).unwrap_or(0);
        let slot = enshwick_abi::headed!(ClientSlot {
            flags: if bar.contains(&inventory) {
                ClientSlot::ACTIONBAR
            } else {
                0
            },
            ..slot
        });
        if count < MAX_SLOTS {
            words[2 + 3 * count..5 + 3 * count].copy_from_slice(&pack_slot(&slot));
            count += 1;
        } else {
            dropped = true;
        }
    });
    if dropped {
        log_dropped();
    }
    words[0] = u64::try_from(count).unwrap_or(0);
    words[1] = at;
    if let Some(named) = named {
        let read = &words[2..2 + 3 * count];
        let before = STATE.equipment.load();
        STATE
            .equipment
            .write(equipment_words(before.as_ref(), named, read, at));
    }
    STATE.slots.write(words);
    read_watch(component, slots, player, at);
}

/// A slot layout of 12-byte stacks, item, count and entity, for tests and
/// the benchmark over records of their own.
#[cfg(any(test, feature = "test"))]
const TEST_SLOTS: SlotLayout = SlotLayout {
    setup: 1,
    inventory: 2,
    links: SetupLayout {
        links: 0,
        categories: 4 * LINKS,
        initialized: 5 * LINKS,
    },
    stack: 12,
    stack_count: 4,
    stack_pide: 8,
    actionbar_entities: None,
    equipment: None,
};

/// Readies one step's slot read for its benchmark over a stand-in player:
/// four sub-inventories and a virtual link of two more, 48 slots, and the
/// player's whole inventory watched, as a craft waits, or nothing watched.
#[cfg(feature = "test")]
#[doc(hidden)]
pub fn bench_slot_read(watching: bool) -> impl FnMut() -> u64 {
    let setup = |links: &[(u32, u8)]| {
        let mut bytes = vec![0u8; 6 * LINKS];
        for (i, &(link, category)) in links.iter().enumerate() {
            bytes[4 * i..4 * i + 4].copy_from_slice(&link.to_ne_bytes());
            bytes[4 * LINKS + i] = category;
        }
        bytes[5 * LINKS] = 1;
        bytes
    };
    let player = setup(&[
        (70, 0),
        (71, 0),
        (72, 0),
        (73, 0),
        (60, InventoryCategory::VIRTUAL.0),
    ]);
    let equipment = setup(&[(80, 0), (81, 0)]);
    let stacks = vec![7u8; 12 * usize::from(SLOTS)];
    watch(if watching { [PLAYER, 0] } else { [0, 0] }, 1);
    let mut at = 1000;
    move || {
        let component = |entity: u32, hash: u32| -> Option<*const u8> {
            match (entity, hash) {
                (7, 1) => Some(player.as_ptr()),
                (60, 1) => Some(equipment.as_ptr()),
                (70..=73 | 80 | 81, 2) => Some(stacks.as_ptr()),
                _ => None,
            }
        };
        at += 16;
        read_slots(&component, &TEST_SLOTS, 7, [0; 2], None, at);
        STATE.watched_changes.load(Ordering::Relaxed)
    }
}

/// Readies one step's read of the 20 equipment slots `SlotSelection` names,
/// as client 26430625 lays them out, for its benchmark: with none moved, the
/// read of every step, or with one moved back and forth each step.
#[cfg(feature = "test")]
#[doc(hidden)]
pub fn bench_equipment_step(moving: bool) -> impl FnMut() -> u64 {
    const STRIDE: usize = 8;
    let mut selection = [0u8; STRIDE * 20];
    let stacks = vec![7u8; 12 * usize::from(SLOTS)];
    let slots = SlotLayout {
        equipment: Some(EquipmentLayout {
            selection: 3,
            offset: 0,
            stride: STRIDE,
            count: 20,
        }),
        ..TEST_SLOTS
    };
    let mut step = 0u64;
    move || {
        step += 1;
        if moving {
            let slot: u64 = if step.is_multiple_of(2) {
                0x50
            } else {
                0x51 | 1 << 32
            };
            selection[18 * STRIDE..19 * STRIDE].copy_from_slice(&slot.to_ne_bytes());
        }
        let data = selection.as_ptr();
        let component = |entity: u32, hash: u32| -> Option<*const u8> {
            match (entity, hash) {
                (7, 3) => Some(data),
                (0x50 | 0x51, 2) => Some(stacks.as_ptr()),
                _ => None,
            }
        };
        equipment_step(&component, &slots, 7, None).map_or(0, |named| named[18])
    }
}

#[cold]
fn log_dropped() {
    crate::log_once!(
        WARN,
        "{SLOTS_FEATURE}: the inventory holds more than {MAX_SLOTS} slots; the rest stay unread"
    );
}

/// Fingerprints the watched inventories' slots, read at `at`, and counts a
/// change against the read before of the same watch.
fn read_watch(
    component: &dyn Fn(u32, u32) -> Option<*const u8>,
    at_layout: &SlotLayout,
    player: u32,
    at: u64,
) {
    let generation = STATE.watch_generation.load(Ordering::Acquire);
    let watch = STATE.watch.load(Ordering::Relaxed);
    if watch == 0 {
        return;
    }
    let mut print = PRINT_SEED;
    for entity in [watch & 0xffff_ffff, watch >> 32] {
        let entity = match u32::try_from(entity).unwrap_or(0) {
            PLAYER => player,
            entity => entity,
        };
        if entity == 0 {
            continue;
        }
        let mut mix = |slot: ClientSlot| print = mixed(print, &pack_slot(&slot));
        if component(entity, at_layout.setup).is_some() {
            walk(component, at_layout, entity, &mut mix);
        } else {
            stacks(component, at_layout, entity, 0, &mut mix);
        }
    }
    if counts(STATE.watched.load(Ordering::Relaxed), generation, at)
        && STATE.watched_print.load(Ordering::Relaxed) != print
    {
        STATE.watched_changes.fetch_add(1, Ordering::Relaxed);
    }
    STATE.watched_print.store(print, Ordering::Relaxed);
    STATE.watched.store(
        u64::from(generation) << 48 | at & WATCHED_AT,
        Ordering::Release,
    );
}

/// Whether a read of watch `generation` at `at` compares with `last`, the
/// read before: of the same watch and within a second. Else it is the new
/// baseline, so a command never takes stale slots for its effect.
fn counts(last: u64, generation: u16, at: u64) -> bool {
    last >> 48 == u64::from(generation) && super::local_held::fresh(last & WATCHED_AT, at)
}

/// FNV-1a's offset basis, the fingerprint of no slots.
const PRINT_SEED: u64 = 0xcbf2_9ce4_8422_2325;

/// `print` with a slot's words folded in, FNV-1a by word: each step is a
/// bijection, so one changed word always changes the fingerprint.
const fn mixed(print: u64, words: &[u64; 3]) -> u64 {
    const PRIME: u64 = 0x100_0000_01b3;
    let mut print = print;
    let mut i = 0;
    while i < words.len() {
        print = (print ^ words[i]).wrapping_mul(PRIME);
        i += 1;
    }
    print
}

/// Hands `out` every slot of the inventory `root` holds, its virtual links
/// followed to [`MAX_DEPTH`], in the game's order, as the server's reads
/// walk one.
fn walk(
    component: &dyn Fn(u32, u32) -> Option<*const u8>,
    at: &SlotLayout,
    root: u32,
    out: &mut dyn FnMut(ClientSlot),
) {
    let mut setup = |entity: Entity| component(entity.get(), at.setup);
    let mut visit = |link: Entity, category: InventoryCategory| {
        if category != InventoryCategory::VIRTUAL {
            stacks(component, at, link.get(), category.0, out);
        }
        core::ops::ControlFlow::<()>::Continue(())
    };
    // SAFETY: `component` hands only an entity's live component of the
    // type asked for, on the thread that reads it (the callers' context),
    // and the setup's layout is the build's reflected one.
    let _walked = unsafe {
        inventory_links::walk(
            Entity::from_engine(root),
            MAX_DEPTH,
            Entity::NONE,
            &at.links,
            &mut setup,
            &mut visit,
        )
    };
}

/// Hands `out` the slots of the sub-inventory `link`, of `category`; none
/// when it has no `Inventory`.
fn stacks(
    component: &dyn Fn(u32, u32) -> Option<*const u8>,
    at: &SlotLayout,
    link: u32,
    category: u8,
    out: &mut dyn FnMut(ClientSlot),
) {
    let Some(stacks) = component(link, at.inventory) else {
        return;
    };
    for index in 0..SLOTS {
        let base = usize::from(index) * at.stack;
        let field = |offset: usize| {
            // SAFETY: `stacks` is the link's live `Inventory` of `SLOTS`
            // reflected `ItemStack`s, `index < SLOTS`; each stack holds its
            // u32 item id first, then its u32 count and u32 item entity at
            // the reflected offsets this is called with.
            unsafe { enshwick_core::raw::read_at::<u32>(stacks, base + offset) }
        };
        let (item, count, pide) = (field(0), field(at.stack_count), field(at.stack_pide));
        out(enshwick_abi::headed!(ClientSlot {
            slot: u64::from(link) | u64::from(index) << 32,
            item,
            count,
            pide: Entity::from_engine(pide),
            category,
            ..ClientSlot::EMPTY
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_read_comes_back_whole_from_its_words() {
        let read = Read {
            parts: Parts::POSITION.with(Parts::VITALS).with(Parts::ACTIONBAR),
            position: [-5 << 32, 1 << 40, i64::MIN],
            look: [0.0, -0.707, 0.0, 0.707],
            vitals: [37, 120, 0, u16::MAX],
            actionbar: 7,
        };
        assert_eq!(Read::unpack(read.pack(42)), (read, 42));
    }

    #[test]
    fn a_slot_comes_back_whole_from_its_words() {
        let slot = enshwick_abi::headed!(ClientSlot {
            slot: 0x41 | 7 << 32,
            item: 0xdead_beef,
            count: 65_535,
            pide: Entity::from_engine(9000),
            category: 4,
            flags: ClientSlot::ACTIONBAR,
            ..ClientSlot::EMPTY
        });
        let back = unpack_slot(pack_slot(&slot));
        assert_eq!(
            (
                back.slot,
                back.item,
                back.count,
                back.pide,
                back.category,
                back.flags
            ),
            (
                slot.slot,
                slot.item,
                slot.count,
                slot.pide,
                slot.category,
                slot.flags
            )
        );
    }

    #[test]
    fn only_a_read_of_the_same_watch_in_the_last_second_is_counted_against() {
        let last = (3 << 48) + 1000;
        assert!(counts(last, 3, 1500), "the same watch, fresh");
        assert!(!counts(last, 4, 1500), "a new watch: a baseline");
        assert!(
            !counts(last, 3, 1000 + local_held_fresh_past()),
            "the last read is stale: a baseline"
        );
        assert!(!counts(0, 0, 1500), "no read before");
    }

    #[test]
    fn one_changed_slot_word_changes_the_fingerprint() {
        let slots = [[1, 2, 3], [4, 5, 6]];
        let print = |slots: &[[u64; 3]]| slots.iter().fold(PRINT_SEED, mixed);
        let before = print(&slots);
        for (slot, word) in [(0, 0), (0, 2), (1, 1)] {
            let mut moved = slots;
            moved[slot][word] ^= 1;
            assert_ne!(print(&moved), before, "slot {slot} word {word}");
        }
        assert_ne!(print(&slots[..1]), before, "a slot fewer");
    }

    /// A sub-inventory `link`'s `Inventory` of `SLOTS` stacks of 12 bytes,
    /// item, count and entity, the one at `index` holding `item`.
    fn stacks_of(item: u32, index: usize) -> Vec<u8> {
        let mut bytes = vec![0u8; 12 * usize::from(SLOTS)];
        bytes[12 * index..12 * index + 4].copy_from_slice(&item.to_ne_bytes());
        bytes
    }

    #[test]
    fn a_watch_counts_a_change_of_its_own_inventory_only_and_reads_a_bare_link() {
        let layout = TEST_SLOTS;
        // The player 7 links its sub-inventory 70; 80 is a chest's, read
        // alone; 90 is another's, unwatched.
        let mut setup = [0u8; 6 * LINKS];
        setup[..4].copy_from_slice(&70u32.to_ne_bytes());
        setup[5 * LINKS] = 1;
        let mut inventories = [stacks_of(1, 0), stacks_of(2, 0), stacks_of(3, 0)];
        let step = |inventories: &[Vec<u8>; 3], at: u64| {
            let component = |entity: u32, hash: u32| -> Option<*const u8> {
                match (entity, hash) {
                    (7, 1) => Some(setup.as_ptr()),
                    (70, 2) => Some(inventories[0].as_ptr()),
                    (80, 2) => Some(inventories[1].as_ptr()),
                    (90, 2) => Some(inventories[2].as_ptr()),
                    _ => None,
                }
            };
            read_watch(&component, &layout, 7, at);
            STATE.watched_changes.load(Ordering::Relaxed)
        };
        watch([PLAYER, 80], 1000);
        let base = step(&inventories, 1000);
        assert!(watch_fresh(1100), "the baseline is read");
        assert_eq!(step(&inventories, 1100), base, "nothing moved");
        inventories[2] = stacks_of(9, 3);
        assert_eq!(step(&inventories, 1200), base, "an unwatched inventory");
        inventories[1] = stacks_of(2, 5);
        assert_eq!(step(&inventories, 1300), base + 1, "the chest's slot");
        inventories[0] = stacks_of(4, 0);
        assert_eq!(step(&inventories, 1400), base + 2, "the player's own");
        watch([80, 0], 1500);
        assert!(!watch_fresh(1500), "a new watch waits for its baseline");
        inventories[1] = stacks_of(6, 1);
        assert_eq!(step(&inventories, 1500), base + 2, "its baseline");
    }

    #[test]
    fn the_action_bar_flags_its_slots_on_a_build_without_the_equipment_layout() {
        let slots = SlotLayout {
            actionbar_entities: Some(0),
            ..TEST_SLOTS
        };
        let layout = Layout {
            position: None,
            look: None,
            vitals: None,
            actionbar: Some((3, 8)),
            slots: None,
        };
        let mut selection = [0u8; 16];
        selection[..4].copy_from_slice(&70u32.to_ne_bytes());
        let mut setup = [0u8; 6 * LINKS];
        setup[..4].copy_from_slice(&70u32.to_ne_bytes());
        setup[5 * LINKS] = 1;
        let stacks = stacks_of(5, 0);
        let component = |entity: u32, hash: u32| -> Option<*const u8> {
            match (entity, hash) {
                (7, 1) => Some(setup.as_ptr()),
                (70, 2) => Some(stacks.as_ptr()),
                _ => None,
            }
        };
        STATE.slots_asked.store(1000, Ordering::Relaxed);
        read_selected(
            &component,
            &layout,
            &slots,
            7,
            Some(selection.as_ptr()),
            1000,
        );
        let words = STATE.slots.load().unwrap();
        let first = unpack_slot(words[2..5].try_into().unwrap());
        assert_eq!(
            first.flags & ClientSlot::ACTIONBAR,
            ClientSlot::ACTIONBAR,
            "the action bar's inventory 70 is flagged"
        );
    }

    /// A time past the last read's freshness.
    fn local_held_fresh_past() -> u64 {
        crate::actions::local_held::FRESH_MS + 1
    }

    #[test]
    fn the_slots_read_only_for_a_few_seconds_after_an_ask() {
        assert!(!slots_wanted(10_000), "never asked");
        STATE.slots_asked.store(10_000, Ordering::Relaxed);
        assert!(slots_wanted(10_000 + SLOTS_ASK_MS));
        assert!(!slots_wanted(10_001 + SLOTS_ASK_MS));
    }

    #[test]
    fn a_part_counts_only_when_its_bit_is_set() {
        let parts = Parts::LOOK.with(Parts::ACTIONBAR);
        assert!(parts.contains(Parts::LOOK));
        assert!(!parts.contains(Parts::VITALS));
        assert!(!Parts::default().contains(Parts::POSITION));
    }
}
