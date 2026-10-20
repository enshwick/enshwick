//! Served resources: mods add game resources or change vanilla ones, and the
//! game reads them as if they came from its archive.
//!
//! The loader links the resource database's open, resolve and key lookup.
//! Opening a served key returns a handle with the top bit set; resolving it
//! builds the resource from its base on first use and keeps it. Several mods
//! may patch one key: their patches run in load order, each on the result of
//! the one before. Every other key and handle goes to the game unchanged.

use core::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};

use enshwick_api::abi::{HookFlags, ResourceKey, ResourcePatchFn, Status};

use crate::chains::{LOADER, Link};
use crate::host::Loader;

/// Marks a handle the loader made.
const SERVED: u32 = 0x8000_0000;
/// The `resolve` result's third word for resident data, as the game writes it.
const RESIDENT: u64 = 0x1_0000_0000;

/// One mod's patch of a served resource.
struct Patch {
    owner: usize,
    apply: ResourcePatchFn,
    user: usize,
}

/// A served resource.
struct Served {
    key: ResourceKey,
    base: ResourceKey,
    patches: Vec<Patch>,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    /// The base's handle in the game's database, once opened.
    base_handle: u32,
    /// The built resource, once built: address and length.
    built: Option<(usize, usize)>,
    /// Set when building failed; the resource then reads as missing.
    failed: bool,
}

/// Mod texts while mods initialise: tag id, language, text.
static PENDING_TEXTS: Mutex<Vec<(u32, u32, String)>> = Mutex::new(Vec::new());
/// Mod texts once sealed: tag id to `(language, text)`, read without a lock.
static TEXTS: OnceLock<std::collections::HashMap<u32, Vec<(u32, &'static str)>>> = OnceLock::new();
static TEXT_NEXT: AtomicUsize = AtomicUsize::new(0);
static ENGLISH_NEXT: AtomicUsize = AtomicUsize::new(0);
static POOL_LANGUAGE: AtomicUsize = AtomicUsize::new(usize::MAX);
/// The English the game falls back to.
const ENGLISH: u32 = 1;

type TextFn =
    unsafe extern "system" fn(out: *mut [u64; 3], pool: *mut u8, id: u32) -> *mut [u64; 3];

/// Registers a mod text; refused after init.
pub(crate) fn add_text(id: u32, language: u32, text: &str) -> Status {
    if TEXTS.get().is_some() {
        return Status::TOO_LATE;
    }
    PENDING_TEXTS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push((id, language, text.to_owned()));
    Status::OK
}

/// The text of `id` for `language`: its own, else any-language, else English.
fn text_for(id: u32, language: u32) -> Option<&'static str> {
    let texts = TEXTS.get()?.get(&id)?;
    let pick = |wanted: u32| texts.iter().find(|(l, _)| *l == wanted).map(|(_, t)| *t);
    pick(language).or_else(|| pick(0)).or_else(|| pick(ENGLISH))
}

fn answer(out: *mut [u64; 3], text: &str) -> *mut [u64; 3] {
    if !out.is_null() {
        // SAFETY: the game's out record: error 0, the UTF-8 text and its length.
        unsafe {
            out.write([
                0,
                text.as_ptr().expose_provenance() as u64,
                text.len() as u64,
            ]);
        };
    }
    out
}

unsafe extern "system" fn lookup(out: *mut [u64; 3], pool: *mut u8, id: u32) -> *mut [u64; 3] {
    let offset = POOL_LANGUAGE.load(Ordering::Relaxed);
    let language = if pool.is_null() || offset == usize::MAX {
        ENGLISH
    } else {
        // SAFETY: the pool holds its current language there.
        unsafe { pool.wrapping_add(offset).cast::<u32>().read_unaligned() }
    };
    match text_for(id, language) {
        Some(text) => answer(out, text),
        // SAFETY: forwarded unchanged.
        None => unsafe { next::<TextFn>(&TEXT_NEXT)(out, pool, id) },
    }
}

