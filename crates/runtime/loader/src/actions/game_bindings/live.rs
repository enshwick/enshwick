//! The bindings in the client's memory, read and written in the interface's
//! frame on its thread, which is where the game itself applies a change of
//! the player's; mods read what the frame last saw, from any thread.

use core::cell::RefCell;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use enshwick_base::Relock;
use enshwick_core::abi::Side;
use enshwick_core::controls::Key;
use enshwick_gamedata::catalogue;

use super::GameActions;
use super::keys::{self, Operation};
use super::written::{Combo, Firsts, Written};
use crate::engine::fuse::Fuse;
use crate::engine::loader::Loader;
use crate::engine::memory;
use crate::engine::pinned;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::ui::CONTEXT_STATE.id(),
    pinned::input::BINDINGS.id(),
    pinned::input::BINDING_SIZE.id(),
    pinned::input::BINDING_FIRST.id(),
    pinned::input::BINDING_SECOND.id(),
    pinned::input::BINDING_MODIFIERS.id(),
    pinned::input::BINDING_MASK.id(),
    pinned::input::MODIFIER_SIZE.id(),
    pinned::input::MODIFIER_ITEMS.id(),
    pinned::input::MODIFIER_COUNT.id(),
];

/// The feature's name in the log and on the Mods screen.
const FEATURE: &str = "the game's key bindings";
static FUSE: Fuse = Fuse::new(FEATURE);

/// Interface frames between two reads: a quarter second at 60 frames a
/// second. A rebind asks for the next frame.
const EVERY: u32 = 16;
/// The modifiers a mask's bits can name.
const MAX_MODIFIERS: usize = 32;
/// The actions a binding's byte numbers.
const MAX_ACTIONS: usize = 256;
/// An operation's bytes, at the start of each modifier list item.
const OPERATION: usize = 12;

/// Where the bindings lie on this build.
#[derive(Debug)]
struct Layout {
    /// The interface's state in its context, and the table in the state.
    context_state: usize,
    bindings: usize,
    /// One action's entry, its keyboard and mouse combinations in it, and a
    /// combination's modifier mask in the combination.
    size: usize,
    first: usize,
    second: usize,
    mask: usize,
    /// The keyboard and mouse modifier list in the table, the game's
    /// dynamic array: where it keeps its items' pointer and its count, and
    /// one item's size.
    list: usize,
    list_items: usize,
    list_count: usize,
    modifier_size: usize,
    actions: GameActions,
}

impl Layout {
    /// Whether each combination lies in its entry and the entries lie
    /// before the modifier list, as on every kept client; a build where a
    /// route found something else has the bindings off.
    fn holds(&self) -> bool {
        let combo = |at: usize| {
            at.checked_add(OPERATION)
                .is_some_and(|end| end <= self.size)
                && at
                    .checked_add(self.mask)
                    .and_then(|m| m.checked_add(4))
                    .is_some_and(|end| end <= self.size)
        };
        combo(self.first)
            && combo(self.second)
            && self.modifier_size >= OPERATION
            && self.actions.count() <= MAX_ACTIONS
            && self
                .actions
                .count()
                .checked_mul(self.size)
                .is_some_and(|end| end <= self.list)
    }
}

/// The bindings' state: the layout once linked, what the interface last
/// read, and the rebinds mods asked for.
struct State {
    layout: OnceLock<Layout>,
    /// Each action's first and second key in the key form, as last read.
    keys: [[AtomicU32; 2]; MAX_ACTIONS],
    /// Whether a frame read them yet.
    ready: AtomicBool,
    /// The modifier bits the binding's modifier list holds, as last read.
    listed: AtomicU32,
    /// The rebinds asked for, an action's number and its first key; the
    /// last one of an action wins.
    wanted: Mutex<Vec<(u8, u32)>>,
    /// How often `wanted` changed, so a frame copies it only then.
    wanted_at: AtomicU64,
    /// Whether the next frame reads and writes, without waiting its turn.
    due: AtomicBool,
}

