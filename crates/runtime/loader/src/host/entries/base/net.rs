//! Host functions for mod messages between server and client.

use enshwick_core::abi as api_frame;
use enshwick_core::abi::Status;

use crate::host::layer::{Caller, List, Mod, Out, Text, entries};

entries! {
    fn net_channel(
        me as Mod,
        name as Text,
        callback: api_frame::NetMessageFn,
        user: *mut core::ffi::c_void,
        out as Out<u32>,
    ) -> Status {
        out.put(crate::base::net::open(me.loader, me.index, name, callback, user)?)
    }

    fn net_send(
        me as Mod,
        channel: u32,
        peer: enshwick_core::PlayerKey,
        [data, size] as List<u8>,
    ) -> Status {
        crate::base::net::send(me.loader, me.index, channel, peer.get(), data)?;
        Ok(())
    }

    fn net_ready(_loader as Caller, peer: enshwick_core::PlayerKey) -> Status {
        if crate::base::net::ready(peer.get()) {
            Ok(())
        } else {
            Err(Status::MISSING.into())
        }
    }
}
