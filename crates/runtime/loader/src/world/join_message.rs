//! The game's join message and the answer its ban branch gives, read in
//! the decompiler on server 26430253 and client 26430625: which accounts a
//! machine brings, the key the game gives each, the reply that tells a
//! refused machine it is banned, and the peer slot that closes it. Every
//! offset comes from the build table; the routes' shapes hold the values
//! named here.

use enshwick_core::PlayerKey;

use crate::engine::pinned::session;

/// The bytes of an account id the game hashes.
const ID_SIZE: usize = 8;
/// The session platform of Steam, the one platform whose accounts the game
/// keys: there it keys every account of a kind other than zero.
pub(crate) const STEAM: u8 = 3;
/// The game's start value for the FNV-1a 64 hash of an account id.
const KEY_SEED: u64 = 0x3f63_b56d_b289_0a16;
/// The FNV-1a 64 prime.
const FNV_PRIME: u64 = 0x0100_0000_01b3;
/// The reply kind the ban branch sends.
pub(crate) const REFUSED_KIND: u32 = 8;
/// The peer id bits that index the session's peer slots.
const PEER_MASK: u16 = 0x3f;
/// The ban branch's wait before the session closes a refused peer: one
/// second.
pub(crate) const CLOSE_IN_NS: u64 = 1_000_000_000;
/// The largest message the loader reads, and the stack room it keeps for a
/// message's accounts and for a reply.
pub(crate) const MAX_MESSAGE: usize = 0x1000; // bound, not a game offset: room the loader keeps

/// Where a build keeps a join message's accounts, its reply's banned flags
/// and a session peer slot's fields; made only by [`JoinLayout::new`], which
/// checks that each lies inside what holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct JoinLayout {
    /// The size of a decoded session message, and of the reply.
    message_size: usize,
    /// Where the message keeps its `u32` count of accounts.
    count: usize,
    /// Where its accounts start.
    accounts: usize,
    /// The size of one account entry, which starts with the id's `u32`
    /// length.
    account_size: usize,
    /// Where an entry keeps its account id.
    account_id: usize,
    /// Where an entry keeps its `u8` account kind.
    account_kind: usize,
    /// The reply's two `u8` flags the ban branch sets.
    banned: [usize; 2],
    /// The size of one peer slot, which starts with its `u16` peer id.
    peer_slot_size: usize,
    /// Where a slot keeps the `u8` that is 1 while the peer may send more
    /// than a join.
    slot_open: usize,
    /// Where a slot keeps the `u64` nanoseconds until the session closes
    /// the peer.
    slot_close: usize,
}

/// The values a [`JoinLayout`] is made from, as the build table gives them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct JoinOffsets {
    pub(crate) message_size: usize,
    pub(crate) count: usize,
    pub(crate) accounts: usize,
    pub(crate) account_size: usize,
    pub(crate) account_id: usize,
    pub(crate) account_kind: usize,
    /// Where the ban branch's frame keeps its reply.
    pub(crate) reply_frame: usize,
    /// Where that frame keeps the reply's two banned flags.
    pub(crate) banned_frame: [usize; 2],
    pub(crate) peer_slot_size: usize,
    /// Where the session keeps its peer slots, and the first slot's open
    /// flag and close wait.
    pub(crate) peer_slots: usize,
    pub(crate) peer_slot_open: usize,
    pub(crate) peer_slot_close: usize,
}

