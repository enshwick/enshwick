//! Host functions for the game build: symbols, systems, layouts and constants.

use enshwick::abi as api_frame;
use enshwick::abi::{BuildInfo, EventTypeInfo, Header, Level, ModHandle, Status, Str};

use crate::host::join::keep;
use crate::host::memory::{put, put_headed, put_headed_list, text};
use crate::host::{caller, entry, loader};

pub(crate) extern "system" fn build(out: *mut BuildInfo) {
    entry("build", || {
        let Some(loader) = loader() else {
            return;
        };
        // SAFETY: `HostApi::build` has the mod pass `out` null or pointing at
        // its version of `BuildInfo`, writable for the size its header states,
        // that no other thread touches during the call.
        let _written = unsafe {
            put_headed(
                out,
                BuildInfo {
                    header: Header::of::<BuildInfo>(),
                    side: loader.side,
                    base: loader.base,
                    fingerprint: enshwick::abi::Fingerprint {
                        guid: loader.fingerprint.guid(),
                        age: loader.fingerprint.age(),
                    },
                    table: match loader.build.kind {
                        crate::builds::Kind::Exact => enshwick::abi::BuildTable::EXACT,
                        crate::builds::Kind::Fallback => enshwick::abi::BuildTable::PATTERNS,
                    },
                },
            )
        };
    });
}

pub(crate) extern "system" fn symbol(me: ModHandle, name: Str, out: *mut u64) -> Status {
    entry("symbol", || {
        let (Some(loader), Some(name)) = (loader(), text(name)) else {
            return Status::BAD_ARGS;
        };
        let Some((index, _)) = loader.record(me) else {
            return Status::BAD_ARGS;
        };
        loader
            .resolve_symbol(index, name)
            .map_or(Status::MISSING, |address| {
                crate::diag::name(address, name);
                // SAFETY: `HostApi::symbol` has the mod pass `out` null or
                // pointing at a writable, aligned `u64` that no other thread
                // touches during the call.
                unsafe { put(out, address) }
            })
    })
}

pub(crate) fn lookup(error: &enshwick_gamedata::reflect::LookupError) -> Status {
    match error {
        enshwick_gamedata::reflect::LookupError::Ambiguous(_) => Status::AMBIGUOUS,
        _ => Status::MISSING,
    }
}

pub(crate) extern "system" fn system(name: Str, out: *mut u64) -> Status {
    entry("system", || {
        let (Some(loader), Some(name)) = (loader(), text(name)) else {
            return Status::BAD_ARGS;
        };
        match loader.reflection.system(name) {
            Ok(found) if found.entry() != 0 => {
                crate::diag::name(found.entry(), name);
                // SAFETY: `HostApi::system` has the mod pass `out` null or
                // pointing at a writable, aligned `u64` that no other thread
                // touches during the call.
                unsafe { put(out, found.entry()) }
            }
            Ok(_) => Status::MISSING,
            Err(error) => lookup(&error),
        }
    })
}

pub(crate) extern "system" fn field_offset(type_name: Str, field: Str, out: *mut u64) -> Status {
    entry("field_offset", || {
        let (Some(loader), Some(type_name), Some(field)) = (loader(), text(type_name), text(field))
        else {
            return Status::BAD_ARGS;
        };
        match loader
            .reflection
            .field_offset(crate::catalogue::type_name(type_name), field)
        {
            // SAFETY: `HostApi::field_offset` has the mod pass `out` null or
            // pointing at a writable, aligned `u64` that no other thread
            // touches during the call.
            Ok(offset) => unsafe { put(out, offset) },
            Err(error) => lookup(&error),
        }
    })
}

pub(crate) extern "system" fn type_size(type_name: Str, out: *mut u32) -> Status {
    entry("type_size", || {
        let (Some(loader), Some(type_name)) = (loader(), text(type_name)) else {
            return Status::BAD_ARGS;
        };
        match loader
            .reflection
            .type_info(crate::catalogue::type_name(type_name))
        {
            // SAFETY: `HostApi::type_size` has the mod pass `out` null or
            // pointing at a writable, aligned `u32` that no other thread
            // touches during the call.
            Ok(info) => unsafe { put(out, info.size()) },
            Err(error) => lookup(&error),
        }
    })
}

