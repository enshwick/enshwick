//! The keyboard and mouse as Windows reports them, beside the game's own
//! Raw Input: scan codes turned into virtual keys for the layout of the
//! game window's thread, each key read once a frame, and key names in the
//! player's layout.

use core::cell::RefCell;

use enshwick::controls::{Key, chord_down};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyNameTextW, GetKeyboardLayout, MAPVK_VSC_TO_VK_EX, MapVirtualKeyExW,
    VK_CONTROL, VK_DECIMAL, VK_ESCAPE, VK_LBUTTON, VK_MBUTTON, VK_MENU, VK_NUMPAD0, VK_NUMPAD1,
    VK_NUMPAD2, VK_NUMPAD3, VK_NUMPAD4, VK_NUMPAD5, VK_NUMPAD6, VK_NUMPAD7, VK_NUMPAD8, VK_NUMPAD9,
    VK_RBUTTON, VK_SHIFT, VK_XBUTTON1, VK_XBUTTON2,
};

/// The modifiers' virtual keys, in the order of their bits in a [`Key`].
const MODIFIERS: [u16; 3] = [VK_SHIFT, VK_CONTROL, VK_MENU];
/// The modifiers' scan codes, for their names in the player's layout.
const MODIFIER_CODES: [u32; 3] = [0x2a, 0x1d, 0x38];
/// The mouse buttons' virtual keys, by their number in a [`Key`].
const MOUSE: [u16; 5] = [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON, VK_XBUTTON1, VK_XBUTTON2];

/// The controls' keys as virtual keys, made again only when the keys or the
/// layout change.
#[derive(Default)]
struct Mapped {
    layout: usize,
    bound: Vec<[u32; 2]>,
    vks: Vec<[u16; 2]>,
}

thread_local! {
    static MAPPED: RefCell<Mapped> = RefCell::new(Mapped::default());
}

/// The thread of the game's window while it has the focus.
pub(crate) fn focused_thread() -> Option<u32> {
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };
    // SAFETY: `GetForegroundWindow` takes no arguments and may return null,
    // checked below.
    let window = unsafe { GetForegroundWindow() };
    if window.is_null() {
        return None;
    }
    let mut process = 0u32;
    // SAFETY: `window` is a non-null handle, checked by the guard before it,
    // and `process` is a live `u32` the call writes; a handle that closed
    // meanwhile only fails the call, returning 0 and leaving `process` 0.
    let thread = unsafe { GetWindowThreadProcessId(window, &raw mut process) };
    // SAFETY: `GetCurrentProcessId` takes no arguments and cannot fail.
    (thread != 0 && process == unsafe { GetCurrentProcessId() }).then_some(thread)
}

/// Whether the game's window has the focus.
pub(crate) fn focused() -> bool {
    focused_thread().is_some()
}

/// The keyboard layout of thread `thread`, the game window's, whose input
/// the player types in.
fn layout(thread: u32) -> usize {
    // SAFETY: `GetKeyboardLayout` takes a thread id by value; an id that
    // ended meanwhile returns null.
    unsafe { GetKeyboardLayout(thread) }.expose_provenance()
}

/// The numpad digit or point of scan code `code`: `MapVirtualKeyExW` gives
/// these codes the navigation keys' virtual keys (Home, Up, Insert...),
/// which they send only while Num Lock is off.
fn numpad(code: u32) -> Option<u16> {
    let vk = match code {
        0x47 => VK_NUMPAD7,
        0x48 => VK_NUMPAD8,
        0x49 => VK_NUMPAD9,
        0x4b => VK_NUMPAD4,
        0x4c => VK_NUMPAD5,
        0x4d => VK_NUMPAD6,
        0x4f => VK_NUMPAD1,
        0x50 => VK_NUMPAD2,
        0x51 => VK_NUMPAD3,
        0x52 => VK_NUMPAD0,
        0x53 => VK_DECIMAL,
        _ => return None,
    };
    Some(vk)
}

/// The virtual key of the key with scan code `code` (bit 8 extended, bit 9
/// a mouse button) in layout `layout`; 0 for none.
fn virtual_key(code: u32, layout: usize) -> u16 {
    if code & 0x200 != 0 {
        return MOUSE.get((code & 0xff) as usize).copied().unwrap_or(0);
    }
    if let Some(vk) = numpad(code) {
        return vk;
    }
    let scan = if code & 0x100 != 0 {
        0xe000 | (code & 0xff)
    } else {
        code & 0xff
    };
    // SAFETY: `MapVirtualKeyExW` takes its arguments by value and returns 0
    // for a code without a key or a layout that is gone.
    let vk = unsafe {
        MapVirtualKeyExW(
            scan,
            MAPVK_VSC_TO_VK_EX,
            core::ptr::with_exposed_provenance_mut(layout),
        )
    };
    u16::try_from(vk).unwrap_or(0)
}

