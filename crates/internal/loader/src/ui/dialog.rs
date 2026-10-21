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
//! When the interface hook could not be armed, a dialog shows as a system
//! message box on a thread of its own instead, so the game never waits.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use enshwick_loader_api::abi::{Status, UiAnswerFn, UiDialog};

use super::engine::{self, Buf, DialogLayout, Engine, PopupFn};
use crate::host::Registered;

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
                    // SAFETY: the mod's callback with its own pointer, once.
                    crate::profile::call(
                        registered.owner,
                        crate::profile::Feature::Interface,
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
        }
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
}

/// The dialog on screen: its answerer, popup state and texts.
struct Current {
    answerer: Answerer,
    state: Box<Buf<{ engine::POPUP_SIZE }>>,
    _texts: Arc<Shown>,
}

const QUEUED: u8 = 0;
const IN_GAME: u8 = 1;
const SYSTEM: u8 = 2;

static MODE: AtomicU8 = AtomicU8::new(QUEUED);
static WAITING: Mutex<VecDeque<Pending>> = Mutex::new(VecDeque::new());
static SHOWN: Mutex<VecDeque<Arc<Shown>>> = Mutex::new(VecDeque::new());
static NEXT_SLOT: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static CURRENT: core::cell::RefCell<Option<Current>> = const { core::cell::RefCell::new(None) };
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
    enshwick_loader_reflect::fnv1a32(&TAGS[index])
}

/// The text of a dialog text tag, by its id, while its dialog is kept.
pub(crate) fn text(id: u32) -> Option<&'static str> {
    let index = (0..TAGS.len()).find(|&i| tag_id(i) == id)?;
    let shown = SHOWN.lock().unwrap_or_else(PoisonError::into_inner);
    let texts = shown.iter().find(|s| s.slot == index / 2)?;
    let text: &str = if index % 2 == 0 {
        &texts.title
    } else {
        &texts.message
    };
    // SAFETY: the texts stay in `SHOWN` for the next `KEPT - 1` dialogs, and
    // the game reads a tag's text only while drawing its dialog, which shows
    // one at a time.
    Some(unsafe { &*core::ptr::from_ref::<str>(text) })
}

/// Queues `dialog`, or shows it as a system message box when the interface
/// hook could not be armed.
pub(crate) fn queue(dialog: Pending) -> Status {
    if MODE.load(Ordering::Acquire) == SYSTEM {
        system_box(dialog);
        return Status::OK;
    }
    let mut waiting = WAITING.lock().unwrap_or_else(PoisonError::into_inner);
    if waiting.len() >= MAX_WAITING {
        return Status::REFUSED;
    }
    waiting.push_back(dialog);
    Status::OK
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
    MODE.store(if in_game { IN_GAME } else { SYSTEM }, Ordering::Release);
    if !in_game {
        let waiting = std::mem::take(&mut *WAITING.lock().unwrap_or_else(PoisonError::into_inner));
        for dialog in waiting {
            system_box(dialog);
        }
    }
}

/// Draws the dialog on screen, taking the next when none shows, and hears
/// its answer.
///
/// # Safety
///
/// Inside a frame, on its thread, no popup of the game's showing; `next` is
/// the rest of the popup's chain.
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
        let Some(shown) = current.as_mut() else {
            return;
        };
        // SAFETY: the host's answer flag, on the thread that draws it.
        let pending = unsafe { crate::world::read::<u8>(host, layout.answered) };
        if pending != 0 {
            // An answer of the game's own waits for its flow.
            return;
        }
        let nav = engine.nav(context);
        // SAFETY: inside the frame, the loader's own popup state.
        unsafe { next(shown.state.ptr()) };
        engine.restore_nav(context, nav);
        // SAFETY: as above.
        let answered = unsafe { crate::world::read::<u8>(host, layout.answered) };
        if answered == 0 {
            return;
        }
        // SAFETY: as above.
        let index = unsafe { crate::world::read::<u8>(host, layout.answer) };
        let flag = core::ptr::with_exposed_provenance_mut::<u8>(host + layout.answered);
        // SAFETY: the flag the click set, cleared on the thread that draws.
        unsafe { flag.write(0) };
        if let Some(done) = current.take() {
            done.answerer.answer(u32::from(index));
        }
    });
}

fn take_next(layout: &DialogLayout, active: usize) -> Option<Current> {
    let dialog = WAITING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .pop_front()?;
    let slot = NEXT_SLOT.fetch_add(1, Ordering::Relaxed) % KEPT;
    let texts = Arc::new(Shown {
        slot,
        title: dialog.title.into_boxed_str(),
        message: dialog.message.into_boxed_str(),
        detail: dialog.detail.into_boxed_str(),
    });
    {
        let mut shown = SHOWN.lock().unwrap_or_else(PoisonError::into_inner);
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
        state,
        _texts: texts,
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
    let spawned = std::thread::Builder::new()
        .name("enshwick-dialog".into())
        .spawn(move || {
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
            // SAFETY: NUL-terminated strings and no owner window.
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
    if let Err(e) = spawned {
        crate::log!(WARN, "cannot show a dialog: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_are_distinct_from_each_other_and_the_notices() {
        let ids: std::collections::HashSet<u32> = (0..TAGS.len()).map(tag_id).collect();
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
