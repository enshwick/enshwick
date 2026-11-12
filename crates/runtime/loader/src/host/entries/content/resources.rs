//! Host functions for served resources: serving, rebuilding and reading game
//! resources, and the archive's keys, hashes and types.

use crate::host::memory::{put, put_list};
use crate::host::{caller, calling, entry};
use enshwick_core::abi as api_frame;
use enshwick_core::abi::{ModHandle, Status};

pub(crate) extern "system" fn serve_resource(
    me: ModHandle,
    key: *const api_frame::ResourceKey,
    base: *const api_frame::ResourceKey,
    patch: api_frame::ResourcePatchFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("serve_resource", || {
        let (_, index, _) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        // SAFETY: non-null keys from a mod point at valid keys for the call.
        let (Some(key), Some(base)) = (unsafe { key.as_ref() }, unsafe { base.as_ref() }) else {
            return Status::BAD_ARGS;
        };
        crate::content::served::register(index, *key, *base, patch, user)
    })
}

pub(crate) extern "system" fn rebuild_resource(
    me: ModHandle,
    key: *const api_frame::ResourceKey,
) -> Status {
    entry("rebuild_resource", || {
        let (_, index, _) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        // SAFETY: a non-null key from a mod points at a valid key for the call.
        let Some(key) = (unsafe { key.as_ref() }) else {
            return Status::BAD_ARGS;
        };
        crate::content::served::rebuild(index, *key)
    })
}

