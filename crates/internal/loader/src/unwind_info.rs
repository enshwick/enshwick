//! Shared reading of x64 `UNWIND_INFO`: the function-table entry layout, the
//! size of an unwind code, the walk over chained entries, and where a moved
//! prologue instruction's unwind code counts from in a trampoline. Used by
//! the crash walk ([`crate::crash`]) and the detour engine
//! ([`crate::detour`]).
//!
//! The layout and the codes are Microsoft's x64 exception handling
//! (`learn.microsoft.com/cpp/build/exception-handling-x64`).

/// Where an `UNWIND_INFO`'s codes start, each a slot of two bytes.
#[cfg(windows)]
pub(crate) const CODES: usize = 4; // bound, not a game offset: a fact of the PE format
/// One unwind code slot.
#[cfg(windows)]
pub(crate) const SLOT: usize = 2; // bound, not a game offset: a fact of the PE format
/// `UNW_FLAG_CHAININFO`, this info continues a previous entry.
#[cfg(windows)]
pub(crate) const CHAINED: u8 = 4; // bound, not a game offset: a fact of the PE format

/// An entry of a function table for `RtlAddFunctionTable` or
/// `RtlInstallFunctionTableCallback`: a function's start, end and unwind
/// info, each an offset from the table's base.
#[cfg(windows)]
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct RuntimeFunction {
    pub(crate) begin: u32,
    pub(crate) end: u32,
    pub(crate) unwind: u32,
}

/// The slots an unwind code takes, by its operation `op` (the low nibble of
/// the code's second byte) and its `info` (the high nibble); `None` for a
/// code this does not know.
///
/// `UWOP_EPILOG` (6) takes two slots and `UWOP_SPARE` (7) three; version 1
/// emits neither, but the crash walk accepts them as the old walk did.
pub(crate) fn slots(op: u8, info: u8) -> Option<usize> {
    Some(match op {
        // PUSH_NONVOL, ALLOC_SMALL, SET_FPREG, UWOP_EPILOG, PUSH_MACHFRAME
        0 | 2 | 3 | 6 | 10 => 1 + usize::from(op == 6),
        // ALLOC_LARGE: one extra slot for a scaled size, two for an unscaled
        1 if info == 0 => 2,
        // ALLOC_LARGE unscaled, SAVE_NONVOL_FAR, SAVE_XMM128_FAR, UWOP_SPARE
        1 | 5 | 7 | 9 => 3,
        // SAVE_NONVOL, SAVE_XMM128
        4 | 8 => 2,
        _ => return None,
    })
}

/// Whether the unwind operation `op` describes a prologue effect on the
/// stack, as against an epilogue (`UWOP_EPILOG`) or spare (`UWOP_SPARE`)
/// code that a version 2 info may carry.
pub(crate) fn prologue_code(op: u8) -> bool {
    !matches!(op, 6 | 7)
}

/// The offset in a trampoline entry from which a moved prologue instruction
/// ending at `end` (an address in the hooked function) counts as run: zero
/// for one before the hooked bytes start at `target`, else its place among
/// the moved bytes after the relay of `relay` bytes. The moved bytes keep a
/// function's prologue byte for byte, since a prologue holds no branch the
/// encoder would rewrite, so the offset carries straight over.
pub(crate) fn prologue_offset(relay: usize, target: usize, stolen: usize, end: usize) -> u8 {
    if end <= target {
        return 0;
    }
    if end <= target + stolen
        && let Ok(offset) = u8::try_from(relay + (end - target))
    {
        return offset;
    }
    u8::MAX
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_takes_the_slots_the_table_gives_it() {
        assert_eq!(slots(0, 3), Some(1)); // push rbx
        assert_eq!(slots(2, 4), Some(1)); // alloc small
        assert_eq!(slots(1, 0), Some(2)); // alloc large, scaled
        assert_eq!(slots(1, 1), Some(3)); // alloc large, unscaled
        assert_eq!(slots(4, 3), Some(2)); // save nonvol
        assert_eq!(slots(5, 3), Some(3)); // save nonvol far
        assert_eq!(slots(6, 0), Some(2)); // epilogue
        assert_eq!(slots(7, 0), Some(3)); // spare
        assert_eq!(slots(8, 6), Some(2)); // save xmm128
        assert_eq!(slots(10, 0), Some(1)); // push machframe
        assert_eq!(slots(11, 0), None); // unknown
    }

    #[test]
    fn only_prologue_codes_describe_the_stack() {
        for op in [0, 1, 2, 3, 4, 5, 8, 9, 10] {
            assert!(prologue_code(op));
        }
        assert!(!prologue_code(6));
        assert!(!prologue_code(7));
    }

    #[test]
    fn a_prologue_code_counts_from_the_moved_byte_it_ends_at() {
        // A 14-byte relay before 6 stolen bytes.
        assert_eq!(prologue_offset(14, 0x1000, 6, 0x1000), 0); // at the start
        assert_eq!(prologue_offset(14, 0x1000, 6, 0x0fff), 0); // before it
        assert_eq!(prologue_offset(14, 0x1000, 6, 0x1004), 18); // 14 + 4
        assert_eq!(prologue_offset(14, 0x1000, 6, 0x1006), 20); // 14 + 6, the end
        assert_eq!(prologue_offset(14, 0x1000, 6, 0x1007), u8::MAX); // past it
    }
}
