//! The fake's answers about the game build: symbols, systems, layouts and constants.

use crate::abi::{BuildInfo, CallerFrame, Header, Level, ModHandle, Status, Str};
use crate::test::FakeHost;
use crate::test::table::{found, put, text, with};

pub(crate) extern "system" fn build(out: *mut BuildInfo) {
    let host = with(FakeHost::new(), |s| s.host.clone());
    let _written = put(
        out,
        BuildInfo {
            header: Header::of::<BuildInfo>(),
            side: host.side,
            base: host.base,
            fingerprint: host.fingerprint,
            table: host.table,
        },
    );
}

pub(crate) extern "system" fn symbol(_me: ModHandle, name: Str, out: *mut u64) -> Status {
    let name = text(name);
    found(with(None, |s| s.host.symbols.get(&name).copied()), out)
}

pub(crate) extern "system" fn system(name: Str, out: *mut u64) -> Status {
    let name = text(name);
    found(with(None, |s| s.host.systems.get(&name).copied()), out)
}

pub(crate) extern "system" fn field_offset(type_name: Str, field: Str, out: *mut u64) -> Status {
    let key = (text(type_name), text(field));
    found(with(None, |s| s.host.field_offsets.get(&key).copied()), out)
}

pub(crate) extern "system" fn type_size(type_name: Str, out: *mut u32) -> Status {
    let name = text(type_name);
    found(with(None, |s| s.host.type_sizes.get(&name).copied()), out)
}

pub(crate) extern "system" fn caller_frame(out: *mut CallerFrame) {
    let frame = with(CallerFrame::default(), |s| s.host.caller_frame);
    let _written = put(out, frame);
}

pub(crate) extern "system" fn constant(_me: ModHandle, name: Str, out: *mut u64) -> Status {
    let name = text(name);
    found(with(None, |s| s.host.constants.get(&name).copied()), out)
}

pub(crate) extern "system" fn data_dir(_me: ModHandle, out: *mut Str) -> Status {
    let dir = with(None, |s| Some(s.host.data_dir.clone()));
    // The loader's path lives for the process; a test leaks a few bytes for it.
    found(
        dir.map(|dir| Str::new(Box::leak(dir.into_boxed_str()))),
        out,
    )
}

pub(crate) extern "system" fn log_level(_me: ModHandle) -> Level {
    with(Level::INFO, |s| s.host.log_level)
}

pub(crate) extern "system" fn type_hash(type_name: Str, out: *mut u32) -> Status {
    let name = text(type_name);
    found(with(None, |s| s.host.type_hashes.get(&name).copied()), out)
}
