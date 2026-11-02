//! The game's message socket calls the carrier makes, from the build table,
//! each checked against its code before the first call.
//!
//! The argument layouts below are those of server 399 and client 426 (SVN
//! 1273043); the game reflects none of them. Each function's pattern or
//! body in the table spans the code that reads them, so a build that moves
//! a field fails the check and the own channel stays off.

use core::ffi::c_void;
use std::time::Instant;

use crate::catalogue;
use crate::host::Loader;
use crate::memory;
use crate::pinned;
use crate::pinned::{Constant, Symbol};
use crate::wire::Link;

use super::{LARGEST_PACKET, PACKET_BURST, Peer, Probe};

/// The socket a create call makes: its kind, its game channel, and the
/// machine it serves for a machine socket.
#[repr(C)]
#[derive(Default)]
struct Descriptor {
    kind: u8,
    _pad: [u8; 3],
    channel: u32,
    user: u64,
    local_player: u32,
    machine: u32,
    player: u32,
    tail: u32,
}

/// A send request: the message's least and most bytes, and that it has a
/// size.
#[repr(C)]
struct Request {
    min: u64,
    max: u64,
    sized: u8,
    _pad: [u8; 7],
}

/// An open message, which commit sends and abort drops: its bytes go into
/// the buffer, their count into `size`.
#[repr(C)]
#[derive(Default)]
struct Handle {
    message: usize,
    socket: usize,
    reserved: u64,
    size: u32,
    tail: u32,
}

/// What open answers.
#[repr(C)]
#[derive(Default)]
struct Opened {
    status: u8,
    _pad: [u8; 7],
    handle: Handle,
    data: usize,
    capacity: u64,
}

/// What receive answers; `node` onwards is the message release takes.
#[repr(C)]
#[derive(Default)]
struct Received {
    status: u8,
    _pad: [u8; 7],
    node: usize,
    sender: u32,
    channel: u32,
    data: usize,
    size: u64,
}

const _: () = {
    assert!(size_of::<Descriptor>() == 0x20);
    assert!(size_of::<Request>() == 0x18);
    assert!(size_of::<Opened>() == 0x38);
    assert!(size_of::<Received>() == 0x28);
};

/// A message receive took.
pub(super) enum Taken {
    /// Its bytes, copied out.
    Bytes(usize),
    /// Longer than the buffer: dropped.
    TooLong,
}

/// Open's and receive's status for a message.
const OK: u8 = 0;
/// The session's packet limit modes: one packet, the u16 limit of long
/// messages, the oversize limit.
const LIMIT_MODES: [u8; 3] = [0, 1, 2];

type CreateFn = unsafe extern "system" fn(session: usize, descriptor: *const Descriptor) -> usize;
type DestroyFn = unsafe extern "system" fn(session: usize, socket: usize);
type OpenFn = unsafe extern "system" fn(
    out: *mut Opened,
    session: usize,
    socket: usize,
    request: *const Request,
) -> *mut Opened;
type FinishFn = unsafe extern "system" fn(session: usize, handle: *mut Handle);
type ReceiveFn =
    unsafe extern "system" fn(out: *mut Received, session: usize, socket: usize) -> *mut Received;
type ReleaseFn = unsafe extern "system" fn(session: usize, message: *mut c_void);
type LimitFn = unsafe extern "system" fn(manager: usize, mode: u8) -> u32;

/// The game's socket calls and the offsets they need on this build.
#[derive(Clone, Copy, Debug)]
pub(super) struct Game {
    create: CreateFn,
    destroy: DestroyFn,
    open: OpenFn,
    commit: FinishFn,
    abort: FinishFn,
    receive: ReceiveFn,
    release: ReleaseFn,
    limit: LimitFn,
    /// The session's socket manager, from the session.
    manager: usize,
    /// The game channel the sockets use.
    channel: u32,
    /// The same channel as the session numbers it, which a socket keeps.
    session_channel: u32,
    socket_channel: usize,
    socket_peer: usize,
    machine_kind: u8,
    /// A client's own machine socket kind; `None` on the server.
    local_kind: Option<u8>,
    /// The game client's net object, whose first word is its session.
    client_net: Option<usize>,
    server_session: usize,
    host_machine: usize,
}

/// The entry of game function `F` at `address`.
///
/// # Safety
///
/// - `address` is the entry, checked by [`Loader::code_holds`], of a game
///   function of type `F` on this build.
unsafe fn function<F: Copy>(address: usize) -> F {
    const { assert!(size_of::<F>() == size_of::<usize>()) };
    // SAFETY: `F` is a function pointer of `usize`'s size, by the assertion
    // above, and `address` the entry of a function of its type by this
    // function's `# Safety`.
    unsafe { core::mem::transmute_copy::<usize, F>(&address) }
}