/// Whether virtual key `vk` is down now.
fn down(vk: u16) -> bool {
    // SAFETY: `GetAsyncKeyState` takes a key code by value and keeps no
    // pointer; an unknown code returns 0. Only the sign bit is read: the
    // low bit belongs to whichever reader asked last.
    unsafe { GetAsyncKeyState(i32::from(vk)) < 0 }
}

/// The modifiers held now, as the bits of a [`Key`].
fn modifiers() -> u32 {
    MODIFIERS
        .iter()
        .enumerate()
        .filter(|(_, vk)| down(**vk))
        .fold(0, |bits, (bit, _)| bits | 1 << (16 + bit))
}

/// Fills `out` with whether each control on `bound` is down: one of its
/// keys with exactly its modifiers, in the layout of the game window's
/// thread `window`. With no window, the game's window lacks the focus or a
/// window is open, and nothing is down or read.
pub(super) fn chords(bound: &[[u32; 2]], window: Option<u32>, out: &mut Vec<bool>) {
    out.clear();
    let Some(window) = window else {
        out.resize(bound.len(), false);
        return;
    };
    MAPPED.with_borrow_mut(|mapped| {
        let layout = layout(window);
        if mapped.layout != layout || mapped.bound != bound {
            mapped.layout = layout;
            mapped.bound.clear();
            mapped.bound.extend_from_slice(bound);
            mapped.vks.clear();
            mapped.vks.extend(bound.iter().map(|keys| {
                keys.map(|bits| {
                    if bits == 0 {
                        0
                    } else {
                        virtual_key(bits & 0x3ff, layout)
                    }
                })
            }));
        }
        let held = modifiers();
        // Each key is read once a frame, however many controls share it:
        // 0 not read yet, 1 up, 2 down.
        let mut seen = [0u8; 256];
        let mut is_down = |vk: u16| {
            let slot = &mut seen[usize::from(vk & 0xff)];
            if *slot == 0 {
                *slot = if down(vk) { 2 } else { 1 };
            }
            *slot == 2
        };
        out.extend(bound.iter().zip(&mapped.vks).map(|(keys, vks)| {
            keys.iter().zip(vks).any(|(&bits, &vk)| {
                vk != 0
                    && Key::from_bits(bits).is_some_and(|key| chord_down(key, is_down(vk), held))
            })
        }));
    });
}

/// The name of one key's scan code in the player's layout.
fn code_name(code: u32) -> String {
    if code & 0x200 != 0 {
        return format!("Mouse{}", (code & 0xff) + 1);
    }
    let lparam = ((code & 0xff) << 16) | if code & 0x100 != 0 { 1 << 24 } else { 0 };
    let mut name = [0u16; 64];
    // SAFETY: `name` is a live buffer of 64 UTF-16 units, its length passed
    // with it; the call writes at most that and returns how many it wrote.
    let len = unsafe {
        GetKeyNameTextW(
            lparam.cast_signed(),
            name.as_mut_ptr(),
            i32::try_from(name.len()).unwrap_or(0),
        )
    };
    match usize::try_from(len) {
        Ok(len) if len > 0 => String::from_utf16_lossy(&name[..len.min(name.len())]),
        _ => Key::from_bits(code).map_or_else(String::new, |k| k.to_string()),
    }
}

/// The name of the binding `bits` in the player's layout, such as
/// `Umschalt+E`; empty for none.
pub(crate) fn key_name(bits: u32) -> String {
    let Some(key) = Key::from_bits(bits) else {
        return String::new();
    };
    let mut parts: Vec<String> = MODIFIER_CODES
        .iter()
        .enumerate()
        .filter(|(bit, _)| key.modifiers() & (1 << (16 + bit)) != 0)
        .map(|(_, code)| code_name(*code))
        .collect();
    parts.push(code_name(key.code()));
    parts.join("+")
}

/// Every key a control may take, whether each is down, the modifiers held
/// and whether Escape is down: what the controls page's capture looks at.
pub(crate) fn capture_keys() -> (Vec<Key>, Vec<bool>, u32, bool) {
    let layout = layout(focused_thread().unwrap_or(0));
    let keys: Vec<Key> = Key::every().collect();
    let held = keys
        .iter()
        .map(|key| down(virtual_key(key.code(), layout)))
        .collect();
    (keys, held, modifiers(), down(VK_ESCAPE))
}
