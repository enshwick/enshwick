//! Shared reading of x64 `UNWIND_INFO` for the crash walk ([`crate::crash`])
//! and the detour engine ([`crate::detour`]): the function-table entry
//! layout, the size of an unwind code, the link to a chained parent, which
//! trampolines may carry their function's codes, and where a moved prologue
//! instruction's code counts from in one.
//!
//! The layout and the codes are Microsoft's x64 exception handling
//! (`learn.microsoft.com/cpp/build/exception-handling-x64`).

/// Where an `UNWIND_INFO`'s codes start, each a slot of two bytes.
pub(crate) const CODES: usize = 4; // bound, not a game offset: a fact of the PE format
/// One unwind code slot.
pub(crate) const SLOT: usize = 2; // bound, not a game offset: a fact of the PE format
/// `UNW_FLAG_CHAININFO`, this info continues a previous entry.
pub(crate) const CHAINED: u8 = 4; // bound, not a game offset: a fact of the PE format
/// The bytes a [`RuntimeFunction`] takes, as a chained info's parent entry.
pub(crate) const TABLE_ENTRY: usize = size_of::<RuntimeFunction>();
/// Where a chained info's parent entry holds the parent's unwind info.
pub(crate) const UNWIND_OF: usize = core::mem::offset_of!(RuntimeFunction, unwind);
/// The most chained entries a walk follows, so a looping chain ends.
pub(crate) const CHAIN: usize = 32;

/// An entry of a function table for `RtlAddFunctionTable` or
/// `RtlInstallFunctionTableCallback`: a function's start, end and unwind
/// info, each an offset from the table's base.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct RuntimeFunction {
    pub(crate) begin: u32,
    pub(crate) end: u32,
    pub(crate) unwind: u32,
}

/// The bytes of the `UNWIND_INFO` whose header is `head`: the header, the
/// codes padded to an even count, and the parent entry of a chained info;
/// a handler and its data are not counted.
pub(crate) fn info_len(head: [u8; 4]) -> usize {
    let chained = (head[0] >> 3) & CHAINED != 0;
    CODES + usize::from(head[2]).next_multiple_of(2) * SLOT + if chained { TABLE_ENTRY } else { 0 }
}

/// The unwind info, as an offset from the image, that the chained info
/// `info` continues in; `None` for a primary info or one cut short.
pub(crate) fn parent(info: &[u8]) -> Option<u32> {
    let head: [u8; 4] = info.get(..CODES)?.try_into().ok()?;
    if (head[0] >> 3) & CHAINED == 0 {
        return None;
    }
    let at = info_len(head) - TABLE_ENTRY + UNWIND_OF;
    Some(u32::from_le_bytes(info.get(at..at + 4)?.try_into().ok()?))
}

/// Whether a hook's trampoline may carry its function's unwind codes: only
/// a hook at the start of its function's entry (`at_start`), whose moved
/// bytes run the prologue as the function does, and only when none of them
/// past the prologue's `prolog` bytes moves the stack pointer
/// (`stack_moves`, each such moved instruction's offset from the start).
///
/// The codes describe the prologue alone. Past it they hold for the body,
/// where the function moves the stack pointer only in an epilogue or, with
/// a frame pointer, a dynamic allocation; inside the function a hook stops
/// where no code's offset says what has run. Such a trampoline gets no
/// entry and unwinds as a leaf.
pub(crate) fn copies_frame(at_start: bool, prolog: u8, stack_moves: &[usize]) -> bool {
    at_start && stack_moves.iter().all(|&at| at < usize::from(prolog))
}

