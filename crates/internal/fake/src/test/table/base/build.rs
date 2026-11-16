//! The fake's answers about the game build: symbols, systems, layouts and constants.

use crate::test::FakeHost;
use crate::test::table::{found, put, text, with};
use enshwick_core::abi::{BuildInfo, CallerFrame, Header, Level, ModHandle, Status, Str};

pub(crate) extern "system" fn build(_me: ModHandle, out: *mut BuildInfo) -> Status {
    let host = with(FakeHost::new(), |s| s.host.clone());
    crate::test::table::put_headed(
        out,
        BuildInfo {
            header: Header::of::<BuildInfo>(),
            side: host.side,
            base: host.base,
            fingerprint: host.fingerprint,
            complete: u8::from(host.build_complete),
        },
    )
}

pub(crate) extern "system" fn symbol(_me: ModHandle, name: Str, out: *mut u64) -> Status {
    let name = text(name);
    found(with(None, |s| s.host.symbols.get(&name).copied()), out)
}

pub(crate) extern "system" fn system(_me: ModHandle, name: Str, out: *mut u64) -> Status {
    let name = text(name);
    found(with(None, |s| s.host.systems.get(&name).copied()), out)
}

pub(crate) extern "system" fn field_offset(
    _me: ModHandle,
    type_name: Str,
    field: Str,
    out: *mut u64,
) -> Status {
    let key = (text(type_name), text(field));
    found(with(None, |s| s.host.field_offsets.get(&key).copied()), out)
}

pub(crate) extern "system" fn type_size(_me: ModHandle, type_name: Str, out: *mut u32) -> Status {
    let name = text(type_name);
    found(with(None, |s| s.host.type_sizes.get(&name).copied()), out)
}

pub(crate) extern "system" fn caller_frame(_me: ModHandle, out: *mut CallerFrame) -> Status {
    let frame = with(CallerFrame::default(), |s| s.host.caller_frame);
    put(out, frame)
}

pub(crate) extern "system" fn constant(_me: ModHandle, name: Str, out: *mut u64) -> Status {
    let name = text(name);
    found(with(None, |s| s.host.constants.get(&name).copied()), out)
}

pub(crate) extern "system" fn data_dir(_me: ModHandle, out: *mut Str) -> Status {
    let dir = with(None, |s| {
        s.host.world_key.is_some().then(|| s.host.data_dir.clone())
    });
    // The loader's path lives for the process; a test leaks a few bytes for it.
    found(
        dir.map(|dir| Str::new(Box::leak(dir.into_boxed_str()))),
        out,
    )
}

pub(crate) extern "system" fn mod_dir(_me: ModHandle, out: *mut Str) -> Status {
    let dir = with(None, |s| Some(s.host.mod_dir.clone()));
    // The loader's path lives for the process; a test leaks a few bytes for it.
    found(
        dir.map(|dir| Str::new(Box::leak(dir.into_boxed_str()))),
        out,
    )
}

pub(crate) extern "system" fn log_level(me: ModHandle, out: *mut Level) -> Status {
    let level = with(Level::INFO, |s| {
        s.host
            .pack_log_levels
            .iter()
            .find(|(pack, _)| *pack == me)
            .map_or(s.host.log_level, |(_, level)| *level)
    });
    put(out, level)
}

pub(crate) extern "system" fn type_hash(_me: ModHandle, type_name: Str, out: *mut u32) -> Status {
    let name = text(type_name);
    found(with(None, |s| s.host.type_hashes.get(&name).copied()), out)
}
