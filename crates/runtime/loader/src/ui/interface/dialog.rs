//! Dialogs in the game's own style: the game's error popup, drawn again with
//! the loader's own popup state when no popup of the game's shows.
//!
//! The popup reads its title and message by text tag, so each dialog's texts
//! go into tags of the loader's own, answered by its text lookup; the detail
//! text and the buttons sit in the state itself, and the game labels the
//! buttons in the player's language. A click writes the button's index and a
//! flag into the interface host, which the loader reads and clears right
//! after drawing, so the game's own flow never sees it.
//!
//! When the interface hook could not be armed, or it or its dialogs turned
//! off since, a dialog shows as a system message box on a thread of its own
//! instead, so the game never waits.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use enshwick_core::abi::{Side, Status, UiAnswerFn, UiDialog};

use crate::engine::loader::{Loader, Registered};
use crate::engine::mod_index::ModIndex;
use enshwick_base::Relock;

use super::engine::{self, Buf, DialogLayout, Engine, PopupFn};

/// Dialogs waiting at most.
const MAX_WAITING: usize = 16;
/// Dialogs whose texts stay readable after they showed, so a text the game
/// still draws is never freed under it.
const KEPT: usize = 4;
/// The longest title, message or detail, in bytes.
const MAX_TEXT: usize = 4096;

/// Who hears the answer.
pub(crate) enum Answerer {
    /// A mod's callback.
    Mod(Registered<UiAnswerFn>),
    /// The loader's own.
    Loader(Box<dyn FnOnce(Option<u32>) + Send>),
}

impl Answerer {
    fn answer(self, raw: u32) {
        match self {
            Self::Mod(registered) => {
                if registered.live() {
                    // SAFETY: `registered.live()` says the mod still runs, so
                    // its DLL and the callback in it are loaded; the callback
                    // gets the pointer its mod registered with it, once, as
                    // `self` is consumed; an `extern "system"` function aborts
                    // rather than unwinds.
                    crate::engine::profile::call(
                        registered.owner,
                        crate::engine::profile::Feature::Interface,
                        || unsafe { (registered.callback)(raw, registered.user()) },
                    );
                }
            }
            Self::Loader(callback) => callback((raw != UiDialog::CLOSED).then_some(raw)),
        }
    }
}

/// A dialog waiting to show.
pub(crate) struct Pending {
    title: String,
    message: String,
    detail: String,
    buttons: u32,
    answerer: Answerer,
    /// Whether the dialog belongs to the world it was queued in, and goes
    /// with it.
    world: bool,
}

impl Pending {
    /// A dialog with texts cut at [`MAX_TEXT`] bytes and at least one button.
    pub(crate) fn new(
        title: &str,
        message: &str,
        detail: &str,
        buttons: u32,
        answerer: Answerer,
    ) -> Self {
        let buttons = buttons & UiDialog::BUTTONS;
        Self {
            title: cut(title),
            message: cut(message),
            detail: cut(detail),
            buttons: if buttons == 0 {
                1 << UiDialog::OK
            } else {
                buttons
            },
            answerer,
            world: false,
        }
    }

    /// The dialog marked as the world's when its mod `owner` is a server mod
    /// or the client shows a world, so a hosted game's end or a change of
    /// servers closes it.
    pub(crate) fn of_world(self, loader: &Loader, owner: ModIndex) -> Self {
        let server_mod = loader
            .mods
            .get(owner.get())
            .is_some_and(|m| m.manifest.side == Side::SERVER);
        self.marked(server_mod || STATE.in_world.load(Ordering::Acquire))
    }

    fn marked(self, world: bool) -> Self {
        Self { world, ..self }
    }
}

