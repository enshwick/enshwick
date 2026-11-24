//! The interface's own menu commands a join and a leave post, found by
//! their patterns on the running build.

use super::{address, offset};
use crate::engine::loader::Loader;
use crate::engine::pinned;

/// The interface's posters of the menu commands a join and a leave use,
/// each `post(ui context) -> command`: it reserves its command in the UI
/// queue with its kind byte written and the rest zeroed, or answers null
/// when the queue is full (client 26430625).
#[derive(Debug)]
pub(crate) struct MenuPosts {
    password: PostFn,
    join: PostFn,
    start_play: PostFn,
    back: PostFn,
    /// Where the password command (0x25) holds its text, and its length as
    /// a `u16` after the text's room.
    password_text: usize,
    password_len: usize,
    /// Where the join command (0x26) holds its area byte, the join target
    /// right after it, and the address's last `u16`, which ends it.
    join_area: usize,
    join_tail: usize,
}

/// A poster: the UI context in, its command or null out.
type PostFn = unsafe extern "system" fn(context: usize) -> *mut u8;

/// The area byte of an online join (`0x140873710` case 0x2a).
const AREA_ONLINE: u8 = 3;

impl MenuPosts {
    pub(super) fn resolve(loader: &Loader) -> Result<Self, &'static str> {
        let poster = |id: pinned::Symbol| {
            // Called, not hooked: only a pattern that matched once counts.
            if !loader.code_holds(id) {
                return Err(id.id());
            }
            let at = usize::try_from(address(loader, id)?).map_err(|_| id.id())?;
            // SAFETY: `code_holds` found the poster at `at`, its pattern
            // matched once from its first instruction; the type is its
            // signature on client 26430625 (one UI context in, the command
            // out).
            Ok(unsafe { core::mem::transmute::<usize, PostFn>(at) })
        };
        let password_text = offset(loader, pinned::ui::POST_PASSWORD_TEXT)?;
        let password_len = offset(loader, pinned::ui::POST_PASSWORD_LEN)?;
        // The text's room holds the longest password the game takes.
        if password_text + enshwick_core::abi::ClientCommand::MAX_PASSWORD > password_len {
            return Err(pinned::ui::POST_PASSWORD_LEN.id());
        }
        Ok(Self {
            password_text,
            password_len,
            join_area: offset(loader, pinned::ui::POST_JOIN_AREA)?,
            join_tail: offset(loader, pinned::ui::POST_JOIN_TAIL)?,
            password: poster(pinned::ui::POST_PASSWORD)?,
            join: poster(pinned::ui::POST_JOIN)?,
            start_play: poster(pinned::ui::POST_START_PLAY)?,
            back: poster(pinned::ui::POST_BACK)?,
        })
    }

    /// Where the join target starts: right after its one area byte.
    const fn join_target(&self) -> usize {
        self.join_area + 1
    }

    /// How many bytes of join target the join command holds: from after
    /// its area byte to the end of the address's last `u16`.
    pub(crate) const fn join_room(&self) -> usize {
        (self.join_tail + size_of::<u16>()).saturating_sub(self.join_target())
    }

    /// Posts `post` to the interface's queue; whether the queue took it.
    /// A join target past the command's room or a password past the game's
    /// is not posted.
    ///
    /// # Safety
    ///
    /// - Called inside the interface frame on its thread, with `context` the
    ///   frame's own live UI context, which the global holds only then.
    pub(crate) unsafe fn post(
        &self,
        context: usize,
        post: crate::actions::client_commands::Post<'_>,
    ) -> bool {
        use crate::actions::client_commands::Post;
        let len = match post {
            Post::Password(text) => text.len(),
            _ => 0,
        };
        let fits = match post {
            Post::Password(_) => len <= enshwick_core::abi::ClientCommand::MAX_PASSWORD,
            Post::Join(target) => target.len() <= self.join_room(),
            Post::StartPlay | Post::Back => true,
        };
        // The game's room keeps a password's length in its `u16`.
        let Ok(len) = u16::try_from(len) else {
            return false;
        };
        if !fits {
            return false;
        }
        let poster = match post {
            Post::Password(_) => self.password,
            Post::Join(_) => self.join,
            Post::StartPlay => self.start_play,
            Post::Back => self.back,
        };
        // SAFETY: the routed poster of its type; the caller's contract gives
        // the live context on the interface thread.
        let command = unsafe { poster(context) };
        if command.is_null() {
            return false;
        }
        match post {
            Post::Password(text) => {
                // SAFETY: the poster's command holds the text's room from
                // `password_text` up to its length at `password_len`, which
                // `resolve` checked fits the longest password and `fits` keeps
                // the text in; the queue drains on this thread after the frame.
                unsafe { enshwick_core::raw::write_bytes_at(command, self.password_text, text) };
                // SAFETY: the same command's length, the `u16` its poster
                // zeroes at `password_len`.
                unsafe { enshwick_core::raw::write_at(command, self.password_len, len) };
            }
            Post::Join(target) => {
                // SAFETY: the poster's command holds the address the main
                // menu copies, the area byte at `join_area` first.
                unsafe { enshwick_core::raw::write_at(command, self.join_area, AREA_ONLINE) };
                // SAFETY: the same command; `fits` keeps the target between
                // the area byte and the address's end.
                unsafe { enshwick_core::raw::write_bytes_at(command, self.join_target(), target) };
            }
            Post::StartPlay | Post::Back => {}
        }
        true
    }
}
