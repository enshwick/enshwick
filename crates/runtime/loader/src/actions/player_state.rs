//! The local player's own state on the client: where it stands and where
//! its camera looks, its health and stamina, its action bar's selected
//! slot and the slots of its inventory, read beside the held controls in
//! `player_ui`'s entry, on the row that read found, and kept for any thread
//! to read.
//!
//! Every value is the client's replicated copy, the one its interface
//! draws: `player_ui` copies the same `NetworkPlayerAttributes` fields into
//! the HUD (client 26430625), so a value may trail the server by a step.

use std::sync::atomic::{AtomicU64, Ordering};

use enshwick_core::__private::{INVENTORY_LINKS as LINKS, INVENTORY_SLOTS as SLOTS};
use enshwick_core::Entity;
use enshwick_core::abi::ClientSlot;
use enshwick_gamedata::catalogue;

use crate::engine::copied::Copied;
use crate::engine::ecs::Components;
use crate::engine::fuse::Fuse;
use crate::engine::loader::Loader;

/// The feature's name in the log and on the Mods screen.
pub(crate) const FEATURE: &str = "the local player's position, look, vitals and inventory";

/// The switch of these reads alone, so a fault leaves the held controls
/// they run beside read.
pub(crate) static FUSE: Fuse = Fuse::new(FEATURE);

/// The most slots a read keeps: every link of the player's inventory and
/// of one level of virtual links below it.
pub(crate) const MAX_SLOTS: usize = 2 * LINKS * SLOTS as usize;
/// How deep the slot walk follows virtual links, as the server's reads do.
const MAX_DEPTH: u32 = 1;
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
        slot.slot & 0xff_ffff_ffff | u64::from(slot.category) << 40 | u64::from(slot.flags) << 48,
        u64::from(slot.item) | u64::from(slot.count) << 32,
        u64::from(slot.pide.0),
    ]
}

/// The slot `pack_slot` packed into `words`.
fn unpack_slot(words: [u64; 3]) -> ClientSlot {
    let [at, stack, pide] = words;
    let byte = |shift: u32| u8::try_from(at >> shift & 0xff).unwrap_or(0);
    let low = |word: u64| u32::try_from(word & 0xffff_ffff).unwrap_or(0);
    ClientSlot {
        slot: at & 0xff_ffff_ffff,
        item: low(stack),
        count: low(stack >> 32),
        pide: Entity(low(pide)),
        category: byte(40),
        flags: byte(48),
        ..ClientSlot::EMPTY
    }
}

/// The words of the slot copy: the count, the read's time, then three per
/// slot.
const SLOT_WORDS: usize = 2 + 3 * MAX_SLOTS;

/// What the steps keep, written only on `player_ui`'s thread.
struct State {
    /// The last read of the player's parts.
    parts: Copied<8>,
    /// The last read of the inventory's slots.
    slots: Copied<SLOT_WORDS>,
    /// When a mod last asked for the slots, as `now_ms` counts; 0 never.
    slots_asked: AtomicU64,
    /// How many reads found the slots changed from the read before.
    slot_changes: AtomicU64,
}

static STATE: State = State {
    parts: Copied::new(),
    slots: Copied::new(),
    slots_asked: AtomicU64::new(0),
    slot_changes: AtomicU64::new(0),
};