fn cut(text: &str) -> String {
    let mut end = text.len().min(MAX_TEXT);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

/// The texts of a dialog that showed, read by tag.
struct Shown {
    slot: usize,
    title: Box<str>,
    message: Box<str>,
    detail: Box<str>,
    /// Whether the game asked for a text yet, logged once.
    read: AtomicBool,
}

/// The dialog on screen: its answerer, buttons, popup state and texts.
struct Current {
    answerer: Answerer,
    buttons: u32,
    state: Box<Buf<{ engine::POPUP_SIZE }>>,
    texts: Arc<Shown>,
    /// Whether it belongs to a world, and the worlds ended before it showed.
    world: bool,
    ended: u64,
}

impl Current {
    /// The dialog to show again elsewhere, with its texts, buttons and
    /// answerer.
    fn into_pending(self) -> Pending {
        Pending {
            title: self.texts.title.to_string(),
            message: self.texts.message.to_string(),
            detail: self.texts.detail.to_string(),
            buttons: self.buttons,
            answerer: self.answerer,
            world: self.world,
        }
    }

    /// Whether a world it belongs to ended since it showed.
    fn outlived(&self) -> bool {
        self.world && self.ended != STATE.ended.load(Ordering::Acquire)
    }
}

const QUEUED: u8 = 0;
const IN_GAME: u8 = 1;
const SYSTEM: u8 = 2;

/// The dialogs waiting to show, oldest first.
struct Waiting {
    /// Set, under the lock of `dialogs`, while it may hold a dialog, so a
    /// frame with none waiting takes no lock.
    any: AtomicBool,
    dialogs: Mutex<VecDeque<Pending>>,
}

impl Waiting {
    const fn new() -> Self {
        Self {
            any: AtomicBool::new(false),
            dialogs: Mutex::new(VecDeque::new()),
        }
    }

    /// Queues `dialog` behind `dialogs`, the queue's own guard.
    fn push(&self, dialogs: &mut VecDeque<Pending>, dialog: Pending) {
        dialogs.push_back(dialog);
        self.any.store(true, Ordering::Release);
    }

    /// The oldest dialog waiting; no lock when none waits.
    fn pop(&self) -> Option<Pending> {
        if !self.any.load(Ordering::Acquire) {
            return None;
        }
        let mut dialogs = self.dialogs.relock();
        let next = dialogs.pop_front();
        self.any.store(!dialogs.is_empty(), Ordering::Release);
        next
    }

    /// Takes every dialog waiting, oldest first.
    fn take_all(&self) -> VecDeque<Pending> {
        let mut dialogs = self.dialogs.relock();
        self.any.store(false, Ordering::Release);
        std::mem::take(&mut *dialogs)
    }

    /// Takes the dialogs of the world that went, oldest first, leaving the
    /// others waiting in their order.
    fn take_world(&self) -> VecDeque<Pending> {
        let mut dialogs = self.dialogs.relock();
        let (world, others): (VecDeque<_>, VecDeque<_>) = dialogs.drain(..).partition(|d| d.world);
        *dialogs = others;
        self.any.store(!dialogs.is_empty(), Ordering::Release);
        world
    }
}

/// What the dialogs keep while the game runs. A world's end closes only
/// the dialogs that belong to it ([`close_world_dialogs`]): where dialogs show is
/// settled once at arming, other dialogs belong to mods that outlive the
/// world and wait for their answer, and the game may still read a kept
/// dialog's texts.
struct State {
    /// Where dialogs show: [`QUEUED`] until the hooks are armed, then
    /// [`IN_GAME`] or [`SYSTEM`].
    mode: AtomicU8,
    waiting: Waiting,
    /// The texts of the last [`KEPT`] dialogs that showed, one per slot.
    shown: Mutex<VecDeque<Arc<Shown>>>,
    /// The slot the next dialog takes, counted round [`KEPT`].
    next_slot: AtomicUsize,
    /// Whether the client showed a world in its last frame.
    in_world: AtomicBool,
    /// The worlds that ended, counted, so a world's dialog on screen sees
    /// its world went.
    ended: AtomicU64,
    /// The count of `ended` whose waiting dialogs the interface thread
    /// closed.
    closed: AtomicU64,
}

static STATE: State = State {
    mode: AtomicU8::new(QUEUED),
    waiting: Waiting::new(),
    shown: Mutex::new(VecDeque::new()),
    next_slot: AtomicUsize::new(0),
    in_world: AtomicBool::new(false),
    ended: AtomicU64::new(0),
    closed: AtomicU64::new(0),
};

thread_local! {
    static CURRENT: core::cell::RefCell<Option<Current>> = const { core::cell::RefCell::new(None) };
}

/// Whether a dialog shows, on the interface thread.
pub(super) fn showing() -> bool {
    CURRENT.with_borrow(Option::is_some)
}

/// The GUIDs of the loader's dialog text tags: title and message per slot.
#[expect(clippy::cast_possible_truncation, reason = "the index fits a byte")]
const TAGS: [[u8; 16]; 2 * KEPT] = {
    let mut tags = [[0u8; 16]; 2 * KEPT];
    let mut i = 0;
    while i < 2 * KEPT {
        // `e5c8a7d2-6b1f-4f0e-9c3a-0e57<index>`, beside the notices' range.
        tags[i] = [
            0xe5, 0xc8, 0xa7, 0xd2, 0x6b, 0x1f, 0x4f, 0x0e, 0x9c, 0x3a, 0x0e, 0x57, 0x4e, 0x0c,
            0x12, i as u8,
        ];
        i += 1;
    }
    tags
};

fn tag_id(index: usize) -> u32 {
    enshwick_gamedata::reflect::fnv1a32(&TAGS[index])
}

/// The ids of the dialog text tags.
pub(crate) fn tag_ids() -> impl Iterator<Item = u32> {
    (0..TAGS.len()).map(tag_id)
}

/// The text of a dialog text tag, by its id, while its dialog is kept.
pub(crate) fn text(id: u32) -> Option<&'static str> {
    let index = (0..TAGS.len()).find(|&i| tag_id(i) == id)?;
    let shown = STATE.shown.relock();
    let texts = shown.iter().find(|s| s.slot == index / 2)?;
    if !texts.read.swap(true, Ordering::Relaxed) {
        crate::log!(INFO, "interface: the game reads dialog \"{}\"", texts.title);
    }
    let text: &str = if index % 2 == 0 {
        &texts.title
    } else {
        &texts.message
    };
    // SAFETY: the `Box<str>` stays in the state's `shown` until `KEPT` more dialogs
    // showed, and the game reads a tag's text only while drawing its dialog,
    // which shows one at a time. The `'static` is wider than that: a caller
    // that keeps the text past `KEPT` dialogs reads freed memory.
    Some(unsafe { &*core::ptr::from_ref::<str>(text) })
}

