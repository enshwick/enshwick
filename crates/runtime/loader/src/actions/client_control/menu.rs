//! The interface's own menu commands a join, a host and a leave post,
//! found by their patterns on the running build. Never posted: 0x12 and
//! 0x17, which mark a character's or a world's slot as the game deletes it
//! (client 26430625, `0x140ddc3f0`, `0x140ddcc80`).

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

/// The interface's posters of the menu commands a host uses, beside the
/// join's: a build without them still joins (client 26430625).
#[derive(Debug)]
pub(crate) struct HostPosts {
    character: PostFn,
    world: PostFn,
    session: PostFn,
    play_world: PostFn,
    /// Where the character command (0x11) holds its slot id, a `u16`.
    character_slot: usize,
    /// Where the world command (0x16) holds its slot id, a `u16`.
    world_slot: usize,
    /// Where the session command (0x27) holds its kind byte.
    session_kind: usize,
}

/// Where the main menu keeps the character it has selected, from the
/// interface state (client 26430625, seen live on 2026-11-25): the state
/// sits in an object that also holds the menu object the main menu's
/// command switch `0x140873710` takes, whose save system holds the selected
/// slot id, a `u16`, and the byte that says one is set.
#[derive(Debug)]
pub(crate) struct SelectedCharacter {
    /// The interface state in the UI context.
    context_state: usize,
    /// The menu object's pointer from the state: its place in their owner
    /// less the state's.
    state_menu: usize,
    /// The menu's save system, and its slot id and set byte.
    menu_save: usize,
    slot: usize,
    slot_set: usize,
}

impl SelectedCharacter {
    pub(super) fn resolve(loader: &Loader) -> Result<Self, &'static str> {
        let state_owner = offset(loader, pinned::ui::STATE_OWNER)?;
        let owner_menu = offset(loader, pinned::ui::OWNER_MENU)?;
        Ok(Self {
            context_state: offset(loader, pinned::ui::CONTEXT_STATE)?,
            state_menu: owner_menu
                .checked_sub(state_owner)
                .ok_or(pinned::ui::OWNER_MENU.id())?,
            menu_save: offset(loader, pinned::ui::MENU_SAVE)?,
            slot: offset(loader, pinned::save::SELECTED_CHARACTER)?,
            slot_set: offset(loader, pinned::save::SELECTED_CHARACTER_SET)?,
        })
    }

    /// The selected character's place in the main menu's list, as a host
    /// posts it, in the frame of the UI context `context`; `None` while none
    /// is set. Every step reads through the kernel's copy, so a way that
    /// reads wrong on a new build gives none rather than a fault.
    pub(crate) fn read(&self, context: usize) -> Option<u16> {
        let pointer = |at: usize| {
            let mut word = [0u8; size_of::<usize>()];
            let at =
                crate::engine::pages::copy_into(at, &mut word).then(|| usize::from_ne_bytes(word));
            at.filter(|&at| at != 0)
        };
        let state = pointer(context.checked_add(self.context_state)?)?;
        let menu = pointer(state.checked_add(self.state_menu)?)?;
        let save = pointer(menu.checked_add(self.menu_save)?)?;
        let mut set = [0u8; 1];
        let mut slot = [0u8; size_of::<u16>()];
        let read = crate::engine::pages::copy_into(save.checked_add(self.slot_set)?, &mut set)
            && crate::engine::pages::copy_into(save.checked_add(self.slot)?, &mut slot);
        if !read || set[0] == 0 {
            return None;
        }
        crate::actions::client_commands::place(u16::from_ne_bytes(slot))
    }
}

/// A poster: the UI context in, its command or null out.
type PostFn = unsafe extern "system" fn(context: usize) -> *mut u8;

/// The session command's kind byte for an offline world and for one open
/// online (`0x140873710` case 0x27, client 26430625).
const SESSION_OFFLINE: u8 = 0;
const SESSION_ONLINE: u8 = 2;

/// The poster `id` on `loader`'s build, when its pattern matched once.
fn poster(loader: &Loader, id: pinned::Symbol) -> Result<PostFn, &'static str> {
    // Called, not hooked: only a pattern that matched once counts.
    if !loader.code_holds(id) {
        return Err(id.id());
    }
    let at = usize::try_from(address(loader, id)?).map_err(|_| id.id())?;
    // SAFETY: `code_holds` found the poster at `at`, its pattern matched
    // once from its first instruction; every poster has this signature on
    // client 26430625 (one UI context in, the command out).
    Ok(unsafe { core::mem::transmute::<usize, PostFn>(at) })
}

