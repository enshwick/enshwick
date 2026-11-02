//! Reliable, ordered delivery of sealed messages to one peer over packets
//! the game may lose, repeat or reorder.
//!
//! A message is cut into fragments that each fit one packet; fragments are
//! numbered in one sequence across messages, the last of a message marked.
//! Every packet carries a cumulative ack and a bitmap of the 64 fragments
//! after it, so a lost fragment goes again alone. Sizes, the fragments in
//! flight and the bytes a message gathers are bounded; a peer that stops
//! acknowledging is given up on. The link reads only the clock it is handed.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::MAX_SEALED;

/// The version of the link's packet header, its first byte.
pub const LINK: u8 = 1;
/// The longest packet header: the version, a varint ack, a varint ack
/// bitmap, a varint sequence number and the flags.
pub const MAX_LINK_HEADER: usize = 1 + 5 + 10 + 5 + 1;
/// Fragments sent and not yet acknowledged at most.
pub const WINDOW: u32 = 64;
/// Fragments a link holds at most: in flight and waiting for the window.
pub const BACKLOG: u32 = 512;
/// How long a fragment waits for its ack before it goes again, at first.
pub const FIRST_TIMEOUT: Duration = Duration::from_millis(200);
/// The longest wait for an ack, after backing off.
pub const LAST_TIMEOUT: Duration = Duration::from_secs(2);
/// How often one fragment goes before the peer counts as gone: about 19
/// seconds of silence with the timeouts above.
pub const MAX_TRIES: u8 = 12;

/// Flag: the packet carries a fragment.
const DATA: u8 = 1;
/// Flag: the fragment ends its message.
const LAST: u8 = 2;

#[derive(Serialize, Deserialize)]
struct Head {
    version: u8,
    ack: u32,
    acks: u64,
    seq: u32,
    flags: u8,
}

/// Why a received packet was refused.
///
/// # Examples
///
/// ```
/// use enshwick_loader::wire::{Link, ReadPacketError};
///
/// let mut link = Link::new(1200).ok_or("packet too small")?;
/// assert_eq!(link.receive(&[], |_| {}), Err(ReadPacketError::Unreadable));
/// # Ok::<(), &str>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ReadPacketError {
    /// The header does not read, or names fragments never sent.
    #[error("an unreadable packet")]
    Unreadable,
    /// Another link version.
    #[error("link version {0}")]
    Version(u8),
    /// A fragment over the packet size, or a message over
    /// [`MAX_SEALED`](super::MAX_SEALED) bytes.
    #[error("a fragment or message over its bound")]
    TooLarge,
}

/// The backlog has no room for the message now.
///
/// # Examples
///
/// ```
/// use enshwick_loader::wire::{BACKLOG, Link};
///
/// let mut link = Link::new(64).ok_or("packet too small")?;
/// while link.send(&[7; 40]).is_ok() {}
/// assert_eq!(link.backlog(), BACKLOG);
/// # Ok::<(), &str>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the link to the peer is full")]
pub struct QueueMessageError;

/// A fragment on its way out.
#[derive(Debug, Default)]
struct Outgoing {
    data: Vec<u8>,
    last: bool,
    acked: bool,
    sent: Option<Instant>,
    tries: u8,
    timeout: Duration,
}

/// A fragment that came before the ones it follows.
#[derive(Debug, Default)]
struct Early {
    data: Vec<u8>,
    last: bool,
    here: bool,
}

/// One direction pair of reliable delivery with one peer.
///
/// # Examples
///
/// ```
/// use std::time::Instant;
/// use enshwick_loader::wire::Link;
///
/// let (mut a, mut b) = (Link::new(1200).ok_or("small")?, Link::new(1200).ok_or("small")?);
/// a.send(&[1; 3000]).map_err(|_| "full")?;
/// let mut packets = Vec::new();
/// a.poll(Instant::now(), usize::MAX, |p| packets.push(p.to_vec()));
/// let mut got = Vec::new();
/// for packet in &packets {
///     b.receive(packet, |message| got.push(message.len())).map_err(|_| "refused")?;
/// }
/// assert_eq!(got, [3000]);
/// # Ok::<(), &str>(())
/// ```
#[derive(Debug)]
pub struct Link {
    /// Fragment bytes per packet.
    fragment: usize,
    /// The oldest fragment not yet acknowledged.
    base: u32,
    /// The sequence number the next fragment takes.
    next: u32,
    outgoing: Box<[Outgoing]>,
    /// The fragment this side delivers next.
    expect: u32,
    early: Box<[Early]>,
    /// The message gathered from fragments so far.
    gathered: Vec<u8>,
    ack_due: bool,
    packet: Vec<u8>,
}

