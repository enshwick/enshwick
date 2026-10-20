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
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use arc_swap::ArcSwapOption;

use enshwick_loader_api::abi::{HookFlags, Side, Status};
use rustc_hash::FxHashMap;

use crate::chains::Next;
use crate::host::Loader;

/// The type hash of the client's UI image resource, which items name in
/// `iconImage` and the UI registry lists.
pub(crate) const UI_IMAGE: u32 = 0xf9e8_6997;
/// A served content hash's last word, so it never meets one of the game's
/// by chance.
const MARK: u32 = 0x4b57_4845;

/// A content hash as the game keeps it: the size, then three words.
pub(crate) type ContentHash = [u32; 4];

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

// SAFETY: each slot joins the chain of the function its type names.
static REQUEST_NEXT: Next<RequestFn> = unsafe { Next::new() };
// SAFETY: as above.
static KEYS_NEXT: Next<KeysFn> = unsafe { Next::new() };
// SAFETY: as above.
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

/// Whether the eight bytes at `symbol` are the build table's `bytes`.
fn code_matches(loader: &Loader, symbol: &str, bytes: &str) -> bool {
    let (Some(address), Some(expected)) = (loader.address(symbol), loader.build.constant(bytes))
    else {
        return false;
    };
    let Ok(address) = usize::try_from(address) else {
        return false;
    };
    // SAFETY: an address inside the game's image, which stays mapped.
    let found = unsafe { core::ptr::with_exposed_provenance::<u64>(address).read_unaligned() };
    found == expected
}

