//! Join rules: the mods' say over an account joining, asked in the game's
//! join message handler before any player of the machine is added.
//!
//! A machine one rule refuses gets what the game's ban branch gives an
//! account on the ban list: the banned reply, and its peer closed a second
//! later. Its player reads that the host denies access and loads no world.
//! The ban list itself is left alone, and so is a peer that already
//! joined, whose join message the game drops.

use core::ffi::c_void;
use std::sync::OnceLock;
use std::time::Duration;

use enshwick_core::PlayerKey;
use enshwick_core::abi::{Header, HookFlags, JoinCall, JoinRuleFn, Status};

use super::join_message::{self, JoinLayout, JoinOffsets};
use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, Registered, Target};
use crate::engine::mod_index::ModIndex;
use crate::engine::pinned::session;
use crate::engine::pinned::{Constant, Symbol};
use crate::engine::registry::Registry;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    session::FIND_MACHINE.id(),
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

/// The most often a refusal a peer can cause is logged.
const REFUSALS_LOGGED_EVERY: Duration = Duration::from_secs(10);

/// The session's send of a reply of `kind` to `peer`; true when sent.
type ReplyFn =
    unsafe extern "system" fn(session: *mut u8, peer: u16, kind: u32, message: *const u8) -> u64;

/// The session's machine of `peer`, null when the peer has none.
type FindMachineFn = unsafe extern "system" fn(session: *mut u8, peer: u16) -> *mut u8;

/// The game function and offsets the join check uses, from the build table.
#[derive(Debug, Clone, Copy)]
struct Layout {
    reply: ReplyFn,
    find_machine: FindMachineFn,
    /// Where the session keeps its peer slots.
    peer_slots: usize,
    /// Where the session keeps its `u8` platform.
    platform: usize,
    join: JoinLayout,
}

static LAYOUT: OnceLock<Result<Layout, (&'static str, &'static str)>> = OnceLock::new();
/// The switch of the mods' join rules.
static FUSE: Fuse = Fuse::new("join rules");
static RULES: Registry<Registered<JoinRuleFn>> = Registry::new();

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
        let find_machine = symbol(session::FIND_MACHINE)?;
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
            // SAFETY: the build table's `session.find_machine` is the address
            // of the session's machine lookup with the `FindMachineFn`
            // signature on this build, the call the join handler starts with.
            find_machine: unsafe { core::mem::transmute::<usize, FindMachineFn>(find_machine) },
            peer_slots,
            platform: constant(session::PLATFORM)?,
            join: JoinLayout::new(&offsets).map_err(|id| (id, MISFIT))?,
        })
    })
}

/// Registers mod `owner`'s join rule; [`Status::MISSING`] on a build
/// without the join path, so the mod can choose another way.
pub(crate) fn register(
    loader: &Loader,
    owner: ModIndex,
    callback: JoinRuleFn,
    user: *mut c_void,
) -> Status {
    if RULES.is_sealed() {
        return Status::TOO_LATE;
    }
    if layout(loader).is_none() {
        return Status::MISSING;
    }
    RULES
        .add(Registered::new(owner, callback, user))
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
        // SAFETY: the handler's message is the session's decoded join
        // message, non-null (checked just before), holding its `u32` count
        // at the table's `session.join_count`, unchanged while the game
        // handles it on this thread.
        let count: u32 = unsafe { enshwick_core::raw::read_at(message, layout.join.count()) };
        let Some(size) = layout.join.accounts_size(count) else {
            // SAFETY: `session` is the handler's own non-null session (checked
            // earlier), on the thread that handles its messages, and `layout`
            // is this build's.
            if unsafe { joined(layout, session, peer) } {
                return onward.call();
            }
            crate::log_every!(
                WARN,
                REFUSALS_LOGGED_EVERY,
                "a join that lists more accounts than its message holds refused"
            );
            // SAFETY: `session` is the handler's own non-null session (checked
            // earlier), on the thread that handles its messages, and `layout`
            // is this build's.
            return unsafe { refuse(layout, session, peer) };
        };
        let mut room = [0; join_message::MAX_MESSAGE];
        let entries = &mut room[..size];
        // SAFETY: the message holds `count` account entries from the table's
        // `session.join_accounts`: the game's handler reads the same entries
        // next, and `accounts_size` bounds them by the table's message size
        // and `MAX_MESSAGE`.
        unsafe { enshwick_core::raw::read_bytes_at(message, layout.join.accounts(), entries) };
        // SAFETY: the handler's non-null session (checked earlier) keeps its
        // `u8` platform at the table's `session.platform`, which nothing
        // writes while it handles a message.
        let platform: u8 = unsafe { enshwick_core::raw::read_at(session, layout.platform) };
        let steam = platform == join_message::STEAM;
        let Some((rule, player)) = refuser(rules, &layout.join, entries, steam) else {
            return onward.call();
        };
        // SAFETY: `session` is the handler's own non-null session (checked
        // earlier), on the thread that handles its messages, and `layout`
        // is this build's.
        if unsafe { joined(layout, session, peer) } {
            return onward.call();
        }
        let key = player.map_or(0, PlayerKey::get);
        let id = loader.mods.get(rule.owner.get()).map_or("?", |m| m.id.as_str());
        crate::log_every!(
            INFO,
            REFUSALS_LOGGED_EVERY,
            "join of account {key:016x} refused by {id}"
        );
        // SAFETY: `session` is the handler's own non-null session (checked
        // earlier), on the thread that handles its messages, and `layout`
        // is this build's.
        unsafe { refuse(layout, session, peer) }
    }
}

