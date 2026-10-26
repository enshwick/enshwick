//! Host functions for served resources, recipes, texts and item data.

use enshwick::abi as api_frame;
use enshwick::abi::{ModHandle, Status, Str};

use crate::host::memory::{put, text};
use crate::host::{caller, caller_index, calling, entry};

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
        crate::content::register(index, *key, *base, patch, user)
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
        crate::content::rebuild(index, *key)
    })
}

pub(crate) extern "system" fn recipe_rule(
    me: ModHandle,
    kind: enshwick::abi::RecipeRuleKind,
    name: Str,
    disable: u32,
) -> Status {
    entry("recipe_rule", || {
        let (_, index, _) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        let Some(name) = text(name) else {
            return Status::BAD_ARGS;
        };
        crate::recipes::rule(index, kind, name, disable != 0)
    })
}

pub(crate) extern "system" fn recipe_disabled(me: ModHandle, recipe: u32, out: *mut u32) -> Status {
    entry("recipe_disabled", || {
        if let Err(status) = caller(me) {
            return status;
        }
        if out.is_null() {
            return Status::BAD_ARGS;
        }
        // SAFETY: `out` is non-null, checked just before; the ABI has the mod
        // pass it pointing at a writable, aligned `u32`.
        unsafe { out.write(u32::from(crate::recipes::is_disabled(recipe))) };
        Status::OK
    })
}

pub(crate) extern "system" fn recipe_kind(
    me: ModHandle,
    item: u32,
    out: *mut api_frame::RecipeKind,
) -> Status {
    entry("recipe_kind", || {
        if let Err(status) = caller(me) {
            return status;
        }
        match crate::item_kinds::ITEMS.kind(item) {
            // SAFETY: `HostApi::recipe_kind` has the mod pass `out` null or
            // pointing at a writable, aligned `RecipeKind` for the call.
            Some(kind) => unsafe { put(out, kind) },
            None => Status::MISSING,
        }
    })
}

pub(crate) extern "system" fn change_data(
    me: ModHandle,
    change: *const api_frame::DataChange,
) -> Status {
    entry("change_data", || {
        let (loader, index, _) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        // SAFETY: `HostApi::change_data` has the mod pass null or a record
        // with its header, valid for the call; an all-zero record is valid,
        // and `item` reads as zero for a record of a mod built before it.
        let Some(raw) = (unsafe {
            api_frame::read_headed(change, core::mem::offset_of!(api_frame::DataChange, item))
        }) else {
            return Status::BAD_ARGS;
        };
        let Ok(Some(change)) = enshwick::content::changes::Change::from_abi(&raw) else {
            return Status::BAD_ARGS;
        };
        if !crate::data_changes::reachable(loader, change.field) {
            return Status::MISSING;
        }
        let source = text(raw.source).unwrap_or_default().to_owned();
        crate::data_changes::declare(index, change, source)
    })
}

