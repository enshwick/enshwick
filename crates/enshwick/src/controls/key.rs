//! Keys as scan codes, so a binding stays under the same finger on any
//! keyboard layout, with their US names for code and files.

use core::fmt;

/// A key or mouse button and the modifiers that must be down with it.
///
/// The key is its scan code, the key's place on the keyboard, not the letter
/// a layout prints on it: `Key::parse("Q")` is the key left of W, also on
/// an AZERTY keyboard. Names are those of a US keyboard; the controls page
/// shows each player the name their own layout gives the key. Mouse buttons
/// are `Mouse1` (left) to `Mouse5`.
///
/// # Examples
///
/// ```
/// use enshwick::controls::Key;
///
/// let key = Key::parse("Shift+E")?;
/// assert_eq!(key.to_string(), "Shift+E");
/// assert_eq!(Key::parse("ctrl + f5")?, Key::parse("Ctrl+F5")?);
/// assert!(Key::parse("Escape").is_err());
/// # Ok::<(), enshwick::controls::ParseKeyError>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Key(u32);

/// Why a text names no key a control may take.
///
/// # Examples
///
/// See [`Key`].
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ParseKeyError {
    /// No key has this name.
    #[error("no key is named {0}")]
    Unknown(String),
    /// The key belongs to the game or the system: Escape, the Windows keys,
    /// Alt+F4, or a modifier alone.
    #[error("{0} cannot be bound")]
    Protected(String),
}

impl From<ParseKeyError> for crate::Off {
    fn from(why: ParseKeyError) -> Self {
        Self::new(why.to_string())
    }
}

/// The bit of a scan code a key with the extended prefix carries.
const EXTENDED: u32 = 0x100;
/// The bit of a mouse button, whose number is in the low bits.
const MOUSE: u32 = 0x200;
/// The bits of the code itself.
const CODE: u32 = 0x3ff;

/// The modifiers, by name, and their bits.
const MODIFIERS: [(&str, u32); 3] = [("Shift", 1 << 16), ("Ctrl", 1 << 17), ("Alt", 1 << 18)];

/// Every key a control may take, by its US name, from the scan codes of
/// the PC keyboard's set 1 (as `MapVirtualKeyW` takes them).
const NAMES: &[(&str, u32)] = &[
    ("1", 0x02),
    ("2", 0x03),
    ("3", 0x04),
    ("4", 0x05),
    ("5", 0x06),
    ("6", 0x07),
    ("7", 0x08),
    ("8", 0x09),
    ("9", 0x0a),
    ("0", 0x0b),
    ("Minus", 0x0c),
    ("Equals", 0x0d),
    ("Backspace", 0x0e),
    ("Tab", 0x0f),
    ("Q", 0x10),
    ("W", 0x11),
    ("E", 0x12),
    ("R", 0x13),
    ("T", 0x14),
    ("Y", 0x15),
    ("U", 0x16),
    ("I", 0x17),
    ("O", 0x18),
    ("P", 0x19),
    ("LeftBracket", 0x1a),
    ("RightBracket", 0x1b),
    ("Enter", 0x1c),
    ("A", 0x1e),
    ("S", 0x1f),
    ("D", 0x20),
    ("F", 0x21),
    ("G", 0x22),
    ("H", 0x23),
    ("J", 0x24),
    ("K", 0x25),
    ("L", 0x26),
    ("Semicolon", 0x27),
    ("Quote", 0x28),
    ("Backquote", 0x29),
    ("Backslash", 0x2b),
    ("Z", 0x2c),
    ("X", 0x2d),
    ("C", 0x2e),
    ("V", 0x2f),
    ("B", 0x30),
    ("N", 0x31),
    ("M", 0x32),
    ("Comma", 0x33),
    ("Period", 0x34),
    ("Slash", 0x35),
    ("NumpadMultiply", 0x37),
    ("Space", 0x39),
    ("CapsLock", 0x3a),
    ("F1", 0x3b),
    ("F2", 0x3c),
    ("F3", 0x3d),
    ("F4", 0x3e),
    ("F5", 0x3f),
    ("F6", 0x40),
    ("F7", 0x41),
    ("F8", 0x42),
    ("F9", 0x43),
    ("F10", 0x44),
    ("ScrollLock", 0x46),
    ("Numpad7", 0x47),
    ("Numpad8", 0x48),
    ("Numpad9", 0x49),
    ("NumpadMinus", 0x4a),
    ("Numpad4", 0x4b),
    ("Numpad5", 0x4c),
    ("Numpad6", 0x4d),
    ("NumpadPlus", 0x4e),
    ("Numpad1", 0x4f),
    ("Numpad2", 0x50),
    ("Numpad3", 0x51),
    ("Numpad0", 0x52),
    ("NumpadPeriod", 0x53),
    ("F11", 0x57),
    ("F12", 0x58),
    ("NumpadEnter", EXTENDED | 0x1c),
    ("NumpadDivide", EXTENDED | 0x35),
    ("Home", EXTENDED | 0x47),
    ("Up", EXTENDED | 0x48),
    ("PageUp", EXTENDED | 0x49),
    ("Left", EXTENDED | 0x4b),
    ("Right", EXTENDED | 0x4d),
    ("End", EXTENDED | 0x4f),
    ("Down", EXTENDED | 0x50),
    ("PageDown", EXTENDED | 0x51),
    ("Insert", EXTENDED | 0x52),
    ("Delete", EXTENDED | 0x53),
    ("Mouse1", MOUSE),
    ("Mouse2", MOUSE | 1),
    ("Mouse3", MOUSE | 2),
    ("Mouse4", MOUSE | 3),
    ("Mouse5", MOUSE | 4),
];

