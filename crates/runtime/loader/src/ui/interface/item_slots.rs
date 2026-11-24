//! The game's item slot in mods' windows, and the slot of the game's own
//! inventory screen under the cursor.
//!
//! Every inventory screen draws each slot with one widget function,
//! `ui.item_slot(answer, slot, drop, screen)` (`doItemSlot`): `slot` holds a
//! pointer to the stack's record (its packed slot, item, entity and count),
//! `screen` the screen's own state or null, and the answer says whether the
//! cursor is on the slot and whether the player clicked it (client 26430625,
//! decompiler). A mod's slot is the same call with no screen, so the game
//! skips its drag, drop and tooltip; the loader's detour on it notes the slot
//! of a screen the cursor is on, which the mods' frames read.

use core::cell::Cell;
use std::sync::OnceLock;

use enshwick_core::abi::{HookFlags, UiWidget};

use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, Target};
use crate::engine::pinned;
use crate::ui::widget_checks::WidgetError;

use super::engine::CLICKED;

/// Mods' item slots.
pub(super) static SLOTS_FUSE: Fuse = Fuse::new("item slots");
/// The slot of the game's screen under the cursor, in what mods' frames
/// read.
pub(super) static HOVER_FUSE: Fuse = Fuse::new("hovered inventory slot");

/// The `u64`s of room a slot's data takes on the loader's stack.
const SLOT_DATA: usize = 0x14; // bound, not a game offset: room the loader keeps; the link checks the init's routed extent (0x91 bytes on client 26430625) against it

/// The bytes of the widget's state block, which it builds in a local and
/// copies into its answer with one `movups`, the copy `ui.item_slot_state`
/// matches.
const STATE_BYTES: usize = 16;

/// The answer of the game's slot widget, its return value: the slot's
/// rectangle, then its state bytes (client 26430625, decompiler). The link
/// checks each state byte and the answer's extent against the routes.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Zeroable, bytemuck::Pod)]
pub(crate) struct Answer {
    rect: [f32; 4],
    /// Non-zero while the slot holds the screen's focus, which follows the
    /// selection with a mouse and the stick with a gamepad.
    focused: u8,
    cleared: u8,
    /// Non-zero while the mouse cursor is on the slot; the widget tests the
    /// cursor only while the gamepad does not drive the interface.
    pointer: u8,
    entered: [u8; 2],
    /// The press test's answer, [`CLICKED`] for a click.
    pressed: u8,
    press_rest: [u8; 3],
    /// The hit test's answer, [`CLICKED`] for a click; the game selects the
    /// slot on either.
    hit: u8,
    rest: [u8; 6],
}

/// A stack's record, which the slot data's first field points at and the
/// item viewer's descriptor starts with. Its slot, item and entity sit where
/// the reflected `ItemInstanceInfo` keeps them, and the link checks the
/// routed offsets and size against these fields. At 0x10, where the
/// reflection puts `stateMask`, the slot widget reads the stack's count: it
/// compares it with the item's largest stack before a split (client
/// 26430625, decompiler).
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable, bytemuck::Pod)]
struct Record {
    /// The packed slot, `inventory | index << 32`; 0 for none.
    slot: u64,
    item: u32,
    pide: u32,
    count: u32,
    rest: [u32; 13],
}

const _: () = assert!(size_of::<Answer>() == 32 && size_of::<Record>() == 72);

/// What the player points at: what the mouse cursor is on, else, while the
/// gamepad drives the interface, what holds the focus; `navigating` is read
/// only then. The one rule for a slot's answer and for a frame's notes.
fn pointed<T>(
    pointer: Option<T>,
    focused: Option<T>,
    navigating: impl FnOnce() -> bool,
) -> Option<T> {
    pointer.or_else(|| focused.filter(|_| navigating()))
}

impl Answer {
    /// Whether the player points at the slot, by [`pointed`].
    fn pointed(&self, navigating: impl FnOnce() -> bool) -> bool {
        let set = |byte: u8| (byte != 0).then_some(());
        pointed(set(self.pointer), set(self.focused), navigating).is_some()
    }
}

/// Where the interface says whether the gamepad drives it: the context's
/// input record, and the byte in it that is non-zero while it does
/// (`ui.item_slot` tests the cursor only while that byte is 0, client
/// 26430625).
#[derive(Clone, Copy)]
struct Input {
    record: usize,
    navigating: usize,
}

