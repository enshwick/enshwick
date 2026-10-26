//! Pixels for mods' textures on the client: content served from memory,
//! served UI images listed with the game's own, and the game's archive read
//! for vanilla pixels.
//!
//! A texture resource names its pixels by a content hash, which the game
//! reads from its containers through the resource database's content
//! request (`content.request`). The link answers requests for a hash a mod
//! serves straight from memory, as the database's own IO thread answers
//! the rest: it copies the asked range into the request's buffer and posts
//! a completion to the requester's ring, the record the game posts itself.
//!
//! The client's UI keeps a registry of every UI image key, filled once by
//! listing the database's keys of the UI image type; naming a key it lacks
//! ends the game. The links add the served UI images to that listing and
//! make the registry list once more after arming, so a served icon is known
//! before any item names it. Until both have happened [`ready`] stays false
//! and items keep their vanilla icons.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use arc_swap::ArcSwapOption;
use enshwick::abi::{HookFlags, Side, Status};
use enshwick::raw::{read_at, read_bytes_at, read_ptr_at, write_at, write_ptr_at};
use rustc_hash::FxHashMap;

use crate::catalogue::types::UI_IMAGE;
use crate::chains::Next;
use crate::host::Loader;
use crate::locks::Relock;
use crate::pinned;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::textures::KEYS.id(),
    pinned::textures::REGISTRY_FILL.id(),
    pinned::textures::REGISTRY_LISTED.id(),
    pinned::textures::REGISTRY_LISTED_AT.id(),
    pinned::textures::REQUEST.id(),
    pinned::textures::REQUEST_CONTEXT.id(),
    pinned::textures::REQUEST_HASH.id(),
    pinned::textures::REQUEST_OFFSET.id(),
    pinned::textures::REQUEST_POST.id(),
    pinned::textures::REQUEST_RING.id(),
    pinned::textures::REQUEST_SIGNAL.id(),
    pinned::textures::REQUEST_SIZE.id(),
    pinned::textures::RING_EVENT.id(),
    pinned::textures::RING_HEAD.id(),
    pinned::textures::RING_LAST.id(),
    pinned::textures::RING_RECORDS.id(),
    pinned::textures::RING_TAIL.id(),
    pinned::textures::RING_TAIL_NEXT.id(),
    pinned::textures::SIGNAL.id(),
];

/// A served content hash's last word, so it never meets one of the game's
/// by chance.
const MARK: u32 = 0x4b57_4845;

pub(crate) use crate::archive::ContentHash;

/// The content mods serve, by hash, swapped whole so requests read it
/// without a lock; `SERVING` orders the writers.
static CONTENT: ArcSwapOption<FxHashMap<ContentHash, &'static [u8]>> = ArcSwapOption::const_empty();
static SERVING: Mutex<()> = Mutex::new(());
/// Whether the content link ran, so served pixels reach the GPU.
static CONTENT_LINKED: AtomicBool = AtomicBool::new(false);
/// Whether the listing link handed the served UI images to the registry.
static LISTED: AtomicBool = AtomicBool::new(false);
/// Whether the registry listed again after arming.
static RELISTED: AtomicBool = AtomicBool::new(false);
static LAYOUT: OnceLock<Option<Layout>> = OnceLock::new();
/// The listing handed back for the game's own array (its address and
/// count): built once, as the game lists its UI images once or twice.
static COMBINED: OnceLock<Combined> = OnceLock::new();

