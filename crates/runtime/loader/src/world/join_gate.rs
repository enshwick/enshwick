//! Join rules: the mods' say over an account joining, asked in the game's
//! join message handler before any player of the machine is added.
//!
//! A machine one rule refuses gets what the game's ban branch gives an
//! account on the ban list: the banned reply, and its peer closed a second
//! later. Its player reads that the host denies access and loads no world.
//! The ban list itself is left alone.

use core::ffi::c_void;
use std::sync::OnceLock;
use std::sync::atomic::Ordering;

use enshwick_core::PlayerKey;
use enshwick_core::abi::{Header, HookFlags, JoinCall, JoinRuleFn, Status};

use super::join_message::{self, JoinLayout, JoinOffsets};
use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, Target};
use crate::engine::mod_index::ModIndex;
use crate::engine::pinned::session;
use crate::engine::pinned::{Constant, Symbol};
use crate::engine::registry::Registry;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    session::JOIN_ACCOUNT_ID.id(),
    session::JOIN_ACCOUNT_KIND.id(),
    session::JOIN_ACCOUNT_SIZE.id(),
    session::JOIN_ACCOUNTS.id(),
    session::JOIN_COUNT.id(),
    session::JOIN_GAME.id(),
    session::JOIN_REPLY.id(),
    session::MESSAGE_SIZE.id(),
    session::PEER_SLOT_CLOSE.id(),
    session::PEER_SLOT_OPEN.id(),
    session::PEER_SLOT_SIZE.id(),
    session::PEER_SLOTS.id(),
    session::PLATFORM.id(),
    session::REPLY_BANNED_A.id(),
    session::REPLY_BANNED_B.id(),
    session::REPLY_FRAME.id(),
];

/// Why the join rules stay off when the table's offsets do not fit together.
const MISFIT: &str = "does not fit the join message's other offsets";

/// The session's send of a reply of `kind` to `peer`; true when sent.
type ReplyFn =
    unsafe extern "system" fn(session: *mut u8, peer: u16, kind: u32, message: *const u8) -> u64;

/// The game function and offsets the join check uses, from the build table.
#[derive(Debug, Clone, Copy)]
struct Layout {
    reply: ReplyFn,
    /// Where the session keeps its peer slots.
    peer_slots: usize,
    /// Where the session keeps its `u8` platform.
    platform: usize,
    join: JoinLayout,
}

/// A mod's join rule.
#[derive(Debug, Clone, Copy)]
struct Rule {
    owner: ModIndex,
    callback: JoinRuleFn,
    user: usize,
}

static LAYOUT: OnceLock<Result<Layout, (&'static str, &'static str)>> = OnceLock::new();
/// The switch of the mods' join rules.
static FUSE: Fuse = Fuse::new("join rules");
static RULES: Registry<Rule> = Registry::new();

fn layout(loader: &Loader) -> Option<&'static Layout> {
    found(loader).as_ref().ok()
}

/// The join check's layout, or the entry it lacks and why.
fn found(loader: &Loader) -> &'static Result<Layout, (&'static str, &'static str)> {
    LAYOUT.get_or_init(|| {
        let absent = |id: &'static str| (id, crate::engine::builds::ABSENT);
        let symbol = |name: Symbol| {
            loader
                .build
                .symbol(name.id())
                .and_then(|rva| usize::try_from(loader.base + rva).ok())
                .ok_or(absent(name.id()))
        };
        let constant = |name: Constant| {
            loader
                .build
                .constant(name.id())
                .and_then(|v| usize::try_from(v).ok())
                .ok_or(absent(name.id()))
        };
        let reply = symbol(session::JOIN_REPLY)?;
        let peer_slots = constant(session::PEER_SLOTS)?;
        let offsets = JoinOffsets {
            message_size: constant(session::MESSAGE_SIZE)?,
            count: constant(session::JOIN_COUNT)?,
            accounts: constant(session::JOIN_ACCOUNTS)?,
            account_size: constant(session::JOIN_ACCOUNT_SIZE)?,
            account_id: constant(session::JOIN_ACCOUNT_ID)?,
            account_kind: constant(session::JOIN_ACCOUNT_KIND)?,
            reply_frame: constant(session::REPLY_FRAME)?,
            banned_frame: [
                constant(session::REPLY_BANNED_A)?,
                constant(session::REPLY_BANNED_B)?,
            ],
            peer_slot_size: constant(session::PEER_SLOT_SIZE)?,
            peer_slots,
            peer_slot_open: constant(session::PEER_SLOT_OPEN)?,
            peer_slot_close: constant(session::PEER_SLOT_CLOSE)?,
        };
        Ok(Layout {
            // SAFETY: the build table's `session.join_reply` is the address
            // of the session's reply send with the `ReplyFn` signature on
            // this build.
            reply: unsafe { core::mem::transmute::<usize, ReplyFn>(reply) },
            peer_slots,
            platform: constant(session::PLATFORM)?,
            join: JoinLayout::new(&offsets).ok_or((session::MESSAGE_SIZE.id(), MISFIT))?,
        })
    })
}