fn layout(loader: &Loader) -> Option<&'static Layout> {
    LAYOUT
        .get_or_init(|| {
            let checks = [
                ("textures.request", "textures.request_bytes"),
                ("textures.request_ring", "textures.request_ring_bytes"),
                ("textures.request_post", "textures.request_post_bytes"),
                ("textures.request_signal", "textures.request_signal_bytes"),
                ("textures.keys", "textures.keys_bytes"),
                ("textures.registry_fill", "textures.registry_fill_bytes"),
                (
                    "textures.registry_listed_at",
                    "textures.registry_listed_bytes",
                ),
                ("textures.signal", "textures.signal_bytes"),
            ];
            if let Some((symbol, _)) = checks.iter().find(|(s, b)| !code_matches(loader, s, b)) {
                crate::log!(
                    WARN,
                    "mod textures off: {symbol} is not the code the loader knows"
                );
                return None;
            }
            let o = |name: &str| loader.offset(&format!("textures.{name}"));
            let signal = usize::try_from(loader.address("textures.signal")?).ok()?;
            Some(Layout {
                // SAFETY: the symbol is the game's event signal on this build,
                // its code checked above.
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
    let _serving = SERVING.lock().unwrap_or_else(PoisonError::into_inner);
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
        loader.address("textures.request"),
        &REQUEST_NEXT,
        request as RequestFn,
        none,
    ) && loader.link_feature(
        "mod textures (keys)",
        loader.address("textures.keys"),
        &KEYS_NEXT,
        keys as KeysFn,
        none,
    ) && loader.link_feature(
        "mod textures (UI registry)",
        loader.address("textures.registry_fill"),
        &FILL_NEXT,
        fill as FillFn,
        none,
    );
    if linked {
        crate::log!(INFO, "mod textures: linked for the served UI images");
    }
}

/// Reads a `T` at `base + offset`.
///
/// # Safety
///
/// The address holds a readable `T`.
unsafe fn read<T: Copy>(base: *const u8, offset: usize) -> T {
    // SAFETY: forwarded from the caller.
    unsafe { base.wrapping_add(offset).cast::<T>().read_unaligned() }
}

/// Writes `value` at `base + offset`.
///
/// # Safety
///
/// The address holds a writable `T`.
unsafe fn write<T: Copy>(base: *mut u8, offset: usize, value: T) {
    // SAFETY: forwarded from the caller.
    unsafe { base.wrapping_add(offset).cast::<T>().write_unaligned(value) };
}

unsafe extern "system" fn request(requester: *mut u8, requests: *mut Requests) {
    CONTENT_LINKED.store(true, Ordering::Release);
    let content = CONTENT.load();
    let layout = LAYOUT.get().copied().flatten();
    let Some(content) = content.as_ref().filter(|m| !m.is_empty()) else {
        // SAFETY: forwarded unchanged.
        return unsafe { REQUEST_NEXT.get()(requester, requests) };
    };
    if requests.is_null() || requester.is_null() {
        // SAFETY: as above.
        return unsafe { REQUEST_NEXT.get()(requester, requests) };
    }
    let Some(layout) = layout else {
        // SAFETY: as above.
        return unsafe { REQUEST_NEXT.get()(requester, requests) };
    };
    // SAFETY: the game's slice of requests, valid for the call.
    let slice = unsafe { requests.read() };
    let size = layout.request_size;
    let mut kept: Vec<u8> = Vec::new();
    let mut ours = 0usize;
    for index in 0..slice.count {
        let at = slice.data.wrapping_add(index * size);
        // SAFETY: one request of the slice.
        let hash: ContentHash = unsafe { read(at, layout.request_hash) };
        match served(content, &hash) {
            Some(data) => {
                ours += 1;
                let _done = std::panic::catch_unwind(|| {
                    // SAFETY: a live request and its requester, on the
                    // thread the game asks on.
                    unsafe { answer(&layout, requester, at, data) }
                });
            }
            None => {
                // SAFETY: as above, `size` bytes.
                kept.extend_from_slice(unsafe { core::slice::from_raw_parts(at, size) });
            }
        }
    }
    if ours == 0 {
        // SAFETY: forwarded unchanged.
        return unsafe { REQUEST_NEXT.get()(requester, requests) };
    }
    if kept.is_empty() {
        return;
    }
    let mut rest = Requests {
        data: kept.as_mut_ptr(),
        count: kept.len() / size,
    };
    // SAFETY: the game's content request with the requests it did not get
    // answered here, laid out as it laid them out.
    unsafe { REQUEST_NEXT.get()(requester, &raw mut rest) };
}

/// Copies the range a request asks for into its buffer and posts the
/// completion, as the game's IO thread does.
///
/// # Safety
///
/// `request` is a live request of `requester`'s, on the requesting thread.
unsafe fn answer(layout: &Layout, requester: *mut u8, request: *const u8, data: &[u8]) {
    // SAFETY: the request starts with its buffer and the bytes it asks for.
    let buffer: *mut u8 = unsafe { read(request, 0) };
    // SAFETY: as above.
    let wanted: u64 = unsafe { read(request, 8) };
    // SAFETY: the request's offset into the content.
    let offset: u32 = unsafe { read(request, layout.request_offset) };
    // SAFETY: the request's completion context.
    let context: [u8; 16] = unsafe { read(request, layout.request_context) };
    let range = usize::try_from(offset)
        .ok()
        .zip(usize::try_from(wanted).ok())
        .and_then(|(start, len)| data.get(start..start.checked_add(len)?));
    let code = match range {
        Some(bytes) if !buffer.is_null() => {
            // SAFETY: the game's buffer of `wanted` bytes for this request.
            unsafe { buffer.copy_from_nonoverlapping(bytes.as_ptr(), bytes.len()) };
            0u8
        }
        _ => 0x1f,
    };
    // SAFETY: forwarded from the caller.
    unsafe { post(layout, requester, code, context) };
}

/// Posts a completion record, `code` and the request's context, to the
/// requester's ring and wakes its reader, as the game does.
///
/// # Safety
///
/// `requester` is the live requester, on the requesting thread.
unsafe fn post(layout: &Layout, requester: *mut u8, code: u8, context: [u8; 16]) {
    // SAFETY: the requester's ring fields at the build's offsets.
    let head: u32 = unsafe { read(requester, layout.ring_head) };
    // SAFETY: as above.
    let last: u32 = unsafe { read(requester, layout.ring_last) };
    let next = if head == last { 0 } else { head + 1 };
    // SAFETY: as above.
    let tail: u32 = unsafe { read(requester, layout.ring_tail) };
    let room = next != tail || {
        // SAFETY: as above.
        let fresh: u32 = unsafe { read(requester, layout.ring_tail_next) };
        // SAFETY: as above.
        unsafe { write(requester, layout.ring_tail, fresh) };
        next != fresh
    };
    if room {
        // SAFETY: as above.
        let records: *mut u8 = unsafe { read(requester, layout.ring_records) };
        let mut record = [0u8; 24];
        record[0] = code;
        record[8..].copy_from_slice(&context);
        // SAFETY: the ring's record at `head`, 24 bytes.
        unsafe { write(records, head as usize * 24, record) };
        // SAFETY: as above.
        unsafe { write(requester, layout.ring_head, next) };
    }
    // SAFETY: as above.
    let event: *mut u8 = unsafe { read(requester, layout.ring_event) };
    if !event.is_null() {
        // SAFETY: the game's signal of the requester's event.
        unsafe { (layout.signal)(event) };
    }
}

unsafe extern "system" fn keys(
    out: *mut KeysOut,
    database: *mut u8,
    type_hash: u32,
    found: *mut u32,
) -> *mut KeysOut {
    // SAFETY: forwarded unchanged.
    let result = unsafe { KEYS_NEXT.get()(out, database, type_hash, found) };
    if type_hash != UI_IMAGE || out.is_null() {
        return result;
    }
    let _done = std::panic::catch_unwind(|| {
        // SAFETY: the game's result, written by the listing.
        unsafe { append(out) };
    });
    result
}

/// Hands back the listing with the served UI images after the game's own.
///
/// # Safety
///
/// `out` is the listing's result.
unsafe fn append(out: *mut KeysOut) {
    // SAFETY: the game's result.
    let listed = unsafe { &mut *out };
    let count = usize::try_from(listed.count).unwrap_or(0);
    let original = (listed.keys.addr(), count);
    if let Some(&(from, keys)) = COMBINED.get()
        && from == original
    {
        listed.keys = keys.as_ptr();
        listed.count = keys.len() as u64;
        listed.status = 0;
        LISTED.store(true, Ordering::Release);
        return;
    }
    let extra = crate::content::new_keys_of_type(UI_IMAGE);
    if extra.is_empty() {
        return;
    }
    let mut keys: Vec<[u8; KEY]> = Vec::with_capacity(count + extra.len());
    if !listed.keys.is_null() {
        // SAFETY: the game's array of `count` keys, which no caller frees.
        keys.extend_from_slice(unsafe { core::slice::from_raw_parts(listed.keys, count) });
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
    listed.count = keys.len() as u64;
    listed.keys = keys.as_ptr();
    listed.status = 0;
    LISTED.store(true, Ordering::Release);
}

unsafe extern "system" fn fill(registry: *mut u8) {
    if !RELISTED.load(Ordering::Acquire)
        && !registry.is_null()
        && let Some(layout) = LAYOUT.get().copied().flatten()
    {
        // The registry listed before the loader armed: list once more, so
        // the served UI images join; the game skips keys it has.
        // SAFETY: the registry's "listed" byte, which only this function
        // reads, on the UI thread.
        unsafe { write(registry, layout.registry_listed, 0u8) };
        // SAFETY: the rest of the chain takes the registry the game passed.
        unsafe { FILL_NEXT.get()(registry) };
        // SAFETY: as above.
        let listed: u8 = unsafe { read(registry, layout.registry_listed) };
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
    // SAFETY: forwarded unchanged.
    unsafe { FILL_NEXT.get()(registry) };
}

/// The bytes of content `hash` from the game's own archive: the content map
/// of `<game>/<name>.kfc`, then the container `<name>_<index>.dat`. `None`
/// for a hash the archive lacks or keeps compressed.
pub(crate) fn read_game(game: &std::path::Path, name: &str, hash: &ContentHash) -> Option<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};

    let header = std::fs::read(game.join(format!("{name}.kfc"))).ok()?;
    let (container, offset) = archive::find(&header, hash)?;
    let mut file = std::fs::File::open(game.join(format!("{name}_{container:03}.dat"))).ok()?;
    file.seek(SeekFrom::Start(u64::from(offset))).ok()?;
    let mut data = vec![0u8; usize::try_from(hash[0]).ok()?];
    file.read_exact(&mut data).ok()?;
    Some(data)
}

/// Reads the game's archive for a mod: the loader's own copy of `kfc`
/// lookups, free of the game's state.
pub(crate) fn content_of(hash: &ContentHash) -> Result<Vec<u8>, Status> {
    let loader = LOADED
        .get()
        .copied()
        .or_else(crate::host::loader)
        .ok_or(Status::MISSING)?;
    let game = loader.mods_dir.parent().ok_or(Status::MISSING)?;
    let name = if loader.side == Side::CLIENT {
        "enshrouded"
    } else {
        "enshrouded_server"
    };
    read_game(game, name, hash).ok_or(Status::MISSING)
}

/// The archive's content map, read from its header.
mod archive {
    use super::ContentHash;

    /// A location in the header: an offset from the location's own field,
    /// and a count.
    fn location(header: &[u8], field: usize) -> Option<(usize, usize)> {
        let offset = u32::from_le_bytes(header.get(field..field + 4)?.try_into().ok()?);
        let count = u32::from_le_bytes(header.get(field + 4..field + 8)?.try_into().ok()?);
        Some((field.checked_add(offset as usize)?, count as usize))
    }

    fn word(bytes: &[u8], at: usize) -> Option<u32> {
        Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
    }

    /// The header's magic.
    const MAGIC: u32 = 0x3443_464b;
    /// The content map's buckets, keys and values, as header fields.
    const BUCKETS: usize = 0x40;
    const KEYS: usize = 0x48;
    const VALUES: usize = 0x50;

    /// The container and offset of an uncompressed content `hash`.
    pub(super) fn find(header: &[u8], hash: &ContentHash) -> Option<(u16, u32)> {
        if word(header, 0)? != MAGIC {
            return None;
        }
        let (buckets, bucket_count) = location(header, BUCKETS)?;
        let (keys, _) = location(header, KEYS)?;
        let (values, _) = location(header, VALUES)?;
        if bucket_count == 0 || !bucket_count.is_power_of_two() {
            return None;
        }
        let bucket = buckets + (hash[1] as usize & (bucket_count - 1)) * 8;
        let (first, count) = (
            word(header, bucket)? as usize,
            word(header, bucket + 4)? as usize,
        );
        for index in first..first.checked_add(count)? {
            let key = keys + index * 16;
            if (0..4).all(|i| word(header, key + i * 4) == Some(hash[i])) {
                let value = values + index * 16;
                let offset = word(header, value)?;
                let flags = u16::from_le_bytes(header.get(value + 4..value + 6)?.try_into().ok()?);
                let container =
                    u16::from_le_bytes(header.get(value + 6..value + 8)?.try_into().ok()?);
                return (flags == 0).then_some((container, offset));
            }
        }
        None
    }

    #[cfg(test)]
    mod tests {
        use enshwick_loader_api::content::icons::{Tint, decode, formats, tint};

        use super::*;

        /// A header with one bucket and two contents, the second
        /// compressed.
        fn header() -> Vec<u8> {
            let mut h = vec![0u8; 0x60];
            h[..4].copy_from_slice(&MAGIC.to_le_bytes());
            let put = |h: &mut Vec<u8>, field: usize, at: usize, count: u32| {
                h[field..field + 4]
                    .copy_from_slice(&u32::try_from(at - field).unwrap().to_le_bytes());
                h[field + 4..field + 8].copy_from_slice(&count.to_le_bytes());
            };
            put(&mut h, BUCKETS, 0x60, 1);
            put(&mut h, KEYS, 0x68, 2);
            put(&mut h, VALUES, 0x88, 2);
            h.resize(0xa8, 0);
            h[0x60..0x64].copy_from_slice(&0u32.to_le_bytes());
            h[0x64..0x68].copy_from_slice(&2u32.to_le_bytes());
            for (i, key) in [[10u32, 1, 2, 3], [20, 4, 5, 6]].iter().enumerate() {
                for (j, w) in key.iter().enumerate() {
                    let at = 0x68 + i * 16 + j * 4;
                    h[at..at + 4].copy_from_slice(&w.to_le_bytes());
                }
            }
            h[0x88..0x8c].copy_from_slice(&500u32.to_le_bytes());
            h[0x8e..0x90].copy_from_slice(&7u16.to_le_bytes());
            h[0x98..0x9c].copy_from_slice(&900u32.to_le_bytes());
            h[0x9c..0x9e].copy_from_slice(&1u16.to_le_bytes());
            h
        }

        #[test]
        fn a_content_is_found_in_its_bucket() {
            let h = header();
            assert_eq!(find(&h, &[10, 1, 2, 3]), Some((7, 500)));
            assert_eq!(find(&h, &[20, 4, 5, 6]), None, "compressed");
            assert_eq!(find(&h, &[10, 1, 2, 4]), None);
            assert_eq!(find(&h[..0x10], &[10, 1, 2, 3]), None);
        }

        /// The installed client's archive, when `ENSHWICK_CLIENT_DIR` names
        /// its folder (build 1269579): the Blacksmith's staff icon, the
        /// content its UI image names, reads whole.
        #[test]
        fn the_client_archive_holds_a_staff_icon() {
            let Ok(dir) = std::env::var("ENSHWICK_CLIENT_DIR") else {
                return;
            };
            let hash = [0x1_5570, 0xfa5b_71ba, 0x1c35_6c97, 0x170c_1f07];
            let data =
                super::super::read_game(std::path::Path::new(&dir), "enshrouded", &hash).unwrap();
            assert_eq!(data.len(), 0x1_5570);
            let mut rgba = decode(formats::BC7, 256, 256, &data).unwrap();
            let opaque = rgba
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[3] > 200)
                .count();
            assert!(opaque > 256 * 256 / 2, "{opaque} opaque pixels");
            tint(
                &mut rgba,
                Tint {
                    colour: [120, 170, 255],
                    amount: 60,
                },
            );
            assert!(rgba.as_chunks::<4>().0.iter().any(|p| p[2] > p[0]), "bluer");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_served_hash_carries_its_size_and_the_mark() {
        let hash = hash_of(b"pixels");
        assert_eq!((hash[0], hash[3]), (6, MARK));
        assert_eq!(hash, hash_of(b"pixels"));
        assert_ne!(hash, hash_of(b"pixelz"));
    }
}