/// Queues `dialog`, or shows it as a system message box once dialogs show
/// so. The mode is read under the queue's lock, so a dialog queued while
/// [`fall_back`] runs is either drained by it or shows as a box itself.
pub(crate) fn queue(dialog: Pending) -> Status {
    let mut waiting = STATE.waiting.dialogs.relock();
    if STATE.mode.load(Ordering::Acquire) == SYSTEM {
        drop(waiting);
        system_box(dialog);
        return Status::OK;
    }
    if waiting.len() >= MAX_WAITING {
        return Status::REFUSED;
    }
    STATE.waiting.push(&mut waiting, dialog);
    Status::OK
}

/// Notes, each frame on the interface thread, whether the client shows a
/// world, loading included, and closes the dialogs of every world that went
/// since the last frame: leaving a world for the menu ends it too. Each
/// waiting one answers as closed here, on the thread a mod's answer comes
/// on, so no mod waits for it; one on screen closes at its next draw.
pub(super) fn note_world(in_world: bool) {
    if STATE.in_world.swap(in_world, Ordering::AcqRel) && !in_world {
        close_world_dialogs();
    }
    let ended = STATE.ended.load(Ordering::Acquire);
    if STATE.closed.swap(ended, Ordering::AcqRel) == ended {
        return;
    }
    for dialog in STATE.waiting.take_world() {
        dialog.answerer.answer(UiDialog::CLOSED);
    }
}

/// Ends the world the dialogs belong to, when a client's hosted game ends,
/// it leaves for the menu or it changes servers, from any thread: the next
/// frame closes that world's dialogs ([`note_world`]).
pub(crate) fn close_world_dialogs() {
    STATE.ended.fetch_add(1, Ordering::AcqRel);
}

