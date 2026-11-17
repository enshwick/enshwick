//! The id of a game window's root node, made from the window's name as
//! `ui.window_begin` makes it, so the loader can bind an action to a window
//! of the game's that it did not open, such as the popup its dialogs draw.

use core::num::NonZeroU64;

use crate::base::handed::hash;

/// The longest window name `ui.window_begin` takes, its suffix included:
/// it formats the name into 64 bytes and stops the game on a longer one
/// (client 26430625).
pub(crate) const NAME_MAX: usize = 64;

/// The id of the root node of the window the game opens as `name` while
/// the view's slot is `slot`; `None` for a name too long for the game.
///
/// On client 26430625 `ui.window_begin` names the window `"<name> [n]"`,
/// `n` the slot's low four bits, while the slot is non-zero, else plain
/// `name`. It hashes that name with its text hash, and the root node's id
/// is plain FNV-1a 64 over the name hash's eight little-endian bytes,
/// without the text hash's sign extension (`0x14119d320`). Seen live on two
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
    let id = hash(&full[..len])
        .to_le_bytes()
        .iter()
        .fold(FNV_BASIS, |id, &byte| {
            (id ^ u64::from(byte)).wrapping_mul(FNV_PRIME)
        });
    NonZeroU64::new(id)
}

const FNV_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x100_0000_01b3;

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
        assert_eq!(hash(b"errorMessageWindow [0]"), 0xb118_3730_8158_a476);
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
    fn a_name_longer_than_the_game_takes_has_no_id() {
        let longest = [b'a'; NAME_MAX];
        assert!(root_id(&longest, 0).is_some());
        assert!(root_id(&longest, 1).is_none());
        assert!(root_id(&longest[..NAME_MAX - 4], 1).is_some());
        assert!(root_id(&longest[..NAME_MAX - 4], 10).is_none());
    }
}