impl Input {
    fn read(loader: &Loader) -> Result<Self, Failed> {
        Ok(Self {
            record: offset(loader, pinned::ui::CONTEXT_INPUT)?,
            navigating: offset(loader, pinned::ui::INPUT_NAVIGATING)?,
        })
    }

    /// Whether the gamepad drives the interface of `context`; false for 0.
    ///
    /// # Safety
    ///
    /// - Called on the interface thread.
    /// - `context` is 0 or the context `Engine::context` read in this frame.
    unsafe fn navigating(self, context: usize) -> bool {
        use crate::engine::memory::read;
        // SAFETY: a non-zero context is the frame's, live on this thread by
        // this function's `# Safety`; its input record pointer sits at the
        // table's `ui.context_input`, as `ui.item_slot`'s test reads it.
        let record = unsafe { follow(context, self.record) };
        // SAFETY: a non-zero input record, the context's own, keeps its
        // navigation byte at the table's `ui.input_navigating`, read as a
        // byte, never as a bool.
        record != 0 && unsafe { read::<u8>(record, self.navigating) } != 0
    }
}

/// The game's `ui.item_slot(answer, slot, drop, screen)`.
type SlotFn = unsafe extern "system" fn(*mut Answer, *mut u8, *mut u8, *mut u8) -> *mut Answer;
/// The game's `ui.item_slot_init(slot)`, which writes a slot's defaults.
type InitFn = unsafe extern "system" fn(*mut u8) -> *mut u8;
/// The game's `item.info(item, table)`: the item's `ItemInfo`, null for an
/// item the table lacks.
type InfoFn = unsafe extern "system" fn(u32, usize) -> *const u8;

/// What the slots found of this build.
#[derive(Clone, Copy)]
struct Linked {
    draw: SlotFn,
    init: InitFn,
    info: InfoFn,
    clickable: usize,
    /// The flag that adds a slot to its window's navigation and has its look
    /// follow the focus and the cursor, as the inventory screens' slots;
    /// `None` on a build that lacks it.
    focusable: Option<usize>,
    player: usize,
    player_data: usize,
    player_items: usize,
}

/// What the slots and the hovered slot found of this build, each set once
/// when it links: mods' slots also read `input` for their hover, so a build
/// lacking only what the hover reads still draws them.
struct State {
    linked: OnceLock<Linked>,
    input: OnceLock<Input>,
}

static STATE: State = State {
    linked: OnceLock::new(),
    input: OnceLock::new(),
};

/// A slot of the game's screen under the cursor: its packed slot, item,
/// count and the item's own entity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Hovered {
    pub(crate) slot: u64,
    pub(crate) item: u32,
    pub(crate) count: u32,
    pub(crate) pide: u32,
}

impl Hovered {
    const NONE: Self = Self {
        slot: 0,
        item: 0,
        count: 0,
        pide: 0,
    };
}

thread_local! {
    /// What the game's screens noted under the mouse cursor in this frame
    /// so far.
    static NOW: Cell<Hovered> = const { Cell::new(Hovered::NONE) };
    /// What they noted holding the focus in this frame so far.
    static FOCUSED: Cell<Hovered> = const { Cell::new(Hovered::NONE) };
    /// What the player pointed at in the last whole frame, which mods'
    /// frames read.
    static SHOWN: Cell<Hovered> = const { Cell::new(Hovered::NONE) };
}

/// Ends the frame's notes: the slot the player pointed at is what the
/// frames read from now on, and the next frame notes afresh. On the
/// interface thread, with the frame's `context`, 0 for none.
pub(super) fn end_frame(context: usize) {
    let (pointer, focused) = (NOW.replace(Hovered::NONE), FOCUSED.replace(Hovered::NONE));
    // SAFETY: on the interface thread with the frame's context, by this
    // function's contract.
    SHOWN.set(pointed_slot(pointer, focused, || unsafe {
        gamepad_drives(context)
    }));
}

/// The slot the player pointed at in a frame whose screens noted `pointer`
/// under the mouse cursor and `focused` holding the focus, by [`pointed`].
/// Deciding once a frame keeps the mode off each slot's detour.
fn pointed_slot(pointer: Hovered, focused: Hovered, navigating: impl FnOnce() -> bool) -> Hovered {
    let noted = |slot: Hovered| (slot != Hovered::NONE).then_some(slot);
    pointed(noted(pointer), noted(focused), navigating).unwrap_or(Hovered::NONE)
}

/// The slot of the game's screen under the cursor in the last whole frame.
pub(super) fn hovered() -> Hovered {
    SHOWN.get()
}

