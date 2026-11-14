//! Host functions for served resources: serving, rebuilding and reading game
//! resources, and the archive's keys, hashes and types.

use enshwick_core::abi as api_frame;
use enshwick_core::abi::{ModHandle, ResourceKey, Status};

use crate::host::layer::{
    BytesOut, BytesSlot, Caller, In, List, ListOut, Mod, Out, Refused, SizedListOut, entries,
};

entries! {
    fn serve_resource(
        me as Mod,
        key as In<ResourceKey>,
        base as In<ResourceKey>,
        patch: api_frame::ResourcePatchFn,
        user: *mut core::ffi::c_void,
    ) -> Status {
        Ok(crate::content::served::register(me.index, key, base, patch, user))
    }

    fn rebuild_resource(me as Mod, key as In<ResourceKey>) -> Status {
        Ok(crate::content::served::rebuild(me.index, key))
    }

    fn content_serve(
        _loader as Caller,
        [data, len] as List<u8>,
        out as Out<[u32; 4]>,
    ) -> Status {
        if data.is_empty() {
            return Err(Status::BAD_ARGS.into());
        }
        out.put(crate::content::textures::serve(data)?)
    }

    fn content_read(_loader as Caller, hash as In<[u32; 4]>, [out, capacity, len] as BytesOut) -> Status {
        out.put(&crate::content::textures::content_of(&hash)?)
    }

    fn resource_read(_loader as Caller, key as In<ResourceKey>, [out, capacity, len] as BytesOut) -> Status {
        copy_keeping("resource_read", key, &out, |key| {
            crate::engine::archive::game()
                .and_then(|archive| archive.resource(&key))
                .ok_or(Status::MISSING)
        })
    }

    fn resource_served(_loader as Caller, key as In<ResourceKey>, [out, capacity, len] as BytesOut) -> Status {
        copy_keeping("resource_served", key, &out, crate::content::served::served_now)
    }

    fn resource_buffer(me: ModHandle, len: u32, out as Out<*mut u8>) -> Status {
        // A mod that is off still gets one: a rebuild runs the patches of
        // the mods that built what the game holds.
        let known = me == crate::host::LOADER_HANDLE
            || crate::engine::loader::loader().is_some_and(|loader| loader.record(me).is_some());
        if !known {
            return Err(Status::BAD_ARGS.into());
        }
        let len = usize::try_from(len).map_err(|_| Status::BAD_ARGS)?;
        out.put(crate::content::served::lend(len).ok_or(Status::REFUSED)?)
    }

    fn resource_keys(
        _loader as Caller,
        type_hash: u32,
        [out, capacity, count] as ListOut<ResourceKey>,
    ) -> Status {
        let keys = RESOURCE_KEYS
            .get_or_init(|| crate::engine::archive::game().map(crate::engine::archive::Archive::keys))
            .as_ref()
            .ok_or(Status::MISSING)?;
        if type_hash == 0 {
            return out.put(keys);
        }
        let of_type: Vec<ResourceKey> = keys
            .iter()
            .filter(|key| key.type_hash == type_hash)
            .copied()
            .collect();
        out.put(&of_type)
    }

    fn content_hashes(_loader as Caller, [out, capacity, count] as ListOut<[u32; 4]>) -> Status {
        let hashes = CONTENT_HASHES
            .get_or_init(|| {
                crate::engine::archive::game().map(crate::engine::archive::Archive::contents)
            })
            .as_ref()
            .ok_or(Status::MISSING)?;
        out.put(hashes)
    }

    fn types(
        loader as Caller,
        [out, entry_size, capacity, count] as SizedListOut<api_frame::TypeEntry>,
    ) -> Status {
        let table = TYPES
            .get_or_init(|| {
                let registry = loader.reflection.registry();
                (!registry.is_empty())
                    .then(|| enshwick_core::resources::TypeTable::new(registry.iter()))
            })
            .as_ref()
            .ok_or(Status::MISSING)?;
        out.put(table.entries())
    }
}

/// Copies `key`'s bytes for `call` to `out`; [`Status::BAD_ARGS`] when `out`
/// is null or short. A short buffer keeps the bytes for this thread's retry
/// of the same `call` and key; a call that only asks the length keeps
/// nothing, since it may never come back.
fn copy_keeping(
    call: &'static str,
    key: ResourceKey,
    out: &BytesSlot<'_>,
    read: impl FnOnce(ResourceKey) -> Result<Vec<u8>, Status>,
) -> Result<(), Refused> {
    let data = short_or(call, key, read)?;
    let copied = out.put(&data);
    if copied
        .as_ref()
        .is_err_and(|refused| refused.status() == Status::BAD_ARGS)
        && !out.asks_length()
    {
        keep(call, key, data);
    }
    copied
}

/// Keeps `data` for this thread's retry of `call` with `key`.
fn keep(call: &'static str, key: ResourceKey, data: Vec<u8>) {
    SHORT.set(Some(Kept {
        call,
        key,
        begun: crate::content::served::builds_begun(),
        data,
    }));
}

/// A read whose buffer was short, kept for its retry.
struct Kept {
    call: &'static str,
    key: ResourceKey,
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
    key: ResourceKey,
    read: impl FnOnce(ResourceKey) -> Result<Vec<u8>, Status>,
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
static RESOURCE_KEYS: std::sync::OnceLock<Option<Vec<ResourceKey>>> = std::sync::OnceLock::new();

/// Every content hash of the game's archive, read once.
static CONTENT_HASHES: std::sync::OnceLock<Option<Vec<[u32; 4]>>> = std::sync::OnceLock::new();

/// Every reflected type as ABI entries, built once.
static TYPES: std::sync::OnceLock<Option<enshwick_core::resources::TypeTable>> =
    std::sync::OnceLock::new();

#[cfg(test)]
mod tests {
    use core::cell::Cell;

    use super::*;
    use crate::host::layer::BytesSlot;

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
        // SAFETY: `out` is null and `len` a local `u32` that outlives the slot.
        let asks = unsafe { BytesSlot::from_raw(core::ptr::null_mut(), 0, &raw mut len) };
        let status = copy_keeping("resource_served", KEY, &asks, read).map_err(|r| r.status());
        assert_eq!((status, len), (Err(Status::BAD_ARGS), 3));
        assert!(SHORT.take().is_none(), "a length-only call keeps nothing");
        let mut short = [0u8; 2];
        // SAFETY: `short` is writable for its 2 bytes and `len` a local
        // `u32`, both outliving the slot.
        let too_short = unsafe { BytesSlot::from_raw(short.as_mut_ptr(), 2, &raw mut len) };
        let status = copy_keeping("resource_served", KEY, &too_short, read).map_err(|r| r.status());
        assert_eq!(status, Err(Status::BAD_ARGS));
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