static STATE: State = State {
    layout: OnceLock::new(),
    keys: [const { [const { AtomicU32::new(0) }; 2] }; MAX_ACTIONS],
    ready: AtomicBool::new(false),
    listed: AtomicU32::new(0),
    wanted: Mutex::new(Vec::new()),
    wanted_at: AtomicU64::new(0),
    due: AtomicBool::new(false),
};

/// What the interface thread keeps between frames.
#[derive(Default)]
struct Frame {
    /// Frames since the last read.
    count: u32,
    /// The [`State::wanted_at`] its copy of the rebinds belongs to.
    wanted_at: u64,
    wanted: Vec<(u8, u32)>,
    written: Written,
}

thread_local! {
    static FRAME: RefCell<Frame> = RefCell::new(Frame::default());
}

fn layout(loader: &Loader) -> Result<Layout, &'static str> {
    let at = |id: pinned::Constant| loader.offset(id).ok_or(id.id());
    let ty = catalogue::type_name(catalogue::ids::GAMEPLAY_ACTION);
    let info = loader
        .reflection
        .type_info(ty)
        .map_err(|_| catalogue::ids::GAMEPLAY_ACTION)?;
    let layout = Layout {
        context_state: at(pinned::ui::CONTEXT_STATE)?,
        bindings: at(pinned::input::BINDINGS)?,
        size: at(pinned::input::BINDING_SIZE)?,
        first: at(pinned::input::BINDING_FIRST)?,
        second: at(pinned::input::BINDING_SECOND)?,
        mask: at(pinned::input::BINDING_MASK)?,
        list: at(pinned::input::BINDING_MODIFIERS)?,
        list_items: at(pinned::input::MODIFIER_ITEMS)?,
        list_count: at(pinned::input::MODIFIER_COUNT)?,
        modifier_size: at(pinned::input::MODIFIER_SIZE)?,
        actions: GameActions::new(info.enum_values().iter().map(|v| (v.name(), v.value()))),
    };
    if layout.holds() {
        Ok(layout)
    } else {
        Err(pinned::input::BINDING_MODIFIERS.id())
    }
}

/// Finds the bindings on a client; a build without them has the feature
/// off, with the entry that lacks.
pub(crate) fn link(loader: &Loader) {
    if loader.side != Side::CLIENT {
        return;
    }
    match layout(loader) {
        Ok(layout) => {
            let _first = STATE.layout.set(layout);
        }
        Err(id) => {
            let _off = loader.fuse_off(&FUSE, id, "this build lacks it or it does not fit");
        }
    }
}

/// Reads the bindings every [`EVERY`] frames, and at once after a rebind,
/// writing the rebinds first, in the interface frame of the live context
/// `context` on the interface thread.
pub(crate) fn frame(context: usize) {
    let Some(layout) = STATE.layout.get() else {
        return;
    };
    if FUSE.is_off() {
        return;
    }
    let turn = FRAME.with_borrow_mut(|f| {
        f.count = f.count.wrapping_add(1);
        f.count.is_multiple_of(EVERY)
    });
    let due = STATE.due.load(Ordering::Relaxed) && STATE.due.swap(false, Ordering::Acquire);
    if !turn && !due {
        return;
    }
    let _ran = FUSE.run("the interface frame", || {
        // SAFETY: the one caller passes the frame's live, non-zero
        // interface context, on the interface thread.
        unsafe { look(context, layout) };
    });
}

