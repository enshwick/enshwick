use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Storage::FileSystem::{
    FILE_BEGIN, FlushFileBuffers, SetEndOfFile, SetFilePointerEx, WriteFile,
};
use windows_sys::Win32::System::Diagnostics::Debug::{
    CONTEXT, IMAGE_RUNTIME_FUNCTION_ENTRY, RtlLookupFunctionEntry, RtlVirtualUnwind,
    UNW_FLAG_NHANDLER,
};

use super::FRAMES;

/// The first byte and the end of the readable region `address` lies in.
pub(super) fn readable(address: usize) -> Option<(usize, usize)> {
    let region = crate::pages::query(address).filter(crate::pages::Region::reads)?;
    Some((region.base, region.end))
}

/// The word at `address`, `None` unless all its bytes read without a fault.
pub(super) fn read_word(address: usize) -> Option<u64> {
    let (_, end) = readable(address)?;
    if address.checked_add(8)? > end {
        return None;
    }
    // SAFETY: the earlier check keeps the eight bytes at `address` inside one
    // committed, readable region that `VirtualQuery` reported.
    Some(unsafe { crate::memory::read(address, 0) })
}

/// What unwinding one function's frame reads, from its `UNWIND_INFO`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Reach {
    /// The frame register's number and its offset below the frame's base.
    pub(super) frame: Option<(u8, usize)>,
    /// The bytes above the frame's base the unwind reads, the return
    /// address included.
    pub(super) bytes: usize,
    /// The `UNWIND_INFO` the frame's unwinding continues in.
    pub(super) chained: Option<u32>,
}

/// The longest `UNWIND_INFO`: the header, 256 code slots and a chained
/// entry.
pub(super) const UNWIND_INFO: usize = 4 + 256 * 2 + 12;

/// What unwinding reads for the `UNWIND_INFO` in `info`; `None` for a
/// version or code this does not know, or info cut short.
pub(super) fn reach(info: &[u8]) -> Option<Reach> {
    let (&head, &count, &frame) = (info.first()?, info.get(2)?, info.get(3)?);
    let version = head & 7;
    if !matches!(version, 1 | 2) {
        return None;
    }
    let count = usize::from(count);
    let codes = info.get(4..4 + count * 2)?;
    let slot = |i: usize| -> Option<usize> {
        let bytes = codes.get(i * 2..i * 2 + 2)?;
        Some(usize::from(u16::from_le_bytes([bytes[0], bytes[1]])))
    };
    let wide = |i: usize| -> Option<usize> { Some(slot(i)? | (slot(i + 1)? << 16)) };
    let (mut size, mut saves, mut i) = (0usize, 0usize, 0usize);
    while i < count {
        let byte = codes[i * 2 + 1];
        let op = byte & 0xf;
        let op_info = usize::from(byte >> 4);
        let taken = crate::unwind_info::slots(op, byte >> 4)?;
        match op {
            0 => size += 8,
            1 if op_info == 0 => size += slot(i + 1)? * 8,
            1 => size += wide(i + 1)?,
            2 => size += op_info * 8 + 8,
            4 => saves = saves.max(slot(i + 1)? * 8 + 8),
            5 => saves = saves.max(wide(i + 1)? + 8),
            8 => saves = saves.max(slot(i + 1)? * 16 + 16),
            9 => saves = saves.max(wide(i + 1)? + 16),
            10 => size += if op_info == 0 { 40 } else { 48 },
            // SET_FPREG and the version 2 epilogue codes move no `RSP`.
            _ => {}
        }
        i += taken;
    }
    let chained = if (head >> 3) & 4 == 0 {
        None
    } else {
        let at = 4 + count.next_multiple_of(2) * 2 + 8;
        let bytes = info.get(at..at + 4)?;
        Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    };
    let register = frame & 0xf;
    Some(Reach {
        frame: (register != 0).then_some((register, usize::from(frame >> 4) * 16)),
        bytes: (size + 8).max(saves),
        chained,
    })
}

/// What unwinding the function of `entry` in the image at `image` reads,
/// its chained entries included.
///
/// # Safety
///
/// - `entry` is the function entry `RtlLookupFunctionEntry` returned for the
///   loaded image at `image`, readable for an
///   `IMAGE_RUNTIME_FUNCTION_ENTRY`.
/// - The image stays loaded during the call.
pub(super) unsafe fn reach_of(
    image: usize,
    entry: *const IMAGE_RUNTIME_FUNCTION_ENTRY,
) -> Option<Reach> {
    let unwind_data = core::mem::offset_of!(IMAGE_RUNTIME_FUNCTION_ENTRY, Anonymous);
    // SAFETY: the caller's contract makes `entry` a readable function entry,
    // whose union at `Anonymous` is the `u32` RVA of its unwind data.
    let mut rva: u32 = unsafe { enshwick::raw::read_at(entry.cast(), unwind_data) };
    let mut total: Option<Reach> = None;
    for _ in 0..32 {
        let at = image.checked_add(rva as usize)?;
        let (_, end) = readable(at)?;
        // SAFETY: `readable` reported the bytes from `at` up to `end` as one
        // committed, readable region of the image, which stays loaded by the
        // caller's contract, and the slice is dropped within the loop.
        let info = unsafe {
            core::slice::from_raw_parts(
                core::ptr::with_exposed_provenance::<u8>(at),
                (end - at).min(UNWIND_INFO),
            )
        };
        let step = reach(info)?;
        let merged = match total {
            None => step,
            Some(first) => Reach {
                bytes: first.bytes + step.bytes - 8,
                chained: step.chained,
                ..first
            },
        };
        match merged.chained {
            Some(next) => (total, rva) = (Some(merged), next),
            None => return Some(merged),
        }
    }
    None
}

