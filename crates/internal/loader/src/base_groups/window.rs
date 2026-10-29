//! Where bases past the game's 16 live: base `n` of a grouping takes record
//! index `n` while it fits the game's array, and an index of the window
//! behind the array's tail after that.
//!
//! The game keeps its bases in a fixed array of 16 records with fields
//! behind it, and a building stock slot per base further on, where the
//! build table's [`Geometry`] puts them. An object grown to
//! [`Geometry::block`] bytes holds records and stock slots for indices
//! [`WINDOW`] to 255 past every vanilla field; indices 16 to 92 overlap
//! those fields and are never handed out. Base indices are a u8 wherever the
//! game keeps one. Server 399 and client 426.

/// The records the game's own array holds.
pub(crate) const VANILLA: usize = 16;
/// The first index of the window. Ours, not the game's: with the record and
/// stock slot strides of server 399 and client 426 it is the first index
/// whose record and stock slot lie behind every vanilla field, and a build
/// whose strides put it on a vanilla field keeps 16 bases
/// ([`Geometry::new`]).
pub(crate) const WINDOW: usize = 93;
/// One past the highest index, as a u8 holds it.
pub(crate) const INDICES: usize = 256;
/// The most bases the array and the window hold together.
pub(crate) const MOST: usize = VANILLA + INDICES - WINDOW;
/// The most bytes of a record the loader copies.
pub(crate) const MOST_RECORD: usize = 0x400; // bound, not a game offset: the loader's room
/// The most bytes of a stock slot: the server's create steps through them
/// with a signed byte.
const MOST_SLOT: usize = 0x7f; // bound, not a game offset: an imm8's positive range

/// Where a bases object keeps its records and its building stock slots, from
/// the build table, checked to put the window behind every vanilla field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Geometry {
    records: usize,
    record: usize,
    slots: usize,
    slot: usize,
    block: usize,
}

/// Why a build's geometry leaves the game its 16 bases.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum GeometryError {
    /// A record's stride is zero or past [`MOST_RECORD`].
    #[error("a record of {0:#x} bytes is out of range")]
    Record(usize),
    /// A stock slot's stride is zero or past an imm8.
    #[error("a stock slot of {0:#x} bytes is out of range")]
    Slot(usize),
    /// The array's records do not end before the first stock slot.
    #[error("the records run into the stock slots")]
    Records,
    /// The array's stock slots do not end inside the server's object.
    #[error("the stock slots run past the server's object")]
    Slots,
    /// Index [`WINDOW`]'s record or stock slot lies on a vanilla field or
    /// on another index's stock slot.
    #[error("index {WINDOW} overlaps the game's fields")]
    Overlap,
    /// The grown object does not fit a 32-bit size.
    #[error("the grown object does not fit a 32-bit size")]
    Block,
}

impl Geometry {
    /// The geometry of a first record at `records` and a first stock slot at
    /// `slots` with their strides, for vanilla objects of `server` and
    /// `client` bytes; refused unless every window index's record and stock
    /// slot lie behind both objects' fields and behind every stock slot.
    ///
    /// # Errors
    ///
    /// The [`GeometryError`] of the first check that fails.
    pub(crate) const fn new(
        (records, record): (usize, usize),
        (slots, slot): (usize, usize),
        (server, client): (usize, usize),
    ) -> Result<Self, GeometryError> {
        if record == 0 || record > MOST_RECORD {
            return Err(GeometryError::Record(record));
        }
        if slot == 0 || slot > MOST_SLOT {
            return Err(GeometryError::Slot(slot));
        }
        let (
            Some(array_end),
            Some(slots_end),
            Some(window_slot),
            Some(every_slot),
            Some(window_record),
            Some(block),
        ) = (
            span(records, record, VANILLA),
            span(slots, slot, VANILLA),
            span(slots, slot, WINDOW),
            span(slots, slot, INDICES),
            span(records, record, WINDOW),
            span(records, record, INDICES),
        )
        else {
            return Err(GeometryError::Block);
        };
        if array_end > slots {
            return Err(GeometryError::Records);
        }
        if slots_end > server {
            return Err(GeometryError::Slots);
        }
        let vanilla = if server > client { server } else { client };
        if window_slot < vanilla || window_record < every_slot {
            return Err(GeometryError::Overlap);
        }
        if block > u32::MAX as usize {
            return Err(GeometryError::Block);
        }
        Ok(Self {
            records,
            record,
            slots,
            slot,
            block,
        })
    }