/// The slots an unwind code takes, by its operation `op` (the low nibble of
/// the code's second byte) and its `info` (the high nibble); `None` for a
/// code this does not know.
///
/// `UWOP_EPILOG` (6) takes two slots and `UWOP_SPARE` (7) three; version 1
/// emits neither. The detour copier skips both as non-prologue; the crash
/// walk, where a spare code means a corrupt info, stops on op 7 itself.
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
/// for one before the hooked bytes start at `target`, its place among the
/// moved bytes after the relay of `relay` bytes for one inside them, and
/// never (255) for one after, which runs only once control jumps back;
/// `None` for one that ends inside a moved instruction, which no offset in
/// the trampoline describes, and for one past what a byte holds.
///
/// `moved` pairs each moved instruction's start offset from `target` with
/// its start offset in the trampoline, and `moved_end` is where the moved
/// bytes end there. The trampoline offset is read from these, not computed
/// from the function, so an instruction the encoder re-encoded longer (a far
/// `__chkstk` call, a widened branch) shifts the codes after it.
pub(crate) fn prologue_offset(
    relay: usize,
    target: usize,
    stolen: usize,
    moved: &[(usize, usize)],
    moved_end: usize,
    end: usize,
) -> Option<u8> {
    if end <= target {
        return Some(0);
    }
    let off = end - target;
    if off > stolen {
        return Some(u8::MAX);
    }
    // The code ends where the next moved instruction starts, or, for the
    // last, at the end of the moved bytes.
    let tramp = if off == stolen {
        moved_end
    } else {
        moved.iter().find(|&&(old, _)| old == off)?.1
    };
    u8::try_from(relay + tramp).ok()
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
        // Three moved instructions of 1, 1 and 4 bytes, laid out byte for
        // byte in the trampoline after a 14-byte relay: 6 stolen bytes.
        let moved = [(0, 0), (1, 1), (2, 2)];
        assert_eq!(prologue_offset(14, 0x1000, 6, &moved, 6, 0x1000), Some(0)); // at the start
        assert_eq!(prologue_offset(14, 0x1000, 6, &moved, 6, 0x0fff), Some(0)); // before it
        assert_eq!(prologue_offset(14, 0x1000, 6, &moved, 6, 0x1001), Some(15)); // after push, 14 + 1
        assert_eq!(prologue_offset(14, 0x1000, 6, &moved, 6, 0x1002), Some(16)); // after push, 14 + 2
        assert_eq!(prologue_offset(14, 0x1000, 6, &moved, 6, 0x1006), Some(20)); // after sub, 14 + 6
        assert_eq!(
            prologue_offset(14, 0x1000, 6, &moved, 6, 0x1007),
            Some(u8::MAX)
        ); // past it
    }

    #[test]
    fn a_re_encoded_longer_instruction_shifts_the_codes_after_it() {
        // The first moved instruction grew from 5 to 11 bytes in the
        // trampoline (a `call rel32` turned into its far form), so the sub
        // after it, ending at offset 9 in the function, sits at 11 there.
        let moved = [(0, 0), (5, 11)];
        assert_eq!(prologue_offset(14, 0x2000, 9, &moved, 15, 0x2005), Some(25)); // after the call, 14 + 11
        assert_eq!(prologue_offset(14, 0x2000, 9, &moved, 15, 0x2009), Some(29)); // after the sub, 14 + 15
    }

    #[test]
    fn a_code_ending_inside_a_moved_instruction_has_no_offset() {
        // A code ending at offset 3, inside the 5-byte call: no trampoline
        // offset says it has run, so the trampoline gets no codes at all.
        let moved = [(0, 0), (5, 11)];
        assert_eq!(prologue_offset(14, 0x2000, 9, &moved, 15, 0x2003), None);
    }

    #[test]
    fn an_info_takes_its_padded_codes_and_a_chained_parent_entry() {
        assert_eq!(info_len([1, 0, 0, 0]), 4);
        assert_eq!(info_len([1, 4, 1, 0]), 8); // one code, padded to two
        assert_eq!(info_len([1, 4, 2, 0]), 8);
        assert_eq!(info_len([(CHAINED << 3) | 1, 0, 1, 0]), 20);
    }

    #[test]
    fn a_chained_info_names_its_parent_and_a_primary_none() {
        let mut chained = vec![(CHAINED << 3) | 1, 0, 1, 0, 0x00, 0x30, 0, 0];
        chained.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0x78, 0x56, 0x34, 0x12]);
        assert_eq!(parent(&chained), Some(0x1234_5678));
        assert_eq!(parent(&chained[..19]), None, "cut short");
        assert_eq!(parent(&[1, 4, 1, 0, 4, 0x42, 0, 0]), None, "a primary");
    }

    #[test]
    fn only_a_hook_at_the_start_that_moves_no_epilogue_copies_the_frame() {
        // `push rbx; sub rsp, 0x20`, a 5-byte prologue that moves rsp.
        assert!(copies_frame(true, 5, &[0, 1]));
        assert!(copies_frame(true, 5, &[]));
        assert!(!copies_frame(false, 5, &[]), "inside the function");
        // `add rsp` at offset 5, past the prologue: an epilogue.
        assert!(!copies_frame(true, 5, &[0, 1, 5]));
    }
}
