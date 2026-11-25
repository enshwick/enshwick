//! The bindings in the client's memory, read and written in the interface's
//! frame on its thread; mods read what the frame last saw, from any thread.

use core::cell::RefCell;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use enshwick_base::Relock;
use enshwick_core::abi::Side;
use enshwick_core::controls::Key;
use enshwick_gamedata::catalogue;

use super::keys::{self, Operation};
use super::rebinds::Rebinds;
use super::written::{Combo, Firsts, Written};
use super::{ActionNumber, GameActions};
use crate::engine::fuse::Fuse;
use crate::engine::loader::Loader;
use crate::engine::memory;
use crate::engine::mod_index::ModIndex;
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
    /// Whether a frame read them yet; set after `keys` and `listed`.
    ready: AtomicBool,
    /// The modifier bits the binding's modifier list holds, as last read.
    listed: AtomicU32,
    asked: Mutex<Rebinds>,
    /// How often `asked` changed, so a frame copies it only then.
    asked_at: AtomicU64,
    /// Whether the next frame reads and writes, without waiting its turn.
    due: AtomicBool,
}

static STATE: State = State::new();

/// What the interface thread keeps between frames.
#[derive(Default)]
struct Frame {
    /// Frames since the start.
    count: u32,
    /// The [`State::asked_at`] its copy of the rebinds belongs to.
    asked_at: u64,
    asked: Rebinds,
    /// What this look writes: of each action, the newest rebind of a mod
    /// that runs.
    wanted: Vec<(ActionNumber, u32)>,
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

/// Whether the frame numbered `count` reads the bindings: every frame until
/// the first read, then every [`EVERY`] frames.
const fn reads_now(count: u32, ready: bool) -> bool {
    !ready || count.is_multiple_of(EVERY)
}

/// Reads the bindings at the first frame, every [`EVERY`] frames after and
/// at once after a rebind, writing the rebinds first, in the interface
/// frame of the live context `context` on the interface thread. Once the
/// feature is off after a fault, it gives the game back every slot a rebind
/// holds instead.
pub(crate) fn frame(context: usize) {
    let Some(layout) = STATE.layout.get() else {
        return;
    };
    FRAME.with_borrow_mut(|f| {
        if FUSE.is_off() {
            // SAFETY: the one caller passes the frame's live, non-zero
            // interface context, on the interface thread.
            unsafe { give_back(f, context, layout) };
            return;
        }
        f.count = f.count.wrapping_add(1);
        let turn = reads_now(f.count, STATE.ready.load(Ordering::Relaxed));
        let due = STATE.due.load(Ordering::Relaxed) && STATE.due.swap(false, Ordering::Acquire);
        if !turn && !due {
            return;
        }
        let _ran = FUSE.run("the interface frame", || {
            let runs = |owner: ModIndex| !crate::engine::loader::is_off(owner);
            // SAFETY: the one caller passes the frame's live, non-zero
            // interface context, on the interface thread.
            unsafe { look(&STATE, f, context, layout, runs) };
        });
    });
}

/// The live bindings table of the interface context `context`; `None`
/// while the interface has no state or no table yet.
///
/// # Safety
///
/// - `context` is the live interface context of the frame being drawn, on
///   the interface thread.
unsafe fn live_table(context: usize, layout: &Layout) -> Option<usize> {
    // SAFETY: the caller's live context, whose state pointer sits at
    // `ui.context_state`, as the game's key glyph reads it (client 26430625).
    let state = unsafe { memory::read::<usize>(context, layout.context_state) };
    if state == 0 {
        return None;
    }
    // SAFETY: the non-null interface state, which holds the table's pointer
    // at `input.bindings` (the glyph and the HUD's binding reads).
    let table = unsafe { memory::read::<usize>(state, layout.bindings) };
    (table != 0).then_some(table)
}

/// One look at the bindings: the rebinds of the mods that `runs` written,
/// then every action's keys read into `state`.
///
/// # Safety
///
/// - `context` is the live interface context of the frame being drawn, on
///   the interface thread.
unsafe fn look(
    state: &State,
    f: &mut Frame,
    context: usize,
    layout: &Layout,
    runs: impl Fn(ModIndex) -> bool,
) {
    // SAFETY: the caller's live context on the interface thread.
    let Some(table) = (unsafe { live_table(context, layout) }) else {
        return;
    };
    let mut list = [[0u8; OPERATION]; MAX_MODIFIERS];
    // SAFETY: the live, non-null table, read on the interface thread.
    let listed = unsafe { modifiers(table, layout, &mut list) };
    let modifiers = list.get(..listed).unwrap_or_default();
    let mut slots = Slots { table, layout };
    let at = state.asked_at.load(Ordering::Acquire);
    if at != f.asked_at {
        f.asked_at = at;
        f.asked.clone_from(&state.asked.relock());
    }
    f.asked.wanted(runs, &mut f.wanted);
    if !f.wanted.is_empty() || f.written.any() {
        let refused = f.written.apply(&f.wanted, modifiers, &mut slots);
        if !refused.is_empty() {
            drop_refused(state, &refused, &layout.actions);
        }
    }
    for (action, keys) in state.keys.iter().enumerate().take(layout.actions.count()) {
        for (slot, at) in keys.iter().zip([layout.first, layout.second]) {
            let (op, mask) = slots.combo(action, at);
            slot.store(keys::key_bits(&op, mask, modifiers), Ordering::Relaxed);
        }
    }
    state
        .listed
        .store(keys::listed(modifiers), Ordering::Relaxed);
    state.ready.store(true, Ordering::Release);
}

/// Gives the game back every slot a rebind holds, once the feature is off
/// after a fault, so no mod's key outlives it; a fault while it does is
/// swallowed, the feature being off already.
///
/// # Safety
///
/// - `context` is the live interface context of the frame being drawn, on
///   the interface thread.
#[cold]
unsafe fn give_back(f: &mut Frame, context: usize, layout: &Layout) {
    if !f.written.any() {
        return;
    }
    let mut written = core::mem::take(&mut f.written);
    let _given = std::panic::catch_unwind(core::panic::AssertUnwindSafe(|| {
        // SAFETY: the caller's live context on the interface thread.
        if let Some(table) = unsafe { live_table(context, layout) } {
            let _none = written.apply(&[], &[], &mut Slots { table, layout });
        }
    }));
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

/// Ends the rebinds the game's bindings refused, with a warning each; the
/// frames copy the rebinds again.
#[cold]
fn drop_refused(state: &State, refused: &[super::written::Refused], actions: &GameActions) {
    let mut asked = state.asked.relock();
    for &(action, bits, why) in refused {
        let name = actions.name(action).unwrap_or("?");
        let key = Key::from_bits(bits).map_or_else(|| format!("{bits:#x}"), |k| k.to_string());
        crate::log!(
            WARN,
            "{FEATURE}: the rebind of {name} to {key} is dropped: {why}"
        );
        asked.drop_key(action, bits);
    }
    state.asked_at.fetch_add(1, Ordering::Release);
}

/// The table's first combinations, written and read in the frame.
struct Slots<'a> {
    table: usize,
    layout: &'a Layout,
}

impl Slots<'_> {
    /// The combination at `at` in the entry of the action at `index`.
    fn combo(&self, index: usize, at: usize) -> Combo {
        let entry = self.table + index * self.layout.size + at;
        let mut op = [0u8; OPERATION];
        // SAFETY: `index` is below the build's count, whose entries lie
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
    fn read(&self, action: ActionNumber) -> Combo {
        self.combo(action.index(), self.layout.first)
    }

    fn write(&mut self, action: ActionNumber, (op, mask): Combo) {
        let entry = self.table + action.index() * self.layout.size + self.layout.first;
        // SAFETY: `action` is below the build's count, so its first
        // combination's operation lies in the live table's heap memory
        // (`holds`), which no Rust code reads but the frame's own look on
        // this thread. The game's apply rewrites the table in place in the
        // same unsynchronised way (client 26430625, seen live 2026-11-23);
        // which threads the game's input reads it on is not settled, so such
        // a reader may see this combination half written for one frame. A
        // direct write acts on input and glyphs at once (seen live).
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

impl State {
    const fn new() -> Self {
        Self {
            layout: OnceLock::new(),
            keys: [const { [const { AtomicU32::new(0) }; 2] }; MAX_ACTIONS],
            ready: AtomicBool::new(false),
            listed: AtomicU32::new(0),
            asked: Mutex::new(Rebinds::new()),
            asked_at: AtomicU64::new(0),
            due: AtomicBool::new(false),
        }
    }

    /// The layout while the bindings run, once a frame read them.
    fn running(&self) -> Result<&Layout, BindingError> {
        let layout = self
            .layout
            .get()
            .filter(|_| !FUSE.is_off())
            .ok_or(BindingError::Unavailable)?;
        // Acquire pairs with the look's Release after it stored the keys
        // and the modifier list, so a reader that sees the read sees them.
        if self.ready.load(Ordering::Acquire) {
            Ok(layout)
        } else {
            Err(BindingError::Unavailable)
        }
    }

    /// The number of `action`, a reflected name, once the bindings read.
    fn found(&self, action: &str) -> Result<ActionNumber, BindingError> {
        self.running()?
            .actions
            .number(action)
            .ok_or_else(|| BindingError::NoAction(action.to_owned()))
    }

    fn binding(&self, action: &str) -> Result<[u32; 2], BindingError> {
        let number = self.found(action)?;
        let keys = self
            .keys
            .get(number.index())
            .ok_or(BindingError::Unavailable)?;
        Ok(keys.each_ref().map(|key| key.load(Ordering::Relaxed)))
    }

    fn rebind(&self, owner: ModIndex, action: &str, key: u32) -> Result<(), BindingError> {
        let number = self.found(action)?;
        if key != 0 {
            let parsed = Key::from_bits(key).ok_or_else(|| {
                BindingError::BadKey(format!("{key:#x} is no key a control may take"))
            })?;
            if parsed.modifiers() & !self.listed.load(Ordering::Relaxed) != 0 {
                return Err(BindingError::BadKey(format!(
                    "the game's bindings list no modifier of {parsed}"
                )));
            }
        }
        let mut asked = self.asked.relock();
        if asked.set(owner, number, key) {
            self.asked_at.fetch_add(1, Ordering::Release);
            drop(asked);
            self.due.store(true, Ordering::Release);
        }
        Ok(())
    }

    fn sharing(&self, control: u32) -> Vec<ActionNumber> {
        let Ok(layout) = self.running() else {
            return Vec::new();
        };
        let fires = |keys: &[AtomicU32; 2]| {
            keys.iter()
                .any(|game| keys::fires_with(control, game.load(Ordering::Relaxed)))
        };
        (0..=u8::MAX)
            .zip(&self.keys)
            .take(layout.actions.count())
            .filter(|(_, keys)| fires(keys))
            .map(|(number, _)| ActionNumber::new(number))
            .collect()
    }
}

/// The first and second key of `action`, its reflected name, as the
/// interface last read them.
///
/// # Errors
///
/// [`BindingError`] while the bindings do not read, or for an action the
/// build does not name.
pub(crate) fn binding(action: &str) -> Result<[u32; 2], BindingError> {
    STATE.binding(action)
}

/// Sets `owner`'s rebind of `action`'s first key to `key`, or with 0 ends
/// it, from the next interface frame on; the newest rebind of an action of
/// a mod that runs is the one written, and a mod's rebind is not written
/// while the mod is off.
///
/// # Errors
///
/// [`BindingError`] while the bindings do not read, for an action the build
/// does not name, a key a mod control may not take, or a modifier the
/// game's list lacks.
pub(crate) fn rebind(owner: ModIndex, action: &str, key: u32) -> Result<(), BindingError> {
    STATE.rebind(owner, action, key)
}

/// The game's actions, as the interface last read them, with a key that
/// fires the mod control on `control` too, in their numbers' order; for the
/// clashes the mods' key bindings name.
pub(crate) fn sharing(control: u32) -> Vec<ActionNumber> {
    STATE.sharing(control)
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: ModIndex = ModIndex::new(0);
    const ACTIONS: usize = 2;
    const SIZE: usize = 0x40; // bound, not a game offset: the test table's own entry size
    const SHIFT: u32 = 1 << 16;

    fn key(name: &str) -> u32 {
        Key::parse(name).unwrap().bits()
    }

    fn op(usage: u8) -> Operation {
        [1, 0, usage, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    }

    /// A client's interface context, its state and a bindings table of
    /// `Jump` on Space and `ContextualAction` on E in heap memory, with
    /// `modifiers` as the table's modifier list.
    struct Game {
        context: Box<usize>,
        _state: Box<usize>,
        table: Box<[u8]>,
        _modifiers: Box<[Operation]>,
    }

    impl Game {
        fn new(modifiers: &[Operation]) -> Self {
            let modifiers: Box<[Operation]> = modifiers.into();
            let mut table = vec![0u8; ACTIONS * SIZE + 16].into_boxed_slice();
            table[..OPERATION].copy_from_slice(&op(0x2c));
            table[SIZE..SIZE + OPERATION].copy_from_slice(&op(0x08));
            let list = ACTIONS * SIZE;
            let items = if modifiers.is_empty() {
                0
            } else {
                modifiers.as_ptr() as usize
            };
            table[list..list + 8].copy_from_slice(&items.to_le_bytes());
            table[list + 8..list + 16].copy_from_slice(&modifiers.len().to_le_bytes());
            let state = Box::new(table.as_ptr() as usize);
            let context = Box::new(&raw const *state as usize);
            Self {
                context,
                _state: state,
                table,
                _modifiers: modifiers,
            }
        }

        fn context(&self) -> usize {
            &raw const *self.context as usize
        }

        fn jump(&self) -> Operation {
            self.table[..OPERATION].try_into().unwrap()
        }
    }

    fn layout() -> Layout {
        Layout {
            context_state: 0,
            bindings: 0,
            size: SIZE,
            first: 0,
            second: 0x20,
            mask: 0x10,
            list: ACTIONS * SIZE,
            list_items: 0,
            list_count: 8,
            modifier_size: OPERATION,
            actions: GameActions::new([("Jump", 0), ("ContextualAction", 1), ("Count", 2)]),
        }
    }

    fn linked() -> State {
        let state = State::new();
        state.layout.set(layout()).unwrap();
        state
    }

    fn look_at(state: &State, f: &mut Frame, game: &Game, runs: impl Fn(ModIndex) -> bool) {
        // SAFETY: the game's context points at its state, which points at
        // its table, all alive for the call.
        unsafe { look(state, f, game.context(), state.layout.get().unwrap(), runs) };
    }

    #[test]
    fn the_first_frame_reads_and_the_reads_after_it_wait_their_turn() {
        assert!(reads_now(1, false));
        assert!(!reads_now(1, true));
        assert!(reads_now(EVERY, true));
    }

    #[test]
    fn a_rebind_before_the_first_read_is_unavailable_and_then_takes_effect() {
        let (state, game, mut f) = (linked(), Game::new(&[]), Frame::default());
        assert_eq!(
            state.rebind(A, "Jump", key("J")),
            Err(BindingError::Unavailable)
        );
        assert_eq!(state.binding("Jump"), Err(BindingError::Unavailable));
        look_at(&state, &mut f, &game, |_| true);
        assert_eq!(state.binding("Jump"), Ok([key("Space"), 0]));
        assert_eq!(state.binding("ContextualAction"), Ok([key("E"), 0]));
        state.rebind(A, "Jump", key("J")).unwrap();
        look_at(&state, &mut f, &game, |_| true);
        assert_eq!(game.jump(), op(0x0d));
        assert_eq!(state.binding("Jump"), Ok([key("J"), 0]));
    }

    #[test]
    fn a_rebind_of_a_mod_that_is_off_gives_the_game_its_key_back() {
        let (state, game, mut f) = (linked(), Game::new(&[]), Frame::default());
        look_at(&state, &mut f, &game, |_| true);
        state.rebind(A, "Jump", key("J")).unwrap();
        look_at(&state, &mut f, &game, |_| true);
        assert_eq!(game.jump(), op(0x0d));
        look_at(&state, &mut f, &game, |owner| owner != A);
        assert_eq!(game.jump(), op(0x2c), "Space while the mod is off");
        assert_eq!(state.binding("Jump"), Ok([key("Space"), 0]));
    }

    #[test]
    fn the_feature_turned_off_gives_back_every_slot_a_rebind_holds() {
        let (state, game, mut f) = (linked(), Game::new(&[]), Frame::default());
        look_at(&state, &mut f, &game, |_| true);
        state.rebind(A, "Jump", key("J")).unwrap();
        look_at(&state, &mut f, &game, |_| true);
        // SAFETY: the game's context points at its state, which points at
        // its table, all alive for the call.
        unsafe { give_back(&mut f, game.context(), state.layout.get().unwrap()) };
        assert_eq!(game.jump(), op(0x2c));
        assert!(!f.written.any());
    }

    #[test]
    fn a_rebind_takes_a_modifier_the_game_lists_and_refuses_one_it_does_not() {
        let (state, game, mut f) = (linked(), Game::new(&[op(0xe1)]), Frame::default());
        look_at(&state, &mut f, &game, |_| true);
        assert_eq!(state.rebind(A, "Jump", key("Shift+J")), Ok(()));
        assert!(matches!(
            state.rebind(A, "Jump", key("Ctrl+J")),
            Err(BindingError::BadKey(_))
        ));
        assert_eq!(
            state.rebind(A, "Sneak", key("J")),
            Err(BindingError::NoAction("Sneak".to_owned()))
        );
        look_at(&state, &mut f, &game, |_| true);
        assert_eq!(state.binding("Jump"), Ok([key("J") | SHIFT, 0]));
    }

    #[test]
    fn a_mod_control_names_each_game_action_whose_key_it_fires_on() {
        let (state, game, mut f) = (linked(), Game::new(&[]), Frame::default());
        assert_eq!(state.sharing(key("E")), [], "nothing read yet");
        look_at(&state, &mut f, &game, |_| true);
        assert_eq!(state.sharing(key("E")), [ActionNumber::new(1)]);
        assert_eq!(state.sharing(key("Space")), [ActionNumber::new(0)]);
        assert_eq!(state.sharing(key("Shift+E")), []);
        assert_eq!(state.sharing(key("F")), []);
        assert_eq!(state.sharing(0), []);
    }
}
