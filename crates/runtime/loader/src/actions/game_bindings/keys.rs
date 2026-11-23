//! The keys of the game's runtime bindings in the loader's key form.
//!
//! The game names a key by its USB HID keyboard usage, the loader by its PC
//! set 1 scan code, with Shift, Ctrl and Alt in bits 16 to 18 (client
//! 26430625, the key name parser `0xe9ad20`; seen live on 2026-11-23).

/// One input operation of a binding, as the game lays it out.
///
/// Its kind sits at 0, a key's HID usage (`u16`) at 2 and a mouse button's
/// number at 4, the rest copied from the game's serialized gesture and axis
/// (client 26430625, the builders `0xe80c10` and `0xe80f80`; seen live).
pub type Operation = [u8; 12];

/// Where an operation keeps its kind, its key and its mouse button.
const KIND: usize = 0;
const KEY: usize = 2;
const BUTTON: usize = 4;

/// The kinds of `ControlOperationType` a key form names: no input, a key
/// and a mouse button (reflection of client 410).
const KIND_NONE: u8 = 0;
const KIND_KEY: u8 = 1;
const KIND_MOUSE: u8 = 2;

/// A slot that holds no key.
pub const NONE: u32 = 0;
/// A slot that holds input the key form does not name, the ABI's
/// `GameBinding::OTHER`.
pub const OTHER: u32 = enshwick_core::abi::GameBinding::OTHER;

/// The bit of an extended scan code, of a mouse button, and the code's own.
const EXTENDED: u32 = 0x100;
const MOUSE: u32 = 0x200;
const CODE: u32 = 0x3ff;

/// The modifier bits of the key form.
const SHIFT: u32 = 1 << 16;
const CTRL: u32 = 1 << 17;
const ALT: u32 = 1 << 18;

/// The modifiers by bit and name, with the HID usages of their left and
/// right keys, left first.
const MODIFIERS: [(u32, &str, [u16; 2]); 3] = [
    (SHIFT, "Shift", [0xe1, 0xe5]),
    (CTRL, "Ctrl", [0xe0, 0xe4]),
    (ALT, "Alt", [0xe2, 0xe6]),
];

/// The buttons a mouse operation may name, from 1 (left).
const BUTTONS: u8 = 5;

/// Each HID keyboard usage the key form names, with its set 1 scan code:
/// every key a mod control may take, then the modifiers alone, Escape and
/// the Windows keys, which a game control may hold and a mod's may not.
const KEYS: &[(u16, u32)] = &[
    (0x04, 0x1e),
    (0x05, 0x30),
    (0x06, 0x2e),
    (0x07, 0x20),
    (0x08, 0x12),
    (0x09, 0x21),
    (0x0a, 0x22),
    (0x0b, 0x23),
    (0x0c, 0x17),
    (0x0d, 0x24),
    (0x0e, 0x25),
    (0x0f, 0x26),
    (0x10, 0x32),
    (0x11, 0x31),
    (0x12, 0x18),
    (0x13, 0x19),
    (0x14, 0x10),
    (0x15, 0x13),
    (0x16, 0x1f),
    (0x17, 0x14),
    (0x18, 0x16),
    (0x19, 0x2f),
    (0x1a, 0x11),
    (0x1b, 0x2d),
    (0x1c, 0x15),
    (0x1d, 0x2c),
    (0x1e, 0x02),
    (0x1f, 0x03),
    (0x20, 0x04),
    (0x21, 0x05),
    (0x22, 0x06),
    (0x23, 0x07),
    (0x24, 0x08),
    (0x25, 0x09),
    (0x26, 0x0a),
    (0x27, 0x0b),
    (0x28, 0x1c),
    (0x2a, 0x0e),
    (0x2b, 0x0f),
    (0x2c, 0x39),
    (0x2d, 0x0c),
    (0x2e, 0x0d),
    (0x2f, 0x1a),
    (0x30, 0x1b),
    (0x31, 0x2b),
    (0x33, 0x27),
    (0x34, 0x28),
    (0x35, 0x29),
    (0x36, 0x33),
    (0x37, 0x34),
    (0x38, 0x35),
    (0x39, 0x3a),
    (0x3a, 0x3b),
    (0x3b, 0x3c),
    (0x3c, 0x3d),
    (0x3d, 0x3e),
    (0x3e, 0x3f),
    (0x3f, 0x40),
    (0x40, 0x41),
    (0x41, 0x42),
    (0x42, 0x43),
    (0x43, 0x44),
    (0x44, 0x57),
    (0x45, 0x58),
    (0x47, 0x46),
    (0x49, EXTENDED | 0x52),
    (0x4a, EXTENDED | 0x47),
    (0x4b, EXTENDED | 0x49),
    (0x4c, EXTENDED | 0x53),
    (0x4d, EXTENDED | 0x4f),
    (0x4e, EXTENDED | 0x51),
    (0x4f, EXTENDED | 0x4d),
    (0x50, EXTENDED | 0x4b),
    (0x51, EXTENDED | 0x50),
    (0x52, EXTENDED | 0x48),
    (0x54, EXTENDED | 0x35),
    (0x55, 0x37),
    (0x56, 0x4a),
    (0x57, 0x4e),
    (0x59, 0x4f),
    (0x5a, 0x50),
    (0x5b, 0x51),
    (0x5c, 0x4b),
    (0x5d, 0x4c),
    (0x5e, 0x4d),
    (0x5f, 0x47),
    (0x60, 0x48),
    (0x61, 0x49),
    (0x62, 0x52),
    (0x63, 0x53),
    (0x29, 0x01),
    (0xe0, 0x1d),
    (0xe1, 0x2a),
    (0xe2, 0x38),
    (0xe3, EXTENDED | 0x5b),
    (0xe4, EXTENDED | 0x1d),
    (0xe5, 0x36),
    (0xe6, EXTENDED | 0x38),
    (0xe7, EXTENDED | 0x5c),
];

