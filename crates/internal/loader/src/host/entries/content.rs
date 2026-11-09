//! Host functions for served resources, recipes, texts and item data.

use enshwick::abi as api_frame;
use enshwick::abi::{ModHandle, Status, Str};

use crate::host::memory::{put, put_list, text};
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

pub(crate) extern "system" fn recipe_disabled(
    me: ModHandle,
    recipe: enshwick::RecipeId,
    out: *mut u32,
) -> Status {
    entry("recipe_disabled", || {
        if let Err(status) = caller(me) {
            return status;
        }
        if out.is_null() {
            return Status::BAD_ARGS;
        }
        // SAFETY: `out` is non-null, checked just before; the ABI has the mod
        // pass it pointing at a writable, aligned `u32`.
        unsafe { out.write(u32::from(crate::recipes::is_disabled(recipe.get()))) };
        Status::OK
    })
}

pub(crate) extern "system" fn recipe_kind(
    me: ModHandle,
    item: enshwick::ItemId,
    out: *mut api_frame::RecipeKind,
) -> Status {
    entry("recipe_kind", || {
        if let Err(status) = caller(me) {
            return status;
        }
        match crate::item_kinds::ITEMS.kind(item.get()) {
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
        // and `item`, `select` and `categories` read as zero for a record of
        // a mod built before them.
        let Some(raw) = (unsafe {
            api_frame::read_headed(change, core::mem::offset_of!(api_frame::DataChange, item))
        }) else {
            return Status::BAD_ARGS;
        };
        let source = text(raw.source).unwrap_or_default().to_owned();
        if let Ok(Some(change)) = enshwick::content::changes::RecipesChange::from_abi(&raw) {
            if !crate::data_changes::reachable(loader, change.field) {
                return Status::MISSING;
            }
            return crate::data_changes::declare_recipes(index, change, source);
        }
        if raw.select != api_frame::DataSelect::ENTRY {
            let Ok(Some(change)) = enshwick::content::changes::ItemsChange::from_abi(&raw) else {
                return Status::BAD_ARGS;
            };
            if !crate::data_changes::reachable(loader, change.field) {
                return Status::MISSING;
            }
            return crate::data_changes::declare_items(index, change, source);
        }
        let Ok(Some(change)) = enshwick::content::changes::Change::from_abi(&raw) else {
            return Status::BAD_ARGS;
        };
        if !crate::data_changes::reachable(loader, change.field) {
            return Status::MISSING;
        }
        crate::data_changes::declare(index, change, source)
    })
}

pub(crate) extern "system" fn declare_change(
    me: ModHandle,
    change: *const api_frame::DataChange,
    out: *mut api_frame::ChangeId,
) -> Status {
    entry("declare_change", || {
        let (loader, index, record) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if out.is_null() {
            return Status::BAD_ARGS;
        }
        // SAFETY: `HostApi::declare_change` has the mod pass null or a
        // record with its header, valid for the call; a mod that knows this
        // entry knows every field of the record.
        let Some(mut raw) = (unsafe {
            api_frame::read_headed(
                change,
                <api_frame::DataChange as enshwick::abi::Headed>::BORN,
            )
        }) else {
            return Status::BAD_ARGS;
        };
        raw.op = api_frame::ChangeOp::TIMES;
        raw.value = 1.0;
        let source = text(raw.source).unwrap_or_default().to_owned();
        let (target, field) = if raw.select == api_frame::DataSelect::ENTRY {
            match enshwick::content::changes::Change::from_abi(&raw) {
                Ok(Some(change)) => (
                    crate::data_changes::LiveTarget::Entry(change.entry, source),
                    change.field,
                ),
                _ => return Status::BAD_ARGS,
            }
        } else {
            match enshwick::content::changes::ItemsChange::from_abi(&raw) {
                Ok(Some(change)) => (
                    crate::data_changes::LiveTarget::Items(change.items, source),
                    change.field,
                ),
                _ => return Status::BAD_ARGS,
            }
        };
        if field == enshwick::content::changes::Field::Ingredient {
            return Status::BAD_ARGS;
        }
        if field.seen_by_clients() && !record.joins_when_on() {
            return Status::REFUSED;
        }
        if !crate::data_changes::reachable(loader, field) {
            return Status::MISSING;
        }
        match crate::data_changes::declare_change(index, target, field) {
            // SAFETY: `out` is non-null by the check above and, by
            // `HostApi::declare_change`, points at a writable `ChangeId` for
            // the call.
            Ok(id) => unsafe { put(out, id) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn set_change(
    me: ModHandle,
    id: api_frame::ChangeId,
    op: api_frame::ChangeOp,
    value: f64,
) -> Status {
    entry("set_change", || {
        let (_, index) = match caller_index(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        let value = match op {
            api_frame::ChangeOp::SET => Some(enshwick::content::changes::Value::Set(value)),
            api_frame::ChangeOp::TIMES => Some(enshwick::content::changes::Value::Times(value)),
            api_frame::ChangeOp::RESET => None,
            _ => return Status::BAD_ARGS,
        };
        crate::data_changes::set_change(index, id, value)
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
    pide: enshwick::Entity,
    item: enshwick::ItemId,
    out: *mut u8,
    capacity: u32,
    len: *mut u32,
) -> Status {
    entry("item_data_get", || {
        if let Err(status) = caller(me) {
            return status;
        }
        let value = match crate::item_data::get(kind, pide.0, item.get()) {
            Ok(value) => value,
            Err(status) => return status,
        };
        // SAFETY: `HostApi::item_data_get` has the mod pass `len` null or a
        // writable, aligned `u32`, and `out` null or `capacity` writable
        // bytes apart from the loader's own buffer.
        unsafe { enshwick::__private::copy_out(&value, out, capacity, len) }
    })
}

pub(crate) extern "system" fn item_data_set(
    me: ModHandle,
    kind: u32,
    pide: enshwick::Entity,
    item: enshwick::ItemId,
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
        match crate::item_data::set(kind, pide.0, item.get(), value) {
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
        // SAFETY: `HostApi::content_read` has the mod pass `len` null or a
        // writable, aligned `u32`, and `out` null or `capacity` writable
        // bytes apart from the loader's own buffer.
        unsafe { enshwick::__private::copy_out(&data, out, capacity, len) }
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
        // SAFETY: the ABI's guarantees for these arguments are `copy_resource`'s.
        unsafe {
            copy_resource("resource_read", me, key, out, capacity, len, |key| {
                crate::archive::game()
                    .and_then(|archive| archive.resource(&key))
                    .ok_or(Status::MISSING)
            })
        }
    })
}

pub(crate) extern "system" fn resource_served(
    me: ModHandle,
    key: *const enshwick::abi::ResourceKey,
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
                crate::content::served_now,
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
    key: *const enshwick::abi::ResourceKey,
    out: *mut u8,
    capacity: u32,
    len: *mut u32,
    read: impl FnOnce(enshwick::abi::ResourceKey) -> Result<Vec<u8>, Status>,
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
    key: enshwick::abi::ResourceKey,
    out: *mut u8,
    capacity: u32,
    len: *mut u32,
    read: impl FnOnce(enshwick::abi::ResourceKey) -> Result<Vec<u8>, Status>,
) -> Status {
    let data = match short_or(call, key, read) {
        Ok(data) => data,
        Err(status) => return status,
    };
    // SAFETY: the caller guarantees `len` null or writable and aligned, and
    // `out` null or `capacity` writable bytes apart from `data`, which this
    // function owns.
    let status = unsafe { enshwick::__private::copy_out(&data, out, capacity, len) };
    if status == Status::BAD_ARGS && !out.is_null() {
        keep(call, key, data);
    }
    status
}

/// Keeps `data` for this thread's retry of `call` with `key`.
fn keep(call: &'static str, key: enshwick::abi::ResourceKey, data: Vec<u8>) {
    SHORT.set(Some(Kept {
        call,
        key,
        begun: crate::content::builds_begun(),
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
        match crate::content::lend(len) {
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
    key: enshwick::abi::ResourceKey,
    /// [`crate::content::builds_begun`] when it was kept.
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
    key: enshwick::abi::ResourceKey,
    read: impl FnOnce(enshwick::abi::ResourceKey) -> Result<Vec<u8>, Status>,
) -> Result<Vec<u8>, Status> {
    match SHORT.take() {
        Some(kept)
            if kept.call == call
                && kept.key == key
                && kept.begun == crate::content::builds_begun() =>
        {
            Ok(kept.data)
        }
        _ => read(key),
    }
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

/// Every resource key of the game's archive, in archive order, read once.
static RESOURCE_KEYS: std::sync::OnceLock<Option<Vec<api_frame::ResourceKey>>> =
    std::sync::OnceLock::new();
/// Every content hash of the game's archive, read once.
static CONTENT_HASHES: std::sync::OnceLock<Option<Vec<[u32; 4]>>> = std::sync::OnceLock::new();
/// Every reflected type as ABI entries, built once.
static TYPES: std::sync::OnceLock<Option<enshwick::resources::TypeTable>> =
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
            .get_or_init(|| crate::archive::game().map(crate::archive::Archive::keys))
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
            .get_or_init(|| crate::archive::game().map(crate::archive::Archive::contents))
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
                (!registry.is_empty()).then(|| enshwick::resources::TypeTable::new(registry.iter()))
            })
            .as_ref()
        else {
            return Status::MISSING;
        };
        let entries = table.entries();
        // SAFETY: `HostApi::types` has the mod pass `out` null or writable
        // for `capacity` entries of `entry_size` bytes and `count` null or a
        // writable `u32`, untouched by the mod during the call.
        unsafe { crate::host::entries::mods::list(entries, out, entry_size, capacity, count) }
    })
}

pub(crate) extern "system" fn game_version(me: ModHandle, out: *mut Str) -> Status {
    entry("game_version", || {
        if let Err(status) = caller(me) {
            return status;
        }
        match crate::archive::game().and_then(crate::archive::Archive::version) {
            // SAFETY: `HostApi::game_version` has the mod pass `out` null or
            // pointing at a writable, aligned `Str` that no other thread
            // touches during the call; the archive lives for the process.
            Some(version) => unsafe { put(out, Str::new(version)) },
            None => Status::MISSING,
        }
    })
}

#[cfg(test)]
mod tests {
    use core::cell::Cell;

    use enshwick::abi::ResourceKey;

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
        drop(crate::content::enter_build(KEY));
        assert_eq!(short_or("resource_served", KEY, read), Ok(vec![1]));
        assert!(SHORT.take().is_none(), "a stale kept read goes");
    }
}
