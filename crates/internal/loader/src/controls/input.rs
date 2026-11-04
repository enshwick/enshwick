//! The keyboard and mouse as Windows reports them, beside the game's own
//! Raw Input: scan codes turned into virtual keys for the current layout,
//! each key read once a frame, and key names in the player's layout.

use core::cell::RefCell;

use enshwick::controls::{Key, chord_down};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyNameTextW, GetKeyboardLayout, MAPVK_VSC_TO_VK_EX, MapVirtualKeyExW,
    VK_CONTROL, VK_ESCAPE, VK_LBUTTON, VK_MBUTTON, VK_MENU, VK_RBUTTON, VK_SHIFT, VK_XBUTTON1,
    VK_XBUTTON2,
};

/// The modifiers' virtual keys, in the order of their bits in a [`Key`].
const MODIFIERS: [u16; 3] = [VK_SHIFT, VK_CONTROL, VK_MENU];
/// The modifiers' scan codes, for their names in the player's layout.
const MODIFIER_CODES: [u32; 3] = [0x2a, 0x1d, 0x38];
/// The mouse buttons' virtual keys, by their number in a [`Key`].
const MOUSE: [u16; 5] = [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON, VK_XBUTTON1, VK_XBUTTON2];

/// The bit of a key that went down since the last read of it.
const TAPPED: u8 = 1;
/// The bit of a key that is down.
const DOWN: u8 = 2;

thread_local! {
    /// The layout the virtual keys were made for, and each scan code's
    /// virtual key, made on a layout change.
    static LAYOUT: RefCell<(isize, Vec<(u32, u16)>)> = const { RefCell::new((0, Vec::new())) };
}

/// Whether the game's window has the focus.
pub(crate) fn focused() -> bool {
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };
    // SAFETY: `GetForegroundWindow` takes no arguments and may return null,
    // checked below.
    let window = unsafe { GetForegroundWindow() };
    if window.is_null() {
        return false;
    }
    let mut process = 0u32;
    // SAFETY: `window` is a non-null handle, checked by the guard before it,
    // and `process` is a live `u32` the call writes; a handle that closed
    // meanwhile only fails the call, leaving `process` 0.
    unsafe { GetWindowThreadProcessId(window, &raw mut process) };
    // SAFETY: `GetCurrentProcessId` takes no arguments and cannot fail.
    process == unsafe { GetCurrentProcessId() }
}

/// The virtual key of the key with scan code `code` (bit 8 extended, bit 9
/// a mouse button) in the current layout; 0 for none.
fn virtual_key(code: u32) -> u16 {
    if code & 0x200 != 0 {
        return MOUSE.get((code & 0xff) as usize).copied().unwrap_or(0);
    }
    // SAFETY: `GetKeyboardLayout` takes a thread id by value; 0 is this
    // thread.
    let layout = unsafe { GetKeyboardLayout(0) };
    LAYOUT.with_borrow_mut(|(made_for, keys)| {
        if *made_for != layout as isize {
            *made_for = layout as isize;
            keys.clear();
        }
        if let Some((_, vk)) = keys.iter().find(|(c, _)| *c == code) {
            return *vk;
        }
        let scan = if code & 0x100 != 0 {
            0xe000 | (code & 0xff)
        } else {
            code & 0xff
        };
        // SAFETY: `MapVirtualKeyExW` takes its arguments by value and
        // returns 0 for a code without a key.
        let vk = unsafe { MapVirtualKeyExW(scan, MAPVK_VSC_TO_VK_EX, layout) };
        let vk = u16::try_from(vk).unwrap_or(0);
        keys.push((code, vk));
        vk
    })
}

/// What Windows says of virtual key `vk`: [`DOWN`] and [`TAPPED`] bits.
fn state(vk: u16) -> u8 {
    // SAFETY: `GetAsyncKeyState` takes a key code by value and keeps no
    // pointer; an unknown code returns 0.
    let raw = unsafe { GetAsyncKeyState(i32::from(vk)) };
    let mut bits = 0;
    if raw < 0 {
        bits |= DOWN;
    }
    if raw & 1 != 0 {
        bits |= TAPPED;
    }
    bits
}

/// The keys of one frame, each read once.
pub(super) struct States {
    keys: [u8; 256],
    modifiers: u32,
}

/// Reads every key in `bound` and the modifiers, once: also while the
/// controls do not fire, so a key typed in the chat does not count as a tap
/// once it closes.
pub(super) fn read(bound: &[[u32; 2]]) -> States {
    let mut keys = [0u8; 256];
    for &bits in bound.iter().flatten().filter(|&&b| b != 0) {
        let vk = virtual_key(bits & 0x3ff);
        if let Some(slot) = keys.get_mut(usize::from(vk))
            && vk != 0
            && *slot == 0
        {
            *slot = state(vk) | 0x80;
        }
    }
    let mut modifiers = 0;
    for (bit, vk) in MODIFIERS.iter().enumerate() {
        if state(*vk) & DOWN != 0 {
            modifiers |= 1 << (16 + bit);
        }
    }
    States { keys, modifiers }
}

impl States {
    /// Whether a control on `bound` is down now, and whether one of its
    /// keys was tapped since the last frame, each with exactly its
    /// modifiers.
    pub(super) fn chord(&self, bound: [u32; 2]) -> (bool, bool) {
        let mut down = false;
        let mut tapped = false;
        for key in bound.iter().filter_map(|&bits| Key::from_bits(bits)) {
            let vk = virtual_key(key.code());
            let state = self.keys.get(usize::from(vk)).copied().unwrap_or(0);
            down |= chord_down(key, state & DOWN != 0, self.modifiers);
            tapped |= chord_down(key, state & TAPPED != 0, self.modifiers);
        }
        (down, tapped)
    }
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
    let keys: Vec<Key> = Key::every().collect();
    let down = keys
        .iter()
        .map(|key| state(virtual_key(key.code())) & DOWN != 0)
        .collect();
    let modifiers = MODIFIERS
        .iter()
        .enumerate()
        .filter(|(_, vk)| state(**vk) & DOWN != 0)
        .fold(0, |bits, (bit, _)| bits | 1 << (16 + bit));
    (keys, down, modifiers, state(VK_ESCAPE) & DOWN != 0)
}