impl Game {
    /// The calls and offsets from `loader`'s table and the game's
    /// reflection; the first id it lacks, or whose code does not hold.
    pub(super) fn resolve(loader: &Loader) -> Result<Self, &'static str> {
        let call = |name: Symbol| {
            loader
                .address(name)
                .filter(|_| loader.code_holds(name))
                .and_then(|address| usize::try_from(address).ok())
                .ok_or(name.id())
        };
        let offset = |name: Constant| loader.offset(name).ok_or(name.id());
        let byte = |name: Constant| {
            offset(name).and_then(|value| u8::try_from(value).map_err(|_| name.id()))
        };
        let channel = catalogue::constant(
            catalogue::ids::GAME_MESSAGE_CHANNEL_VOICE_CHAT,
            &loader.reflection,
        )
        .and_then(|value| u32::try_from(value).ok())
        .ok_or(catalogue::ids::GAME_MESSAGE_CHANNEL_VOICE_CHAT)?;
        let base = offset(pinned::net::CHANNEL_BASE)
            .and_then(|value| u32::try_from(value).map_err(|_| pinned::net::CHANNEL_BASE.id()))?;
        let client = loader.side == enshwick::abi::Side::CLIENT;
        Ok(Self {
            // SAFETY: `net.socket_create` is the game's socket create on
            // this build, `CreateFn` on server 399 and client 426, checked by
            // `call`; and so for each call below.
            create: unsafe { function(call(pinned::net::SOCKET_CREATE)?) },
            // SAFETY: `net.socket_destroy`, `DestroyFn`, checked by `call`.
            destroy: unsafe { function(call(pinned::net::SOCKET_DESTROY)?) },
            // SAFETY: `net.message_open`, `OpenFn`, checked by `call`.
            open: unsafe { function(call(pinned::net::MESSAGE_OPEN)?) },
            // SAFETY: `net.message_commit`, `FinishFn`, checked by `call`.
            commit: unsafe { function(call(pinned::net::MESSAGE_COMMIT)?) },
            // SAFETY: `net.message_abort`, `FinishFn`, checked by `call`.
            abort: unsafe { function(call(pinned::net::MESSAGE_ABORT)?) },
            // SAFETY: `net.message_receive`, `ReceiveFn`, checked by `call`.
            receive: unsafe { function(call(pinned::net::MESSAGE_RECEIVE)?) },
            // SAFETY: `net.message_release`, `ReleaseFn`, checked by `call`.
            release: unsafe { function(call(pinned::net::MESSAGE_RELEASE)?) },
            // SAFETY: `net.session_limit`, `LimitFn`, checked by `call`.
            limit: unsafe { function(call(pinned::net::SESSION_LIMIT)?) },
            manager: offset(pinned::net::SESSION_MANAGER)?,
            channel,
            session_channel: channel
                .checked_add(base)
                .ok_or(pinned::net::CHANNEL_BASE.id())?,
            socket_channel: offset(pinned::net::SOCKET_CHANNEL)?,
            socket_peer: offset(pinned::net::SOCKET_PEER)?,
            machine_kind: byte(pinned::net::SOCKET_KIND_MACHINE)?,
            local_kind: if client {
                Some(byte(pinned::net::SOCKET_KIND_LOCAL_MACHINE)?)
            } else {
                None
            },
            client_net: if client {
                Some(offset(pinned::net::CLIENT)?)
            } else {
                None
            },
            server_session: offset(pinned::server::SESSION)?,
            host_machine: offset(pinned::session::HOST_MACHINE)?,
        })
    }

    pub(super) const fn channel(&self) -> u32 {
        self.channel
    }

    pub(super) const fn machine_kind(&self) -> u8 {
        self.machine_kind
    }

    pub(super) const fn local_machine_kind(&self) -> Option<u8> {
        self.local_kind
    }

    /// The session of the live `server`; `None` while it has none.
    pub(super) fn server_session(&self, server: usize) -> Option<usize> {
        // SAFETY: `server` is the live server on its game thread between
        // steps (`world::server`); its session pointer, null while it has
        // none, sits at the table's `server.session`.
        let session: usize = unsafe { memory::read(server, self.server_session) };
        (session != 0).then_some(session)
    }

    /// The machine handle of the session's host.
    pub(super) fn host_machine(&self, session: usize) -> u32 {
        // SAFETY: `session` is the live session, whose host machine handle
        // sits at the table's `session.host_machine`.
        unsafe { memory::read(session, self.host_machine) }
    }

    /// The session of the game client `client`; `None` without one.
    ///
    /// # Safety
    ///
    /// - `client` is the game client, non-null, on the thread that sends its
    ///   chat.
    pub(super) unsafe fn client_session(&self, client: *mut u8) -> Option<usize> {
        let offset = self.client_net?;
        // SAFETY: `client` is the game client by this function's `# Safety`;
        // its net object pointer sits at the table's `net.client`, which the
        // game's chat send reads without a null check on client 426.
        let net: usize = unsafe { memory::read(client.expose_provenance(), offset) };
        if net == 0 {
            return None;
        }
        // SAFETY: `net` is the client's non-null net object, whose first
        // word is its session on client 426, as the chat send reads it.
        let session: usize = unsafe { memory::read(net, 0) };
        (session != 0).then_some(session)
    }

    /// Whether `socket` still holds the channel and peer it was opened for:
    /// a slot the session gave to another socket does not.
    pub(super) fn holds(&self, socket: usize, slot: u32) -> bool {
        // SAFETY: `socket` is a slot of the live session's socket array,
        // which lives as long as the session; the channel and peer fields at
        // the table's offsets are written once, when the slot is taken.
        let channel: u32 = unsafe { memory::read(socket, self.socket_channel) };
        // SAFETY: as for the channel field: the same slot, its peer field.
        let peer: u32 = unsafe { memory::read(socket, self.socket_peer) };
        channel == self.session_channel && peer == slot
    }

    /// Opens a socket of `kind` on the own channel, for `machine` on the
    /// server, with its link sized to the session's packets and numbered
    /// from `epoch`.
    pub(super) fn open_peer(
        &self,
        session: usize,
        kind: u8,
        machine: u32,
        epoch: u32,
        now: Instant,
    ) -> Option<Peer> {
        let descriptor = Descriptor {
            kind,
            channel: self.channel,
            machine,
            ..Descriptor::default()
        };
        // SAFETY: `session` is the live session on its game thread, and the
        // descriptor names a socket kind this build's table gives; create
        // logs and answers null when it cannot make one.
        let socket = unsafe { (self.create)(session, &raw const descriptor) };
        if socket == 0 {
            return None;
        }
        // SAFETY: `socket` is the slot create just took, whose peer field it
        // wrote at the table's `net.socket_peer` before it answered.
        let slot: u32 = unsafe { memory::read(socket, self.socket_peer) };
        let manager = session.wrapping_add(self.manager);
        // SAFETY: `manager` is the live session's socket manager at the
        // table's `net.session_manager`, which the limit call reads.
        let limits = LIMIT_MODES.map(|mode| unsafe { (self.limit)(manager, mode) });
        crate::log!(
            INFO,
            "own channel: the session takes {} bytes a packet, {} and {} for long messages",
            limits[0],
            limits[1],
            limits[2]
        );
        let packet = usize::try_from(limits[0]).ok()?.min(LARGEST_PACKET);
        let Some(link) = Link::new(packet, epoch) else {
            crate::log!(
                WARN,
                "own channel off: packets of {packet} bytes hold no message"
            );
            self.close(session, socket);
            return None;
        };
        Some(Peer {
            machine,
            socket,
            slot,
            link,
            heard: false,
            rate: crate::wire::Peer::with_tokens(PACKET_BURST),
            opened: now,
            nudged: false,
            probe: Probe::default(),
        })
    }

    /// Closes `socket`.
    pub(super) fn close(&self, session: usize, socket: usize) {
        // SAFETY: `socket` is one the carrier opened in the live `session`
        // and still holds; destroy gives its slot back.
        unsafe { (self.destroy)(session, socket) };
    }

    /// Takes the next message on `socket` into `out`; `None` when none
    /// waits.
    pub(super) fn take(&self, session: usize, socket: usize, out: &mut [u8]) -> Option<Taken> {
        let mut received = Received::default();
        // SAFETY: `session` is live on its game thread and `socket` one of
        // its sockets; receive fills `received` and takes a lock of its own.
        unsafe { (self.receive)(&raw mut received, session, socket) };
        if received.status != OK {
            return None;
        }
        let size = usize::try_from(received.size).ok();
        let fits = size.filter(|&size| received.data != 0 && size <= out.len());
        if let Some(size) = fits {
            // SAFETY: receive answered `size` bytes at `data`, which stay the
            // game's until the release below; `out` holds `size`.
            unsafe { memory::read_bytes(received.data, 0, &mut out[..size]) };
        }
        // SAFETY: `received.node` onwards is the message receive answered,
        // released once as the game's own receive loop releases it.
        unsafe { (self.release)(session, (&raw mut received.node).cast()) };
        Some(fits.map_or(Taken::TooLong, Taken::Bytes))
    }

    /// Sends `packet` on `socket`; false when the session refused it, which
    /// the link sends again.
    pub(super) fn transmit(&self, session: usize, socket: usize, packet: &[u8]) -> bool {
        let Ok(size) = u32::try_from(packet.len()) else {
            return false;
        };
        let request = Request {
            min: u64::from(size),
            max: u64::from(size),
            sized: 1,
            _pad: [0; 7],
        };
        let mut opened = Opened::default();
        // SAFETY: `session` is live on its game thread, `socket` one of its
        // sockets and `request` asks for exactly the packet's bytes.
        unsafe { (self.open)(&raw mut opened, session, socket, &raw const request) };
        if opened.status != OK {
            return false;
        }
        if opened.data == 0 || opened.capacity < u64::from(size) {
            // SAFETY: open answered a message, which abort drops unsent.
            unsafe { (self.abort)(session, &raw mut opened.handle) };
            return false;
        }
        // SAFETY: open answered a buffer of `capacity` bytes, at least the
        // packet's, which is the carrier's until commit.
        unsafe { memory::write_bytes(opened.data, 0, packet) };
        opened.handle.size = size;
        // SAFETY: `opened.handle` is the open message with its size written,
        // which commit queues once.
        unsafe { (self.commit)(session, &raw mut opened.handle) };
        true
    }
}