/// The first rule of a running mod that refuses an account of the join
/// message `entries`, and that account's key; `steam` when the session is
/// of Steam.
fn refuser<'a>(
    rules: &'a [Registered<JoinRuleFn>],
    join: &JoinLayout,
    entries: &[u8],
    steam: bool,
) -> Option<(&'a Registered<JoinRuleFn>, Option<PlayerKey>)> {
    join.keys(entries, steam).find_map(|player| {
        rules
            .iter()
            .filter(|r| r.live())
            .find(|r| !allows(r, player))
            .map(|r| (r, player))
    })
}

/// Whether `rule` lets `player` in.
fn allows(rule: &Registered<JoinRuleFn>, player: Option<PlayerKey>) -> bool {
    let call = JoinCall {
        header: Header::of::<JoinCall>(),
        player: player.unwrap_or(PlayerKey::new(0)),
    };
    let user = rule.user();
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

/// Whether `peer` already has a machine in `session`, which makes the
/// game's handler drop its join message untouched.
///
/// # Safety
///
/// `session` is the live session the join handler was called with, on the
/// thread handling its messages, and `layout.find_machine` is the session's
/// machine lookup of this build.
unsafe fn joined(layout: &Layout, session: *mut u8, peer: u16) -> bool {
    // SAFETY: the lookup the game's join handler calls first, with the same
    // live session and peer, by the caller's contract; it only reads the
    // session's machines.
    !unsafe { (layout.find_machine)(session, peer) }.is_null()
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
    // a reply buffer of at least the message size the send reads, which
    // outlives the call.
    unsafe { (layout.reply)(session, peer, join_message::REFUSED_KIND, reply.as_ptr()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "system" fn allow(_: *const JoinCall, _: *mut c_void) -> u8 {
        1
    }

    /// Refuses the account whose key is the `user` word.
    unsafe extern "system" fn refuse_key(call: *const JoinCall, user: *mut c_void) -> u8 {
        // SAFETY: the join check hands a valid call for this one.
        let player = unsafe { (*call).player };
        u8::from(player.get() != user.addr() as u64)
    }

    unsafe extern "system" fn no_machine(_: *mut u8, _: u16) -> *mut u8 {
        core::ptr::null_mut()
    }

    unsafe extern "system" fn a_machine(session: *mut u8, _: u16) -> *mut u8 {
        session
    }

    unsafe extern "system" fn sent_nothing(_: *mut u8, _: u16, _: u32, _: *const u8) -> u64 {
        0
    }

    fn test_layout(find_machine: FindMachineFn, reply: ReplyFn) -> Layout {
        Layout {
            reply,
            find_machine,
            peer_slots: 0x100,
            platform: 0,
            join: JoinLayout::new(&join_message::tests::offsets()).unwrap(),
        }
    }

    /// A join message's account entries for the Steam ids `ids`.
    fn entries(ids: &[u64]) -> Vec<u8> {
        let o = join_message::tests::offsets();
        let mut bytes = vec![0; ids.len() * o.account_size];
        for (entry, id) in bytes.chunks_exact_mut(o.account_size).zip(ids) {
            entry[..4].copy_from_slice(&8_u32.to_le_bytes());
            entry[o.account_id..o.account_id + 8].copy_from_slice(&id.to_le_bytes());
            entry[o.account_kind] = join_message::STEAM;
        }
        bytes
    }

    #[test]
    fn a_join_rule_on_a_build_without_the_join_path_is_refused_as_missing() {
        let loader: &'static Loader = Box::leak(Box::new(crate::start::tests::bare_loader()));
        assert_eq!(
            register(loader, ModIndex::new(0), allow, core::ptr::null_mut()),
            Status::MISSING,
            "a mod learns at init that its rule will not run"
        );
        link(loader);
        assert!(!FUSE.is_off(), "nothing to turn off without a rule");
    }

    #[test]
    fn the_first_rule_that_refuses_an_account_names_it() {
        let layout = test_layout(no_machine, sent_nothing);
        let steam = 76_561_198_447_973_054_u64;
        let key = layout
            .join
            .keys(&entries(&[steam]), true)
            .next()
            .flatten()
            .unwrap();
        let user = core::ptr::without_provenance_mut(usize::try_from(key.get()).unwrap());
        let rules = [
            Registered::<JoinRuleFn>::new(ModIndex::LOADER, allow, core::ptr::null_mut()),
            Registered::<JoinRuleFn>::new(ModIndex::LOADER, refuse_key, user),
        ];
        let (rule, player) = refuser(&rules, &layout.join, &entries(&[1, steam]), true).unwrap();
        assert_eq!(
            rule.user(),
            user,
            "the refusing rule, not the one before it"
        );
        assert_eq!(player, Some(key));
        assert!(refuser(&rules, &layout.join, &entries(&[1, 2]), true).is_none());
        assert!(
            refuser(&rules, &layout.join, &entries(&[steam]), false).is_none(),
            "off Steam no account has a key to refuse"
        );
    }

    #[test]
    fn a_peer_with_a_machine_has_joined() {
        let mut session = [0_u8; 8];
        // SAFETY: the test lookup reads nothing of the session.
        let found = unsafe {
            joined(
                &test_layout(a_machine, sent_nothing),
                session.as_mut_ptr(),
                3,
            )
        };
        assert!(found);
        // SAFETY: the test lookup reads nothing of the session.
        let found = unsafe {
            joined(
                &test_layout(no_machine, sent_nothing),
                session.as_mut_ptr(),
                3,
            )
        };
        assert!(!found);
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
        let layout = test_layout(no_machine, reply);
        let join = layout.join;
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
        assert_eq!(sent[0].2, join.refusal()[..join.message_size()]);
    }
}
