//! The mods' commands a player types in chat: taken out of the chat and
//! queued to run, or, while their switch is off, still kept out of it.

use super::{linked, read_start, read_text};
use crate::engine::fuse::Fuse;
use crate::engine::loader::Loader;

/// The switch of the mods' commands typed in chat.
static COMMANDS_FUSE: Fuse = Fuse::new("chat commands");
/// The switch of the check that keeps a line naming a mod's commands out of
/// the chat while [`COMMANDS_FUSE`] or the chat's line handling is off.
static COMMAND_WORDS_FUSE: Fuse = Fuse::new("chat command words");

/// Whether the line `text` the player of account `key` typed leaves the
/// chat as a mod's command: queued to run after the step under
/// [`COMMANDS_FUSE`], or, while that is off or panicked, [`kept_out`].
///
/// # Safety
///
/// - `text` is null or the game's span of pointer and byte length, its bytes
///   readable and unchanged for the call.
pub(super) unsafe fn taken(loader: &'static Loader, text: *const [usize; 2], key: u64) -> bool {
    COMMANDS_FUSE
        .run("the chat command check", || {
            // SAFETY: the caller guarantees `text` null or readable for the
            // call.
            unsafe { slash_line(text) }
                .is_some_and(|line| crate::actions::commands::queue(loader, key, &line))
        })
        // SAFETY: the caller guarantees `text` null or readable for the call.
        .unwrap_or_else(|| unsafe { kept_out(text) })
}

/// Whether the line `text` names a mod's commands, for the chat to drop
/// when the commands could not check it, under [`COMMAND_WORDS_FUSE`]:
/// nothing runs it, and its words reach no other player.
///
/// # Safety
///
/// - `text` is null or the game's span of pointer and byte length, its bytes
///   readable and unchanged for the call.
#[cold]
pub(super) unsafe fn kept_out(text: *const [usize; 2]) -> bool {
    COMMAND_WORDS_FUSE
        .run("the chat command word check", || {
            // SAFETY: the caller guarantees `text` null or readable for the
            // call.
            unsafe { slash_line(text) }
                .is_some_and(|line| crate::actions::commands::leaves_chat(&line))
        })
        .unwrap_or(false)
}

/// The line `text` when it starts with `/` and the chat is linked; reads the
/// whole text only then.
///
/// # Safety
///
/// - `text` is null or the game's span of pointer and byte length, its bytes
///   readable and unchanged for the call.
unsafe fn slash_line(text: *const [usize; 2]) -> Option<String> {
    let (_, layout) = linked()?;
    let mut first = [0u8; 1];
    // SAFETY: the caller guarantees `text` null or readable for the call.
    if unsafe { read_start(text, &mut first) } != Some(1) || first != *b"/" {
        return None;
    }
    // SAFETY: the caller guarantees `text` null or readable for the call.
    Some(unsafe { read_text(text, layout.max_text) })
}