pub(crate) extern "system" fn add_text(me: ModHandle, id: u32, language: u32, text: Str) -> Status {
    entry("add_text", || {
        let Some(text) = self::text(text) else {
            return Status::BAD_ARGS;
        };
        match caller(me) {
            Ok(_) => crate::content::add_text(id, language, text),
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn item_data_open(me: ModHandle, id: Str, out: *mut u32) -> Status {
    entry("item_data_open", || {
        let Some(id) = text(id) else {
            return Status::BAD_ARGS;
        };
        match caller_index(me).and_then(|(loader, index)| crate::item_data::open(loader, index, id))
        {
            // SAFETY: `HostApi::item_data_open` has the mod pass `out` null or
            // pointing at a writable, aligned `u32` that no other thread
            // touches during the call.
            Ok(kind) => unsafe { put(out, kind) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn entity_data_open(
    me: ModHandle,
    id: Str,
    flags: api_frame::EntityDataFlags,
    out: *mut u32,
) -> Status {
    entry("entity_data_open", || {
        let Some(id) = text(id) else {
            return Status::BAD_ARGS;
        };
        let sent = flags.0 & api_frame::EntityDataFlags::SENT.0 != 0;
        match caller_index(me)
            .and_then(|(loader, index)| crate::item_data::open_entity(loader, index, id, sent))
        {
            // SAFETY: `HostApi::entity_data_open` has the mod pass `out` null
            // or pointing at a writable, aligned `u32` that no other thread
            // touches during the call.
            Ok(kind) => unsafe { put(out, kind) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn item_data_get(
    me: ModHandle,
    kind: u32,
    pide: u32,
    item: u32,
    out: *mut u8,
    capacity: u32,
    len: *mut u32,
) -> Status {
    entry("item_data_get", || {
        if let Err(status) = caller(me) {
            return status;
        }
        let value = match crate::item_data::get(kind, pide, item) {
            Ok(value) => value,
            Err(status) => return status,
        };
        let size = u32::try_from(value.len()).unwrap_or(u32::MAX);
        if !len.is_null() {
            // SAFETY: `len` is non-null, checked just before; the ABI has the
            // mod pass it pointing at a writable, aligned `u32`.
            unsafe { len.write(size) };
        }
        if size > capacity || out.is_null() {
            return Status::BAD_ARGS;
        }
        // SAFETY: `out` is non-null and the copy is at most `capacity` bytes,
        // both checked just before; the ABI has the mod pass `capacity`
        // writable bytes at `out`, apart from the loader's own buffer.
        unsafe { out.copy_from_nonoverlapping(value.as_ptr(), value.len()) };
        Status::OK
    })
}

pub(crate) extern "system" fn item_data_set(
    me: ModHandle,
    kind: u32,
    pide: u32,
    item: u32,
    data: *const u8,
    len: u32,
) -> Status {
    entry("item_data_set", || {
        if let Err(status) = caller(me) {
            return status;
        }
        let Ok(len) = usize::try_from(len) else {
            return Status::BAD_ARGS;
        };
        let value = if len == 0 {
            &[][..]
        } else if data.is_null() {
            return Status::BAD_ARGS;
        } else {
            // SAFETY: the caller passes `len` bytes at `data` for the call.
            unsafe { core::slice::from_raw_parts(data, len) }
        };
        match crate::item_data::set(kind, pide, item, value) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
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
        match crate::textures::serve(bytes) {
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
        let data = match crate::textures::content_of(&hash) {
            Ok(data) => data,
            Err(status) => return status,
        };
        let size = u32::try_from(data.len()).unwrap_or(u32::MAX);
        if !len.is_null() {
            // SAFETY: `len` is non-null, checked just before; the ABI has the
            // mod pass it pointing at a writable, aligned `u32`.
            unsafe { len.write(size) };
        }
        if size > capacity || out.is_null() {
            return Status::BAD_ARGS;
        }
        // SAFETY: `out` is non-null and the copy is at most `capacity` bytes,
        // both checked just before; the ABI has the mod pass `capacity`
        // writable bytes at `out`, apart from the loader's own buffer.
        unsafe { out.copy_from_nonoverlapping(data.as_ptr(), data.len()) };
        Status::OK
    })
}

pub(crate) extern "system" fn resource_read(
    me: ModHandle,
    key: *const enshwick::abi::ResourceKey,
    out: *mut u8,
    capacity: u32,
    len: *mut u32,
) -> Status {
    entry("resource_read", || {
        if let Err(status) = caller(me) {
            return status;
        }
        // SAFETY: the caller passes a key for the call; `as_ref` turns away null.
        let Some(key) = (unsafe { key.as_ref() }).copied() else {
            return Status::BAD_ARGS;
        };
        let Some(data) = crate::archive::game().and_then(|archive| archive.resource(&key)) else {
            return Status::MISSING;
        };
        let size = u32::try_from(data.len()).unwrap_or(u32::MAX);
        if !len.is_null() {
            // SAFETY: `len` is non-null, checked just before; the ABI has the
            // mod pass it pointing at a writable, aligned `u32`.
            unsafe { len.write(size) };
        }
        if size > capacity || out.is_null() {
            return Status::BAD_ARGS;
        }
        // SAFETY: `out` is non-null and the copy is at most `capacity` bytes,
        // both checked just before; the ABI has the mod pass `capacity`
        // writable bytes at `out`, apart from the loader's own buffer.
        unsafe { out.copy_from_nonoverlapping(data.as_ptr(), data.len()) };
        Status::OK
    })
}

pub(crate) extern "system" fn game_text(
    me: ModHandle,
    id: u32,
    language: u32,
    out: *mut Str,
) -> Status {
    entry("game_text", || {
        if let Err(status) = caller(me) {
            return status;
        }
        match crate::game_texts::text(id, language) {
            // SAFETY: `HostApi::game_text` has the mod pass `out` null or
            // pointing at a writable, aligned `Str` that no other thread
            // touches during the call.
            Some(text) => unsafe { put(out, Str::new(text)) },
            None => Status::MISSING,
        }
    })
}

pub(crate) extern "system" fn icons_ready(me: ModHandle) -> Status {
    entry("icons_ready", || match caller(me) {
        Ok(_) if crate::textures::ready() => Status::OK,
        Ok(_) => Status::MISSING,
        Err(status) => status,
    })
}