impl JoinLayout {
    /// The layout of `offsets`; else the build table id of the first entry
    /// that falls outside what holds it, as a misread table would give.
    pub(crate) fn new(offsets: &JoinOffsets) -> Result<Self, &'static str> {
        let o = offsets;
        let inside = |at: usize, size: usize, room: usize| {
            at.checked_add(size).is_some_and(|end| end <= room)
        };
        let fits = |holds: bool, id: &'static str| if holds { Ok(()) } else { Err(id) };
        let banned_a = o.banned_frame[0].checked_sub(o.reply_frame);
        let banned_b = o.banned_frame[1].checked_sub(o.reply_frame);
        let slot_open = o.peer_slot_open.checked_sub(o.peer_slots);
        let slot_close = o.peer_slot_close.checked_sub(o.peer_slots);
        fits(o.message_size <= MAX_MESSAGE, session::MESSAGE_SIZE.id())?;
        fits(
            inside(o.count, size_of::<u32>(), o.message_size),
            session::JOIN_COUNT.id(),
        )?;
        fits(
            o.account_size >= size_of::<u32>(),
            session::JOIN_ACCOUNT_SIZE.id(),
        )?;
        fits(
            inside(o.accounts, o.account_size, o.message_size),
            session::JOIN_ACCOUNTS.id(),
        )?;
        fits(
            inside(o.account_id, ID_SIZE, o.account_size),
            session::JOIN_ACCOUNT_ID.id(),
        )?;
        fits(
            inside(o.account_kind, 1, o.account_size),
            session::JOIN_ACCOUNT_KIND.id(),
        )?;
        let banned = [
            banned_a
                .filter(|&flag| inside(flag, 1, o.message_size))
                .ok_or(session::REPLY_BANNED_A.id())?,
            banned_b
                .filter(|&flag| inside(flag, 1, o.message_size))
                .ok_or(session::REPLY_BANNED_B.id())?,
        ];
        fits(
            o.peer_slot_size >= size_of::<u16>(),
            session::PEER_SLOT_SIZE.id(),
        )?;
        let slot_open = slot_open
            .filter(|&at| inside(at, 1, o.peer_slot_size))
            .ok_or(session::PEER_SLOT_OPEN.id())?;
        let slot_close = slot_close
            .filter(|&at| inside(at, size_of::<u64>(), o.peer_slot_size))
            .ok_or(session::PEER_SLOT_CLOSE.id())?;
        Ok(Self {
            message_size: o.message_size,
            count: o.count,
            accounts: o.accounts,
            account_size: o.account_size,
            account_id: o.account_id,
            account_kind: o.account_kind,
            banned,
            peer_slot_size: o.peer_slot_size,
            slot_open,
            slot_close,
        })
    }

    /// Where a join message keeps its `u32` count of accounts.
    pub(crate) const fn count(&self) -> usize {
        self.count
    }

    /// Where a join message's accounts start.
    pub(crate) const fn accounts(&self) -> usize {
        self.accounts
    }

    /// The bytes `count` accounts take from [`Self::accounts`], at most
    /// [`MAX_MESSAGE`]; `None` for a count past the entries the message
    /// holds, which no game sends.
    pub(crate) fn accounts_size(&self, count: u32) -> Option<usize> {
        let count = usize::try_from(count).ok()?;
        (count <= (self.message_size - self.accounts) / self.account_size)
            .then(|| count * self.account_size)
    }

    /// The key of each account entry in `entries`, as the game gives it:
    /// `None` for an account of kind zero, and on a session of another
    /// platform than Steam for every account.
    pub(crate) fn keys<'a>(
        &'a self,
        entries: &'a [u8],
        steam: bool,
    ) -> impl Iterator<Item = Option<PlayerKey>> + 'a {
        entries.chunks_exact(self.account_size).map(move |entry| {
            (steam && entry[self.account_kind] != 0).then(|| {
                let len = usize::try_from(u32_at(entry, 0)).map_or(ID_SIZE, |n| n.min(ID_SIZE));
                let mut id = [0; ID_SIZE];
                id[..len].copy_from_slice(&entry[self.account_id..][..len]);
                key(id)
            })
        })
    }

    /// The reply the ban branch sends, kind [`REFUSED_KIND`], in its first
    /// message size bytes.
    pub(crate) fn refusal(&self) -> [u8; MAX_MESSAGE] {
        let mut reply = [0; MAX_MESSAGE];
        for flag in self.banned {
            reply[flag] = 1;
        }
        reply
    }

    /// The size of the reply the session's send reads.
    #[cfg(test)]
    pub(crate) const fn message_size(&self) -> usize {
        self.message_size
    }

    /// Where peer `peer`'s slot starts among the session's peer slots.
    pub(crate) fn peer_slot(&self, peer: u16) -> usize {
        usize::from(peer & PEER_MASK) * self.peer_slot_size
    }

    /// Where a slot keeps its open flag.
    pub(crate) const fn slot_open(&self) -> usize {
        self.slot_open
    }

    /// Where a slot keeps its close wait.
    pub(crate) const fn slot_close(&self) -> usize {
        self.slot_close
    }

    /// The room the session's peer slots take.
    #[cfg(test)]
    pub(crate) fn peer_slots_size(&self) -> usize {
        (usize::from(PEER_MASK) + 1) * self.peer_slot_size
    }
}