/// The game's array, as its address and count, and the listing with the
/// served keys after it.
type Combined = ((usize, usize), &'static [[u8; KEY]]);
static LOADED: OnceLock<&'static Loader> = OnceLock::new();

// SAFETY: `RequestFn` is the signature of the build table's
// `textures.request`, and `link` uses this slot only as that function's next.
static REQUEST_NEXT: Next<RequestFn> = unsafe { Next::new() };
// SAFETY: `KeysFn` is the signature of the build table's `textures.keys`,
// and `link` uses this slot only as that function's next.
static KEYS_NEXT: Next<KeysFn> = unsafe { Next::new() };
// SAFETY: `FillFn` is the signature of the build table's
// `textures.registry_fill`, and `link` uses this slot only as that
// function's next.
static FILL_NEXT: Next<FillFn> = unsafe { Next::new() };

type RequestFn = unsafe extern "system" fn(requester: *mut u8, requests: *mut Requests);
type KeysFn = unsafe extern "system" fn(
    out: *mut KeysOut,
    database: *mut u8,
    type_hash: u32,
    found: *mut u32,
) -> *mut KeysOut;
type FillFn = unsafe extern "system" fn(registry: *mut u8);
type SignalFn = unsafe extern "system" fn(event: *mut u8);

/// The requests the game hands the content request, a slice.
#[repr(C)]
struct Requests {
    data: *mut u8,
    count: usize,
}

/// The key listing's result: a status byte, the keys and their count.
#[repr(C)]
struct KeysOut {
    status: u8,
    _reserved: [u8; 7],
    keys: *const [u8; KEY],
    count: u64,
}

/// A key in the database's tables: GUID, type, part and two zero words.
const KEY: usize = 32;

/// The build's offsets, after their code checked out.
#[derive(Clone, Copy, Debug)]
struct Layout {
    signal: SignalFn,
    request_size: usize,
    request_hash: usize,
    request_context: usize,
    request_offset: usize,
    ring_records: usize,
    ring_head: usize,
    ring_last: usize,
    ring_tail: usize,
    ring_tail_next: usize,
    ring_event: usize,
    registry_listed: usize,
}

/// The symbols whose code the texture hooks rely on.
const CHECKED: [pinned::Symbol; 8] = [
    pinned::textures::REQUEST,
    pinned::textures::REQUEST_RING,
    pinned::textures::REQUEST_POST,
    pinned::textures::REQUEST_SIGNAL,
    pinned::textures::KEYS,
    pinned::textures::REGISTRY_FILL,
    pinned::textures::REGISTRY_LISTED_AT,
    pinned::textures::SIGNAL,
];

fn layout(loader: &Loader) -> Option<&'static Layout> {
    LAYOUT
        .get_or_init(|| {
            if let Some(symbol) = CHECKED.iter().find(|s| !loader.code_holds(**s)) {
                crate::log!(
                    WARN,
                    "mod textures off: {symbol} is not the code the loader knows"
                );
                return None;
            }
            let o = |name: &str| loader.offset_by_name(&format!("textures.{name}"));
            let signal = usize::try_from(loader.address(pinned::textures::SIGNAL)?).ok()?;
            Some(Layout {
                // SAFETY: the build table's `textures.signal` is the game's
                // event signal with the `SignalFn` signature on this build,
                // and its code held in `Loader::code_holds`.
                signal: unsafe { core::mem::transmute::<usize, SignalFn>(signal) },
                request_size: o("request_size")?,
                request_hash: o("request_hash")?,
                request_context: o("request_context")?,
                request_offset: o("request_offset")?,
                ring_records: o("ring_records")?,
                ring_head: o("ring_head")?,
                ring_last: o("ring_last")?,
                ring_tail: o("ring_tail")?,
                ring_tail_next: o("ring_tail_next")?,
                ring_event: o("ring_event")?,
                registry_listed: o("registry_listed")?,
            })
        })
        .as_ref()
}

/// The hash a served `data` gets: its size, two FNV-1a words over it and
/// the loader's mark.
pub(crate) fn hash_of(data: &[u8]) -> ContentHash {
    let fnv = |seed: u32| {
        data.iter()
            .fold(seed, |h, &b| (h ^ u32::from(b)).wrapping_mul(0x0100_0193))
    };
    [
        u32::try_from(data.len()).unwrap_or(u32::MAX),
        fnv(0x811c_9dc5),
        fnv(0x2f69_3d1b),
        MARK,
    ]
}

/// Serves `data` as content and returns its hash; the same bytes give the
/// same hash.
pub(crate) fn serve(data: &[u8]) -> Result<ContentHash, Status> {
    if data.is_empty() || u32::try_from(data.len()).is_err() {
        return Err(Status::BAD_ARGS);
    }
    let hash = hash_of(data);
    let _serving = SERVING.relock();
    let current = CONTENT.load_full();
    if current.as_ref().is_some_and(|map| map.contains_key(&hash)) {
        return Ok(hash);
    }
    let mut map = current.map_or_else(FxHashMap::default, |map| (*map).clone());
    map.insert(hash, Box::leak(data.to_vec().into_boxed_slice()));
    CONTENT.store(Some(Arc::new(map)));
    Ok(hash)
}

