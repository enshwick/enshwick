//! The fake's served resources, recipes, data changes, game rates, texts
//! and item data.

use core::ffi::c_void;

use crate::test::ServedResource;
use crate::test::table::{Registered, Registration, found, put, put_list, text, with};
use enshwick_core::abi::{ModHandle, RateId, ResourceKey, ResourcePatchFn, Status, Str};

/// The changes of vanilla values a mod declared, each with its source.
#[derive(Debug, Default)]
pub(crate) struct DataChanges {
    /// Changes of one entry.
    pub(crate) one: Vec<(enshwick_core::content::changes::Change, String)>,
    /// Changes of every item a selection picks.
    pub(crate) items: Vec<(enshwick_core::content::changes::ItemsChange, String)>,
    /// Changes of every recipe a selection picks.
    pub(crate) recipes: Vec<(enshwick_core::content::changes::RecipesChange, String)>,
    /// Changes the mod sets later, numbered from 1 in this order, with
    /// the value in force.
    pub(crate) live: Vec<crate::test::LiveChange>,
}

/// The bytes from `address` to the end of the test's component that holds
/// it, at most `max`, as the loader counts the read-only set-up data a
/// component lies in; 0 outside every component.
pub(crate) extern "system" fn read_only(
    _me: ModHandle,
    address: usize,
    max: usize,
    out: *mut usize,
) -> Status {
    let len = with(0, |s| {
        s.host
            .components
            .values()
            .find(|&&(start, len)| (start..start + len).contains(&address))
            .map_or(0, |&(start, len)| (start + len - address).min(max))
    });
    put(out, len)
}

pub(crate) extern "system" fn rate(_me: ModHandle, rate: RateId) -> Status {
    with(Status::MISSING, |s| {
        if enshwick_core::__fake::Rate::from_id(rate).is_none() {
            return Status::BAD_ARGS;
        }
        let joins = s.manifest.as_ref().is_none_or(|manifest| manifest.joins);
        if !enshwick_core::__fake::rules::may_declare_rate(rate, joins) {
            return Status::REFUSED;
        }
        if s.host.missing_rates.contains(&rate) {
            return Status::MISSING;
        }
        if s.registration == Registration::Closed {
            return Status::TOO_LATE;
        }
        if !s.rates.iter().any(|&(declared, _)| declared == rate) {
            s.rates.push((rate, 1.0));
        }
        Status::OK
    })
}

pub(crate) extern "system" fn set_rate(_me: ModHandle, rate: RateId, factor: f32) -> Status {
    if !enshwick_core::__fake::is_valid_factor(factor) {
        return Status::BAD_ARGS;
    }
    with(Status::MISSING, |s| {
        match s.rates.iter_mut().find(|(declared, _)| *declared == rate) {
            Some((_, set)) => {
                *set = factor;
                Status::OK
            }
            None => Status::BAD_ARGS,
        }
    })
}

pub(crate) extern "system" fn serve_resource(
    me: ModHandle,
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
        let entry = (
            ServedResource { key, base },
            Registered::new(patch, user, me),
        );
        // As the loader: a change chains onto the key's maker, which goes
        // first whenever it comes.
        match s.served.iter().position(|served| served.0.key == key) {
            Some(at) if s.served[at].0.base == key && base != key => s.served.insert(at, entry),
            Some(at) if base != key && s.served[at].0.base != base => return Status::REFUSED,
            _ => s.served.push(entry),
        }
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
    kind: enshwick_core::abi::RecipeRuleKind,
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
    recipe: enshwick_core::RecipeId,
    out: *mut u32,
) -> Status {
    let disabled = with(false, |s| s.host.disabled_recipes.contains(&recipe.get()));
    if !out.is_null() {
        // SAFETY: the safe layer passes its out value for the call.
        unsafe { out.write(u32::from(disabled)) };
    }
    Status::OK
}

pub(crate) extern "system" fn recipe_kind(
    _me: ModHandle,
    item: enshwick_core::ItemId,
    out: *mut enshwick_core::abi::RecipeKind,
) -> Status {
    found(
        with(None, |s| s.host.recipe_kinds.get(&item.get()).copied()),
        out,
    )
}