/// The value of the register numbered `number` as unwind codes number
/// them.
pub(super) fn register(context: &CONTEXT, number: u8) -> Option<u64> {
    Some(match number {
        0 => context.Rax,
        1 => context.Rcx,
        2 => context.Rdx,
        3 => context.Rbx,
        4 => context.Rsp,
        5 => context.Rbp,
        6 => context.Rsi,
        7 => context.Rdi,
        8 => context.R8,
        9 => context.R9,
        10 => context.R10,
        11 => context.R11,
        12 => context.R12,
        13 => context.R13,
        14 => context.R14,
        15 => context.R15,
        _ => return None,
    })
}

/// Whether unwinding a frame that reads `reach` from the stack pointer
/// `rsp`, and from the frame register's base when it has one, stays in the
/// readable stack `low..end`.
pub(super) fn fits(
    reach: &Reach,
    rsp: usize,
    frame: Option<usize>,
    low: usize,
    end: usize,
) -> bool {
    let within = |base: usize| {
        base >= low
            && base
                .checked_add(reach.bytes)
                .is_some_and(|last| last <= end)
    };
    within(rsp) && frame.is_none_or(within)
}

/// The return addresses of the faulting thread's stack, innermost first,
/// unwound from `context`; how many were found. Every read stays in the
/// committed, readable region of the stack, so a broken stack ends the
/// walk instead of faulting, on whichever thread reports.
///
/// # Safety
///
/// - `context` is the context of a thread stopped in the fault or of the
///   thread running this, so its stack does not change during the walk.
pub(super) unsafe fn unwind(context: &CONTEXT, frames: &mut [usize; FRAMES]) -> usize {
    let mut aligned = crate::detour::ThreadContext(*context);
    let context = &mut aligned.0;
    let mut found = 0;
    while found < FRAMES {
        let rip = context.Rip;
        let Ok(rsp) = usize::try_from(context.Rsp) else {
            break;
        };
        if rip == 0 {
            break;
        }
        let Some((low, end)) = readable(rsp) else {
            break;
        };
        let mut image = 0u64;
        // SAFETY: `RtlLookupFunctionEntry` takes any address, `image` is a
        // writable `u64` and the history table may be null.
        let entry = unsafe { RtlLookupFunctionEntry(rip, &raw mut image, core::ptr::null_mut()) };
        if entry.is_null() {
            // A leaf function: its return address is on top of the stack.
            let Some(back) = read_word(rsp) else {
                break;
            };
            context.Rip = back;
            context.Rsp += 8;
        } else {
            let Ok(base) = usize::try_from(image) else {
                break;
            };
            // SAFETY: `entry` is the non-null function entry
            // `RtlLookupFunctionEntry` just returned for the loaded image at
            // `base`.
            let Some(reach) = (unsafe { reach_of(base, entry) }) else {
                break;
            };
            let frame = match reach.frame {
                None => None,
                Some((number, offset)) => {
                    let value = register(context, number)
                        .and_then(|value| usize::try_from(value).ok())
                        .and_then(|value| value.checked_sub(offset));
                    if value.is_none() {
                        break;
                    }
                    value
                }
            };
            if !fits(&reach, rsp, frame, low, end) {
                break;
            }
            let (mut handler, mut establisher) = (core::ptr::null_mut(), 0u64);
            // SAFETY: `entry` is the function entry found for `rip` in the
            // image at `image`, and `fits` checked that the context's stack
            // holds every byte the entry's unwind codes read.
            unsafe {
                RtlVirtualUnwind(
                    UNW_FLAG_NHANDLER,
                    image,
                    rip,
                    entry,
                    &raw mut *context,
                    &raw mut handler,
                    &raw mut establisher,
                    core::ptr::null_mut(),
                );
            }
            if context.Rsp <= context_rsp(rsp) || context.Rsp > context_rsp(end) {
                break;
            }
        }
        if context.Rip == rip || context.Rip == 0 {
            break;
        }
        frames[found] = usize::try_from(context.Rip).unwrap_or(0);
        found += 1;
    }
    found
}

/// `address` as a context register holds it.
pub(super) fn context_rsp(address: usize) -> u64 {
    u64::try_from(address).unwrap_or(u64::MAX)
}

/// Writes `bytes` to the open file `handle`.
pub(super) fn write(handle: usize, bytes: &[u8]) {
    if handle == 0 {
        return;
    }
    let handle: HANDLE = core::ptr::with_exposed_provenance_mut(handle);
    let mut written = 0u32;
    // SAFETY: `handle` is a non-zero file handle `open` returned, open for
    // the process; `bytes` is readable for its length and `written` a
    // writable `u32`.
    unsafe {
        WriteFile(
            handle,
            bytes.as_ptr(),
            u32::try_from(bytes.len()).unwrap_or(0),
            &raw mut written,
            core::ptr::null_mut(),
        );
    }
    // SAFETY: `handle` is a non-zero file handle `open` returned, open for
    // the process.
    unsafe { FlushFileBuffers(handle) };
}

/// Empties the open file `handle` from the start.
pub(super) fn rewind(handle: usize) {
    if handle == 0 {
        return;
    }
    let handle: HANDLE = core::ptr::with_exposed_provenance_mut(handle);
    // SAFETY: `handle` is a non-zero file handle `open` returned, open for
    // the process; the new position may be null.
    unsafe { SetFilePointerEx(handle, 0, core::ptr::null_mut(), FILE_BEGIN) };
    // SAFETY: `handle` is a non-zero file handle `open` returned, open for
    // the process.
    unsafe { SetEndOfFile(handle) };
}