fn served(
    content: &FxHashMap<ContentHash, &'static [u8]>,
    hash: &ContentHash,
) -> Option<&'static [u8]> {
    content.get(hash).copied()
}

/// Whether served UI images show on this client: the links ran and the
/// registry knows them.
pub(crate) fn ready() -> bool {
    CONTENT_LINKED.load(Ordering::Acquire)
        && LISTED.load(Ordering::Acquire)
        && RELISTED.load(Ordering::Acquire)
}

/// Links the content request, the key listing and the UI registry's fill
/// on a client that serves UI images.
pub(crate) fn link(loader: &'static Loader) {
    if loader.side != Side::CLIENT || crate::content::new_keys_of_type(UI_IMAGE).is_empty() {
        return;
    }
    let _first = LOADED.set(loader);
    if layout(loader).is_none() {
        return;
    }
    let none = HookFlags::NONE;
    let linked = loader.link_feature(
        "mod textures (content)",
        loader.address(pinned::textures::REQUEST),
        &REQUEST_NEXT,
        request as RequestFn,
        none,
    ) && loader.link_feature(
        "mod textures (keys)",
        loader.address(pinned::textures::KEYS),
        &KEYS_NEXT,
        keys as KeysFn,
        none,
    ) && loader.link_feature(
        "mod textures (UI registry)",
        loader.address(pinned::textures::REGISTRY_FILL),
        &FILL_NEXT,
        fill as FillFn,
        none,
    );
    if linked {
        crate::log!(INFO, "mod textures: linked for the served UI images");
    }
}

/// The switch of the mod textures.
static TEXTURES_FUSE: crate::fuse::Fuse = crate::fuse::Fuse::new("mod textures");

unsafe extern "system" fn request(requester: *mut u8, requests: *mut Requests) {
    CONTENT_LINKED.store(true, Ordering::Release);
    let content = CONTENT.load();
    let layout = LAYOUT.get().copied().flatten();
    let Some(content) = content
        .as_ref()
        .filter(|m| !m.is_empty() && !TEXTURES_FUSE.is_off())
    else {
        // SAFETY: this detour runs only as the `textures.request` link, so
        // the slot holds the next function of that signature; the game's
        // arguments pass unchanged.
        return unsafe { REQUEST_NEXT.get()(requester, requests) };
    };
    if requests.is_null() || requester.is_null() {
        // SAFETY: this detour runs only as the `textures.request` link, so
        // the slot holds the next function of that signature; the game's
        // arguments pass unchanged.
        return unsafe { REQUEST_NEXT.get()(requester, requests) };
    }
    let Some(layout) = layout else {
        // SAFETY: this detour runs only as the `textures.request` link, so
        // the slot holds the next function of that signature; the game's
        // arguments pass unchanged.
        return unsafe { REQUEST_NEXT.get()(requester, requests) };
    };
    // SAFETY: `requests` is non-null by the check before it and is the hooked
    // function's slice argument, a `Requests` live for the call.
    let data: *mut u8 = unsafe {
        read_ptr_at(
            requests.cast_const().cast(),
            core::mem::offset_of!(Requests, data),
        )
    };
    // SAFETY: `requests` is non-null by the check before it and is the hooked
    // function's slice argument, a `Requests` live for the call.
    let count: usize = unsafe {
        read_at(
            requests.cast_const().cast(),
            core::mem::offset_of!(Requests, count),
        )
    };
    let size = layout.request_size;
    let mut kept: Vec<u8> = Vec::new();
    let mut ours = 0usize;
    for index in 0..count {
        let at = data.wrapping_add(index * size).cast_const();
        // SAFETY: `at` is request `index` of the game's `count` requests,
        // each `textures.request_size` bytes long, live for the call; its
        // content hash sits at the build table's `textures.request_hash`
        // offset.
        let hash: ContentHash = unsafe { read_at(at, layout.request_hash) };
        let answered = served(content, &hash).and_then(|data| {
            TEXTURES_FUSE.run("the request", || {
                // SAFETY: `at` is a live request of `requester`'s, both
                // non-null hooked function's arguments, and this runs on the
                // thread the game asks on.
                unsafe { answer(&layout, requester, at, data) }
            })
        });
        if answered.is_some() {
            ours += 1;
        } else {
            let start = kept.len();
            kept.resize(start + size, 0);
            // SAFETY: `at` is request `index` of the game's requests, with
            // `textures.request_size` readable bytes live for the call.
            unsafe { read_bytes_at(at, 0, &mut kept[start..]) };
        }
    }
    if ours == 0 {
        // SAFETY: this detour runs only as the `textures.request` link, so
        // the slot holds the next function of that signature; the game's
        // arguments pass unchanged.
        return unsafe { REQUEST_NEXT.get()(requester, requests) };
    }
    if kept.is_empty() {
        return;
    }
    let mut rest = Requests {
        data: kept.as_mut_ptr(),
        count: kept.len() / size,
    };
    // SAFETY: this detour runs only as the `textures.request` link, so the
    // slot holds the next function of that signature; `rest` holds the
    // requests answered nowhere here, byte for byte as the game laid them
    // out, and lives until the call returns.
    unsafe { REQUEST_NEXT.get()(requester, &raw mut rest) };
}