/// Shows the loader's own dialog, with `on_answer` hearing the answer, or
/// `None` when it closed without one.
pub(crate) fn show(
    title: &str,
    message: &str,
    detail: &str,
    buttons: u32,
    on_answer: impl FnOnce(Option<u32>) + Send + 'static,
) {
    let status = queue(Pending::new(
        title,
        message,
        detail,
        buttons,
        Answerer::Loader(Box::new(on_answer)),
    ));
    if !status.is_ok() {
        crate::log!(WARN, "dialog \"{title}\" not shown: too many wait");
    }
}

/// Settles where dialogs show once the hooks are armed: in game when the
/// interface hook is wired, else as system message boxes, those already
/// waiting among them.
pub(crate) fn armed(in_game: bool) {
    STATE
        .mode
        .store(if in_game { IN_GAME } else { SYSTEM }, Ordering::Release);
    if !in_game {
        for dialog in STATE.waiting.take_all() {
            system_box(dialog);
        }
    }
}

/// Shows dialogs as system message boxes once the interface draws them no
/// more: the one on screen, which no frame would answer, then those
/// waiting, and every later one at once. On the interface thread, which
/// holds the dialog on screen.
pub(super) fn fall_back() {
    if STATE.mode.load(Ordering::Acquire) == SYSTEM
        || STATE.mode.swap(SYSTEM, Ordering::AcqRel) == SYSTEM
    {
        return;
    }
    let on_screen = CURRENT.take();
    for dialog in stranded(on_screen, &mut STATE.waiting.take_all()) {
        system_box(dialog);
    }
}

/// The dialog on screen, then those waiting, in the order they would have
/// shown, leaving none waiting.
fn stranded(on_screen: Option<Current>, waiting: &mut VecDeque<Pending>) -> Vec<Pending> {
    on_screen
        .map(Current::into_pending)
        .into_iter()
        .chain(waiting.drain(..))
        .collect()
}

/// Draws the dialog on screen, taking the next when none shows, and hears
/// its answer.
///
/// # Safety
///
/// - Called inside a frame of the game's interface update, on its thread.
/// - `context` is the non-zero context `Engine::context` read in this frame.
/// - No popup of the game's is active.
/// - `next` is the rest of the chain on the game's `ui.popup`.
pub(super) unsafe fn draw(engine: &Engine, next: PopupFn, context: usize) {
    let Some(layout) = engine.dialog else {
        return;
    };
    let host = engine.host(context);
    if host == 0 {
        return;
    }
    CURRENT.with_borrow_mut(|current| {
        if current.is_none() {
            *current = take_next(&layout, engine.layout.popup_active);
        }
        if current.as_ref().is_some_and(Current::outlived) {
            if let Some(gone) = current.take() {
                gone.answerer.answer(UiDialog::CLOSED);
            }
            *current = take_next(&layout, engine.layout.popup_active);
        }
        let Some(shown) = current.as_mut() else {
            return;
        };
        // SAFETY: `host` is non-zero, checked by the guard before it, and is
        // the interface host read from the frame's live context; its answer
        // flag sits at the table's `ui.host_answered`, and only this thread,
        // which draws the frame, writes it.
        let pending = unsafe { crate::engine::memory::read::<u8>(host, layout.answered) };
        if pending != 0 {
            // An answer of the game's own waits for its flow.
            return;
        }
        let nav = engine.nav(context);
        // SAFETY: `next` is the rest of `ui.popup`'s chain and this runs in
        // the frame on its thread, by this function's `# Safety`; the state
        // is the loader's own boxed `POPUP_SIZE` bytes, which hold the
        // table's `ui.popup_extent` (`DialogLayout::read`).
        unsafe { next(shown.state.ptr()) };
        engine.restore_nav(context, nav);
        // SAFETY: `host` is non-zero, checked by the guard before it, and is
        // the interface host read from the frame's live context; its answer
        // flag sits at the table's `ui.host_answered`, and only this thread,
        // which draws the frame, writes it.
        let answered = unsafe { crate::engine::memory::read::<u8>(host, layout.answered) };
        if answered == 0 {
            return;
        }
        // SAFETY: `host` is non-zero, checked by the guard before it, and is
        // the interface host read from the frame's live context; the clicked
        // button's index sits at the table's `ui.host_answer`, written only on
        // this thread.
        let index = unsafe { crate::engine::memory::read::<u8>(host, layout.answer) };
        // SAFETY: `host` is non-zero, checked by the guard before it, and is
        // the interface host read from the frame's live context; the answer
        // flag at the table's `ui.host_answered` is a byte the game reads and
        // writes only on this thread, which draws the frame.
        unsafe { crate::engine::memory::write(host, layout.answered, 0u8) };
        if let Some(done) = current.take() {
            done.answerer.answer(u32::from(index));
        }
    });
}