/// Links the slots on a client whose build has them, when `asked`: a mod
/// draws frames, HUD items or screen extensions, the only places that draw
/// a slot or read the hovered one. Without a mod asking, what the build
/// lacks is only logged, never listed off.
pub(super) fn link(loader: &Loader, asked: bool) {
    let off = |fuse: &'static Fuse, (id, reason): Failed| {
        if asked {
            loader.fuse_off(fuse, id, reason);
        } else {
            let reason = loader.build.reason(id, reason);
            crate::log!(
                INFO,
                "interface: no mod asks for {}, which this build lacks: {id} {reason}",
                fuse.feature()
            );
        }
    };
    match linked(loader) {
        Ok(linked) => {
            let _first = STATE.linked.set(linked);
        }
        Err(failed) => off(&SLOTS_FUSE, failed),
    }
    match hover_holds(loader) {
        Ok(input) if asked => {
            let _first = STATE.input.set(input);
            let _linked = loader.link_feature(
                &HOVER_FUSE,
                Target::Pinned(pinned::ui::ITEM_SLOT),
                &NEXT,
                slot_detour as SlotFn,
                HookFlags::NONE,
            );
        }
        Ok(_) => {}
        Err(failed) => off(&HOVER_FUSE, failed),
    }
}

/// What a link failed on: the entry, and why.
type Failed = (&'static str, &'static str);

/// The build's function `id`, once its code holds its route's check.
fn function(loader: &Loader, id: pinned::Symbol) -> Result<usize, Failed> {
    loader
        .address(id)
        .filter(|_| loader.code_holds(id))
        .and_then(|a| usize::try_from(a).ok())
        .ok_or((id.id(), crate::engine::builds::ABSENT))
}

/// Where the interface says whether the gamepad drives it, once the slot
/// widget is there, the record's fields and the answer's focus and pointer
/// bytes, all the hovered slot reads, lie where the loader keeps them, or
/// the first table entry it lacks or that does not fit, with why.
fn hover_holds(loader: &Loader) -> Result<Input, Failed> {
    use core::mem::offset_of;
    function(loader, pinned::ui::ITEM_SLOT)?;
    record_holds(loader)?;
    answer_holds_byte(
        loader,
        pinned::ui::ITEM_SLOT_FOCUSED,
        offset_of!(Answer, focused),
    )?;
    answer_holds_byte(
        loader,
        pinned::ui::ITEM_SLOT_POINTER,
        offset_of!(Answer, pointer),
    )?;
    Input::read(loader)
}

/// `Ok` when the stack's record keeps its item, entity and count, and has
/// its size, where the loader's [`Record`] does.
fn record_holds(loader: &Loader) -> Result<(), Failed> {
    use core::mem::offset_of;
    let offset = |id| offset(loader, id);
    for (id, field) in [
        (pinned::ui::VIEWER_ITEM, offset_of!(Record, item)),
        (pinned::ui::VIEWER_PIDE, offset_of!(Record, pide)),
        (pinned::ui::ITEM_SLOT_COUNT, offset_of!(Record, count)),
    ] {
        if offset(id)? != field {
            return Err((id.id(), "is not where the slot's record keeps it"));
        }
    }
    let size = pinned::ui::ITEM_SLOT_RECORD_SIZE;
    if offset(size)? != size_of::<Record>() {
        return Err((size.id(), "is not the size of the slot's record"));
    }
    Ok(())
}

/// The build's constant `id`.
fn offset(loader: &Loader, id: pinned::Constant) -> Result<usize, Failed> {
    loader
        .offset(id)
        .ok_or((id.id(), crate::engine::builds::ABSENT))
}

/// `Ok` when the widget's state byte `id` lands at `field` of its
/// [`Answer`], inside the answer.
fn answer_holds_byte(loader: &Loader, id: pinned::Constant, field: usize) -> Result<(), Failed> {
    let state = offset(loader, pinned::ui::ITEM_SLOT_STATE)?;
    if !answer_holds(state + STATE_BYTES) {
        return Err((
            pinned::ui::ITEM_SLOT_STATE.id(),
            "ends the slot's answer past the room the loader keeps for it",
        ));
    }
    let local = offset(loader, pinned::ui::ITEM_SLOT_STATE_LOCAL)?;
    if answer_offset(state, local, offset(loader, id)?) != Some(field) {
        return Err((id.id(), "is not where the slot's answer keeps it"));
    }
    Ok(())
}

