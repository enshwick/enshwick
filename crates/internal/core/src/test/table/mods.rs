//! The fake's answers about the mods: logs, faults, settings and what is loaded.

use core::ffi::c_void;

use crate::abi::{ConfigCheckFn, ContentRename, Level, ModHandle, ModInfo, Status, Str};
use crate::test::table::{Registered, found, put, text, with};

pub(crate) extern "system" fn fault(me: ModHandle, reason: Str) {
    let reason = text(reason);
    // The loader logs every turn-off with its reason.
    let line = format!("off: {}", reason.replace('\t', " "));
    with((), |s| {
        s.faults.push(reason);
        s.sent.push((me, Level::ERROR, line.clone()));
        s.logs.push((Level::ERROR, line));
    });
}

pub(crate) extern "system" fn log(me: ModHandle, level: Level, message: Str) {
    let message = text(message);
    with((), |s| {
        s.sent.push((me, level, message.clone()));
        s.logs.push((level, message));
    });
}

pub(crate) extern "system" fn settings(_me: ModHandle, file: Str, out: *mut Str) -> Status {
    let file = text(file);
    let served = with(None, |s| {
        if !s.config_schemas.contains_key(&file) {
            return None;
        }
        let served: Box<str> = s
            .host
            .settings
            .get(&file)
            .cloned()
            .unwrap_or_default()
            .into();
        // The heap text stays where it is while the box lives in the state.
        let view = Str::new(&served);
        s.settings.push(served);
        Some(view)
    });
    found(served, out)
}

pub(crate) extern "system" fn rename_content(
    _me: ModHandle,
    renames: *const ContentRename,
    count: u32,
) -> Status {
    let renames: Vec<([u8; 16], [u8; 16])> = match count {
        0 => Vec::new(),
        _ if renames.is_null() => return Status::BAD_ARGS,
        // SAFETY: `renames` is non-null, checked just before, and the ABI has
        // the mod keep `count` aligned entries there for the call.
        _ => unsafe { core::slice::from_raw_parts(renames, count as usize) }
            .iter()
            .map(|r| (r.from, r.to))
            .collect(),
    };
    if renames.iter().any(|(from, to)| from == to) {
        return Status::REFUSED;
    }
    with(Status::MISSING, |s| {
        if s.content_renames.is_some() {
            return Status::REFUSED;
        }
        s.content_renames = Some(renames);
        Status::OK
    })
}

pub(crate) extern "system" fn mods(
    _me: ModHandle,
    out: *mut ModInfo,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let mods = with(Vec::new(), |s| s.host.mods.clone());
    list(&mods, out, entry_size, capacity, count)
}

pub(crate) extern "system" fn join_missing(
    _me: ModHandle,
    out: *mut ModInfo,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let mods = with(Vec::new(), |s| s.host.join_missing.clone());
    list(&mods, out, entry_size, capacity, count)
}

/// Copies up to `capacity` of `items` to `out`, `entry_size` bytes apart, and
/// writes how many there are; refuses a non-null `out` whose entries are
/// shorter than a header, as the loader does.
pub(crate) fn list<T: Copy>(
    items: &[T],
    out: *mut T,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let stride = usize::try_from(entry_size).unwrap_or(0);
    let shared = stride.min(size_of::<T>());
    let capacity = usize::try_from(capacity).unwrap_or(0);
    if !out.is_null() {
        if stride < size_of::<crate::abi::Header>() {
            return Status::BAD_ARGS;
        }
        let size = u32::try_from(shared).unwrap_or(u32::MAX);
        for (i, info) in items.iter().take(capacity).enumerate() {
            let entry = out.cast::<u8>().wrapping_add(i * stride);
            // SAFETY: the safe layer passes room for `capacity` entries of
            // `stride` bytes; the fake fills what both sides know.
            unsafe {
                core::ptr::copy_nonoverlapping(core::ptr::from_ref(info).cast(), entry, shared);
            }
            // SAFETY: the header's `size` is the entry's first field, inside
            // its `stride` bytes, at least a header as checked above.
            unsafe { entry.cast::<u32>().write_unaligned(size) };
        }
    }
    put(count, u32::try_from(items.len()).unwrap_or(u32::MAX))
}

pub(crate) extern "system" fn config_schema(
    _me: ModHandle,
    file: Str,
    schema: Str,
    _template: Str,
) -> Status {
    let (file, text) = (text(file), text(schema));
    if !crate::config::__file_name(&file) || crate::config::Schema::parse(&text).is_err() {
        return Status::BAD_ARGS;
    }
    with(Status::REFUSED, |s| {
        if s.config_schemas
            .get(&file)
            .is_some_and(|known| *known != text)
        {
            return Status::REFUSED;
        }
        s.config_schemas.insert(file, text);
        Status::OK
    })
}

pub(crate) extern "system" fn config_check(
    me: ModHandle,
    file: Str,
    callback: ConfigCheckFn,
    user: *mut c_void,
) -> Status {
    let file = text(file);
    with(Status::MISSING, |s| {
        if s.config_checks.contains_key(&file) {
            return Status::REFUSED;
        }
        s.config_checks
            .insert(file, Registered::new(callback, user, me));
        Status::OK
    })
}

pub(crate) extern "system" fn texts(_me: ModHandle, toml: Str) -> Status {
    let toml = text(toml);
    with(Status::MISSING, |s| {
        s.published_texts = Some(toml);
        Status::OK
    })
}

pub(crate) extern "system" fn language(_me: ModHandle) -> u32 {
    with(0, |s| s.host.language)
}