/// The key form of one combination: `operation` with its modifiers.
///
/// The modifiers are those its `mask` names among the binding's
/// `modifiers` list, at most three as the game takes them. [`NONE`] stands
/// for no input and [`OTHER`] for input the key form does not name, such
/// as the wheel or a modifier that is no Shift, Ctrl or Alt.
///
/// # Examples
///
/// ```
/// use enshwick_loader::actions::game_bindings::keys::{OTHER, key_bits};
///
/// let e = [1, 0, 0x08, 0, 0, 0, 0, 0, 0, 0, 0, 0];
/// let shift = [1, 0, 0xe1, 0, 0, 0, 0, 0, 0, 0, 0, 0];
/// assert_eq!(key_bits(&e, 0, &[]), 0x12);
/// assert_eq!(key_bits(&e, 0b1, &[shift]), 0x12 | 1 << 16);
/// assert_eq!(key_bits(&e, 0b10, &[shift]), OTHER);
/// ```
#[must_use]
pub fn key_bits(operation: &Operation, mask: u32, modifiers: &[Operation]) -> u32 {
    let code = match operation[KIND] {
        KIND_NONE => return NONE,
        KIND_KEY => match code(usage(operation)) {
            Some(code) => code,
            None => return OTHER,
        },
        KIND_MOUSE => match operation[BUTTON] {
            button @ 1..=BUTTONS => MOUSE | u32::from(button - 1),
            _ => return OTHER,
        },
        _ => return OTHER,
    };
    let mut bits = code;
    let named = (0..u32::BITS).filter(|bit| mask & (1 << bit) != 0).take(3);
    for bit in named {
        let modifier = usize::try_from(bit).ok().and_then(|at| modifiers.get(at));
        match modifier.and_then(modifier_bit) {
            Some(modifier) => bits |= modifier,
            None => return OTHER,
        }
    }
    bits
}

/// Why a key does not go into the game's bindings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EncodeKeyError {
    /// The key form names no key the game names.
    #[error("the game's bindings name no key {0:#x}")]
    NoKey(u32),
    /// The binding's modifier list has no such modifier.
    #[error("the game's bindings list no {0} modifier")]
    NoModifier(&'static str),
}