/// One look at the bindings: the rebinds written, then every action's keys
/// read.
///
/// # Safety
///
/// - `context` is the live interface context of the frame being drawn, on
///   the interface thread.
unsafe fn look(context: usize, layout: &Layout) {
    // SAFETY: the caller's live context, whose state pointer sits at
    // `ui.context_state`, as the game's key glyph reads it (client 26430625).
    let state = unsafe { memory::read::<usize>(context, layout.context_state) };
    if state == 0 {
        return;
    }
    // SAFETY: the non-null interface state, which holds the table's pointer
    // at `input.bindings` (the glyph and the HUD's binding reads).
    let table = unsafe { memory::read::<usize>(state, layout.bindings) };
    if table == 0 {
        return;
    }
    let mut list = [[0u8; OPERATION]; MAX_MODIFIERS];
    // SAFETY: the live, non-null table, read on the interface thread.
    let listed = unsafe { modifiers(table, layout, &mut list) };
    let modifiers = list.get(..listed).unwrap_or_default();
    let mut slots = Slots { table, layout };
    FRAME.with_borrow_mut(|f| {
        let at = STATE.wanted_at.load(Ordering::Acquire);
        if at != f.wanted_at {
            f.wanted_at = at;
            f.wanted.clone_from(&STATE.wanted.relock());
        }
        if f.wanted.is_empty() && !f.written.any() {
            return;
        }
        let refused = f.written.apply(&f.wanted, modifiers, &mut slots);
        if !refused.is_empty() {
            drop_refused(&refused, &layout.actions);
            f.wanted
                .retain(|&(a, k)| !refused.iter().any(|&(r, b, _)| (r, b) == (a, k)));
        }
    });
    for (action, keys) in STATE.keys.iter().enumerate().take(layout.actions.count()) {
        for (slot, at) in keys.iter().zip([layout.first, layout.second]) {
            let (op, mask) = slots.combo(action, at);
            slot.store(keys::key_bits(&op, mask, modifiers), Ordering::Relaxed);
        }
    }
    STATE
        .listed
        .store(keys::listed(modifiers), Ordering::Relaxed);
    STATE.ready.store(true, Ordering::Release);
}

/// Copies the operations of the table's modifier list into `out`, at most
/// [`MAX_MODIFIERS`]; how many.
///
/// # Safety
///
/// - `table` is the live bindings table, read on the interface thread.
unsafe fn modifiers(table: usize, layout: &Layout, out: &mut [Operation; MAX_MODIFIERS]) -> usize {
    let list = table + layout.list;
    // SAFETY: the live table's modifier list is the game's dynamic array at
    // `input.binding_modifiers`, its items' pointer first (`0xeabd80`,
    // client 26430625), read on the interface thread.
    let items = unsafe { memory::read::<usize>(list, layout.list_items) };
    // SAFETY: the same array's count follows its pointer (`0xeabd80`,
    // client 26430625), read on the interface thread.
    let count = unsafe { memory::read::<usize>(list, layout.list_count) };
    if items == 0 {
        return 0;
    }
    let count = count.min(MAX_MODIFIERS);
    for (at, op) in out.iter_mut().enumerate().take(count) {
        // SAFETY: item `at` lies below the array's own count, each
        // `input.modifier_size` bytes, at least an operation (`holds`).
        unsafe { memory::read_bytes(items, at * layout.modifier_size, op) };
    }
    count
}

#[cold]
fn drop_refused(refused: &[super::written::Refused], actions: &GameActions) {
    let mut wanted = STATE.wanted.relock();
    for &(action, bits, why) in refused {
        let name = actions.name(action).unwrap_or("?");
        let key = Key::from_bits(bits).map_or_else(|| format!("{bits:#x}"), |k| k.to_string());
        crate::log!(
            WARN,
            "{FEATURE}: the rebind of {name} to {key} is dropped: {why}"
        );
        wanted.retain(|&(a, k)| (a, k) != (action, bits));
    }
}

/// The table's first combinations, written and read in the frame.
struct Slots<'a> {
    table: usize,
    layout: &'a Layout,
}

impl Slots<'_> {
    /// The combination at `at` in `action`'s entry.
    fn combo(&self, action: usize, at: usize) -> Combo {
        let entry = self.table + action * self.layout.size + at;
        let mut op = [0u8; OPERATION];
        // SAFETY: `action` is below the build's count, whose entries lie
        // before the modifier list in the live table, and the combination's
        // operation lies in the entry (`holds`); read on the interface
        // thread.
        unsafe { memory::read_bytes(entry, 0, &mut op) };
        // SAFETY: the combination's mask lies in the same entry of the live
        // table (`holds`); read on the interface thread.
        let mask = unsafe { memory::read::<u32>(entry, self.layout.mask) };
        (op, mask)
    }
}

