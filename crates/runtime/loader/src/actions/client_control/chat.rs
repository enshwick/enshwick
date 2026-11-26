//! The chat line's poster, which the chat window's Enter calls, found by its
//! route from that window on the running build.

use super::address;
use crate::engine::loader::Loader;
use crate::engine::pinned;

/// The chat poster: it reserves the chat command (0x74) in the UI queue,
/// copies the text in with the line's type and two words, and answers
/// whether the queue took it (client 26430625, `0x140d72d90`, read in the
/// decompiler).
#[derive(Debug)]
pub(crate) struct ChatPost {
    post: PostFn,
}

/// A text as the game passes one: its first byte and its length.
#[repr(C)]
struct Text {
    bytes: *const u8,
    len: usize,
}

/// The poster: the UI context, the text, the line's type and two words in;
/// whether the queue took it out, in its low byte.
type PostFn =
    unsafe extern "system" fn(context: usize, text: *const Text, kind: u8, a: u32, b: u32) -> u64;

/// The type of a player's own line, both words 0, as the chat window posts
/// it; the game's other types carry a text id or a ping (client 26430625).
const PLAYER_LINE: u8 = 0;

impl ChatPost {
    pub(super) fn resolve(loader: &Loader) -> Result<Self, &'static str> {
        let id = pinned::ui::POST_CHAT.id();
        // Called, not hooked: only a route whose own check matched the
        // poster's first instructions counts.
        if !loader.build.routed(id) {
            return Err(id);
        }
        let at = usize::try_from(address(loader, pinned::ui::POST_CHAT)?).map_err(|_| id)?;
        // SAFETY: the routed poster, its first instructions checked by its
        // route at link; of this type on client 26430625 (decompiled).
        let post = unsafe { core::mem::transmute::<usize, PostFn>(at) };
        Ok(Self { post })
    }

    /// Posts `line` as the local player's own; whether the interface's
    /// queue took it. A line outside 1 to the game's 0x400 bytes, whose copy
    /// would end the game, is not posted.
    ///
    /// # Safety
    ///
    /// - Called inside the interface frame on its thread, with `context` the
    ///   frame's own live UI context, which the global holds only then.
    pub(crate) unsafe fn post(&self, context: usize, line: &str) -> bool {
        if !(1..=enshwick_core::abi::ClientCommand::MAX_CHAT).contains(&line.len()) {
            return false;
        }
        let text = Text {
            bytes: line.as_ptr(),
            len: line.len(),
        };
        // SAFETY: the routed poster of its type; the caller's contract gives
        // the live context on the interface thread; the text, in its bound,
        // is read only during the call, which copies it.
        unsafe { (self.post)(context, &raw const text, PLAYER_LINE, 0, 0) & 0xff != 0 }
    }
}
