//! The id of a game window's root node, made from the window's name as
//! `ui.window_begin` makes it, so the loader can bind an action to a window
//! of the game's that it did not open, such as the popup its dialogs draw;
//! and the bound on the names it takes.

use core::num::NonZeroU64;

use crate::base::fnv::{BASIS, fnv1a64};

/// The longest window name `ui.window_begin` takes, its suffix included:
/// it copies the name into a slice of 64 bytes with no terminator and stops
/// the game on a longer one (client 26430625, `0x140a3c460`).
pub(crate) const NAME_MAX: usize = 64;

/// The longest suffix `ui.window_begin` appends, `" [15]"`.
const SUFFIX_MAX: usize = 5;

/// The longest name the loader hands `ui.window_begin` as it is; a longer
/// one goes as its [`short_name`].
pub(crate) const NAME_FITS: usize = NAME_MAX - SUFFIX_MAX;

/// The prefix of a [`short_name`].
const SHORT_PREFIX: &[u8] = b"enshwick/";

/// The length of a [`short_name`]: the prefix and 16 hex digits.
pub(crate) const SHORT_LEN: usize = SHORT_PREFIX.len() + 16;

/// A window name of [`SHORT_LEN`] bytes standing for `name`, which is too
/// long for the game: the prefix and the hex of the name's FNV-1a 64, so
/// two windows keep two names.
pub(crate) fn short_name(name: &[u8]) -> [u8; SHORT_LEN] {
    let mut short = [0u8; SHORT_LEN];
    let (head, digits) = short.split_at_mut(SHORT_PREFIX.len());
    head.copy_from_slice(SHORT_PREFIX);
    let hash = fnv1a64(BASIS, name);
    for (i, digit) in digits.iter_mut().enumerate() {
        let nibble = (hash >> (60 - 4 * i)) & 0xf;
        *digit = b"0123456789abcdef"[nibble as usize];
    }
    short
}

/// The id of the root node of the window the game opens as `name` while
/// the view's slot is `slot`; `None` for a name too long for the game.
///
/// On client 26430625 `ui.window_begin` names the window `"<name> [n]"`,
/// `n` the slot's low four bits, while the slot is non-zero, else plain
/// `name`. It hashes that name with FNV-1a 64 over its bytes taken
/// unsigned, and the root node's id is FNV-1a 64 again over the name
/// hash's eight little-endian bytes (`0x14119d320`). Seen live on two
/// windows on 2026-11-17.
pub(crate) fn root_id(name: &[u8], slot: i32) -> Option<NonZeroU64> {
    let mut full = [0u8; NAME_MAX];
    let len = if slot == 0 {
        full.get_mut(..name.len())?.copy_from_slice(name);
        name.len()
    } else {
        let n = slot.to_le_bytes()[0] & 0xf;
        let suffix: &[u8] = if n < 10 {
            &[b' ', b'[', b'0' + n, b']']
        } else {
            &[b' ', b'[', b'1', b'0' + n - 10, b']']
        };
        let len = name.len() + suffix.len();
        let (head, tail) = full.get_mut(..len)?.split_at_mut(name.len());
        head.copy_from_slice(name);
        tail.copy_from_slice(suffix);
        len
    };
    let name_hash = fnv1a64(BASIS, &full[..len]);
    NonZeroU64::new(fnv1a64(BASIS, &name_hash.to_le_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(name: &str, slot: i32) -> u64 {
        root_id(name.as_bytes(), slot).map_or(0, NonZeroU64::get)
    }

    #[test]
    fn the_ids_match_the_two_windows_seen_live_on_client_26430625() {
        assert_eq!(id("findServerWindow", 0x10), 0xc046_5477_f433_27e7);
        assert_eq!(id("SystemMenu", 0x10), 0xc656_d38e_3f29_d170);
    }

    #[test]
    fn the_popup_window_s_id_follows_the_slot_s_low_four_bits() {
        assert_eq!(
            fnv1a64(BASIS, b"errorMessageWindow [0]"),
            0xb118_3730_8158_a476
        );
        assert_eq!(id("errorMessageWindow", 0x20), 0x4fe1_18b4_d96c_c59e);
        assert_eq!(id("errorMessageWindow", 0), 0x3085_7f42_f2ed_4d01);
        assert_eq!(
            id("errorMessageWindow", 0x1b),
            id("errorMessageWindow [11]", 0)
        );
        assert_eq!(
            id("errorMessageWindow", -1),
            id("errorMessageWindow [15]", 0)
        );
    }

    #[test]
    fn the_name_hash_takes_bytes_unsigned_unlike_the_text_hash() {
        let name = "вікно".as_bytes();
        let name_hash = fnv1a64(BASIS, name);
        assert_ne!(name_hash, crate::base::handed::hash(name));
        assert_eq!(
            root_id(name, 0).map(NonZeroU64::get),
            Some(fnv1a64(BASIS, &name_hash.to_le_bytes()))
        );
    }

    #[test]
    fn a_name_longer_than_the_game_takes_has_no_id() {
        let longest = [b'a'; NAME_MAX];
        assert!(root_id(&longest, 0).is_some());
        assert!(root_id(&longest, 1).is_none());
        assert!(root_id(&longest[..NAME_MAX - 4], 1).is_some());
        assert!(root_id(&longest[..NAME_MAX - 4], 10).is_none());
        assert!(root_id(&longest[..NAME_FITS], 15).is_some());
    }

    #[test]
    fn a_short_name_fits_every_slot_and_keeps_two_long_names_apart() {
        let first = short_name(&[b'a'; 200]);
        let second = short_name(&[b'b'; 200]);
        assert!(first.starts_with(b"enshwick/"));
        assert!(first.len() <= NAME_FITS);
        assert!((0..16).all(|slot| root_id(&first, slot).is_some()));
        assert_ne!(first, second);
        assert_eq!(short_name(b""), *b"enshwick/cbf29ce484222325");
    }
}