/// How many step reads found the inventory's slots changed, so a command
/// that changes them sees its effect as a later count; asks the steps to
/// read them at `now`.
pub(crate) fn slot_changes(now: u64) -> u64 {
    STATE.slots_asked.store(now, Ordering::Relaxed);
    STATE.slot_changes.load(Ordering::Relaxed)
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
    links: usize,
    categories: usize,
    initialized: usize,
    stack: usize,
    stack_count: usize,
    stack_pide: usize,
    /// `SlotSelection`'s action bar entities, two `u32`.
    actionbar_entities: usize,
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
    /// The parts `loader`'s build reads, each from its reflected names; a
    /// part the build lacks logs its id once and stays unread.
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
            Some(SlotLayout {
                setup: hash(catalogue::ids::INVENTORY_SETUP)?,
                inventory: hash(catalogue::ids::INVENTORY)?,
                links: at(catalogue::ids::INVENTORY_SETUP_LINKS)?,
                categories: at(catalogue::ids::INVENTORY_SETUP_CATEGORIES)?,
                initialized: at(catalogue::ids::INVENTORY_SETUP_INITIALIZED)?,
                stack: crate::engine::catalogue::size(loader, catalogue::ids::ITEM_STACK)?,
                stack_count: at(catalogue::ids::ITEM_STACK_COUNT)?,
                stack_pide: at(catalogue::ids::ITEM_STACK_PIDE)?,
                actionbar_entities: at(catalogue::ids::SLOT_SELECTION_ACTIONBAR_ENTITIES)?,
            })
        };
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
            slots: slots(),
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
            (layout.slots.is_none(), catalogue::ids::INVENTORY),
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
    if let Some(slots) = &layout.slots
        && slots_wanted(at)
    {
        let bar = selection.map_or([0; 2], |data| {
            // SAFETY: `data` is the player's live `SlotSelection`, whose two
            // action bar entity ids sit at the reflected offset.
            unsafe { enshwick_core::raw::read_at::<[u32; 2]>(data, slots.actionbar_entities) }
        });
        let mut words = [0u64; SLOT_WORDS];
        let mut count = 0;
        walk(&component, slots, player, 0, &mut |slot| {
            let inventory = u32::try_from(slot.slot & 0xffff_ffff).unwrap_or(0);
            let slot = ClientSlot {
                flags: if bar.contains(&inventory) {
                    ClientSlot::ACTIONBAR
                } else {
                    0
                },
                ..slot
            };
            if count < MAX_SLOTS {
                words[2 + 3 * count..5 + 3 * count].copy_from_slice(&pack_slot(&slot));
                count += 1;
            }
        });
        words[0] = u64::try_from(count).unwrap_or(0);
        let changed = STATE.slots.load().is_none_or(|last| {
            let used = 2 + 3 * count;
            last[0] != words[0] || last[2..used] != words[2..used]
        });
        if changed {
            STATE.slot_changes.fetch_add(1, Ordering::Relaxed);
        }
        words[1] = at;
        STATE.slots.write(words);
    }
}

/// Hands `out` every slot of the inventory `root` holds, its virtual links
/// followed to [`MAX_DEPTH`], in the game's order, as the server's reads
/// walk one.
fn walk(
    component: &dyn Fn(u32, u32) -> Option<*const u8>,
    at: &SlotLayout,
    root: u32,
    depth: u32,
    out: &mut dyn FnMut(ClientSlot),
) {
    use enshwick_core::raw::read_at;
    const VIRTUAL: u8 = 5;
    let Some(setup) = component(root, at.setup) else {
        return;
    };
    // SAFETY: `setup` is the entity's live `InventorySetup`, whose
    // `isInitialized` byte sits at the reflected offset.
    if unsafe { read_at::<u8>(setup, at.initialized) } == 0 {
        return;
    }
    for i in 0..LINKS {
        // SAFETY: the live `InventorySetup`, whose `LINKS` u32
        // `linksEntities` start at the reflected offset, and `i < LINKS`.
        let link: u32 = unsafe { read_at(setup, at.links + i * size_of::<u32>()) };
        if link == 0 {
            break;
        }
        // SAFETY: the live `InventorySetup`, whose `LINKS` category bytes
        // start at the reflected offset, and `i < LINKS`.
        let category: u8 = unsafe { read_at(setup, at.categories + i) };
        if category == VIRTUAL {
            if depth < MAX_DEPTH {
                walk(component, at, link, depth + 1, out);
            }
            continue;
        }
        let Some(stacks) = component(link, at.inventory) else {
            continue;
        };
        for index in 0..SLOTS {
            let base = usize::from(index) * at.stack;
            let field = |offset: usize| {
                // SAFETY: `stacks` is the link's live `Inventory` of `SLOTS`
                // reflected `ItemStack`s, `index < SLOTS`; each stack holds
                // its u32 item id first, then its u32 count and u32 item
                // entity at the reflected offsets this is called with.
                unsafe { read_at::<u32>(stacks, base + offset) }
            };
            let (item, count, pide) = (field(0), field(at.stack_count), field(at.stack_pide));
            out(ClientSlot {
                slot: u64::from(link) | u64::from(index) << 32,
                item,
                count,
                pide: Entity(pide),
                category,
                ..ClientSlot::EMPTY
            });
        }
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
        let slot = ClientSlot {
            slot: 0x41 | 7 << 32,
            item: 0xdead_beef,
            count: 65_535,
            pide: Entity(9000),
            category: 4,
            flags: ClientSlot::ACTIONBAR,
            ..ClientSlot::EMPTY
        };
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