/// Keys that belong to the game or the system, by name: never a binding.
const PROTECTED: &[&str] = &["Escape", "Win", "LeftWin", "RightWin"];

/// The scan code of F4 and the bit of Alt, for Alt+F4.
const ALT_F4: u32 = 0x3e | (1 << 18);

impl Key {
    /// The key named `text`: modifiers and a key joined by `+`, such as
    /// `Shift+E` or `Mouse4`, in any case and with any spaces around `+`.
    ///
    /// # Errors
    ///
    /// [`ParseKeyError`] for a name no key has, or a key that cannot be
    /// bound: Escape, the Windows keys, Alt+F4 or a modifier alone.
    ///
    /// # Examples
    ///
    /// See [`Key`].
    pub fn parse(text: &str) -> Result<Self, ParseKeyError> {
        let mut bits = 0;
        let mut code = None;
        for part in text.split('+').map(str::trim) {
            if let Some((_, bit)) = MODIFIERS.iter().find(|(n, _)| n.eq_ignore_ascii_case(part)) {
                bits |= bit;
            } else if PROTECTED.iter().any(|n| n.eq_ignore_ascii_case(part)) {
                return Err(ParseKeyError::Protected(text.to_owned()));
            } else if let Some((_, found)) =
                NAMES.iter().find(|(n, _)| n.eq_ignore_ascii_case(part))
                && code.is_none()
            {
                code = Some(*found);
            } else {
                return Err(ParseKeyError::Unknown(text.to_owned()));
            }
        }
        let Some(code) = code else {
            return Err(ParseKeyError::Protected(text.to_owned()));
        };
        Self::from_bits(bits | code).ok_or_else(|| ParseKeyError::Protected(text.to_owned()))
    }

    /// The key a binding's bits hold, as [`Key::bits`] gives them; `None`
    /// for 0, unknown bits or a key that cannot be bound.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::controls::Key;
    ///
    /// let key = Key::parse("Alt+G")?;
    /// assert_eq!(Key::from_bits(key.bits()), Some(key));
    /// assert_eq!(Key::from_bits(0), None);
    /// # Ok::<(), enshwick::controls::ParseKeyError>(())
    /// ```
    #[must_use]
    pub fn from_bits(bits: u32) -> Option<Self> {
        let modifiers: u32 = MODIFIERS.iter().map(|(_, bit)| bit).sum();
        let known = bits & !(CODE | modifiers) == 0
            && NAMES.iter().any(|(_, code)| *code == bits & CODE)
            && bits != ALT_F4;
        known.then_some(Self(bits))
    }

    /// The binding's bits: the scan code in bits 0 to 9 (bit 8 for an
    /// extended key, bit 9 for a mouse button) and Shift, Ctrl and Alt in
    /// bits 16 to 18.
    ///
    /// # Examples
    ///
    /// See [`Key::from_bits`].
    #[must_use]
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// The scan code, without the modifiers.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::controls::Key;
    ///
    /// assert_eq!(Key::parse("Shift+E")?.code(), Key::parse("E")?.code());
    /// # Ok::<(), enshwick::controls::ParseKeyError>(())
    /// ```
    #[must_use]
    pub const fn code(self) -> u32 {
        self.0 & CODE
    }

