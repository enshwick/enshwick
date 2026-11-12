//! A client that hosts the game: its client half and its server half talk
//! in process, through two queues, not through the game's sockets.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use enshwick_core::abi::{Side, Status};

use super::{CARRIER, MAX_LOOPED, MEMBERS_EVERY};
use crate::host::Loader;
use enshwick_base::Relock;

/// A message between a hosting client's two halves.
#[derive(Debug)]
pub(super) struct Looped {
    /// The host's own player when it was queued, which a later host does
    /// not change.
    pub(super) host: u64,
    pub(super) channel: u32,
    pub(super) data: Vec<u8>,
}

/// Whether this client runs the game's server itself, in single player or
/// a game it hosts: its two halves then talk in process. A dedicated
/// server runs a server too, but no client half.
pub(super) fn client_hosts() -> bool {
    #[cfg(test)]
    if CARRIER.hosts.load(Ordering::Acquire) {
        return true;
    }
    crate::engine::world::hosting()
        && crate::host::loader().is_some_and(|loader| loader.side == Side::CLIENT)
}

/// Makes this test process a client that hosts, its own player `key`.
#[cfg(test)]
pub(crate) fn host_for_test(key: u64) {
    CARRIER.hosts.store(true, Ordering::Release);
    set_host(key);
}

/// Makes this test process a client that hosts nothing again, after
/// [`host_for_test`].
#[cfg(test)]
pub(crate) fn unhost_for_test() {
    CARRIER.hosts.store(false, Ordering::Release);
    CARRIER.host_key.store(0, Ordering::Release);
}

/// Notes the account key of the player of a client that hosts; a new one
/// makes the client half say hello to its server half again.
pub(super) fn set_host(key: u64) {
    let old = CARRIER.host_key.swap(key, Ordering::AcqRel);
    if old != key {
        CARRIER.hosted_hello.store(false, Ordering::Release);
    }
}

/// Whether `key` is the own player of this client that hosts the game.
pub(crate) fn is_host(key: u64) -> bool {
    key != 0 && client_hosts() && CARRIER.host_key.load(Ordering::Acquire) == key
}

pub(super) fn loop_back(
    to_server: bool,
    host: u64,
    channel: u32,
    payload: &[u8],
) -> Result<(), Status> {
    // Before the server half knows its own player, nobody would take it.
    if host == 0 {
        return Err(Status::MISSING);
    }
    let (queue, waiting) = looped_queue(to_server);
    let mut looped = queue.relock();
    if looped.len() >= MAX_LOOPED {
        return Err(Status::REFUSED);
    }
    looped.push(Looped {
        host,
        channel,
        data: payload.to_vec(),
    });
    waiting.store(true, Ordering::Release);
    Ok(())
}

/// The queue for one half of a hosting client, and its flag.
fn looped_queue(to_server: bool) -> (&'static Mutex<Vec<Looped>>, &'static AtomicBool) {
    if to_server {
        (&CARRIER.to_server, &CARRIER.server_waiting)
    } else {
        (&CARRIER.to_client, &CARRIER.client_waiting)
    }
}

/// The messages held for one half of a hosting client, taken whole; no
/// lock while its queue is empty.
pub(super) fn take_looped(to_server: bool) -> Vec<Looped> {
    let (queue, waiting) = looped_queue(to_server);
    if !waiting.swap(false, Ordering::AcqRel) {
        return Vec::new();
    }
    core::mem::take(&mut *queue.relock())
}

pub(super) fn hosted_step(loader: &Loader) {
    let step = CARRIER.hosted_steps.fetch_add(1, Ordering::Relaxed);
    if CARRIER.host_key.load(Ordering::Acquire) == 0
        && step.is_multiple_of(MEMBERS_EVERY)
        && let Ok(machines) = crate::engine::world::machines(loader)
        && let [(key, _)] = machines.as_slice()
    {
        set_host(*key);
    }
    take_hosted();
}

/// Hands a hosting client's server half what its client half sent, each
/// from the host it was queued under.
pub(crate) fn take_hosted() {
    for message in take_looped(true) {
        crate::base::net::server_carried(message.host, message.channel, &message.data, true);
    }
}