impl Link {
    /// A link whose packets hold at most `packet` bytes; `None` when that
    /// leaves no room for a fragment.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader::wire::{Link, MAX_LINK_HEADER};
    ///
    /// assert!(Link::new(MAX_LINK_HEADER).is_none());
    /// assert!(Link::new(MAX_LINK_HEADER + 1).is_some());
    /// ```
    #[must_use]
    pub fn new(packet: usize) -> Option<Self> {
        let fragment = packet.checked_sub(MAX_LINK_HEADER).filter(|&f| f > 0)?;
        Some(Self {
            fragment,
            base: 0,
            next: 0,
            outgoing: (0..BACKLOG).map(|_| Outgoing::default()).collect(),
            expect: 0,
            early: (0..WINDOW).map(|_| Early::default()).collect(),
            gathered: Vec::with_capacity(MAX_SEALED),
            ack_due: false,
            packet: Vec::with_capacity(packet),
        })
    }

    /// Fragments held now: sent and unacknowledged, or waiting.
    ///
    /// # Examples
    ///
    /// See [`QueueMessageError`].
    #[must_use]
    pub const fn backlog(&self) -> u32 {
        self.next.wrapping_sub(self.base)
    }

    /// Whether every fragment sent was acknowledged.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader::wire::Link;
    ///
    /// assert!(Link::new(1200).ok_or("small")?.is_idle());
    /// # Ok::<(), &str>(())
    /// ```
    #[must_use]
    pub const fn is_idle(&self) -> bool {
        self.base == self.next
    }

    /// Queues the sealed `message`, cut into fragments.
    ///
    /// # Errors
    ///
    /// [`QueueMessageError`] when the backlog has no room for its fragments
    /// or the message is over [`MAX_SEALED`](super::MAX_SEALED) bytes.
    ///
    /// # Examples
    ///
    /// See [`Link`].
    pub fn send(&mut self, message: &[u8]) -> Result<(), QueueMessageError> {
        let count = message.len().div_ceil(self.fragment).max(1);
        let room = usize::try_from(BACKLOG - self.backlog()).unwrap_or(0);
        if message.len() > MAX_SEALED || count > room || self.next > u32::MAX - BACKLOG {
            return Err(QueueMessageError);
        }
        let mut parts = message.chunks(self.fragment).peekable();
        for index in 0..count {
            let part = parts.next().unwrap_or_default();
            let slot = &mut self.outgoing[ring(self.next, BACKLOG)];
            slot.data.clear();
            slot.data.extend_from_slice(part);
            slot.last = index + 1 == count;
            slot.acked = false;
            slot.sent = None;
            slot.tries = 0;
            slot.timeout = FIRST_TIMEOUT;
            self.next += 1;
        }
        Ok(())
    }

    /// Takes a packet from the peer and hands each message it completes,
    /// whole and in order, to `deliver`.
    ///
    /// # Errors
    ///
    /// A [`ReadPacketError`] for a packet that does not read or breaks a
    /// bound; the link stays as it was, but a peer that sends one is broken.
    ///
    /// # Examples
    ///
    /// See [`Link`].
    pub fn receive(
        &mut self,
        packet: &[u8],
        mut deliver: impl FnMut(&[u8]),
    ) -> Result<(), ReadPacketError> {
        match packet.first() {
            Some(&LINK) => {}
            Some(&version) => return Err(ReadPacketError::Version(version)),
            None => return Err(ReadPacketError::Unreadable),
        }
        let (head, fragment) =
            postcard::take_from_bytes::<Head>(packet).map_err(|_| ReadPacketError::Unreadable)?;
        if head.ack > self.next {
            return Err(ReadPacketError::Unreadable);
        }
        if fragment.len() > self.fragment {
            return Err(ReadPacketError::TooLarge);
        }
        self.acknowledged(head.ack, head.acks);
        if head.flags & DATA == 0 {
            return Ok(());
        }
        self.ack_due = true;
        let ahead = head.seq.wrapping_sub(self.expect);
        if head.seq < self.expect || ahead >= WINDOW {
            return Ok(());
        }
        let last = head.flags & LAST != 0;
        if ahead > 0 {
            let slot = &mut self.early[ring(head.seq, WINDOW)];
            if !slot.here {
                slot.data.clear();
                slot.data.extend_from_slice(fragment);
                slot.last = last;
                slot.here = true;
            }
            return Ok(());
        }
        self.take(fragment, last, &mut deliver)?;
        self.expect += 1;
        loop {
            let index = ring(self.expect, WINDOW);
            if !self.early[index].here {
                return Ok(());
            }
            let data = core::mem::take(&mut self.early[index].data);
            let last = self.early[index].last;
            self.early[index].here = false;
            let taken = self.take(&data, last, &mut deliver);
            self.early[index].data = data;
            taken?;
            self.expect += 1;
        }
    }