/// Posts `post` through the group that has its poster; whether the
/// interface's queue took it, false on a build without that group.
///
/// # Safety
///
/// - Called inside the interface frame on its thread, with `context` the
///   frame's own live UI context, which the global holds only then.
pub(crate) unsafe fn post(context: usize, post: crate::actions::client_commands::Post<'_>) -> bool {
    use crate::actions::client_commands::Post;
    let groups = &super::GROUPS;
    match post {
        Post::Character(_) | Post::World(_) | Post::Session { .. } | Post::PlayWorld => {
            // SAFETY: the caller's contract.
            groups
                .host
                .get()
                .is_some_and(|host| unsafe { host.post(context, post) })
        }
        Post::Password(_) | Post::Join(_) | Post::StartPlay | Post::Back => {
            // SAFETY: the caller's contract.
            groups
                .menu
                .get()
                .is_some_and(|menu| unsafe { menu.post(context, post) })
        }
    }
}

impl HostPosts {
    pub(super) fn resolve(loader: &Loader) -> Result<Self, &'static str> {
        Ok(Self {
            character: poster(loader, pinned::ui::POST_CHARACTER)?,
            world: poster(loader, pinned::ui::POST_WORLD)?,
            session: poster(loader, pinned::ui::POST_SESSION)?,
            play_world: poster(loader, pinned::ui::POST_PLAY_WORLD)?,
            character_slot: offset(loader, pinned::ui::POST_CHARACTER_SLOT)?,
            world_slot: offset(loader, pinned::ui::POST_WORLD_SLOT)?,
            session_kind: offset(loader, pinned::ui::POST_SESSION_KIND)?,
        })
    }

    /// Posts a host's `post` to the interface's queue; whether it took it.
    ///
    /// # Safety
    ///
    /// - As for [`post`].
    unsafe fn post(&self, context: usize, post: crate::actions::client_commands::Post<'_>) -> bool {
        use crate::actions::client_commands::Post;
        let poster = match post {
            Post::Character(_) => self.character,
            Post::World(_) => self.world,
            Post::Session { .. } => self.session,
            Post::PlayWorld => self.play_world,
            Post::Password(_) | Post::Join(_) | Post::StartPlay | Post::Back => return false,
        };
        // SAFETY: the matched poster of its type; the caller's contract
        // gives the live context on the interface thread.
        let command = unsafe { poster(context) };
        if command.is_null() {
            return false;
        }
        match post {
            // SAFETY: the character command (0x11) is 12 bytes its poster
            // zeroes; the main menu reads its slot as a `u16` at
            // `character_slot`.
            Post::Character(slot) => unsafe {
                enshwick_core::raw::write_at(command, self.character_slot, slot);
            },
            // SAFETY: the world command (0x16) is 3 bytes; the main menu
            // reads its slot as a `u16` at `world_slot`.
            Post::World(slot) => unsafe {
                enshwick_core::raw::write_at(command, self.world_slot, slot);
            },
            Post::Session { online } => {
                let kind = if online {
                    SESSION_ONLINE
                } else {
                    SESSION_OFFLINE
                };
                // SAFETY: the session command (0x27) is 2 bytes; the main
                // menu reads its kind byte at `session_kind`.
                unsafe { enshwick_core::raw::write_at(command, self.session_kind, kind) };
            }
            Post::PlayWorld | Post::Password(_) | Post::Join(_) | Post::StartPlay | Post::Back => {}
        }
        true
    }
}

/// The area byte of an online join (`0x140873710` case 0x2a).
const AREA_ONLINE: u8 = 3;

impl MenuPosts {
    pub(super) fn resolve(loader: &Loader) -> Result<Self, &'static str> {
        let poster = |id| poster(loader, id);
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
    unsafe fn post(&self, context: usize, post: crate::actions::client_commands::Post<'_>) -> bool {
        use crate::actions::client_commands::Post;
        let len = match post {
            Post::Password(text) => text.len(),
            _ => 0,
        };
        let fits = match post {
            Post::Password(_) => len <= enshwick_core::abi::ClientCommand::MAX_PASSWORD,
            Post::Join(target) => target.len() <= self.join_room(),
            Post::StartPlay | Post::Back => true,
            // A host's, which `HostPosts` posts.
            Post::Character(_) | Post::World(_) | Post::Session { .. } | Post::PlayWorld => false,
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
            Post::Character(_) | Post::World(_) | Post::Session { .. } | Post::PlayWorld => {
                return false;
            }
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
            Post::StartPlay
            | Post::Back
            | Post::Character(_)
            | Post::World(_)
            | Post::Session { .. }
            | Post::PlayWorld => {}
        }
        true
    }
}