/// What mods' slots need of this build, or the first table entry it lacks or
/// that does not fit, with why; what only their hover reads is
/// [`hover_holds`]'s.
fn linked(loader: &Loader) -> Result<Linked, Failed> {
    use core::mem::offset_of;
    let draw = function(loader, pinned::ui::ITEM_SLOT)?;
    record_holds(loader)?;
    let (init, info) = (
        function(loader, pinned::ui::ITEM_SLOT_INIT)?,
        function(loader, pinned::item::INFO)?,
    );
    answer_holds_byte(
        loader,
        pinned::ui::ITEM_SLOT_PRESSED,
        offset_of!(Answer, pressed),
    )?;
    answer_holds_byte(loader, pinned::ui::ITEM_SLOT_HIT, offset_of!(Answer, hit))?;
    let offset = |id| offset(loader, id);
    let room = "is past the room the loader keeps for a slot's data";
    if !slot_data_holds(offset(pinned::ui::ITEM_SLOT_INIT_END)?) {
        return Err((pinned::ui::ITEM_SLOT_INIT_END.id(), room));
    }
    let flag = |id: pinned::Constant| {
        let at = offset(id)?;
        if slot_data_holds(at + 1) {
            Ok(at)
        } else {
            Err((id.id(), room))
        }
    };
    let focusable = flag(pinned::ui::ITEM_SLOT_FOCUSABLE);
    let (clickable, focusable_at) = slot_flags(flag(pinned::ui::ITEM_SLOT_CLICKABLE), focusable)?;
    if let Err((id, reason)) = focusable {
        let reason = loader.build.reason(id, reason);
        crate::log!(
            INFO,
            "interface: mods' item slots show no hover look, which this build lacks: {id} {reason}"
        );
    }
    Ok(Linked {
        // SAFETY: `code_holds` checked the code at `ui.item_slot` against its
        // route's shape and check, so it is the game's slot widget, typed
        // `SlotFn` (client 26430625, decompiler).
        draw: unsafe { core::mem::transmute::<usize, SlotFn>(draw) },
        // SAFETY: `code_holds` checked the code at `ui.item_slot_init`
        // against its route's check, so it is the game's slot data init,
        // typed `InitFn` (client 26430625, decompiler).
        init: unsafe { core::mem::transmute::<usize, InitFn>(init) },
        // SAFETY: `code_holds` checked the code at `item.info` against its
        // route's check, so it is the game's item lookup, typed `InfoFn` as
        // the inventory layer's `ItemLookup`.
        info: unsafe { core::mem::transmute::<usize, InfoFn>(info) },
        clickable,
        focusable: focusable_at,
        player: offset(pinned::ui::CONTEXT_PLAYER)?,
        player_data: offset(pinned::ui::PLAYER_DATA)?,
        player_items: offset(pinned::ui::PLAYER_ITEMS)?,
    })
}

/// The click flag, which mods' slots need, and the navigation flag, which
/// only gives them the hover look, so a build lacking it still draws them.
fn slot_flags(
    clickable: Result<usize, Failed>,
    focusable: Result<usize, Failed>,
) -> Result<(usize, Option<usize>), Failed> {
    Ok((clickable?, focusable.ok()))
}

/// Whether the bytes of a slot's data before `end` lie in the `SLOT_DATA`
/// `u64`s the loader keeps for it.
const fn slot_data_holds(end: usize) -> bool {
    end <= SLOT_DATA * size_of::<u64>()
}

/// Whether the bytes of the widget's answer before `end` lie in the
/// [`Answer`] the loader hands it.
const fn answer_holds(end: usize) -> bool {
    end <= size_of::<Answer>()
}

/// Where the answer keeps the state byte the widget writes at `byte` of its
/// stack: the block at `local` goes to `state` of the answer whole. `None`
/// for a byte outside the block.
fn answer_offset(state: usize, local: usize, byte: usize) -> Option<usize> {
    byte.checked_sub(local)
        .filter(|&within| within < STATE_BYTES)
        .map(|within| state + within)
}

crate::engine::chains::chain_detour! {
    /// The loader's link on the game's slot widget, `ui.item_slot`: the
    /// game's slot, then a note of the record of a screen's slot the player
    /// points at. A slot with no screen, as a mod's, is never noted.
    fn slot_detour(answer: *mut Answer, slot: *mut u8, drop: *mut u8, screen: *mut u8) -> *mut Answer;
    next NEXT;
    fuse &HOVER_FUSE, "the hovered slot";
    |onward| {
        let answered = onward.call();
        if screen.is_null() || answered.is_null() || slot.is_null() {
            return answered;
        }
        // SAFETY: the game's slot widget returned its non-null answer.
        if let Some(under_cursor) = unsafe { pointed_by(answered.expose_provenance()) } {
            // SAFETY: the widget was handed its non-null slot data, whose
            // first field is the stack's record (client 26430625).
            unsafe { note(slot.expose_provenance(), under_cursor) };
        }
        answered
    }
}