pub(crate) extern "system" fn caller_frame(out: *mut api_frame::CallerFrame) {
    entry("caller_frame", || {
        let frame = loader()
            .map(|loader| (loader.caller_frame)())
            .unwrap_or_default();
        // SAFETY: `HostApi::caller_frame` has the mod pass `out` null or
        // pointing at a writable, aligned `CallerFrame` that no other thread
        // touches during the call.
        let _written = unsafe { put(out, frame) };
    });
}

pub(crate) extern "system" fn constant(me: ModHandle, name: Str, out: *mut u64) -> Status {
    entry("constant", || {
        let (Some(loader), Some(name)) = (loader(), text(name)) else {
            return Status::BAD_ARGS;
        };
        let Some((index, _)) = loader.record(me) else {
            return Status::BAD_ARGS;
        };
        loader
            .resolve_constant(index, name)
            // SAFETY: `HostApi::constant` has the mod pass `out` null or
            // pointing at a writable, aligned `u64` that no other thread
            // touches during the call.
            .map_or(Status::MISSING, |value| unsafe { put(out, value) })
    })
}

pub(crate) extern "system" fn data_dir(me: ModHandle, out: *mut Str) -> Status {
    entry("data_dir", || {
        let Some(loader) = loader() else {
            return Status::BAD_ARGS;
        };
        let Some((_, record)) = loader.record(me) else {
            return Status::BAD_ARGS;
        };
        let dir = loader.data_dir.join(&record.id);
        if std::fs::create_dir_all(&dir).is_err() {
            return Status::REFUSED;
        }
        // The path lives for the process: the mod keeps it as long as it likes.
        // SAFETY: `HostApi::data_dir` has the mod pass `out` null or pointing
        // at a writable, aligned `Str` that no other thread touches during the
        // call.
        unsafe { put(out, Str::new(keep(&dir.to_string_lossy()))) }
    })
}

pub(crate) extern "system" fn log_level(me: ModHandle) -> Level {
    entry("log_level", || {
        loader()
            .and_then(|loader| loader.record(me))
            .map_or(Level::INFO, |(_, record)| crate::log::level(&record.id))
    })
}

pub(crate) extern "system" fn type_hash(type_name: Str, out: *mut u32) -> Status {
    entry("type_hash", || {
        let (Some(loader), Some(type_name)) = (loader(), text(type_name)) else {
            return Status::BAD_ARGS;
        };
        match loader
            .reflection
            .type_info(crate::catalogue::type_name(type_name))
        {
            // SAFETY: `HostApi::type_hash` has the mod pass `out` null or
            // pointing at a writable, aligned `u32` that no other thread
            // touches during the call.
            Ok(info) => crate::world::type_hash(info.va())
                .map_or(Status::MISSING, |h| unsafe { put(out, h) }),
            Err(error) => lookup(&error),
        }
    })
}

pub(crate) extern "system" fn event_types(
    me: ModHandle,
    out: *mut EventTypeInfo,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("event_types", || {
        let loader = match caller(me) {
            Ok(loader) => loader,
            Err(status) => return status,
        };
        let types: Vec<EventTypeInfo> = crate::catalogue::event_types(&loader.reflection)
            .filter_map(|ty| {
                Some(EventTypeInfo {
                    header: Header::of::<EventTypeInfo>(),
                    name: Str::new(ty.name()),
                    hash: crate::world::type_hash(ty.va())?,
                    size: ty.size(),
                })
            })
            .collect();
        // SAFETY: `HostApi::event_types` has the mod pass `out` null or
        // pointing at `capacity` entries of its version of `EventTypeInfo`,
        // each writable for the size the first entry's header states, and
        // `count` null or pointing at a writable, aligned `u32`, neither
        // touched by another thread during the call.
        unsafe { put_headed_list(&types, out, capacity, count) }
    })
}