fn take_next(layout: &DialogLayout, active: usize) -> Option<Current> {
    // The worlds whose dialogs were closed, read before the pop: a world that
    // ended since, its dialogs still waiting, closes the dialog at its next draw.
    let ended = STATE.closed.load(Ordering::Acquire);
    let dialog = STATE.waiting.pop()?;
    let slot = STATE.next_slot.fetch_add(1, Ordering::Relaxed) % KEPT;
    let texts = Arc::new(Shown {
        slot,
        title: dialog.title.into_boxed_str(),
        message: dialog.message.into_boxed_str(),
        detail: dialog.detail.into_boxed_str(),
        read: AtomicBool::new(false),
    });
    crate::log!(INFO, "interface: dialog \"{}\" shows", texts.title);
    {
        let mut shown = STATE.shown.relock();
        shown.retain(|s| s.slot != slot);
        shown.push_back(Arc::clone(&texts));
    }
    let mut state = Box::new(Buf::<{ engine::POPUP_SIZE }>::zeroed());
    state.u32(layout.title, tag_id(2 * slot));
    state.u32(layout.message, tag_id(2 * slot + 1));
    state.u16(
        layout.buttons,
        u16::try_from(dialog.buttons).unwrap_or(1 << UiDialog::OK),
    );
    state.u64(
        layout.detail,
        texts.detail.as_ptr().expose_provenance() as u64,
    );
    state.u64(layout.detail_len, texts.detail.len() as u64);
    state.u8(active, 1);
    Some(Current {
        answerer: dialog.answerer,
        buttons: dialog.buttons,
        state,
        texts,
        world: dialog.world,
        ended,
    })
}

/// The system message box's buttons for a dialog's, and the answer for each
/// of its results.
fn box_style(buttons: u32) -> (u32, [(i32, u32); 3]) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IDCANCEL, IDIGNORE, IDOK, IDRETRY, MB_ABORTRETRYIGNORE, MB_OK, MB_OKCANCEL, MB_RETRYCANCEL,
    };
    let has = |answer: u32| buttons & 1 << answer != 0;
    let first = (0..9).find(|&a| has(a)).unwrap_or(UiDialog::OK);
    if has(UiDialog::RETRY) && has(UiDialog::IGNORE) {
        (
            MB_ABORTRETRYIGNORE,
            [
                (IDRETRY, UiDialog::RETRY),
                (IDIGNORE, UiDialog::IGNORE),
                (3, UiDialog::CANCEL),
            ],
        )
    } else if has(UiDialog::RETRY) && has(UiDialog::CANCEL) {
        (
            MB_RETRYCANCEL,
            [
                (IDRETRY, UiDialog::RETRY),
                (IDCANCEL, UiDialog::CANCEL),
                (0, UiDialog::CANCEL),
            ],
        )
    } else if has(UiDialog::CANCEL) && buttons != 1 << UiDialog::CANCEL {
        let ok = (0..9)
            .find(|&a| has(a) && a != UiDialog::CANCEL)
            .unwrap_or(UiDialog::OK);
        (
            MB_OKCANCEL,
            [
                (IDOK, ok),
                (IDCANCEL, UiDialog::CANCEL),
                (0, UiDialog::CANCEL),
            ],
        )
    } else {
        (MB_OK, [(IDOK, first), (0, first), (0, first)])
    }
}