pub(crate) extern "system" fn change_data(
    _me: ModHandle,
    change: *const enshwick_core::abi::DataChange,
) -> Status {
    // SAFETY: the safe layer passes null or a record with its header, valid
    // for the call; an all-zero record is valid, and `item` reads as zero
    // for a record of a mod built before it.
    let Some(raw) = (unsafe {
        enshwick_core::abi::read_headed(
            change,
            core::mem::offset_of!(enshwick_core::abi::DataChange, item),
        )
    }) else {
        return Status::BAD_ARGS;
    };
    // SAFETY: the safe layer borrows the source for the call.
    let source = unsafe { raw.source.as_str() }
        .unwrap_or_default()
        .to_owned();
    if raw.select != enshwick_core::abi::DataSelect::ENTRY {
        if let Ok(Some(change)) = enshwick_core::content::changes::RecipesChange::from_abi(&raw) {
            return with(Status::MISSING, |s| {
                s.changes.recipes.push((change, source));
                Status::OK
            });
        }
        let Ok(Some(change)) = enshwick_core::content::changes::ItemsChange::from_abi(&raw) else {
            return Status::BAD_ARGS;
        };
        return with(Status::MISSING, |s| {
            s.changes.items.push((change, source));
            Status::OK
        });
    }
    let Ok(Some(change)) = enshwick_core::content::changes::Change::from_abi(&raw) else {
        return Status::BAD_ARGS;
    };
    with(Status::MISSING, |s| {
        s.changes.one.push((change, source));
        Status::OK
    })
}

pub(crate) extern "system" fn declare_change(
    _me: ModHandle,
    change: *const enshwick_core::abi::DataChange,
    out: *mut enshwick_core::abi::ChangeId,
) -> Status {
    use enshwick_core::content::changes::{Change, Field, ItemsChange, Target};
    if out.is_null() {
        return Status::BAD_ARGS;
    }
    // SAFETY: the safe layer passes null or a full record with its header,
    // valid for the call.
    let Some(raw) = (unsafe {
        enshwick_core::abi::read_headed(
            change,
            <enshwick_core::abi::DataChange as enshwick_core::abi::Headed>::BORN,
        )
    }) else {
        return Status::BAD_ARGS;
    };
    let target = if raw.select == enshwick_core::abi::DataSelect::ENTRY {
        Change::from_abi(&raw)
            .ok()
            .flatten()
            .map(|c| (Target::Entry(c.entry), c.field))
    } else {
        ItemsChange::from_abi(&raw)
            .ok()
            .flatten()
            .map(|c| (Target::Items(c.items), c.field))
    };
    let Some((target, field)) = target.filter(|(_, field)| *field != Field::Ingredient) else {
        return Status::BAD_ARGS;
    };
    // SAFETY: the safe layer borrows the source for the call.
    let source = unsafe { raw.source.as_str() }
        .unwrap_or_default()
        .to_owned();
    with(Status::MISSING, |s| {
        s.changes.live.push(crate::test::content::LiveChange {
            target,
            field,
            source,
            value: None,
        });
        let id =
            enshwick_core::abi::ChangeId(u32::try_from(s.changes.live.len()).unwrap_or(u32::MAX));
        // SAFETY: `out` is non-null by the check above and the safe layer
        // points it at a writable `ChangeId` for the call.
        unsafe { out.write(id) };
        Status::OK
    })
}

pub(crate) extern "system" fn set_change(
    _me: ModHandle,
    id: enshwick_core::abi::ChangeId,
    op: enshwick_core::abi::ChangeOp,
    value: f64,
) -> Status {
    use enshwick_core::content::changes::Value;
    let value = match op {
        enshwick_core::abi::ChangeOp::SET => Some(Value::Set(value)),
        enshwick_core::abi::ChangeOp::TIMES => Some(Value::Times(value)),
        enshwick_core::abi::ChangeOp::RESET => None,
        _ => return Status::BAD_ARGS,
    };
    with(Status::MISSING, |s| {
        let at = usize::try_from(id.0).ok().and_then(|n| n.checked_sub(1));
        let Some(live) = at.and_then(|at| s.changes.live.get_mut(at)) else {
            return Status::BAD_ARGS;
        };
        if value.is_some_and(|value| live.field.check(value).is_err()) {
            return Status::BAD_ARGS;
        }
        live.value = value;
        Status::OK
    })
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
    _flags: enshwick_core::abi::EntityDataFlags,
    out: *mut u32,
) -> Status {
    item_data_open(me, id, out)
}

pub(crate) extern "system" fn item_data_get(
    _me: ModHandle,
    kind: u32,
    pide: enshwick_core::Entity,
    item: enshwick_core::ItemId,
    out: *mut u8,
    capacity: u32,
    len: *mut u32,
) -> Status {
    let value = with(None, |s| {
        s.item_values
            .get(&(kind, pide.0))
            .filter(|(held, _)| *held == item.get())
            .map(|(_, bytes)| bytes.clone())
    });
    let Some(value) = value else {
        return Status::MISSING;
    };
    // SAFETY: the safe layer passes `len` null or a writable, aligned `u32`,
    // and `out` null or `capacity` writable bytes apart from the fake's own.
    unsafe { enshwick_core::__fake::copy_out(&value, out, capacity, len) }
}