/// Copies the range a request asks for into its buffer and posts the
/// completion, as the game's IO thread does.
///
/// # Safety
///
/// - `request` points at a live request of `requester`'s, readable for the
///   build table's `textures.request_size` bytes.
/// - `requester` is the live, non-null requester that asked.
/// - Called on the requesting thread, inside the content request.
unsafe fn answer(layout: &Layout, requester: *mut u8, request: *const u8, data: &[u8]) {
    // SAFETY: the caller guarantees a live request; a request starts with
    // the pointer to its buffer.
    let buffer = unsafe { read_ptr_at::<u8>(request, 0) };
    // SAFETY: the caller guarantees a live request; its second word is the
    // u64 count of bytes it asks for.
    let wanted: u64 = unsafe { read_at(request, 8) };
    // SAFETY: the caller guarantees a live request; its u32 offset into the
    // content sits at the build table's `textures.request_offset` offset.
    let offset: u32 = unsafe { read_at(request, layout.request_offset) };
    // SAFETY: the caller guarantees a live request; its 16-byte completion
    // context sits at the build table's `textures.request_context` offset.
    let context: [u8; 16] = unsafe { read_at(request, layout.request_context) };
    let range = usize::try_from(offset)
        .ok()
        .zip(usize::try_from(wanted).ok())
        .and_then(|(start, len)| data.get(start..start.checked_add(len)?));
    let code = match range {
        Some(bytes) if !buffer.is_null() => {
            // SAFETY: `buffer` is non-null by the guard and is the request's
            // buffer of `wanted` bytes; `bytes` is the `wanted` bytes taken
            // from `data`, which the game's buffer cannot overlap.
            unsafe { buffer.copy_from_nonoverlapping(bytes.as_ptr(), bytes.len()) };
            0u8
        }
        _ => 0x1f,
    };
    // SAFETY: the caller guarantees a live requester and the requesting
    // thread, all `post` asks for.
    unsafe { post(layout, requester, code, context) };
}