/// The operation and modifier mask that bind `bits` in place of `was`,
/// whose gesture and axis it keeps, with the modifiers found in the
/// binding's `modifiers` list, a left key before a right one.
///
/// # Errors
///
/// [`EncodeKeyError`] for a key the game names no usage for, or a modifier
/// the list lacks.
///
/// # Examples
///
/// ```
/// use enshwick_loader::actions::game_bindings::keys::{key_bits, operation};
///
/// let space = [1, 0, 0x2c, 0, 0, 0, 0, 0, 0, 0, 7, 0];
/// let ctrl = [1, 0, 0xe0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
/// let (j, mask) = operation(0x24 | 1 << 17, &space, &[ctrl])?;
/// assert_eq!((j[2], j[10], mask), (0x0d, 7, 0b1));
/// assert_eq!(key_bits(&j, mask, &[ctrl]), 0x24 | 1 << 17);
/// # Ok::<(), enshwick_loader::actions::game_bindings::keys::EncodeKeyError>(())
/// ```
pub fn operation(
    bits: u32,
    was: &Operation,
    modifiers: &[Operation],
) -> Result<(Operation, u32), EncodeKeyError> {
    let mut op = *was;
    let code = bits & CODE;
    let (kind, key, button) = if code & MOUSE != 0 {
        let button = u8::try_from(code & 0xff)
            .ok()
            .filter(|b| *b < BUTTONS)
            .ok_or(EncodeKeyError::NoKey(code))?;
        (KIND_MOUSE, 0, button + 1)
    } else {
        let usage = KEYS
            .iter()
            .find(|&&(_, c)| c == code)
            .map(|&(usage, _)| usage)
            .ok_or(EncodeKeyError::NoKey(code))?;
        (KIND_KEY, usage, 0)
    };
    op[KIND] = kind;
    op[KEY..KEY + 2].copy_from_slice(&key.to_le_bytes());
    op[BUTTON] = button;
    let mut mask = 0;
    for (bit, name, usages) in MODIFIERS {
        if bits & bit == 0 {
            continue;
        }
        let at = usages
            .iter()
            .find_map(|&wanted| {
                modifiers
                    .iter()
                    .take(32)
                    .position(|m| m[KIND] == KIND_KEY && usage(m) == wanted)
            })
            .ok_or(EncodeKeyError::NoModifier(name))?;
        mask |= 1 << at;
    }
    Ok((op, mask))
}

/// The modifier bits of Shift, Ctrl and Alt `modifiers` lists, the ones a
/// rebind may take.
///
/// # Examples
///
/// ```
/// use enshwick_loader::actions::game_bindings::keys::listed;
///
/// let shift = [1, 0, 0xe5, 0, 0, 0, 0, 0, 0, 0, 0, 0];
/// assert_eq!(listed(&[shift]), 1 << 16);
/// ```
#[must_use]
pub fn listed(modifiers: &[Operation]) -> u32 {
    modifiers
        .iter()
        .take(32)
        .filter_map(modifier_bit)
        .fold(0, |bits, bit| bits | bit)
}

/// The key's usage of a key operation.
fn usage(operation: &Operation) -> u16 {
    u16::from_le_bytes([operation[KEY], operation[KEY + 1]])
}

/// The set 1 code of a HID usage the key form names.
fn code(usage: u16) -> Option<u32> {
    KEYS.iter()
        .find(|&&(u, _)| u == usage)
        .map(|&(_, code)| code)
}

/// The modifier bit a modifier list's operation stands for.
fn modifier_bit(operation: &Operation) -> Option<u32> {
    if operation[KIND] != KIND_KEY {
        return None;
    }
    let usage = usage(operation);
    MODIFIERS
        .iter()
        .find(|(_, _, usages)| usages.contains(&usage))
        .map(|&(bit, _, _)| bit)
}

#[cfg(test)]
mod tests {
    use enshwick_core::controls::Key;

    use super::*;

    fn op(kind: u8, usage: u16, button: u8) -> Operation {
        let [lo, hi] = usage.to_le_bytes();
        [kind, 0, lo, hi, button, 0, 0, 0, 0, 0, 0, 0]
    }