/// Registers mod `owner`'s join rule.
pub(crate) fn register(owner: ModIndex, callback: JoinRuleFn, user: *mut c_void) -> Status {
    if RULES.is_sealed() {
        return Status::TOO_LATE;
    }
    RULES
        .add(Rule {
            owner,
            callback,
            user: user.expose_provenance(),
        })
        .map_or(Status::TOO_LATE, |()| Status::OK)
}

/// Links the join check when a mod has a join rule.
pub(crate) fn link(loader: &'static Loader) {
    if RULES.seal().is_empty() {
        return;
    }
    if let Err((id, reason)) = found(loader) {
        let _off = loader.fuse_off(&FUSE, id, reason);
        return;
    }
    let target = Target::Pinned(session::JOIN_GAME);
    if loader.link_fused(&FUSE, target, &JOIN_NEXT, join_game, HookFlags::NONE) {
        crate::log!(INFO, "join rules on");
    }
}

crate::engine::chains::chain_detour! {
    /// The game's join message handler at the table's `session.join_game`:
    /// answers a machine a mod's rule refuses as the game answers a banned
    /// account, and passes every other message on.
    fn join_game(session: *mut u8, message: *const u8, peer: u16) -> u64;
    next JOIN_NEXT;
    fuse &FUSE, "the join";
    |onward| {
        let (Some(loader), Some(rules)) = (crate::engine::loader::loader(), RULES.sealed()) else {
            return onward.call();
        };
        let Some(layout) = layout(loader) else {
            return onward.call();
        };
        if session.is_null() || message.is_null() {
            return onward.call();
        }
        let mut bytes = vec![0; layout.join.message_size()];
        // SAFETY: the handler's message is the session's decoded message
        // buffer, of the table's `session.message_size` bytes and non-null
        // (checked just before), which the game keeps unchanged while it
        // handles it on this thread.
        unsafe { enshwick_core::raw::read_bytes_at(message, 0, &mut bytes) };
        // SAFETY: the handler's non-null session (checked earlier) keeps its
        // `u8` platform at the table's `session.platform`, which nothing
        // writes while it handles a message.
        let platform: u8 = unsafe { enshwick_core::raw::read_at(session, layout.platform) };
        if !refused(loader, rules, &layout.join, &bytes, platform == join_message::STEAM) {
            return onward.call();
        }
        // SAFETY: `session` is the handler's own non-null session (checked
        // earlier), on the thread that handles its messages, and `layout`
        // is this build's.
        unsafe { refuse(layout, session, peer) }
    }
}

/// Whether a mod's rule refuses an account of the join message `message`
/// on a session of Steam when `steam` holds, or the message brings more
/// accounts than it holds.
fn refused(
    loader: &Loader,
    rules: &[Rule],
    join: &JoinLayout,
    message: &[u8],
    steam: bool,
) -> bool {
    let Some(accounts) = join.accounts(message, steam) else {
        crate::log!(
            WARN,
            "a join that lists more accounts than its message holds refused"
        );
        return true;
    };
    accounts.into_iter().any(|player| {
        let refuser = rules
            .iter()
            .filter(|r| {
                loader
                    .mods
                    .get(r.owner.get())
                    .is_some_and(|m| !m.off.load(Ordering::Acquire))
            })
            .find(|r| !allows(r, player));
        if let Some(rule) = refuser {
            let key = player.map_or(0, PlayerKey::get);
            let id = loader
                .mods
                .get(rule.owner.get())
                .map_or("?", |m| m.id.as_str());
            crate::log!(INFO, "join of account {key:016x} refused by {id}");
        }
        refuser.is_some()
    })
}

/// Whether `rule` lets `player` in.
fn allows(rule: &Rule, player: Option<PlayerKey>) -> bool {
    let call = JoinCall {
        header: Header::of::<JoinCall>(),
        player: player.unwrap_or(PlayerKey::new(0)),
    };
    let user = core::ptr::with_exposed_provenance_mut(rule.user);
    // SAFETY: the rule is the mod's registered `JoinRuleFn` in its DLL, which
    // stays loaded for the process, called with a `JoinCall` that outlives
    // the call and the mod's own `user` word; the mod's safe layer catches
    // its panics.
    let answer = crate::engine::profile::call(
        rule.owner,
        crate::engine::profile::Feature::Joins,
        || unsafe { (rule.callback)(&raw const call, user) },
    );
    answer != 0
}

