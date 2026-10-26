//! The fake's served resources, recipes, texts and item data.

use core::ffi::c_void;

use crate::abi::{ModHandle, ResourceKey, ResourcePatchFn, Status, Str};
use crate::test::ServedResource;
use crate::test::table::{found, put, text, with};

pub(crate) extern "system" fn serve_resource(
    _me: ModHandle,
    key: *const ResourceKey,
    base: *const ResourceKey,
    patch: ResourcePatchFn,
    user: *mut c_void,
) -> Status {
    // SAFETY: the safe layer passes valid keys for the call; `as_ref` turns away null.
    let Some(key) = (unsafe { key.as_ref() }).copied() else {
        return Status::BAD_ARGS;
    };
    // SAFETY: the safe layer passes `base` null or pointing at a key valid
    // for the call; `as_ref` turns away null.
    let Some(base) = (unsafe { base.as_ref() }).copied() else {
        return Status::BAD_ARGS;
    };
    with(Status::MISSING, |s| {
        if s.served
            .iter()
            .any(|served| served.0.key == key && served.0.base != base)
        {
            return Status::REFUSED;
        }
        s.served.push((
            ServedResource { key, base },
            patch,
            user.expose_provenance(),
        ));
        Status::OK
    })
}

pub(crate) extern "system" fn rebuild_resource(_me: ModHandle, key: *const ResourceKey) -> Status {
    // SAFETY: the safe layer passes a valid key for the call; `as_ref` turns away null.
    let Some(key) = (unsafe { key.as_ref() }).copied() else {
        return Status::BAD_ARGS;
    };
    with(Status::MISSING, |s| {
        if !s.served.iter().any(|served| served.0.key == key) {
            return Status::BAD_ARGS;
        }
        s.rebuilds.push(key);
        Status::OK
    })
}

pub(crate) extern "system" fn recipe_rule(
    _me: ModHandle,
    kind: crate::abi::RecipeRuleKind,
    name: Str,
    disable: u32,
) -> Status {
    let name = text(name);
    with(Status::MISSING, |s| {
        s.recipe_rules.retain(|(k, n)| !(*k == kind && *n == name));
        if disable != 0 {
            s.recipe_rules.push((kind, name));
        }
        Status::OK
    })
}

pub(crate) extern "system" fn recipe_disabled(
    _me: ModHandle,
    recipe: u32,
    out: *mut u32,
) -> Status {
    let disabled = with(false, |s| s.host.disabled_recipes.contains(&recipe));
    if !out.is_null() {
        // SAFETY: the safe layer passes its out value for the call.
        unsafe { out.write(u32::from(disabled)) };
    }
    Status::OK
}

pub(crate) extern "system" fn recipe_kind(
    _me: ModHandle,
    item: u32,
    out: *mut crate::abi::RecipeKind,
) -> Status {
    found(with(None, |s| s.host.recipe_kinds.get(&item).copied()), out)
}

pub(crate) extern "system" fn add_text(
    _me: ModHandle,
    id: u32,
    language: u32,
    value: Str,
) -> Status {
    let value = text(value);
    with(Status::MISSING, |s| {
        s.texts.push((id, language, value));
        Status::OK
    })
}

pub(crate) extern "system" fn item_data_open(_me: ModHandle, id: Str, out: *mut u32) -> Status {
    let id = text(id);
    let full = if id.contains(':') {
        id
    } else {
        format!("{}:{id}", crate::test::MOD_ID)
    };
    let kind = with(None, |s| {
        let index = s
            .item_kinds
            .iter()
            .position(|k| *k == full)
            .unwrap_or_else(|| {
                s.item_kinds.push(full.clone());
                s.item_kinds.len() - 1
            });
        u32::try_from(index).ok()
    });
    found(kind, out)
}

pub(crate) extern "system" fn entity_data_open(
    me: ModHandle,
    id: Str,
    _flags: crate::abi::EntityDataFlags,
    out: *mut u32,
) -> Status {
    item_data_open(me, id, out)
}

pub(crate) extern "system" fn item_data_get(
    _me: ModHandle,
    kind: u32,
    pide: u32,
    item: u32,
    out: *mut u8,
    capacity: u32,
    len: *mut u32,
) -> Status {
    let value = with(None, |s| {
        s.item_values
            .get(&(kind, pide))
            .filter(|(held, _)| *held == item)
            .map(|(_, bytes)| bytes.clone())
    });
    let Some(value) = value else {
        return Status::MISSING;
    };
    let size = u32::try_from(value.len()).unwrap_or(u32::MAX);
    if !len.is_null() {
        // SAFETY: `len` is non-null, checked just before; the safe layer
        // passes it pointing at a writable, aligned `u32` for the call.
        unsafe { len.write(size) };
    }
    if size > capacity || out.is_null() {
        return Status::BAD_ARGS;
    }
    // SAFETY: `out` is non-null and the copy is at most `capacity` bytes, both
    // checked just before; the safe layer passes `capacity` writable bytes at
    // `out`, apart from the fake's own buffer.
    unsafe { out.copy_from_nonoverlapping(value.as_ptr(), value.len()) };
    Status::OK
}