    /// The modifiers' bits alone.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::controls::Key;
    ///
    /// assert_eq!(Key::parse("E")?.modifiers(), 0);
    /// assert_ne!(Key::parse("Ctrl+E")?.modifiers(), 0);
    /// # Ok::<(), enshwick::controls::ParseKeyError>(())
    /// ```
    #[must_use]
    pub const fn modifiers(self) -> u32 {
        self.0 & !CODE
    }

    /// Every key a control may take, without modifiers, for the loader's
    /// key capture.
    #[doc(hidden)]
    pub fn every() -> impl Iterator<Item = Self> {
        NAMES.iter().map(|(_, code)| Self(*code))
    }

    /// Whether the key is a mouse button.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::controls::Key;
    ///
    /// assert!(Key::parse("Mouse4")?.is_mouse());
    /// # Ok::<(), enshwick::controls::ParseKeyError>(())
    /// ```
    #[must_use]
    pub const fn is_mouse(self) -> bool {
        self.0 & MOUSE != 0
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (name, bit) in MODIFIERS {
            if self.0 & bit != 0 {
                write!(f, "{name}+")?;
            }
        }
        let name = NAMES
            .iter()
            .find(|(_, code)| *code == self.code())
            .map_or("?", |(name, _)| name);
        f.write_str(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_named_key_reads_back_from_its_name_and_its_bits() {
        for (name, code) in NAMES {
            let key = Key::parse(name).unwrap();
            assert_eq!(key.bits(), *code);
            assert_eq!(key.to_string(), *name);
            assert_eq!(Key::from_bits(key.bits()), Some(key));
        }
        let mut codes: Vec<u32> = NAMES.iter().map(|(_, c)| *c).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), NAMES.len());
    }

    #[test]
    fn protected_keys_and_lone_modifiers_are_refused() {
        for text in [
            "Escape",
            "Alt+F4",
            "Shift",
            "Ctrl+Alt",
            "LeftWin",
            "Shift+Win",
        ] {
            assert!(
                matches!(Key::parse(text), Err(ParseKeyError::Protected(_))),
                "{text}"
            );
        }
        assert!(matches!(Key::parse("E+R"), Err(ParseKeyError::Unknown(_))));
        assert!(matches!(
            Key::parse("Hyper"),
            Err(ParseKeyError::Unknown(_))
        ));
        assert_eq!(Key::from_bits(ALT_F4), None);
        assert_eq!(Key::from_bits(1 << 20 | 0x12), None);
        assert!(Key::parse("Ctrl+F4").is_ok());
    }

    #[test]
    fn extended_keys_and_mouse_buttons_carry_their_set_1_codes() {
        for (name, code) in [
            ("NumpadEnter", 0x11c),
            ("NumpadDivide", 0x135),
            ("Home", 0x147),
            ("Up", 0x148),
            ("PageUp", 0x149),
            ("Left", 0x14b),
            ("Right", 0x14d),
            ("End", 0x14f),
            ("Down", 0x150),
            ("PageDown", 0x151),
            ("Insert", 0x152),
            ("Delete", 0x153),
            ("Mouse1", 0x200),
            ("Mouse2", 0x201),
            ("Mouse3", 0x202),
            ("Mouse4", 0x203),
            ("Mouse5", 0x204),
            ("E", 0x12),
            ("F12", 0x58),
        ] {
            assert_eq!(Key::parse(name).unwrap().bits(), code, "{name}");
        }
        assert!(Key::parse("Mouse4").unwrap().is_mouse());
        assert!(!Key::parse("Delete").unwrap().is_mouse());
        assert!(!Key::parse("E").unwrap().is_mouse());
    }

    #[test]
    fn a_code_without_a_name_is_no_key_and_every_named_key_is_listed() {
        assert_eq!(Key::from_bits(0x5f), None);
        assert_eq!(Key::from_bits(0x205), None);
        let every: Vec<Key> = Key::every().collect();
        assert_eq!(every.len(), NAMES.len());
        assert!(every.contains(&Key::parse("Mouse5").unwrap()));
        assert!(every.iter().all(|k| k.modifiers() == 0));
    }

    #[test]
    fn a_modifier_named_twice_counts_once() {
        assert_eq!(Key::parse("Shift+Shift+E"), Key::parse("Shift+E"));
    }

    #[test]
    fn modifiers_print_in_one_order_whatever_order_they_came_in() {
        let key = Key::parse("alt + shift + ctrl + mouse5").unwrap();
        assert_eq!(key.to_string(), "Shift+Ctrl+Alt+Mouse5");
        assert!(key.is_mouse());
    }
}