    /// Hands `transmit` the packets due at `now`, at most `budget`: new and
    /// timed-out fragments within the window, then an ack when one is owed
    /// and no fragment carried it. False once a fragment went
    /// [`MAX_TRIES`] times unacknowledged: the peer is gone.
    ///
    /// # Examples
    ///
    /// See [`Link`].
    pub fn poll(&mut self, now: Instant, budget: usize, mut transmit: impl FnMut(&[u8])) -> bool {
        let (ack, acks) = self.acks();
        let end = self.next.min(self.base.saturating_add(WINDOW));
        let mut sent = 0;
        for seq in self.base..end {
            if sent == budget {
                break;
            }
            let slot = &mut self.outgoing[ring(seq, BACKLOG)];
            if slot.acked
                || slot
                    .sent
                    .is_some_and(|at| now.saturating_duration_since(at) < slot.timeout)
            {
                continue;
            }
            if slot.tries == MAX_TRIES {
                return false;
            }
            if slot.sent.is_some() {
                slot.timeout = (slot.timeout * 2).min(LAST_TIMEOUT);
            }
            slot.tries += 1;
            slot.sent = Some(now);
            let head = Head {
                version: LINK,
                ack,
                acks,
                seq,
                flags: DATA | if slot.last { LAST } else { 0 },
            };
            transmit(write(&mut self.packet, &head, &slot.data));
            sent += 1;
            self.ack_due = false;
        }
        if self.ack_due && sent < budget {
            let head = Head {
                version: LINK,
                ack,
                acks,
                seq: 0,
                flags: 0,
            };
            transmit(write(&mut self.packet, &head, &[]));
            self.ack_due = false;
        }
        true
    }

    /// Marks fragments below `ack` and those `acks` names acknowledged, and
    /// moves the base past the acknowledged run.
    fn acknowledged(&mut self, ack: u32, acks: u64) {
        for seq in self.base..ack {
            self.outgoing[ring(seq, BACKLOG)].acked = true;
        }
        for bit in 0..u64::BITS {
            let seq = ack.saturating_add(1 + bit);
            if acks >> bit & 1 == 1 && seq >= self.base && seq < self.next {
                self.outgoing[ring(seq, BACKLOG)].acked = true;
            }
        }
        while self.base < self.next && self.outgoing[ring(self.base, BACKLOG)].acked {
            self.base += 1;
        }
    }

    /// The cumulative ack and the bitmap of the fragments after it that
    /// came early.
    fn acks(&self) -> (u32, u64) {
        let mut acks = 0u64;
        for bit in 0..WINDOW - 1 {
            if self.early[ring(self.expect.wrapping_add(1 + bit), WINDOW)].here {
                acks |= 1 << bit;
            }
        }
        (self.expect, acks)
    }

    /// Adds the next fragment in order to the message it belongs to.
    fn take(
        &mut self,
        fragment: &[u8],
        last: bool,
        deliver: &mut impl FnMut(&[u8]),
    ) -> Result<(), ReadPacketError> {
        if self.gathered.is_empty() && last {
            deliver(fragment);
            return Ok(());
        }
        if self.gathered.len() + fragment.len() > MAX_SEALED {
            self.gathered.clear();
            return Err(ReadPacketError::TooLarge);
        }
        self.gathered.extend_from_slice(fragment);
        if last {
            deliver(&self.gathered);
            self.gathered.clear();
        }
        Ok(())
    }
}

/// The slot of `seq` in a ring of `size` slots, a power of two.
const fn ring(seq: u32, size: u32) -> usize {
    (seq & (size - 1)) as usize
}

/// Writes the packet of `head` and `fragment` into `packet`.
fn write<'p>(packet: &'p mut Vec<u8>, head: &Head, fragment: &[u8]) -> &'p [u8] {
    let mut header = [0; MAX_LINK_HEADER];
    let size = postcard::to_slice(head, &mut header).map_or(0, |written| written.len());
    packet.clear();
    packet.extend_from_slice(&header[..size]);
    packet.extend_from_slice(fragment);
    packet
}

#[cfg(test)]
mod tests;