/// Answers `peer` with the ban branch's reply and closes it a second later,
/// as the game's join handler does for an account on the ban list; returns
/// what the handler returns.
///
/// # Safety
///
/// - `session` is the live session the join handler was called with, on the
///   thread handling its messages, so nothing else writes its peer slots;
///   it holds [`JoinLayout::peer_slots_size`] bytes of slots at
///   `layout.peer_slots`.
/// - `layout.reply` is the session's reply send of this build.
unsafe fn refuse(layout: &Layout, session: *mut u8, peer: u16) -> u64 {
    let slot = layout.peer_slots + layout.join.peer_slot(peer);
    // SAFETY: by the caller's contract `slot` lies in the session's peer
    // slots, each starting with the `u16` peer id the ban branch writes on
    // this thread.
    unsafe { enshwick_core::raw::write_at(session, slot, peer) };
    let open = slot + layout.join.slot_open();
    // SAFETY: by the caller's contract `slot` lies in the session's peer
    // slots, each holding the `u8` open flag at the table's offset, which
    // the ban branch clears on this thread.
    unsafe { enshwick_core::raw::write_at(session, open, 0_u8) };
    let close = slot + layout.join.slot_close();
    // SAFETY: by the caller's contract `slot` lies in the session's peer
    // slots, each holding the `u64` close wait at the table's offset, which
    // the ban branch sets on this thread.
    unsafe { enshwick_core::raw::write_at(session, close, join_message::CLOSE_IN_NS) };
    let reply = layout.join.refusal();
    // SAFETY: the session's reply send at the table's `session.join_reply`,
    // called as the ban branch calls it, with the live session, the peer and
    // a reply buffer of the message size the send reads, which outlives the
    // call.
    unsafe { (layout.reply)(session, peer, join_message::REFUSED_KIND, reply.as_ptr()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "system" fn allow(_: *const JoinCall, _: *mut c_void) -> u8 {
        1
    }

    #[test]
    fn join_rules_on_a_build_without_the_join_path_are_listed_off() {
        assert_eq!(
            register(ModIndex::new(0), allow, core::ptr::null_mut()),
            Status::OK
        );
        let loader: &'static Loader = Box::leak(Box::new(crate::start::tests::bare_loader()));
        link(loader);
        assert!(FUSE.is_off());
        let listed = crate::engine::diag::parts_off();
        assert!(
            listed
                .iter()
                .any(|p| p.part == "join rules" && p.reason.starts_with(session::JOIN_REPLY.id())),
            "listed for the Mods screen, not only logged: {listed:?}"
        );
    }

    #[test]
    fn a_refusal_closes_the_peer_and_sends_the_banned_reply() {
        static SENT: std::sync::Mutex<Vec<(u16, u32, Vec<u8>)>> = std::sync::Mutex::new(Vec::new());
        unsafe extern "system" fn reply(
            _: *mut u8,
            peer: u16,
            kind: u32,
            message: *const u8,
        ) -> u64 {
            let size = join_message::tests::offsets().message_size;
            // SAFETY: the refusal hands its reply buffer of the message size.
            let bytes = unsafe { core::slice::from_raw_parts(message, size) }.to_vec();
            SENT.lock().unwrap().push((peer, kind, bytes));
            1
        }
        let join = JoinLayout::new(&join_message::tests::offsets()).unwrap();
        let layout = Layout {
            reply,
            peer_slots: 0x100,
            platform: 0,
            join,
        };
        let mut session = vec![0xaa_u8; 0x100 + join.peer_slots_size()];
        // SAFETY: a buffer holding all 64 slots at `peer_slots`, on this thread.
        let sent = unsafe { refuse(&layout, session.as_mut_ptr(), 0x45) };
        assert_eq!(sent, 1);
        let slot = &session[0x100 + 5 * 0x10..][..0x10];
        assert_eq!(slot[..2], 0x45_u16.to_le_bytes());
        assert_eq!(slot[2], 0);
        assert_eq!(
            slot[3..8],
            [0xaa; 5],
            "the ban branch leaves these bytes alone"
        );
        assert_eq!(slot[8..], join_message::CLOSE_IN_NS.to_le_bytes());
        let sent = SENT.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!((sent[0].0, sent[0].1), (0x45, join_message::REFUSED_KIND));
        assert_eq!(sent[0].2, join.refusal());
    }
}