/// Shows `dialog` as a system message box on a thread of its own.
fn system_box(dialog: Pending) {
    let _started = crate::engine::off_thread::start("dialog", move || {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            MB_ICONINFORMATION, MB_SETFOREGROUND, MB_TOPMOST, MessageBoxW,
        };
        let (style, answers) = box_style(dialog.buttons);
        let mut body = dialog.message.clone();
        if !dialog.detail.is_empty() {
            body.push_str("\n\n");
            body.push_str(&dialog.detail);
        }
        let text: Vec<u16> = body.encode_utf16().chain([0]).collect();
        let title: Vec<u16> = dialog.title.encode_utf16().chain([0]).collect();
        // SAFETY: `text` and `title` are live NUL-terminated UTF-16
        // buffers built above, and a null owner window is allowed.
        let result = unsafe {
            MessageBoxW(
                core::ptr::null_mut(),
                text.as_ptr(),
                title.as_ptr(),
                style | MB_ICONINFORMATION | MB_TOPMOST | MB_SETFOREGROUND,
            )
        };
        let answer = answers
            .iter()
            .find(|(id, _)| *id == result)
            .map_or(UiDialog::CLOSED, |&(_, answer)| answer);
        dialog.answerer.answer(answer);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_are_distinct_from_each_other_and_the_notices() {
        let ids: rustc_hash::FxHashSet<u32> = (0..TAGS.len()).map(tag_id).collect();
        assert_eq!(ids.len(), TAGS.len());
    }

    #[test]
    fn a_dialog_keeps_one_button_and_cuts_long_texts() {
        let dialog = Pending::new(
            "t",
            &"ж".repeat(3000),
            "",
            0,
            Answerer::Loader(Box::new(|_| {})),
        );
        assert_eq!(dialog.buttons, 1 << UiDialog::OK);
        assert!(dialog.message.len() <= MAX_TEXT);
        assert!(dialog.message.chars().all(|c| c == 'ж'));
    }

    #[test]
    fn the_dialog_on_screen_falls_back_first_with_its_texts_then_those_waiting() {
        let loader = || Answerer::Loader(Box::new(|_| {}));
        let on_screen = Current {
            answerer: loader(),
            buttons: 1 << UiDialog::CANCEL,
            state: Box::new(Buf::zeroed()),
            texts: Arc::new(Shown {
                slot: 0,
                title: "shown".into(),
                message: "message".into(),
                detail: "detail".into(),
                read: AtomicBool::new(false),
            }),
            world: false,
            ended: 0,
        };
        let mut waiting = VecDeque::from([Pending::new("next", "", "", 0, loader())]);
        let dialogs = stranded(Some(on_screen), &mut waiting);
        let titles: Vec<_> = dialogs.iter().map(|d| d.title.as_str()).collect();
        assert_eq!(titles, ["shown", "next"]);
        assert_eq!(dialogs[0].message, "message");
        assert_eq!(dialogs[0].detail, "detail");
        assert_eq!(dialogs[0].buttons, 1 << UiDialog::CANCEL);
        assert!(waiting.is_empty(), "none is left waiting");
        assert!(stranded(None, &mut waiting).is_empty());
    }

    #[test]
    fn a_world_s_waiting_dialogs_close_on_the_next_frame_after_it_ends_however_it_ends() {
        let loader: &'static Loader = Box::leak(Box::new(crate::start::tests::bare_loader()));
        let ends: [(&str, &dyn Fn()); 3] = [
            ("a hosted game's end", &|| {
                crate::engine::parts::Part::forget_in(crate::ui::PARTS, "ui", loader);
            }),
            ("a change of servers", &|| crate::base::net::joining(false)),
            ("leaving a world for the menu", &|| note_world(true)),
        ];
        for (end, run) in ends {
            let answers = Arc::new(Mutex::new(Vec::new()));
            let heard = |title: &'static str| {
                let answers = Arc::clone(&answers);
                Answerer::Loader(Box::new(move |answer| {
                    answers.relock().push((title, answer));
                }))
            };
            {
                let mut waiting = STATE.waiting.dialogs.relock();
                let world = Pending::new("test world", "", "", 0, heard("test world"));
                STATE.waiting.push(&mut waiting, world.marked(true));
                let menu = Pending::new("test menu", "", "", 0, heard("test menu"));
                STATE.waiting.push(&mut waiting, menu);
            }
            run();
            // The third end is itself a frame, which may close a world another test ended.
            if !end.starts_with("leaving") {
                assert!(
                    answers.relock().is_empty(),
                    "{end}: answered on the interface thread only"
                );
            }
            note_world(false);
            assert_eq!(*answers.relock(), [("test world", None)], "{end}");
            let mut waiting = STATE.waiting.dialogs.relock();
            let titles: Vec<&str> = waiting.iter().map(|d| d.title.as_str()).collect();
            assert!(!titles.contains(&"test world"), "{end}: {titles:?}");
            assert!(titles.contains(&"test menu"), "{end}: a menu dialog stays");
            waiting.retain(|d| d.title != "test menu");
        }
    }

    #[test]
    fn a_world_s_dialog_on_screen_outlives_it_and_another_does_not() {
        let shown = |world: bool| Current {
            answerer: Answerer::Loader(Box::new(|_| {})),
            buttons: 1 << UiDialog::OK,
            state: Box::new(Buf::zeroed()),
            texts: Arc::new(Shown {
                slot: 0,
                title: "".into(),
                message: "".into(),
                detail: "".into(),
                read: AtomicBool::new(false),
            }),
            world,
            ended: STATE.ended.load(Ordering::Acquire),
        };
        let (world, menu) = (shown(true), shown(false));
        assert!(!world.outlived());
        close_world_dialogs();
        assert!(world.outlived());
        assert!(!menu.outlived());
    }

    #[test]
    fn a_world_s_dialog_that_shows_after_its_end_but_before_the_next_frame_still_closes() {
        close_world_dialogs();
        let shown_late = Current {
            answerer: Answerer::Loader(Box::new(|_| {})),
            buttons: 1 << UiDialog::OK,
            state: Box::new(Buf::zeroed()),
            texts: Arc::new(Shown {
                slot: 0,
                title: "".into(),
                message: "".into(),
                detail: "".into(),
                read: AtomicBool::new(false),
            }),
            world: true,
            // What take_next records for a dialog it pops.
            ended: STATE.closed.load(Ordering::Acquire),
        };
        assert!(shown_late.outlived());
    }

    #[test]
    fn waiting_dialogs_pop_oldest_first_and_the_flag_follows_the_queue() {
        let loader = || Answerer::Loader(Box::new(|_| {}));
        let waiting = Waiting::new();
        assert!(waiting.pop().is_none());
        for title in ["first", "second", "third"] {
            waiting.push(
                &mut waiting.dialogs.relock(),
                Pending::new(title, "", "", 0, loader()),
            );
        }
        assert_eq!(waiting.pop().map(|d| d.title), Some("first".to_owned()));
        assert!(waiting.any.load(Ordering::Acquire), "two still wait");
        let rest: Vec<_> = waiting.take_all().into_iter().map(|d| d.title).collect();
        assert_eq!(rest, ["second", "third"]);
        assert!(!waiting.any.load(Ordering::Acquire));
        assert!(waiting.pop().is_none());
    }

    #[test]
    fn system_boxes_map_their_results_to_the_dialogs_answers() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{IDCANCEL, IDOK, MB_OK, MB_OKCANCEL};
        let (style, answers) = box_style(1 << UiDialog::OK | 1 << UiDialog::CANCEL);
        assert_eq!(style, MB_OKCANCEL);
        assert!(answers.contains(&(IDOK, UiDialog::OK)));
        assert!(answers.contains(&(IDCANCEL, UiDialog::CANCEL)));
        let (style, answers) = box_style(1 << UiDialog::OK);
        assert_eq!(style, MB_OK);
        assert_eq!(answers[0], (IDOK, UiDialog::OK));
    }
}
