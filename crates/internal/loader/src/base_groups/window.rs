//! Where bases past the game's 16 live: base `n` of a grouping takes record
//! index `n` while it fits the game's array, and an index of the window
//! behind the array's tail after that.
//!
//! The game keeps its bases in a fixed array of 16 records of `0x130` bytes
//! with fields behind it, and a building stock slot of `0x58` bytes per base
//! further on. An object grown to [`BLOCK`] bytes holds records and stock
//! slots for indices [`WINDOW`] to 255 past every vanilla field; indices 16
//! to 92 overlap those fields and are never handed out. Base indices are a
//! u8 wherever the game keeps one. Server 399 and client 426.

/// The records the game's own array holds.
pub(crate) const VANILLA: usize = 16;
/// The first index of the window, whose record and stock slot lie behind
/// every vanilla field.
pub(crate) const WINDOW: usize = 93;
/// One past the highest index, as a u8 holds it.
pub(crate) const INDICES: usize = 256;
/// The most bases the array and the window hold together.
pub(crate) const MOST: usize = VANILLA + INDICES - WINDOW;

/// Base record `i`: the first one's offset in the bases object and the
/// stride.
pub(crate) const RECORDS: usize = 0x30;
pub(crate) const RECORD: usize = 0x130;
/// Stock slot `i` of the server's object: the first one's offset and the
/// stride.
pub(crate) const SLOTS: usize = 0x1610;
pub(crate) const SLOT: usize = 0x58;
/// The size of a bases object that holds every index: one past record 255.
pub(crate) const BLOCK: usize = RECORDS + INDICES * RECORD;

/// The records the game's teardown releases in one call, from the record it
/// is handed.
pub(crate) const TEARDOWN_SPAN: usize = 16;

/// The record index of base `n` of a grouping; `None` past [`MOST`].
#[must_use]
pub(crate) const fn index(n: usize) -> Option<usize> {
    if n < VANILLA {
        Some(n)
    } else if n < MOST {
        Some(n - VANILLA + WINDOW)
    } else {
        None
    }
}

/// Which base of a grouping record index `i` holds; `None` for an index
/// never handed out.
#[must_use]
pub(crate) const fn position(i: usize) -> Option<usize> {
    if i < VANILLA {
        Some(i)
    } else if i >= WINDOW && i < INDICES {
        Some(i - WINDOW + VANILLA)
    } else {
        None
    }
}

/// The record indices of a grouping of `bases` bases, in base order.
pub(crate) fn indices(bases: usize) -> impl Iterator<Item = usize> {
    (0..bases.min(MOST)).filter_map(index)
}

/// The window's indices among a grouping of `bases` bases.
pub(crate) fn window_indices(bases: usize) -> impl Iterator<Item = usize> {
    indices(bases).skip(VANILLA)
}

/// The first records of the teardown calls that release every window
/// record, ascending: each call covers [`TEARDOWN_SPAN`] records and the
/// last one ends at index 255.
pub(crate) fn teardown_starts() -> impl Iterator<Item = usize> {
    let last = INDICES - TEARDOWN_SPAN;
    (WINDOW..last)
        .step_by(TEARDOWN_SPAN)
        .chain(core::iter::once(last))
}

/// How many bases the client's base list shows for a grouping of `bases`
/// bases when the game's count reads `count`: up to the highest index in
/// use, so the list has an entry at every index a base holds.
#[must_use]
pub(crate) fn shown(count: usize, bases: usize) -> usize {
    indices(bases).last().map_or(count, |i| count.max(i + 1))
}

/// The side of the game a bases object serves; its kind byte tells them
/// apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// The client's own object, which groups the zones the server sends.
    Client,
    /// The server's object, which decides.
    Server,
}

/// How many bases a grouping on a `kind` object may hold: the server holds
/// what the mods asked for, `asked`; the client holds every base `most`
/// allows, so a client joined to a server always holds at least the
/// server's bases.
#[must_use]
pub(crate) fn cap(kind: Kind, asked: usize, most: usize) -> usize {
    match kind {
        Kind::Server => asked.min(most),
        Kind::Client => most,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_16_bases_keep_the_game_s_records_and_the_rest_take_the_window() {
        assert_eq!(index(0), Some(0));
        assert_eq!(index(15), Some(15));
        assert_eq!(index(16), Some(93));
        assert_eq!(index(MOST - 1), Some(255));
        assert_eq!(index(MOST), None);
        assert_eq!(MOST, 179);
        for n in 0..MOST {
            assert_eq!(index(n).and_then(position), Some(n), "base {n}");
        }
        for i in VANILLA..WINDOW {
            assert_eq!(position(i), None, "index {i} overlaps the game's fields");
        }
        assert_eq!(position(INDICES), None);
    }

    #[test]
    fn the_window_starts_behind_every_vanilla_field_and_ends_with_the_block() {
        let server_object = 0x2070;
        let last_slot_end = SLOTS + INDICES * SLOT;
        assert!(SLOTS + WINDOW * SLOT >= server_object);
        assert!(RECORDS + WINDOW * RECORD >= last_slot_end);
        assert!(RECORDS + (WINDOW - 1) * RECORD < last_slot_end);
        assert_eq!(BLOCK, 0x13030);
    }

    #[test]
    fn indices_follow_base_order_and_stop_at_the_most() {
        assert_eq!(indices(3).collect::<Vec<_>>(), [0, 1, 2]);
        let all: Vec<usize> = indices(500).collect();
        assert_eq!(all.len(), MOST);
        assert_eq!(all[16..18], [93, 94]);
        assert!(all.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(window_indices(16).count(), 0);
        assert_eq!(window_indices(18).collect::<Vec<_>>(), [93, 94]);
    }

    #[test]
    fn the_teardown_calls_cover_every_window_record_inside_the_block() {
        let starts: Vec<usize> = teardown_starts().collect();
        assert_eq!(
            starts,
            [93, 109, 125, 141, 157, 173, 189, 205, 221, 237, 240]
        );
        for i in WINDOW..INDICES {
            assert!(
                starts.iter().any(|&k| (k..k + TEARDOWN_SPAN).contains(&i)),
                "record {i}"
            );
        }
        assert!(starts.iter().all(|&k| k + TEARDOWN_SPAN <= INDICES));
    }

    #[test]
    fn the_base_list_reaches_the_highest_index_in_use() {
        assert_eq!(shown(0, 0), 0);
        assert_eq!(shown(3, 3), 3);
        assert_eq!(shown(16, 16), 16);
        assert_eq!(shown(16, 17), 94);
        assert_eq!(shown(16, MOST), INDICES);
    }

    #[test]
    fn the_server_holds_what_was_asked_and_the_client_every_base() {
        assert_eq!(cap(Kind::Server, 32, MOST), 32);
        assert_eq!(cap(Kind::Server, 500, MOST), MOST);
        assert_eq!(cap(Kind::Client, 32, MOST), MOST);
        assert_eq!(cap(Kind::Client, 32, VANILLA), VANILLA);
    }
}