/// The key the game gives the account `id`, the one its player and the ban
/// list carry.
fn key(id: [u8; ID_SIZE]) -> PlayerKey {
    PlayerKey::new(id.iter().fold(KEY_SEED, |hash, &b| {
        (hash ^ u64::from(b)).wrapping_mul(FNV_PRIME)
    }))
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    let mut word = [0; 4];
    word.copy_from_slice(&bytes[at..at + 4]);
    u32::from_le_bytes(word)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The offsets of server 26430253.
    pub(crate) fn offsets() -> JoinOffsets {
        JoinOffsets {
            message_size: 0x490,
            count: 0x38,
            accounts: 0x3c,
            account_size: 0x10,
            account_id: 4,
            account_kind: 0xc,
            reply_frame: 0x250,
            banned_frame: [0x2ac, 0x2a4],
            peer_slot_size: 0x10,
            peer_slots: 0x1_1418,
            peer_slot_open: 0x1_141a,
            peer_slot_close: 0x1_1420,
        }
    }

    fn layout() -> JoinLayout {
        JoinLayout::new(&offsets()).unwrap()
    }

    /// The keys of the account entries `(id length, id, kind)`, read as the
    /// join check reads a message.
    fn keys(entries: &[(u32, &[u8], u8)], steam: bool) -> Vec<Option<PlayerKey>> {
        let o = offsets();
        let mut bytes = vec![0; entries.len() * o.account_size];
        for (entry, (len, id, kind)) in bytes.chunks_exact_mut(o.account_size).zip(entries) {
            entry[..4].copy_from_slice(&len.to_le_bytes());
            entry[o.account_id..o.account_id + id.len()].copy_from_slice(id);
            entry[o.account_kind] = *kind;
        }
        let count = u32::try_from(entries.len()).unwrap();
        assert_eq!(layout().accounts_size(count), Some(bytes.len()));
        layout().keys(&bytes, steam).collect()
    }

    #[test]
    fn a_steam_account_has_the_key_its_player_and_the_ban_list_carry() {
        // Seen live on server 26430253: this Steam id's player and its
        // bannedAccounts entry carry the key.
        let steam = 76_561_198_447_973_054_u64.to_le_bytes();
        assert_eq!(
            keys(&[(8, &steam, STEAM)], true),
            [Some(PlayerKey::new(6_667_203_504_487_031_118))]
        );
    }

    #[test]
    fn an_account_of_any_kind_but_zero_has_a_key_and_a_short_id_hashes_zero_padded() {
        // Server 26430253's join handler keys an entry whenever its kind is
        // not zero on a Steam session, whatever the kind.
        let keys = keys(
            &[(8, &[1; 8], 0), (8, &[1; 8], 1), (2, &[7, 9], STEAM)],
            true,
        );
        assert_eq!(keys[0], None);
        assert_eq!(keys[1], Some(key([1; 8])));
        assert_eq!(keys[2], Some(key([7, 9, 0, 0, 0, 0, 0, 0])));
    }

    #[test]
    fn a_session_of_another_platform_keys_no_account() {
        assert_eq!(keys(&[(8, &[1; 8], STEAM)], false), [None]);
    }

    #[test]
    fn an_id_length_past_eight_bytes_reads_only_eight() {
        assert_eq!(
            keys(&[(u32::MAX, &[5; 8], STEAM)], true),
            [Some(key([5; 8]))]
        );
    }

    #[test]
    fn a_count_past_what_the_message_holds_reads_nothing() {
        let o = offsets();
        let most = (o.message_size - o.accounts) / o.account_size;
        let count = u32::try_from(most).unwrap();
        assert_eq!(layout().accounts_size(count), Some(most * o.account_size));
        assert_eq!(layout().accounts_size(count + 1), None);
        assert_eq!(layout().accounts_size(u32::MAX), None);
        assert!(most * o.account_size <= MAX_MESSAGE);
    }

    #[test]
    fn the_refusal_sets_only_the_two_banned_flags() {
        let reply = layout().refusal();
        let set: Vec<usize> = (0..reply.len()).filter(|&i| reply[i] != 0).collect();
        assert_eq!(set, [0x54, 0x5c]);
    }

    #[test]
    fn peer_slots_wrap_at_sixty_four_peers_with_their_fields_inside() {
        let layout = layout();
        assert_eq!(layout.peer_slot(1), 0x10);
        assert_eq!(layout.peer_slot(0x41), 0x10);
        assert_eq!(layout.peer_slot(0x3f), 0x3f0);
        assert_eq!((layout.slot_open(), layout.slot_close()), (2, 8));
        assert_eq!(layout.peer_slots_size(), 0x400);
    }

    #[test]
    fn a_layout_with_a_field_outside_what_holds_it_is_refused_naming_that_field() {
        let wrong = [
            (
                JoinOffsets {
                    message_size: 0x8000,
                    ..offsets()
                },
                session::MESSAGE_SIZE,
            ),
            (
                JoinOffsets {
                    count: 0x48e,
                    ..offsets()
                },
                session::JOIN_COUNT,
            ),
            (
                JoinOffsets {
                    account_id: 0xc,
                    ..offsets()
                },
                session::JOIN_ACCOUNT_ID,
            ),
            (
                JoinOffsets {
                    account_kind: 0x10,
                    ..offsets()
                },
                session::JOIN_ACCOUNT_KIND,
            ),
            (
                JoinOffsets {
                    banned_frame: [0x2ac, 0x200],
                    ..offsets()
                },
                session::REPLY_BANNED_B,
            ),
            (
                JoinOffsets {
                    banned_frame: [0x6e0, 0x2a4],
                    ..offsets()
                },
                session::REPLY_BANNED_A,
            ),
            (
                JoinOffsets {
                    peer_slot_close: 0x1_1428,
                    ..offsets()
                },
                session::PEER_SLOT_CLOSE,
            ),
            (
                JoinOffsets {
                    peer_slot_open: 0x1_1400,
                    ..offsets()
                },
                session::PEER_SLOT_OPEN,
            ),
        ];
        for (offsets, entry) in wrong {
            assert_eq!(JoinLayout::new(&offsets), Err(entry.id()), "{offsets:?}");
        }
    }
}