pub(crate) extern "system" fn item_data_set(
    _me: ModHandle,
    kind: u32,
    pide: u32,
    item: u32,
    data: *const u8,
    len: u32,
) -> Status {
    if pide == 0 {
        return Status::BAD_ARGS;
    }
    let value = if len == 0 || data.is_null() {
        Vec::new()
    } else {
        // SAFETY: the safe layer passes `len` bytes at `data`.
        unsafe { core::slice::from_raw_parts(data, len as usize) }.to_vec()
    };
    with(Status::MISSING, |s| {
        if value.is_empty() {
            s.item_values.remove(&(kind, pide));
        } else {
            s.item_values.insert((kind, pide), (item, value));
        }
        Status::OK
    })
}

pub(crate) extern "system" fn content_serve(
    _me: ModHandle,
    data: *const u8,
    len: u32,
    out: *mut [u32; 4],
) -> Status {
    if data.is_null() || len == 0 {
        return Status::BAD_ARGS;
    }
    // SAFETY: the safe layer passes `len` bytes at `data`.
    let bytes = unsafe { core::slice::from_raw_parts(data, len as usize) }.to_vec();
    let hash = [
        len,
        u32::try_from(bytes.len()).unwrap_or(0) ^ 0x5a5a,
        bytes.iter().map(|&b| u32::from(b)).sum(),
        1,
    ];
    with(Status::MISSING, |s| {
        s.content.insert(hash, bytes);
        Status::OK
    });
    found(Some(hash), out)
}

pub(crate) extern "system" fn content_read(
    _me: ModHandle,
    hash: *const [u32; 4],
    out: *mut u8,
    capacity: u32,
    len: *mut u32,
) -> Status {
    // SAFETY: the safe layer passes a hash for the call.
    let Some(hash) = (unsafe { hash.as_ref() }).copied() else {
        return Status::BAD_ARGS;
    };
    let Some(data) = with(None, |s| {
        s.content
            .get(&hash)
            .or_else(|| s.host.archive.get(&hash))
            .cloned()
    }) else {
        return Status::MISSING;
    };
    let size = u32::try_from(data.len()).unwrap_or(u32::MAX);
    if !len.is_null() {
        // SAFETY: `len` is non-null, checked just before; the safe layer
        // passes it pointing at a writable, aligned `u32` for the call.
        unsafe { len.write(size) };
    }
    if size > capacity || out.is_null() {
        return Status::BAD_ARGS;
    }
    // SAFETY: `out` is non-null and the copy is at most `capacity` bytes, both
    // checked just before; the safe layer passes `capacity` writable bytes at
    // `out`, apart from the fake's own buffer.
    unsafe { out.copy_from_nonoverlapping(data.as_ptr(), data.len()) };
    Status::OK
}

pub(crate) extern "system" fn icons_ready(_me: ModHandle) -> Status {
    if with(false, |s| s.host.icons_ready) {
        Status::OK
    } else {
        Status::MISSING
    }
}

pub(crate) extern "system" fn resource_read(
    _me: ModHandle,
    key: *const ResourceKey,
    out: *mut u8,
    capacity: u32,
    len: *mut u32,
) -> Status {
    // SAFETY: the safe layer passes a key for the call.
    let Some(key) = (unsafe { key.as_ref() }).copied() else {
        return Status::BAD_ARGS;
    };
    let Some(data) = with(None, |s| {
        s.host
            .resources
            .get(&(key.guid, key.type_hash, key.part))
            .cloned()
    }) else {
        return Status::MISSING;
    };
    let size = u32::try_from(data.len()).unwrap_or(u32::MAX);
    if !len.is_null() {
        // SAFETY: `len` is non-null, checked just before; the safe layer
        // passes it pointing at a writable, aligned `u32` for the call.
        unsafe { len.write(size) };
    }
    if size > capacity || out.is_null() {
        return Status::BAD_ARGS;
    }
    // SAFETY: `out` is non-null and the copy is at most `capacity` bytes, both
    // checked just before; the safe layer passes `capacity` writable bytes at
    // `out`, apart from the fake's own buffer.
    unsafe { out.copy_from_nonoverlapping(data.as_ptr(), data.len()) };
    Status::OK
}

pub(crate) extern "system" fn game_text(
    _me: ModHandle,
    id: u32,
    language: u32,
    out: *mut Str,
) -> Status {
    let english = crate::i18n::Language::EN_US.0;
    let found = with(None, |s| {
        let texts = &s.host.game_texts;
        let language = if language == 0 { english } else { language };
        texts
            .get(&(id, language))
            .or_else(|| texts.get(&(id, english)))
            .copied()
    });
    match found {
        Some(text) => put(out, Str::new(text)),
        None => Status::MISSING,
    }
}