pub(crate) extern "system" fn item_data_set(
    _me: ModHandle,
    kind: u32,
    pide: enshwick_core::Entity,
    item: enshwick_core::ItemId,
    data: *const u8,
    len: u32,
) -> Status {
    if pide.is_none() {
        return Status::BAD_ARGS;
    }
    let value = if len == 0 || data.is_null() {
        Vec::new()
    } else {
        // SAFETY: the safe layer passes `len` bytes at `data`.
        unsafe { core::slice::from_raw_parts(data, len as usize) }.to_vec()
    };
    with(Status::MISSING, |s| {
        // The world's save keeps no player's character in place.
        if !value.is_empty() && s.host.players.iter().any(|p| p.entity == pide) {
            return Status::NOT_KEPT;
        }
        if value.is_empty() {
            s.item_values.remove(&(kind, pide.0));
        } else {
            s.item_values.insert((kind, pide.0), (item.get(), value));
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
    // SAFETY: the safe layer passes `len` null or a writable, aligned `u32`,
    // and `out` null or `capacity` writable bytes apart from the fake's own.
    unsafe { enshwick_core::__fake::copy_out(&data, out, capacity, len) }
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
    // SAFETY: the safe layer's arguments meet `copy_resource`'s guarantees.
    unsafe { copy_resource(key, out, capacity, len, |key| archived(&key)) }
}

pub(crate) extern "system" fn resource_served(
    _me: ModHandle,
    key: *const ResourceKey,
    out: *mut u8,
    capacity: u32,
    len: *mut u32,
) -> Status {
    // SAFETY: the safe layer's arguments meet `copy_resource`'s guarantees.
    unsafe {
        copy_resource(key, out, capacity, len, |key| {
            // As the loader: no read before every patch is in.
            if with(true, |s| s.registration == Registration::Open) {
                return Err(Status::REFUSED);
            }
            let Some(base) = with(None, |s| {
                s.served
                    .iter()
                    .find(|served| served.0.key == key)
                    .map(|served| served.0.base)
            }) else {
                return archived(&key);
            };
            let archived = archived(&base)?;
            match build_resource(&key, &archived) {
                // As the loader: a key whose every patch failed reads as
                // the archive has it.
                Err(Status::MISSING) if key == base => Ok(archived),
                Err(Status::MISSING) => self::archived(&key),
                built => built,
            }
        })
    }
}

/// The fake archive's resource `key`.
fn archived(key: &ResourceKey) -> Result<Vec<u8>, Status> {
    with(None, |s| {
        s.host
            .resources
            .get(&(key.guid, key.type_hash, key.part))
            .cloned()
    })
    .ok_or(Status::MISSING)
}

/// Copies the bytes `read` gives for the key at `key` to `out`, writing
/// their length to `len`; [`Status::BAD_ARGS`] when `capacity` is short.
///
/// # Safety
///
/// - `key` is null or points at a `ResourceKey` readable for the call.
/// - `len` is null or points at a writable, aligned `u32`.
/// - `out` is null or writable for `capacity` bytes, apart from `read`'s.
unsafe fn copy_resource(
    key: *const ResourceKey,
    out: *mut u8,
    capacity: u32,
    len: *mut u32,
    read: impl FnOnce(ResourceKey) -> Result<Vec<u8>, Status>,
) -> Status {
    // SAFETY: the caller guarantees `key` null or readable; `as_ref` turns
    // away null.
    let Some(key) = (unsafe { key.as_ref() }).copied() else {
        return Status::BAD_ARGS;
    };
    let data = match read(key) {
        Ok(data) => data,
        Err(status) => return status,
    };
    // SAFETY: the caller guarantees `len` null or writable and aligned, and
    // `out` null or `capacity` writable bytes apart from `data`, which this
    // function owns.
    unsafe { enshwick_core::__fake::copy_out(&data, out, capacity, len) }
}

/// A served key whose patches run, with the buffers they took, freed when
/// its build ends.
type Frame = (ResourceKey, Vec<Box<[u8]>>);

thread_local! {
    /// The builds running on this thread, as the loader's.
    static BUILDING: core::cell::RefCell<Vec<Frame>> =
        const { core::cell::RefCell::new(Vec::new()) };
}

pub(crate) extern "system" fn resource_buffer(
    _me: ModHandle,
    len: u32,
    out: *mut *mut u8,
) -> Status {
    let Ok(len) = usize::try_from(len) else {
        return Status::BAD_ARGS;
    };
    if out.is_null() {
        return Status::BAD_ARGS;
    }
    let lent = BUILDING.with_borrow_mut(|frames| {
        let (_, buffers) = frames.last_mut()?;
        // As the loader: bytes that do not allocate refuse the call.
        let mut buffer = Vec::new();
        buffer.try_reserve_exact(len).ok()?;
        buffer.resize(len, 0);
        buffers.push(buffer.into_boxed_slice());
        buffers.last_mut().map(|buffer| buffer.as_mut_ptr())
    });
    match lent {
        Some(at) => put(out, at),
        None => Status::REFUSED,
    }
}

/// Builds the served resource `key` from `base` with every mod's patches,
/// in order, as the game's first read of it would: a failed patch is
/// skipped, and [`Status::MISSING`] when none built. As the loader,
/// [`Status::BUILDING`] for a key whose patches already run on this thread.
pub(crate) fn build_resource(key: &ResourceKey, base: &[u8]) -> Result<Vec<u8>, Status> {
    if BUILDING.with_borrow(|building| building.iter().any(|(at, _)| at == key)) {
        return Err(Status::BUILDING);
    }
    let patches: Vec<_> = with(Vec::new(), |s| {
        s.served
            .iter()
            .filter(|s| s.0.key == *key)
            .map(|s| s.1)
            .collect()
    });
    let _building = Building::enter(*key);
    let mut bytes = base.to_vec();
    let mut built = false;
    for patch in patches {
        let mut out = core::ptr::null();
        let mut out_len = 0;
        // SAFETY: `bytes` is readable for the call, the outs are valid, and
        // `user` is the pointer the mod served with.
        let status = patch.run(|patch, user| unsafe {
            patch(
                bytes.as_ptr(),
                bytes.len(),
                user,
                &raw mut out,
                &raw mut out_len,
            )
        });
        if status.is_ok() && !out.is_null() {
            let lent = BUILDING.with_borrow(|frames| {
                frames
                    .last()
                    .is_some_and(|(_, buffers)| buffers.iter().any(|buffer| buffer.as_ptr() == out))
            });
            if !lent {
                with((), |s| s.kept_outputs += 1);
            }
            // SAFETY: on success the patch wrote bytes in a buffer its build
            // holds until it ends, or its own, which live for the process.
            bytes = unsafe { core::slice::from_raw_parts(out, out_len) }.to_vec();
            built = true;
        }
    }
    if built {
        Ok(bytes)
    } else {
        Err(Status::MISSING)
    }
}

/// Marks a key's patches running on this thread until dropped, unwinding
/// included, as the loader's.
struct Building;

impl Building {
    fn enter(key: ResourceKey) -> Self {
        BUILDING.with_borrow_mut(|building| building.push((key, Vec::new())));
        Self
    }
}

impl Drop for Building {
    fn drop(&mut self) {
        BUILDING.with_borrow_mut(Vec::pop);
    }
}

pub(crate) extern "system" fn game_text(
    _me: ModHandle,
    id: u32,
    language: u32,
    out: *mut Str,
) -> Status {
    let english = enshwick_core::i18n::Language::EN_US.0;
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

pub(crate) extern "system" fn resource_keys(
    _me: ModHandle,
    type_hash: u32,
    out: *mut ResourceKey,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let mut keys: Vec<ResourceKey> = with(Vec::new(), |s| {
        s.host
            .resources
            .keys()
            .filter(|(_, ty, _)| type_hash == 0 || *ty == type_hash)
            .map(|&(guid, type_hash, part)| ResourceKey {
                guid,
                type_hash,
                part,
            })
            .collect()
    });
    keys.sort_by_key(|key| (key.type_hash, key.guid, key.part));
    put_list(&keys, out, capacity, count)
}

pub(crate) extern "system" fn content_hashes(
    _me: ModHandle,
    out: *mut [u32; 4],
    capacity: u32,
    count: *mut u32,
) -> Status {
    let hashes: Vec<[u32; 4]> = with(Vec::new(), |s| s.host.archive.keys().copied().collect());
    put_list(&hashes, out, capacity, count)
}

pub(crate) extern "system" fn types(
    _me: ModHandle,
    out: *mut enshwick_core::abi::TypeEntry,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let Some(table) = with(None, |s| s.host.types.clone()) else {
        return Status::MISSING;
    };
    crate::test::table::base::mods::list(table.entries(), out, entry_size, capacity, count)
}

pub(crate) extern "system" fn game_version(_me: ModHandle, out: *mut Str) -> Status {
    match with(None, |s| s.host.game_version) {
        Some(version) => put(out, Str::new(version)),
        None => Status::MISSING,
    }
}