    #[test]
    fn every_key_a_control_may_take_has_a_usage_and_reads_back() {
        for key in Key::every().filter(|k| !k.is_mouse()) {
            let (written, mask) = operation(key.bits(), &op(1, 0x2c, 0), &[])
                .unwrap_or_else(|why| panic!("{key}: {why}"));
            assert_eq!(mask, 0);
            assert_eq!(key_bits(&written, mask, &[]), key.bits(), "{key}");
        }
    }

    #[test]
    fn the_live_preset_reads_as_its_keys() {
        // Client 26430625, seen live on 2026-11-23: Jump Space, Interact E,
        // Sprint LeftShift, the main hand the left mouse button.
        assert_eq!(
            key_bits(&op(1, 0x2c, 0), 0, &[]),
            Key::parse("Space").unwrap().bits()
        );
        assert_eq!(
            key_bits(&op(1, 0x08, 0), 0, &[]),
            Key::parse("E").unwrap().bits()
        );
        assert_eq!(
            key_bits(&op(1, 0xe1, 0), 0, &[]),
            0x2a,
            "Shift alone keeps its code"
        );
        assert_eq!(Key::from_bits(0x2a), None, "which no mod control takes");
        assert_eq!(
            key_bits(&op(2, 0, 1), 0, &[]),
            Key::parse("Mouse1").unwrap().bits()
        );
    }

    #[test]
    fn no_input_and_input_without_a_name_are_told_apart() {
        assert_eq!(key_bits(&op(0, 0x08, 0), 0, &[]), NONE);
        assert_eq!(key_bits(&op(3, 0, 0), 0, &[]), OTHER, "the wheel");
        assert_eq!(key_bits(&op(6, 0, 0), 0, &[]), OTHER, "a gamepad button");
        assert_eq!(key_bits(&op(1, 0x68, 0), 0, &[]), OTHER, "F13");
        assert_eq!(key_bits(&op(2, 0, 6), 0, &[]), OTHER);
    }

    #[test]
    fn a_mask_names_its_modifiers_and_one_past_the_list_reads_as_other() {
        let list = [op(1, 0xe0, 0), op(1, 0xe1, 0), op(6, 0, 0)];
        let e = op(1, 0x08, 0);
        assert_eq!(
            key_bits(&e, 0b11, &list),
            Key::parse("Ctrl+Shift+E").unwrap().bits()
        );
        assert_eq!(key_bits(&e, 0b100, &list), OTHER, "a gamepad modifier");
        assert_eq!(key_bits(&e, 1 << 9, &list), OTHER, "past the list");
    }

    #[test]
    fn a_rebind_keeps_the_gesture_takes_a_mouse_button_and_finds_its_modifiers() {
        let mut jump = op(1, 0x2c, 0);
        jump[10] = 3;
        jump[11] = 4;
        let list = [op(1, 0xe5, 0), op(1, 0xe2, 0)];
        let (mouse, mask) =
            operation(Key::parse("Alt+Mouse4").unwrap().bits(), &jump, &list).unwrap();
        assert_eq!((mouse[KIND], mouse[KEY], mouse[BUTTON]), (KIND_MOUSE, 0, 4));
        assert_eq!((mouse[10], mouse[11], mask), (3, 4, 0b10));
        let (back, mask) = operation(Key::parse("Shift+J").unwrap().bits(), &mouse, &list).unwrap();
        assert_eq!(
            (back[KIND], usage(&back), back[BUTTON], mask),
            (KIND_KEY, 0x0d, 0, 0b1)
        );
        assert_eq!(
            operation(Key::parse("Ctrl+J").unwrap().bits(), &jump, &list),
            Err(EncodeKeyError::NoModifier("Ctrl"))
        );
        assert_eq!(listed(&list), SHIFT | ALT);
    }

    #[test]
    fn every_usage_and_every_code_is_listed_once() {
        let mut usages: Vec<u16> = KEYS.iter().map(|&(u, _)| u).collect();
        let mut codes: Vec<u32> = KEYS.iter().map(|&(_, c)| c).collect();
        usages.sort_unstable();
        usages.dedup();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!((usages.len(), codes.len()), (KEYS.len(), KEYS.len()));
    }
}