/// Whether `answer` says the mouse cursor is on its slot (true) or the slot
/// holds the focus (false); `None` for neither, the cheap test each of a
/// screen's slots takes before [`note`].
///
/// # Safety
///
/// - `answer` is the game slot widget's [`Answer`], live for the call.
#[inline(always)]
unsafe fn pointed_by(answer: usize) -> Option<bool> {
    use core::mem::offset_of;
    // SAFETY: the caller's contract: an `Answer` lives at `answer`.
    let byte = |field| unsafe { crate::engine::memory::read::<u8>(answer, field) };
    if byte(offset_of!(Answer, pointer)) != 0 {
        Some(true)
    } else {
        (byte(offset_of!(Answer, focused)) != 0).then_some(false)
    }
}

/// Notes the slot whose data is at `slot` under the cursor when
/// `under_cursor`, else holding the focus, as [`noted`] reads it.
///
/// # Safety
///
/// As [`noted`].
#[cold]
unsafe fn note(slot: usize, under_cursor: bool) {
    // SAFETY: the caller's contract, which is `noted`'s.
    if let Some(hovered) = unsafe { noted(slot) } {
        // A branch each, not a chosen `&LocalKey`: through a reference the
        // thread-local's accessor stays an indirect call.
        if under_cursor {
            NOW.set(hovered);
        } else {
            FOCUSED.set(hovered);
        }
    }
}

/// Whether the gamepad drives the interface of `context`, by the input the
/// hovered slot linked; false before it links or while its feature is off.
///
/// # Safety
///
/// - Called on the interface thread.
/// - `context` is 0 or the context `Engine::context` read in this frame.
unsafe fn gamepad_drives(context: usize) -> bool {
    // SAFETY: on the interface thread with the frame's context, by this
    // function's `# Safety`.
    hover_input().is_some_and(|input| unsafe { input.navigating(context) })
}

/// The input the hovered slot linked, while its feature is on: a slot's
/// hover reads the answer bytes only its link checked.
fn hover_input() -> Option<Input> {
    STATE.input.get().copied().filter(|_| !HOVER_FUSE.is_off())
}

/// The slot whose data is at `slot`, when its record names one.
///
/// # Safety
///
/// - `slot` is the game slot widget's slot data, live for the call on the
///   interface thread.
// Inlined into `note`: returned through memory, its answer cost a noted slot
// 3 ns more (`item_ui` bench, 5.5 ns against 2.2).
#[inline(always)]
unsafe fn noted(slot: usize) -> Option<Hovered> {
    use crate::engine::memory::read;
    // SAFETY: the caller's contract: the slot data's first field is the
    // record's address.
    let at = unsafe { follow(slot, 0) };
    if at == 0 {
        return None;
    }
    // SAFETY: a non-null record, live while the widget draws it.
    let record = unsafe { read::<Record>(at, 0) };
    (record.slot != 0).then_some(Hovered {
        slot: record.slot,
        item: record.item,
        count: record.count,
        pide: record.pide,
    })
}

/// The pointer `base` keeps at `offset`; 0 for a null `base`.
///
/// # Safety
///
/// - `base` is 0 or readable for eight bytes at `offset`.
unsafe fn follow(base: usize, offset: usize) -> usize {
    if base == 0 {
        return 0;
    }
    // SAFETY: the caller's contract, `base` non-zero by the check before it.
    unsafe { crate::engine::memory::read::<usize>(base, offset) }
}

