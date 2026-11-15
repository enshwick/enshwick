//! The stop block of the workshop recipe limit: its length and the bytes
//! that skip it.

/// The bytes the stop block's replacement needs: `inc r13` and a `jmp`.
const JUMP_LEN: usize = 8;

/// The replacement of the `len` bytes of the stop block at `stop`: `inc r13`
/// (the loop counter the skipped append would have raised), then `jmp` to
/// the next recipe at `next`, the rest `int3`; `None` when the block is too
/// short or the jump too far.
pub(super) fn stop_replacement(stop: usize, next: usize, len: usize) -> Option<Box<[u8]>> {
    if len < JUMP_LEN {
        return None;
    }
    let from = i64::try_from(stop.checked_add(JUMP_LEN)?).ok()?;
    let rel = i32::try_from(i64::try_from(next).ok()? - from).ok()?;
    let mut bytes = vec![0xcc; len];
    bytes[..4].copy_from_slice(&[0x49, 0xff, 0xc5, 0xe9]);
    bytes[4..JUMP_LEN].copy_from_slice(&rel.to_le_bytes());
    Some(bytes.into())
}

/// The most bytes [`stop_len`] decodes before it gives up.
pub(super) const MOST_STOP: usize = 64; // bound, not a game offset: a stop block is a few instructions

/// The length of the stop block whose `code` starts at `stop`, for a block an
/// engine route found, which carries no table body: up to and with its
/// unconditional jump to the next recipe at `next`, or up to the `int3`
/// padding after its last call, which does not return on 1.0 builds;
/// `None` without either in its first [`MOST_STOP`] bytes, or when the block
/// leaves first by another jump or a return, so the patch never covers code
/// past the block.
pub(crate) fn stop_len(code: &[u8], stop: u64, next: u64) -> Option<usize> {
    use iced_x86::{Code, Decoder, DecoderOptions, FlowControl};
    let code = code.get(..code.len().min(MOST_STOP))?;
    let mut decoder = Decoder::with_ip(64, code, stop, DecoderOptions::NONE);
    decoder
        .iter()
        .take_while(|i| !i.is_invalid())
        .find_map(|i| match (i.code(), i.flow_control()) {
            (Code::Int3, _) => Some(Some(i.ip())),
            (_, FlowControl::UnconditionalBranch) => {
                Some((i.near_branch_target() == next).then(|| i.next_ip()))
            }
            (_, FlowControl::IndirectBranch | FlowControl::Return) => Some(None),
            _ => None,
        })
        .flatten()
        .and_then(|end| usize::try_from(end - stop).ok())
}