    /// Where record `i` starts in the object.
    #[must_use]
    pub(crate) const fn record_at(&self, i: usize) -> usize {
        self.records + i * self.record
    }

    /// Where stock slot `i` starts in the object.
    #[must_use]
    pub(crate) const fn slot_at(&self, i: usize) -> usize {
        self.slots + i * self.slot
    }

    /// The bytes of a record.
    #[must_use]
    pub(crate) const fn record(&self) -> usize {
        self.record
    }

    /// The bytes of a stock slot.
    #[must_use]
    pub(crate) const fn slot(&self) -> usize {
        self.slot
    }

    /// Where the first stock slot starts: the constructor writes below it.
    #[must_use]
    pub(crate) const fn slots(&self) -> usize {
        self.slots
    }

    /// The size of a grown object that holds every index: one past record
    /// 255.
    #[must_use]
    pub(crate) const fn block(&self) -> usize {
        self.block
    }
}

/// One past `count` items of `stride` bytes from `first`; `None` on
/// overflow.
const fn span(first: usize, stride: usize, count: usize) -> Option<usize> {
    match stride.checked_mul(count) {
        Some(bytes) => first.checked_add(bytes),
        None => None,
    }
}

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

    /// Server 399's and client 426's objects: records from 0x30 of 0x130
    /// bytes, stock slots from 0x1610 of 0x58 bytes, 0x2070 and 0x16e0
    /// bytes vanilla.
    const RECORDS: (usize, usize) = (0x30, 0x130);
    const SLOTS: (usize, usize) = (0x1610, 0x58);
    const SIZES: (usize, usize) = (0x2070, 0x16e0);

    #[test]
    fn the_window_starts_behind_every_vanilla_field_and_ends_with_the_block() {
        let geometry = Geometry::new(RECORDS, SLOTS, SIZES).unwrap();
        assert!(geometry.slot_at(WINDOW) >= SIZES.0);
        assert!(geometry.record_at(WINDOW) >= geometry.slot_at(INDICES));
        assert!(geometry.record_at(WINDOW - 1) < geometry.slot_at(INDICES));
        assert_eq!(geometry.block(), 0x13030);
        assert_eq!(geometry.record_at(16), 0x1330);
        assert_eq!(geometry.slot_at(1), 0x1668);
    }

    #[test]
    fn strides_that_put_the_window_on_a_vanilla_field_keep_16_bases() {
        let narrow = (RECORDS.0, 0x120);
        assert_eq!(
            Geometry::new(narrow, SLOTS, SIZES),
            Err(GeometryError::Overlap)
        );
        let short = (SLOTS.0, 0x10);
        assert_eq!(
            Geometry::new(RECORDS, short, SIZES),
            Err(GeometryError::Overlap)
        );
    }

    #[test]
    fn a_stride_or_start_out_of_range_keeps_16_bases() {
        for record in [0, MOST_RECORD + 1] {
            assert_eq!(
                Geometry::new((RECORDS.0, record), SLOTS, SIZES),
                Err(GeometryError::Record(record))
            );
        }
        for slot in [0, 0x80, 0xffff_ffff_ffff_ffa8] {
            assert_eq!(
                Geometry::new(RECORDS, (SLOTS.0, slot), SIZES),
                Err(GeometryError::Slot(slot))
            );
        }
        assert_eq!(
            Geometry::new(RECORDS, SLOTS, (0x1b8f, SIZES.1)),
            Err(GeometryError::Slots)
        );
        assert_eq!(
            Geometry::new((0x400, RECORDS.1), SLOTS, SIZES),
            Err(GeometryError::Records)
        );
        assert_eq!(
            Geometry::new((usize::MAX - 0x100, RECORDS.1), SLOTS, SIZES),
            Err(GeometryError::Block)
        );
        let high = (0xffff_0000, SLOTS.1);
        let below = (high.0 - VANILLA * RECORDS.1, RECORDS.1);
        let server = high.0 + VANILLA * high.1;
        assert_eq!(
            Geometry::new(below, high, (server, 0)),
            Err(GeometryError::Block)
        );
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