unsafe extern "system" fn lookup_english(
    out: *mut [u64; 3],
    pool: *mut u8,
    id: u32,
) -> *mut [u64; 3] {
    match text_for(id, ENGLISH) {
        Some(text) => answer(out, text),
        // SAFETY: forwarded unchanged.
        None => unsafe { next::<TextFn>(&ENGLISH_NEXT)(out, pool, id) },
    }
}

/// Seals the mod texts and links the client's text lookups when there are any.
fn seal_texts(loader: &'static Loader) {
    let pending =
        std::mem::take(&mut *PENDING_TEXTS.lock().unwrap_or_else(PoisonError::into_inner));
    let count = pending.len();
    let mut texts: std::collections::HashMap<u32, Vec<(u32, &'static str)>> =
        std::collections::HashMap::new();
    for (id, language, text) in pending {
        let text: &'static str = Box::leak(text.into_boxed_str());
        texts.entry(id).or_default().push((language, text));
    }
    if TEXTS.set(texts).is_err() || count == 0 {
        return;
    }
    // Only the client shows text; the server has no lookups to link.
    let (Some(lookup_rva), Some(english_rva), Some(offset)) = (
        loader.build.symbol("text.lookup"),
        loader.build.symbol("text.lookup_english"),
        loader.build.constant("text.pool_language"),
    ) else {
        return;
    };
    POOL_LANGUAGE.store(
        usize::try_from(offset).unwrap_or(usize::MAX),
        Ordering::Relaxed,
    );
    for (rva, detour, next) in [
        (lookup_rva, lookup as *const (), &TEXT_NEXT),
        (english_rva, lookup_english as *const (), &ENGLISH_NEXT),
    ] {
        let link = Link {
            owner: LOADER,
            detour: detour.addr(),
            next,
            priority: i32::MIN,
            flags: HookFlags::NONE,
        };
        if let Ok(mut chains) = loader.chains.lock() {
            let status = chains.add(loader.base + rva, link);
            if !status.is_ok() {
                crate::log!(WARN, "mod texts off: a lookup was refused ({})", status.0);
            }
        }
    }
    crate::log!(INFO, "{count} texts from mods");
}

/// Registrations while mods initialise.
static PENDING: Mutex<Vec<Served>> = Mutex::new(Vec::new());
/// The registrations once sealed, read without a lock.
static SERVED_LIST: OnceLock<Box<[Served]>> = OnceLock::new();
static OPEN_NEXT: AtomicUsize = AtomicUsize::new(0);
static RESOLVE_NEXT: AtomicUsize = AtomicUsize::new(0);
static KEY_NEXT: AtomicUsize = AtomicUsize::new(0);

type OpenFn = unsafe extern "system" fn(
    out: *mut u32,
    database: *mut u8,
    key: *const ResourceKey,
) -> *mut u32;
type ResolveFn =
    unsafe extern "system" fn(out: *mut [u64; 3], database: *mut u8, handle: u32) -> *mut [u64; 3];
type KeyFn = unsafe extern "system" fn(out: *mut u8, database: *mut u8, handle: u32) -> *mut u8;

/// Adds mod `owner`'s patch of `key`; refused after init and for a key another
/// mod serves from a different base.
pub(crate) fn register(
    owner: usize,
    key: ResourceKey,
    base: ResourceKey,
    patch: ResourcePatchFn,
    user: *mut c_void,
) -> Status {
    if SERVED_LIST.get().is_some() {
        return Status::TOO_LATE;
    }
    let mut pending = PENDING.lock().unwrap_or_else(PoisonError::into_inner);
    let patch = Patch {
        owner,
        apply: patch,
        user: user.expose_provenance(),
    };
    match pending.iter_mut().find(|s| s.key == key) {
        Some(served) if served.base != base => return Status::REFUSED,
        Some(served) => served.patches.push(patch),
        None => pending.push(Served {
            key,
            base,
            patches: vec![patch],
            state: Mutex::new(State::default()),
        }),
    }
    Status::OK
}

/// Ends registration and links the database when anything is served.
pub(crate) fn seal_and_link(loader: &'static Loader) {
    LOADED.get_or_init(|| loader);
    seal_texts(loader);
    let served = std::mem::take(&mut *PENDING.lock().unwrap_or_else(PoisonError::into_inner));
    let count = served.len();
    if SERVED_LIST.set(served.into_boxed_slice()).is_err() || count == 0 {
        return;
    }
    LOADED.get_or_init(|| loader);
    let links: [(&str, *const (), &'static AtomicUsize); 3] = [
        ("resource.open", open as *const (), &OPEN_NEXT),
        ("resource.resolve", resolve as *const (), &RESOLVE_NEXT),
        ("resource.key_of", key_of as *const (), &KEY_NEXT),
    ];
    for (symbol, detour, next) in links {
        let Some(rva) = loader.build.symbol(symbol) else {
            crate::log!(WARN, "served resources off: {symbol} is not in this build");
            return;
        };
        let link = Link {
            owner: LOADER,
            detour: detour.addr(),
            next,
            priority: i32::MIN,
            flags: HookFlags::NONE,
        };
        if let Ok(mut chains) = loader.chains.lock() {
            let status = chains.add(loader.base + rva, link);
            if !status.is_ok() {
                crate::log!(
                    WARN,
                    "served resources off: {symbol} refused ({})",
                    status.0
                );
            }
        }
    }
    crate::log!(INFO, "{count} resources served by mods");
}

static LOADED: OnceLock<&'static Loader> = OnceLock::new();

fn next<F: Copy>(slot: &AtomicUsize) -> F {
    let address = slot.load(Ordering::Acquire);
    // SAFETY: the loader points the slot at the rest of the chain, which has
    // the hooked function's signature, before the link can run.
    unsafe { core::mem::transmute_copy::<usize, F>(&address) }
}

fn served(handle: u32) -> Option<&'static Served> {
    if handle & SERVED == 0 {
        return None;
    }
    let index = usize::try_from((handle & !SERVED).checked_sub(1)?).ok()?;
    SERVED_LIST.get()?.get(index)
}

fn mod_on(owner: usize) -> bool {
    LOADED
        .get()
        .and_then(|loader| loader.mods.get(owner))
        .is_some_and(|m| !m.off.load(Ordering::Acquire))
}

fn owner_on(served: &Served) -> bool {
    served.patches.iter().any(|p| mod_on(p.owner))
}

unsafe extern "system" fn open(
    out: *mut u32,
    database: *mut u8,
    key: *const ResourceKey,
) -> *mut u32 {
    // SAFETY: the game passes a key it owns for the call.
    let wanted = unsafe { key.as_ref() };
    let found = wanted.and_then(|wanted| {
        SERVED_LIST
            .get()?
            .iter()
            .enumerate()
            .find(|(_, s)| s.key == *wanted && owner_on(s))
    });
    let Some((index, entry)) = found else {
        // SAFETY: forwarded unchanged.
        return unsafe { next::<OpenFn>(&OPEN_NEXT)(out, database, key) };
    };
    let mut base_handle = 0u32;
    // SAFETY: the game's open with a key of ours, valid for the call.
    unsafe { next::<OpenFn>(&OPEN_NEXT)(&raw mut base_handle, database, &raw const entry.base) };
    if base_handle == 0 {
        // No base in this archive: the served key stays missing.
        // SAFETY: forwarded unchanged.
        return unsafe { next::<OpenFn>(&OPEN_NEXT)(out, database, key) };
    }
    entry
        .state
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .base_handle = base_handle;
    let handle = u32::try_from(index + 1).unwrap_or(0) | SERVED;
    if !out.is_null() {
        // SAFETY: the game's out handle.
        unsafe { out.write(handle) };
    }
    out
}

unsafe extern "system" fn resolve(
    out: *mut [u64; 3],
    database: *mut u8,
    handle: u32,
) -> *mut [u64; 3] {
    let Some(entry) = served(handle) else {
        // SAFETY: forwarded unchanged.
        return unsafe { next::<ResolveFn>(&RESOLVE_NEXT)(out, database, handle) };
    };
    let result =
        build(entry, database).map_or([0; 3], |(data, len)| [data as u64, len as u64, RESIDENT]);
    if !out.is_null() {
        // SAFETY: the game's out triple.
        unsafe { out.write(result) };
    }
    out
}

/// The served resource, built from its base on first use.
fn build(entry: &Served, database: *mut u8) -> Option<(usize, usize)> {
    let mut state = entry.state.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(built) = state.built {
        return Some(built);
    }
    if state.failed || state.base_handle == 0 {
        return None;
    }
    let mut base = [0u64; 3];
    // SAFETY: the game's resolve with the base's handle.
    unsafe { next::<ResolveFn>(&RESOLVE_NEXT)(&raw mut base, database, state.base_handle) };
    let (data, len) = (
        usize::try_from(base[0]).ok()?,
        usize::try_from(base[1]).ok()?,
    );
    if data == 0 {
        // The base is not resident yet; the game asks again.
        return None;
    }
    // Each patch builds on the one before; a failed one is skipped.
    let (mut current, mut current_len) = (data, len);
    let mut applied = Vec::new();
    for patch in entry.patches.iter().filter(|p| mod_on(p.owner)) {
        let (mut built, mut built_len) = (core::ptr::null::<u8>(), 0usize);
        // SAFETY: the mod's patch function with the bytes so far, valid for the call.
        let status = unsafe {
            (patch.apply)(
                core::ptr::with_exposed_provenance(current),
                current_len,
                core::ptr::with_exposed_provenance_mut(patch.user),
                &raw mut built,
                &raw mut built_len,
            )
        };
        if status != Status::OK || built.is_null() {
            if let Some(loader) = LOADED.get() {
                loader.turn_off(
                    patch.owner,
                    &format!("a served resource failed to build ({})", status.0),
                );
            }
            continue;
        }
        (current, current_len) = (built.addr(), built_len);
        applied.push(patch.owner);
    }
    if applied.is_empty() {
        state.failed = true;
        return None;
    }
    let result = (current, current_len);
    state.built = Some(result);
    if let Some(loader) = LOADED.get() {
        let by: Vec<&str> = applied
            .iter()
            .filter_map(|&owner| loader.mods.get(owner).map(|m| m.id.as_str()))
            .collect();
        crate::log!(
            INFO,
            "served resource {:08x}:{} built by {}, {current_len} bytes",
            entry.key.type_hash,
            hex(&entry.key.guid),
            by.join(", ")
        );
    }
    Some(result)
}

fn hex(bytes: &[u8]) -> String {
    use core::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut text, b| {
            let _written = write!(text, "{b:02x}");
            text
        })
}

unsafe extern "system" fn key_of(out: *mut u8, database: *mut u8, handle: u32) -> *mut u8 {
    let Some(entry) = served(handle) else {
        // SAFETY: forwarded unchanged.
        return unsafe { next::<KeyFn>(&KEY_NEXT)(out, database, handle) };
    };
    if !out.is_null() {
        // Status byte, a word, then the key padded to 32 bytes, as the game writes it.
        let mut bytes = [0u8; 0x28];
        bytes[8..24].copy_from_slice(&entry.key.guid);
        bytes[24..28].copy_from_slice(&entry.key.type_hash.to_le_bytes());
        bytes[28..32].copy_from_slice(&entry.key.part.to_le_bytes());
        // SAFETY: the game's out record of 0x28 bytes.
        unsafe { out.copy_from_nonoverlapping(bytes.as_ptr(), bytes.len()) };
    }
    out
}