impl Firsts for Slots<'_> {
    fn read(&self, action: u8) -> Combo {
        self.combo(usize::from(action), self.layout.first)
    }

    fn write(&mut self, action: u8, (op, mask): Combo) {
        let entry = self.table + usize::from(action) * self.layout.size + self.layout.first;
        // SAFETY: `action` is below the build's count, so its first
        // combination's operation lies in the live table's heap memory
        // (`holds`); written on the interface thread, where the game applies
        // its own changes, so no game write races it. A direct write acts on
        // input and glyphs at once (client 26430625, seen live 2026-11-23).
        unsafe { memory::write_bytes(entry, 0, &op) };
        // SAFETY: the combination's mask lies in the same entry (`holds`),
        // written on the interface thread as the operation before it.
        unsafe { memory::write::<u32>(entry, self.layout.mask, mask) };
    }
}

/// Why a mod's read or rebind of the game's bindings fails.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum BindingError {
    /// Not on a client, before the interface's first read, or on a build
    /// without the bindings.
    #[error("the game's key bindings do not read here")]
    Unavailable,
    /// The build names no such action.
    #[error("this build has no game action {0}")]
    NoAction(String),
    /// The key is not one a rebind takes.
    #[error("{0}")]
    BadKey(String),
}

/// The layout and the action numbered for `action`, while the bindings run.
fn found(action: &str) -> Result<u8, BindingError> {
    let layout = STATE
        .layout
        .get()
        .filter(|_| !FUSE.is_off())
        .ok_or(BindingError::Unavailable)?;
    layout
        .actions
        .number(action)
        .ok_or_else(|| BindingError::NoAction(action.to_owned()))
}

/// The first and second key of `action`, its reflected name, as the
/// interface last read them.
///
/// # Errors
///
/// [`BindingError`] while the bindings do not read, or for an action the
/// build does not name.
pub(crate) fn binding(action: &str) -> Result<[u32; 2], BindingError> {
    let number = found(action)?;
    if !STATE.ready.load(Ordering::Acquire) {
        return Err(BindingError::Unavailable);
    }
    let keys = STATE
        .keys
        .get(usize::from(number))
        .ok_or(BindingError::Unavailable)?;
    Ok(keys.each_ref().map(|key| key.load(Ordering::Relaxed)))
}

/// Rebinds `action`'s first key to `key`, or with 0 gives it back, from the
/// next interface frame on.
///
/// # Errors
///
/// [`BindingError`] while the bindings do not read, for an action the build
/// does not name, a key a mod control may not take, or a modifier the
/// game's list lacks.
pub(crate) fn rebind(action: &str, key: u32) -> Result<(), BindingError> {
    let number = found(action)?;
    if key != 0 {
        let parsed = Key::from_bits(key).ok_or_else(|| {
            BindingError::BadKey(format!("{key:#x} is no key a control may take"))
        })?;
        let listed = STATE.listed.load(Ordering::Relaxed);
        if STATE.ready.load(Ordering::Acquire) && parsed.modifiers() & !listed != 0 {
            return Err(BindingError::BadKey(format!(
                "the game's bindings list no modifier of {parsed}"
            )));
        }
    }
    let mut wanted = STATE.wanted.relock();
    wanted.retain(|&(a, _)| a != number);
    if key != 0 {
        wanted.push((number, key));
    }
    STATE.wanted_at.fetch_add(1, Ordering::Release);
    drop(wanted);
    STATE.due.store(true, Ordering::Release);
    Ok(())
}

/// The game actions one of whose keys is `key`, the same key with the same
/// modifiers, as the interface last read them; for the clashes the mods'
/// key bindings show.
pub(crate) fn sharing(key: u32) -> Vec<&'static str> {
    let Some(layout) = STATE.layout.get() else {
        return Vec::new();
    };
    if key == 0 || FUSE.is_off() || !STATE.ready.load(Ordering::Acquire) {
        return Vec::new();
    }
    STATE
        .keys
        .iter()
        .take(layout.actions.count())
        .enumerate()
        .filter(|(_, keys)| keys.iter().any(|k| k.load(Ordering::Relaxed) == key))
        .filter_map(|(action, _)| layout.actions.name(u8::try_from(action).ok()?))
        .collect()
}