/// Draws a mod's item slot of `raw.count` of `raw.item` and writes its
/// click and hover into `raw.result`.
///
/// # Safety
///
/// - Called inside a window or a screen extension's window open in this
///   frame on the interface thread.
/// - `context` is the non-zero context the frame read.
pub(super) unsafe fn draw(context: usize, raw: &mut UiWidget) -> Result<(), WidgetError> {
    let Some(linked) = STATE.linked.get().filter(|_| !SLOTS_FUSE.is_off()) else {
        return Err(WidgetError::Unavailable);
    };
    if raw.count == 0 {
        return Err(WidgetError::Malformed);
    }
    // SAFETY: the caller's contract: the context is the frame's own, its
    // local player holder at the table's `ui.context_player`.
    let holder = unsafe { follow(context, linked.player) };
    // SAFETY: a non-zero holder points at the local player's pointer, as
    // the slot widget reads it (client 26430625).
    let player = unsafe { follow(holder, 0) };
    // SAFETY: a non-zero player keeps its data at the table's
    // `ui.player_data` (client 26430625).
    let data = unsafe { follow(player, linked.player_data) };
    if data == 0 {
        // Outside a game the widget would read the item table of no player.
        return Err(WidgetError::Unavailable);
    }
    // SAFETY: the player's data is live in this frame, its item table at the
    // table's `ui.player_items`.
    let items = unsafe { follow(data, linked.player_items) };
    // SAFETY: `info` is the game's `item.info`, checked at link, on the
    // client's own item table, as the slot widget calls it.
    if unsafe { (linked.info)(raw.item.get(), items) }.is_null() {
        return Err(WidgetError::NoItem);
    }
    let mut record = Record {
        item: raw.item.get(),
        count: raw.count,
        ..bytemuck::Zeroable::zeroed()
    };
    let mut slot = [0u64; SLOT_DATA];
    // SAFETY: `init` is the game's `ui.item_slot_init`, checked at link, on
    // `SLOT_DATA` `u64`s of this stack, no fewer than its routed extent by
    // `slot_data_holds` at link.
    unsafe { (linked.init)(slot.as_mut_ptr().cast()) };
    slot[0] = (&raw mut record).expose_provenance() as u64;
    for flag in core::iter::once(linked.clickable).chain(linked.focusable) {
        // SAFETY: `slot` holds `SLOT_DATA` `u64`s, its click flag and its
        // navigation flag at the table's `ui.item_slot_clickable` and
        // `ui.item_slot_focusable`, inside them by `slot_data_holds` at link.
        unsafe {
            crate::engine::memory::write::<u8>(slot.as_mut_ptr().expose_provenance(), flag, 1);
        }
    }
    let mut answer: Answer = bytemuck::Zeroable::zeroed();
    // SAFETY: `draw` is the game's `ui.item_slot`, checked at link, inside
    // the window the caller's contract gives, with slot data its init wrote
    // pointing at a live record, no drop target and no screen, and a live
    // answer; `record` and `slot` outlive the call.
    unsafe {
        (linked.draw)(
            &raw mut answer,
            slot.as_mut_ptr().cast(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        )
    };
    let navigating = hover_input().map(|input| {
        // SAFETY: on the interface thread with the frame's context, by this
        // function's `# Safety`.
        move || unsafe { input.navigating(context) }
    });
    raw.result = result(&answer, navigating);
    Ok(())
}

/// The widget's result bits for the slot widget's answer; `navigating`
/// says whether the gamepad drives the interface, `None` where the hovered
/// slot did not link, so only a click counts as a hover.
fn result(answer: &Answer, navigating: Option<impl FnOnce() -> bool>) -> u32 {
    let clicked = answer.pressed == CLICKED || answer.hit == CLICKED;
    let hovered = clicked || navigating.is_some_and(|navigating| answer.pointed(navigating));
    (u32::from(clicked) * UiWidget::SLOT_CLICKED) | (u32::from(hovered) * UiWidget::SLOT_HOVERED)
}

/// Readies the slot widget's detour for its benchmark with a stand-in for
/// the game's widget, which answers the cursor on the slot: the closure runs one
/// slot of a screen (`screen`) or of a mod (none), as the game calls it per
/// slot and frame.
#[cfg(any(test, feature = "test"))]
#[doc(hidden)]
pub fn bench_item_slot(screen: bool) -> impl FnMut() -> u64 {
    unsafe extern "system" fn game(
        answer: *mut Answer,
        _: *mut u8,
        _: *mut u8,
        _: *mut u8,
    ) -> *mut Answer {
        // SAFETY: the benchmark's own live answer.
        unsafe { (*answer).pointer = 1 };
        answer
    }
    let next: SlotFn = game;
    #[expect(
        clippy::fn_to_numeric_cast_any,
        reason = "a slot holds its next function's address"
    )]
    NEXT.slot()
        .store(next as usize, std::sync::atomic::Ordering::Release);
    let mut record = Record {
        slot: 0x3_0000_0041,
        count: 2,
        ..bytemuck::Zeroable::zeroed()
    };
    let mut slot = [0u64; SLOT_DATA];
    let mut state = [0u8; 16];
    move || {
        // The closure owns the record and the screen's state, so their
        // addresses are taken here, where they live, as the game fills a
        // slot's data before each call.
        slot[0] = (&raw mut record).expose_provenance() as u64;
        let screen = if screen {
            state.as_mut_ptr()
        } else {
            core::ptr::null_mut()
        };
        let mut answer: Answer = bytemuck::Zeroable::zeroed();
        // SAFETY: the detour with the stand-in as its next function, on live
        // slot data that points at a live record.
        unsafe {
            slot_detour(
                &raw mut answer,
                slot.as_mut_ptr().cast(),
                core::ptr::null_mut(),
                screen,
            )
        };
        NOW.get().slot
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_answer_and_the_record_keep_the_game_s_offsets() {
        use core::mem::offset_of;
        assert_eq!(
            [
                offset_of!(Answer, focused),
                offset_of!(Answer, pointer),
                offset_of!(Answer, pressed),
                offset_of!(Answer, hit)
            ],
            [0x10, 0x12, 0x15, 0x19]
        );
        assert_eq!(
            [
                offset_of!(Record, item),
                offset_of!(Record, pide),
                offset_of!(Record, count)
            ],
            [8, 0xc, 0x10]
        );
    }

    #[test]
    fn a_click_flag_or_an_init_extent_past_the_kept_slot_data_is_refused() {
        assert!(
            slot_data_holds(0x62 + 1),
            "the click flag on client 26430625"
        );
        assert!(
            slot_data_holds(0x61 + 1),
            "the navigation flag on client 26430625"
        );
        assert!(
            slot_data_holds(0x91),
            "the init's extent on client 26430625"
        );
        assert!(slot_data_holds(SLOT_DATA * 8));
        assert!(!slot_data_holds(SLOT_DATA * 8 + 1));
        assert!(!slot_data_holds(0x122), "the init route's upper bound");
    }

    #[test]
    fn a_build_without_the_navigation_flag_still_draws_slots_but_one_without_the_click_flag_does_not()
     {
        let missing = ("ui.item_slot_focusable", "is absent");
        assert_eq!(slot_flags(Ok(0x62), Ok(0x61)), Ok((0x62, Some(0x61))));
        assert_eq!(
            slot_flags(Ok(0x62), Err(missing)),
            Ok((0x62, None)),
            "only the hover look goes"
        );
        let no_click = ("ui.item_slot_clickable", "is absent");
        assert_eq!(slot_flags(Err(no_click), Ok(0x61)), Err(no_click));
    }

    #[test]
    fn a_state_byte_maps_into_the_answer_only_from_inside_its_block() {
        let (state, local) = (0x10, 0x60);
        assert_eq!(
            [0x60, 0x62, 0x65, 0x69].map(|byte| answer_offset(state, local, byte)),
            [Some(0x10), Some(0x12), Some(0x15), Some(0x19)],
            "client 26430625"
        );
        assert_eq!(answer_offset(state, local, local - 1), None);
        assert_eq!(answer_offset(state, local, local + STATE_BYTES), None);
        assert_eq!(
            answer_offset(state, local, local + STATE_BYTES - 1),
            Some(0x1f)
        );
    }

    #[test]
    fn an_answer_past_the_loader_s_buffer_is_refused() {
        assert!(answer_holds(0x10 + STATE_BYTES), "client 26430625");
        assert!(!answer_holds(0x20 + STATE_BYTES));
    }

    #[test]
    fn a_slot_reads_a_click_from_either_test_and_is_hovered_when_clicked() {
        let mouse = Some(|| false);
        let mut answer = Answer::default();
        assert_eq!(result(&answer, mouse), 0);
        answer.pointer = 1;
        assert_eq!(result(&answer, mouse), UiWidget::SLOT_HOVERED);
        answer.hit = CLICKED;
        assert_eq!(
            result(&answer, mouse),
            UiWidget::SLOT_HOVERED | UiWidget::SLOT_CLICKED
        );
        let pressed = Answer {
            pressed: CLICKED,
            ..Answer::default()
        };
        assert_eq!(
            result(&pressed, mouse),
            UiWidget::SLOT_HOVERED | UiWidget::SLOT_CLICKED
        );
        let other = Answer {
            pressed: CLICKED - 1,
            ..Answer::default()
        };
        assert_eq!(result(&other, mouse), 0);
    }

    #[test]
    fn without_the_hovered_slot_s_link_a_mod_s_slot_is_hovered_only_when_clicked() {
        let unlinked: Option<fn() -> bool> = None;
        let pointed = Answer {
            pointer: 1,
            focused: 1,
            ..Answer::default()
        };
        assert_eq!(result(&pointed, unlinked), 0);
        let clicked = Answer {
            hit: CLICKED,
            ..pointed
        };
        assert_eq!(
            result(&clicked, unlinked),
            UiWidget::SLOT_HOVERED | UiWidget::SLOT_CLICKED
        );
    }

    #[test]
    fn the_focused_slot_counts_as_pointed_at_only_while_the_gamepad_drives() {
        let focused = Answer {
            focused: 1,
            ..Answer::default()
        };
        assert!(!focused.pointed(|| false), "the mouse's selection");
        assert!(focused.pointed(|| true), "the gamepad's focus");
        let under_cursor = Answer {
            pointer: 1,
            ..Answer::default()
        };
        assert!(under_cursor.pointed(|| unreachable!("no mode read")));
        assert!(!Answer::default().pointed(|| true));
        assert_eq!(result(&focused, Some(|| false)), 0);
        assert_eq!(result(&focused, Some(|| true)), UiWidget::SLOT_HOVERED);
    }

    /// What [`noted`] reads of `record` under an answer with `focused` and
    /// `pointer`.
    fn noted_of(record: &Record, focused: u8, pointer: u8) -> Option<(Hovered, bool)> {
        let mut slot = [0u64; SLOT_DATA];
        slot[0] = core::ptr::from_ref(record).expose_provenance() as u64;
        let answer = Answer {
            focused,
            pointer,
            ..Answer::default()
        };
        let answer_at = core::ptr::from_ref(&answer).expose_provenance();
        // SAFETY: `answer`, `slot` and the `record` it points at live on this
        // stack for the call.
        let under_cursor = unsafe { pointed_by(answer_at) }?;
        // SAFETY: `slot` and the `record` it points at live on this stack.
        Some((
            unsafe { noted(slot.as_ptr().expose_provenance()) }?,
            under_cursor,
        ))
    }

    #[test]
    fn a_hovered_slot_is_noted_only_with_a_record_that_names_one() {
        let record = Record {
            slot: 0x3_0000_0041,
            item: 7,
            pide: 900,
            count: 2,
            ..bytemuck::Zeroable::zeroed()
        };
        let hovered = Hovered {
            slot: 0x3_0000_0041,
            item: 7,
            count: 2,
            pide: 900,
        };
        assert_eq!(noted_of(&record, 0, 0), None, "neither");
        assert_eq!(noted_of(&record, 1, 0), Some((hovered, false)), "focused");
        assert_eq!(noted_of(&record, 0, 1), Some((hovered, true)), "cursor");
        assert_eq!(noted_of(&record, 1, 1), Some((hovered, true)), "both");
        let none = Record { slot: 0, ..record };
        assert_eq!(noted_of(&none, 0, 1), None, "no slot");
    }

    #[test]
    fn a_frame_reads_the_slot_under_the_cursor_and_the_focused_one_only_with_a_gamepad() {
        let at = |slot| Hovered {
            slot,
            ..Hovered::NONE
        };
        let (cursor, selected) = (at(0x3_0000_0002), at(0x3_0000_0001));
        let none = Hovered::NONE;
        assert_eq!(
            pointed_slot(none, selected, || false),
            none,
            "the mouse's selection, the cursor on no slot"
        );
        assert_eq!(pointed_slot(cursor, selected, || false), cursor);
        assert_eq!(
            pointed_slot(cursor, selected, || unreachable!("no mode read")),
            cursor
        );
        assert_eq!(pointed_slot(none, selected, || true), selected, "gamepad");
        assert_eq!(pointed_slot(none, none, || true), none);
    }

    #[test]
    fn the_benchmark_of_a_screen_s_slot_notes_the_slot_it_draws() {
        assert_eq!(bench_item_slot(true)(), 0x3_0000_0041);
    }

    #[test]
    fn a_frame_reads_what_the_screens_noted_in_the_last_whole_frame() {
        let noted = Hovered {
            slot: 0x3_0000_0041,
            item: 7,
            count: 2,
            pide: 0,
        };
        NOW.set(noted);
        assert_eq!(hovered(), Hovered::NONE);
        end_frame(0);
        assert_eq!(hovered(), noted);
        end_frame(0);
        assert_eq!(hovered(), Hovered::NONE);
    }
}