pub(crate) extern "system" fn content_serve(
    me: ModHandle,
    data: *const u8,
    len: u32,
    out: *mut [u32; 4],
) -> Status {
    entry("content_serve", || {
        if let Err(status) = caller(me) {
            return status;
        }
        let Ok(len) = usize::try_from(len) else {
            return Status::BAD_ARGS;
        };
        if data.is_null() || len == 0 {
            return Status::BAD_ARGS;
        }
        // SAFETY: the caller passes `len` bytes at `data` for the call.
        let bytes = unsafe { core::slice::from_raw_parts(data, len) };
        match crate::content::textures::serve(bytes) {
            // SAFETY: `HostApi::content_serve` has the mod pass `out` null or
            // pointing at a writable, aligned `[u32; 4]` that no other thread
            // touches during the call.
            Ok(hash) => unsafe { put(out, hash) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn content_read(
    me: ModHandle,
    hash: *const [u32; 4],
    out: *mut u8,
    capacity: u32,
    len: *mut u32,
) -> Status {
    entry("content_read", || {
        if let Err(status) = caller(me) {
            return status;
        }
        // SAFETY: the caller passes a hash for the call; `as_ref` turns away null.
        let Some(hash) = (unsafe { hash.as_ref() }).copied() else {
            return Status::BAD_ARGS;
        };
        let data = match crate::content::textures::content_of(&hash) {
            Ok(data) => data,
            Err(status) => return status,
        };
        // SAFETY: `HostApi::content_read` has the mod pass `len` null or a
        // writable, aligned `u32`, and `out` null or `capacity` writable
        // bytes apart from the loader's own buffer.
        unsafe { enshwick_core::__private::copy_out(&data, out, capacity, len) }
    })
}

pub(crate) extern "system" fn resource_read(
    me: ModHandle,
    key: *const enshwick_core::abi::ResourceKey,
    out: *mut u8,
    capacity: u32,
    len: *mut u32,
) -> Status {
    entry("resource_read", || {
        // SAFETY: the ABI's guarantees for these arguments are `copy_resource`'s.
        unsafe {
            copy_resource("resource_read", me, key, out, capacity, len, |key| {
                crate::engine::archive::game()
                    .and_then(|archive| archive.resource(&key))
                    .ok_or(Status::MISSING)
            })
        }
    })
}

pub(crate) extern "system" fn resource_served(
    me: ModHandle,
    key: *const enshwick_core::abi::ResourceKey,
    out: *mut u8,
    capacity: u32,
    len: *mut u32,
) -> Status {
    entry("resource_served", || {
        // SAFETY: the ABI's guarantees for these arguments are `copy_resource`'s.
        unsafe {
            copy_resource(
                "resource_served",
                me,
                key,
                out,
                capacity,
                len,
                crate::content::served::served_now,
            )
        }
    })
}

/// Copies the bytes `read` gives for the key at `key` to `out`, writing
/// their length to `len`, as [`copy_keeping`] does.
///
/// # Safety
///
/// - `key` is null or points at a `ResourceKey` readable for the call.
/// - `len` is null or points at a writable, aligned `u32`.
/// - `out` is null or writable for `capacity` bytes, apart from `read`'s.
unsafe fn copy_resource(
    call: &'static str,
    me: ModHandle,
    key: *const enshwick_core::abi::ResourceKey,
    out: *mut u8,
    capacity: u32,
    len: *mut u32,
    read: impl FnOnce(enshwick_core::abi::ResourceKey) -> Result<Vec<u8>, Status>,
) -> Status {
    if let Err(status) = caller(me) {
        return status;
    }
    // SAFETY: the caller guarantees `key` null or readable; `as_ref` turns
    // away null.
    let Some(key) = (unsafe { key.as_ref() }).copied() else {
        return Status::BAD_ARGS;
    };
    // SAFETY: the caller's guarantees for `out`, `capacity` and `len` are
    // `copy_keeping`'s.
    unsafe { copy_keeping(call, key, out, capacity, len, read) }
}

/// Copies `key`'s bytes for `call` to `out`, writing their length to
/// `len`; [`Status::BAD_ARGS`] when `out` is null or `capacity` short. A
/// short buffer keeps the bytes for this thread's retry of the same `call`
/// and key; a call that only asks the length keeps nothing, since it may
/// never come back.
///
/// # Safety
///
/// - `len` is null or points at a writable, aligned `u32`.
/// - `out` is null or writable for `capacity` bytes, apart from `read`'s.
unsafe fn copy_keeping(
    call: &'static str,
    key: enshwick_core::abi::ResourceKey,
    out: *mut u8,
    capacity: u32,
    len: *mut u32,
    read: impl FnOnce(enshwick_core::abi::ResourceKey) -> Result<Vec<u8>, Status>,
) -> Status {
    let data = match short_or(call, key, read) {
        Ok(data) => data,
        Err(status) => return status,
    };
    // SAFETY: the caller guarantees `len` null or writable and aligned, and
    // `out` null or `capacity` writable bytes apart from `data`, which this
    // function owns.
    let status = unsafe { enshwick_core::__private::copy_out(&data, out, capacity, len) };
    if status == Status::BAD_ARGS && !out.is_null() {
        keep(call, key, data);
    }
    status
}

/// Keeps `data` for this thread's retry of `call` with `key`.
fn keep(call: &'static str, key: enshwick_core::abi::ResourceKey, data: Vec<u8>) {
    SHORT.set(Some(Kept {
        call,
        key,
        begun: crate::content::served::builds_begun(),
        data,
    }));
}

pub(crate) extern "system" fn resource_buffer(
    me: ModHandle,
    len: u32,
    out: *mut *mut u8,
) -> Status {
    entry("resource_buffer", || {
        // A mod that is off still gets one: a rebuild runs the patches of
        // the mods that built what the game holds.
        let known = me == crate::host::LOADER_HANDLE
            || crate::host::loader().is_some_and(|loader| loader.record(me).is_some());
        if !known {
            return Status::BAD_ARGS;
        }
        let Ok(len) = usize::try_from(len) else {
            return Status::BAD_ARGS;
        };
        if out.is_null() {
            return Status::BAD_ARGS;
        }
        match crate::content::served::lend(len) {
            // SAFETY: `out` is non-null, checked above; `HostApi::resource_buffer`
            // has the mod pass it writable and aligned for one pointer, untouched
            // by other threads during the call.
            Some(at) => unsafe { put(out, at) },
            None => Status::REFUSED,
        }
    })
}

/// A read whose buffer was short, kept for its retry.
struct Kept {
    call: &'static str,
    key: enshwick_core::abi::ResourceKey,
    /// [`crate::content::served::builds_begun`] when it was kept.
    begun: u64,
    data: Vec<u8>,
}

thread_local! {
    /// The last read on this thread whose buffer was short, kept for the
    /// retry with a buffer that fits, so a served key builds once.
    static SHORT: core::cell::RefCell<Option<Kept>> = const { core::cell::RefCell::new(None) };
}

/// `key`'s bytes for `call`: the short read this thread kept for the same
/// call and key while no build of a served key began since, else `read`'s.
/// Any kept read goes either way.
fn short_or(
    call: &'static str,
    key: enshwick_core::abi::ResourceKey,
    read: impl FnOnce(enshwick_core::abi::ResourceKey) -> Result<Vec<u8>, Status>,
) -> Result<Vec<u8>, Status> {
    match SHORT.take() {
        Some(kept)
            if kept.call == call
                && kept.key == key
                && kept.begun == crate::content::served::builds_begun() =>
        {
            Ok(kept.data)
        }
        _ => read(key),
    }
}

/// Every resource key of the game's archive, in archive order, read once.
static RESOURCE_KEYS: std::sync::OnceLock<Option<Vec<api_frame::ResourceKey>>> =
    std::sync::OnceLock::new();

/// Every content hash of the game's archive, read once.
static CONTENT_HASHES: std::sync::OnceLock<Option<Vec<[u32; 4]>>> = std::sync::OnceLock::new();

/// Every reflected type as ABI entries, built once.
static TYPES: std::sync::OnceLock<Option<enshwick_core::resources::TypeTable>> =
    std::sync::OnceLock::new();

pub(crate) extern "system" fn resource_keys(
    me: ModHandle,
    type_hash: u32,
    out: *mut api_frame::ResourceKey,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("resource_keys", || {
        if let Err(status) = caller(me) {
            return status;
        }
        let Some(keys) = RESOURCE_KEYS
            .get_or_init(|| {
                crate::engine::archive::game().map(crate::engine::archive::Archive::keys)
            })
            .as_ref()
        else {
            return Status::MISSING;
        };
        let of_type: Vec<api_frame::ResourceKey>;
        let keys = if type_hash == 0 {
            keys.as_slice()
        } else {
            of_type = keys
                .iter()
                .filter(|key| key.type_hash == type_hash)
                .copied()
                .collect();
            of_type.as_slice()
        };
        // SAFETY: `HostApi::resource_keys` has the mod pass `out` null or
        // writable for `capacity` keys and `count` null or a writable `u32`,
        // untouched by the mod during the call.
        unsafe { put_list(keys, out, capacity, count) }
    })
}

pub(crate) extern "system" fn content_hashes(
    me: ModHandle,
    out: *mut [u32; 4],
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("content_hashes", || {
        if let Err(status) = caller(me) {
            return status;
        }
        let Some(hashes) = CONTENT_HASHES
            .get_or_init(|| {
                crate::engine::archive::game().map(crate::engine::archive::Archive::contents)
            })
            .as_ref()
        else {
            return Status::MISSING;
        };
        // SAFETY: `HostApi::content_hashes` has the mod pass `out` null or
        // writable for `capacity` hashes and `count` null or a writable `u32`,
        // untouched by the mod during the call.
        unsafe { put_list(hashes, out, capacity, count) }
    })
}

pub(crate) extern "system" fn types(
    me: ModHandle,
    out: *mut api_frame::TypeEntry,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("types", || {
        let loader = match caller(me) {
            Ok(loader) => loader,
            Err(status) => return status,
        };
        let Some(table) = TYPES
            .get_or_init(|| {
                let registry = loader.reflection.registry();
                (!registry.is_empty())
                    .then(|| enshwick_core::resources::TypeTable::new(registry.iter()))
            })
            .as_ref()
        else {
            return Status::MISSING;
        };
        let entries = table.entries();
        // SAFETY: `HostApi::types` has the mod pass `out` null or writable
        // for `capacity` entries of `entry_size` bytes and `count` null or a
        // writable `u32`, untouched by the mod during the call.
        unsafe { crate::host::memory::list(entries, out, entry_size, capacity, count) }
    })
}

#[cfg(test)]
mod tests {
    use core::cell::Cell;

    use enshwick_core::abi::ResourceKey;

    use super::*;

    const KEY: ResourceKey = ResourceKey {
        guid: [4; 16],
        type_hash: 2,
        part: 0,
    };

    #[test]
    fn a_retry_after_a_short_read_takes_the_kept_bytes_without_reading_again() {
        let reads = Cell::new(0);
        let read = |_| {
            reads.set(reads.get() + 1);
            Ok(vec![1, 2, 3])
        };
        keep("resource_served", KEY, vec![9]);
        assert_eq!(short_or("resource_served", KEY, read), Ok(vec![9]));
        assert_eq!(reads.get(), 0, "the kept bytes answer the retry");
        assert_eq!(short_or("resource_served", KEY, read), Ok(vec![1, 2, 3]));
        assert_eq!(reads.get(), 1, "a kept read answers once");
    }

    #[test]
    fn a_call_that_only_asks_the_length_keeps_no_bytes_and_a_short_buffer_does() {
        let read = |_| Ok(vec![1, 2, 3]);
        let mut len = 0u32;
        // SAFETY: `out` is null and `len` a local `u32`.
        let status = unsafe {
            copy_keeping(
                "resource_served",
                KEY,
                core::ptr::null_mut(),
                0,
                &raw mut len,
                read,
            )
        };
        assert_eq!((status, len), (Status::BAD_ARGS, 3));
        assert!(SHORT.take().is_none(), "a length-only call keeps nothing");
        let mut short = [0u8; 2];
        // SAFETY: `short` is writable for its 2 bytes and `len` a local `u32`.
        let status = unsafe {
            copy_keeping(
                "resource_served",
                KEY,
                short.as_mut_ptr(),
                2,
                &raw mut len,
                read,
            )
        };
        assert_eq!(status, Status::BAD_ARGS);
        assert!(
            SHORT.take().is_some(),
            "a short buffer keeps them for its retry"
        );
    }

    #[test]
    fn a_kept_read_answers_neither_another_call_nor_another_key() {
        let read = |_| Ok(vec![1]);
        keep("resource_served", KEY, vec![9]);
        assert_eq!(short_or("resource_read", KEY, read), Ok(vec![1]));
        let other = ResourceKey { part: 1, ..KEY };
        keep("resource_served", KEY, vec![9]);
        assert_eq!(short_or("resource_served", other, read), Ok(vec![1]));
        assert!(SHORT.take().is_none(), "a miss drops the kept read");
    }

    #[test]
    fn a_kept_read_answers_no_retry_after_a_build_began_on_the_thread() {
        let read = |_| Ok(vec![1]);
        keep("resource_served", KEY, vec![9]);
        drop(crate::content::served::enter_build(KEY));
        assert_eq!(short_or("resource_served", KEY, read), Ok(vec![1]));
        assert!(SHORT.take().is_none(), "a stale kept read goes");
    }
}