/// Posts a completion record, `code` and the request's context, to the
/// requester's ring and wakes its reader, as the game does.
///
/// # Safety
///
/// - `requester` is the live, non-null requester whose ring takes the
///   record.
/// - Called on the requesting thread, inside the content request, so no
///   other thread posts to that ring meanwhile.
unsafe fn post(layout: &Layout, requester: *mut u8, code: u8, context: [u8; 16]) {
    // SAFETY: the caller guarantees a live requester; its u32 ring head sits
    // at the build table's `textures.ring_head` offset.
    let head: u32 = unsafe { read_at(requester, layout.ring_head) };
    // SAFETY: the caller guarantees a live requester; its u32 last ring index
    // sits at the build table's `textures.ring_last` offset.
    let last: u32 = unsafe { read_at(requester, layout.ring_last) };
    let next = if head == last { 0 } else { head + 1 };
    // SAFETY: the caller guarantees a live requester; its u32 ring tail sits
    // at the build table's `textures.ring_tail` offset.
    let tail: u32 = unsafe { read_at(requester, layout.ring_tail) };
    let room = next != tail || {
        // SAFETY: the caller guarantees a live requester; its u32 next tail
        // sits at the build table's `textures.ring_tail_next` offset.
        let fresh: u32 = unsafe { read_at(requester, layout.ring_tail_next) };
        // SAFETY: the caller guarantees a live requester on the requesting
        // thread; its u32 ring tail sits at the build table's
        // `textures.ring_tail` offset.
        unsafe { write_at(requester, layout.ring_tail, fresh) };
        next != fresh
    };
    if room {
        // SAFETY: the caller guarantees a live requester; its ring records
        // pointer sits at the build table's `textures.ring_records` offset.
        let records = unsafe { read_ptr_at::<u8>(requester, layout.ring_records) };
        let mut record = [0u8; 24];
        record[0] = code;
        record[8..].copy_from_slice(&context);
        // SAFETY: `records` is the live requester's ring of 24-byte records
        // and `head` its current head, the record the game's own post writes
        // next; the caller's requesting thread is the ring's only writer now.
        unsafe { write_at(records, head as usize * 24, record) };
        // SAFETY: the caller guarantees a live requester on the requesting
        // thread; its u32 ring head sits at the build table's
        // `textures.ring_head` offset.
        unsafe { write_at(requester, layout.ring_head, next) };
    }
    // SAFETY: the caller guarantees a live requester; its event pointer sits
    // at the build table's `textures.ring_event` offset.
    let event = unsafe { read_ptr_at::<u8>(requester, layout.ring_event) };
    if !event.is_null() {
        // SAFETY: `event` is non-null by the check before it and is the
        // requester's event; `textures.signal` is the game's signal for it,
        // its code checked in `layout`.
        unsafe { (layout.signal)(event) };
    }
}

unsafe extern "system" fn keys(
    out: *mut KeysOut,
    database: *mut u8,
    type_hash: u32,
    found: *mut u32,
) -> *mut KeysOut {
    // SAFETY: this detour runs only as the `textures.keys` link, so the slot
    // holds the next function of that signature; the game's arguments pass
    // unchanged.
    let result = unsafe { KEYS_NEXT.get()(out, database, type_hash, found) };
    if type_hash != UI_IMAGE || out.is_null() {
        return result;
    }
    let _done = TEXTURES_FUSE.run("the keys", || {
        // SAFETY: `out` is non-null by the check before it and is the hooked
        // listing's result, which the listing has just written.
        unsafe { append(out) };
    });
    result
}

/// Points the listing's result at `keys` and marks it a success.
///
/// # Safety
///
/// - `out` is a live, writable `KeysOut` that nothing else touches during
///   the call.
unsafe fn hand_back(out: *mut KeysOut, keys: &'static [[u8; KEY]]) {
    let out = out.cast::<u8>();
    let (list, count) = (keys.as_ptr().cast_mut(), keys.len() as u64);
    // SAFETY: the caller guarantees a writable `KeysOut`, whose `keys`
    // pointer field sits at its `offset_of!`.
    unsafe { write_ptr_at(out, core::mem::offset_of!(KeysOut, keys), list) };
    // SAFETY: the caller guarantees a writable `KeysOut`, whose u64 `count`
    // field sits at its `offset_of!`.
    unsafe { write_at(out, core::mem::offset_of!(KeysOut, count), count) };
    // SAFETY: the caller guarantees a writable `KeysOut`, whose u8 `status`
    // field sits at its `offset_of!`.
    unsafe { write_at(out, core::mem::offset_of!(KeysOut, status), 0u8) };
}

/// Hands back the listing with the served UI images after the game's own.
///
/// # Safety
///
/// - `out` is the non-null result the game's key listing has just written,
///   live and writable for the call.
/// - Its keys, when not null, are `count` readable keys of [`KEY`] bytes
///   that stay alive, as the game never frees the listing.
/// - Called on the listing's thread, which alone touches `out` meanwhile.
unsafe fn append(out: *mut KeysOut) {
    let result = out.cast_const().cast::<u8>();
    // SAFETY: the caller guarantees a live `KeysOut`, whose `keys` pointer
    // field sits at its `offset_of!`.
    let listed: *const [u8; KEY] =
        unsafe { read_ptr_at::<[u8; KEY]>(result, core::mem::offset_of!(KeysOut, keys)) }
            .cast_const();
    // SAFETY: the caller guarantees a live `KeysOut`, whose u64 `count` field
    // sits at its `offset_of!`.
    let count: u64 = unsafe { read_at(result, core::mem::offset_of!(KeysOut, count)) };
    let count = usize::try_from(count).unwrap_or(0);
    let original = (listed.addr(), count);
    if let Some(&(from, keys)) = COMBINED.get()
        && from == original
    {
        // SAFETY: the caller guarantees `out` is live and writable, and
        // nothing else touches it during the call.
        unsafe { hand_back(out, keys) };
        LISTED.store(true, Ordering::Release);
        return;
    }
    let extra = crate::content::new_keys_of_type(UI_IMAGE);
    if extra.is_empty() {
        return;
    }
    let mut keys: Vec<[u8; KEY]> = Vec::with_capacity(count + extra.len());
    if !listed.is_null() {
        keys.extend((0..count).map(|index| {
            // SAFETY: `listed` is non-null by the check before it, and the caller
            // guarantees it holds `count` readable keys of `KEY` bytes;
            // `index` is below `count`.
            unsafe { read_at::<[u8; KEY]>(listed.cast(), index * KEY) }
        }));
    }
    for key in &extra {
        let mut entry = [0u8; KEY];
        entry[..16].copy_from_slice(&key.guid);
        entry[16..20].copy_from_slice(&key.type_hash.to_le_bytes());
        entry[20..24].copy_from_slice(&key.part.to_le_bytes());
        if !keys.contains(&entry) {
            keys.push(entry);
        }
    }
    let keys: &'static [[u8; KEY]] = Box::leak(keys.into_boxed_slice());
    let _first = COMBINED.set((original, keys));
    // SAFETY: the caller guarantees `out` is live and writable, and nothing
    // else touches it during the call.
    unsafe { hand_back(out, keys) };
    LISTED.store(true, Ordering::Release);
}

unsafe extern "system" fn fill(registry: *mut u8) {
    if !RELISTED.load(Ordering::Acquire)
        && !registry.is_null()
        && let Some(layout) = LAYOUT.get().copied().flatten()
    {
        // The registry listed before the loader armed: list once more, so
        // the served UI images join; the game skips keys it has.
        // SAFETY: `registry` is non-null by the check before it and is the hooked
        // fill's argument, used on the UI thread the game calls it on; its
        // "listed" byte sits at the build table's `textures.registry_listed`
        // offset, and only the fill reads it.
        unsafe { write_at(registry, layout.registry_listed, 0u8) };
        // SAFETY: this detour runs only as the `textures.registry_fill` link,
        // so the slot holds the next function of that signature; the game's
        // registry passes unchanged.
        unsafe { FILL_NEXT.get()(registry) };
        // SAFETY: `registry` is non-null by the check before it and is the hooked
        // fill's argument, live for the call; its "listed" byte sits at the
        // build table's `textures.registry_listed` offset.
        let listed: u8 = unsafe { read_at(registry, layout.registry_listed) };
        if listed == 1 {
            RELISTED.store(true, Ordering::Release);
            crate::log!(
                INFO,
                "mod textures: the UI registry knows the served images: {}",
                LISTED.load(Ordering::Acquire)
            );
        }
        return;
    }
    // SAFETY: this detour runs only as the `textures.registry_fill` link, so
    // the slot holds the next function of that signature; the game's
    // registry passes unchanged.
    unsafe { FILL_NEXT.get()(registry) };
}

/// Reads the game's archive for a mod: the loader's own copy of `kfc`
/// lookups, free of the game's state.
pub(crate) fn content_of(hash: &ContentHash) -> Result<Vec<u8>, Status> {
    crate::archive::game()
        .and_then(|archive| archive.content(hash))
        .ok_or(Status::MISSING)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_client_table_offers_a_check_for_the_code_the_hooks_rely_on() {
        let clients = crate::builds::load()
            .into_iter()
            .filter(|table| table.side() == enshwick_gamedata::Side::Client);
        for table in clients {
            let resolved = table.resolve();
            for symbol in CHECKED {
                let bytes = format!("{symbol}.bytes");
                assert!(
                    resolved.head(symbol.id()).is_some() || resolved.constant(&bytes).is_some(),
                    "{}: {symbol}",
                    table.name()
                );
            }
        }
    }

    #[test]
    fn a_served_hash_carries_its_size_and_the_mark() {
        let hash = hash_of(b"pixels");
        assert_eq!((hash[0], hash[3]), (6, MARK));
        assert_eq!(hash, hash_of(b"pixels"));
        assert_ne!(hash, hash_of(b"pixelz"));
    }
}
