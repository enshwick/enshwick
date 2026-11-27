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
//! and items keep their vanilla icons; once they have, every served key
//! whose build asked before is built again with its icons.

mod store;

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use enshwick_core::abi::{HookFlags, Side};
use enshwick_core::raw::{
    read_at, read_bytes_at, read_ptr_at, write_at, write_bytes_at, write_ptr_at,
};

use crate::engine::catalogue::types::{TEMPLATE, UI_IMAGE};
use crate::engine::chains::{Next, Onward};
use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, Target};
use crate::engine::pinned;
use enshwick_core::content::Guid;
use enshwick_values::fnv::{BASIS32, fnv1a32_from};

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::textures::KEYS.id(),
    pinned::textures::KEY_SIZE.id(),
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
    pinned::textures::RING_RECORD_CONTEXT.id(),
    pinned::textures::RING_RECORD_SIZE.id(),
    pinned::textures::RING_RECORDS.id(),
    pinned::textures::RING_TAIL.id(),
    pinned::textures::RING_TAIL_NEXT.id(),
    pinned::textures::SIGNAL.id(),
];

/// A served content hash's last word, so it never meets one of the game's
/// by chance.
const MARK: u32 = 0x4b57_4845;

pub(crate) use crate::engine::archive::ContentHash;

/// The content mods serve and how far the client's game took the served UI
/// images. The game's UI registry lives for the process, so nothing here
/// ends with a hosted game.
struct Textures {
    /// The content by hash, which requests read without a lock.
    content: store::Store,
    /// Whether the content link ran, so served pixels reach the GPU.
    linked: AtomicBool,
    /// Whether the listing link handed the served UI images to the registry.
    listed: AtomicBool,
    /// Whether the registry listed again after arming.
    relisted: AtomicBool,
    /// Whether the parts that wait for served UI images to show were told.
    shown: AtomicBool,
}

static TEXTURES: Textures = Textures::new();
static LAYOUT: OnceLock<Option<Layout>> = OnceLock::new();
/// The listing handed back for the game's own array (its address and
/// count): built once, as the game lists its UI images once or twice.
static COMBINED: OnceLock<Combined> = OnceLock::new();
/// The template listing handed back, as [`COMBINED`] is for UI images.
static TEMPLATES: OnceLock<Combined> = OnceLock::new();

/// The game's array, as its address and count, and the listing with the
/// served keys after it.
type Combined = ((usize, usize), &'static [[u8; KEY]]);

// SAFETY: `RequestFn` is the signature of the build table's
// `textures.request`, and `link` uses this slot only as that function's next.
static REQUEST_NEXT: Next<RequestFn> = unsafe { Next::new() };

/// The switch of the mod textures, of their two links: the content request
/// and the UI registry's fill, which arm together or not at all. They need
/// the key listing of [`TEMPLATES_FUSE`] as well, and pass the requests on
/// while it is off.
static TEXTURES_FUSE: Fuse = Fuse::new("mod textures");
/// The switch of the mod templates, whose one link is the key listing,
/// which lists the served UI images too while the mod textures are on.
static TEMPLATES_FUSE: Fuse = Fuse::new("mod templates");

type RequestFn = unsafe extern "system" fn(requester: *mut u8, requests: *mut Requests);
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

/// The start of a request: its buffer and how many bytes it asks for, which
/// the game copies as one 16-byte span (client builds 410 and 426).
#[derive(Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
#[repr(C)]
struct RequestSpan {
    buffer: usize,
    wanted: u64,
}

/// The room the loader keeps for one completion record of a ring.
const RECORD_ROOM: usize = 0x40; // bound, not a game offset: the loader's room for a ring record

/// A key in the database's tables: GUID, type, part and two zero words.
/// The table's `textures.key_size` must equal it, or mod textures stay off.
const KEY: usize = 32; // bound, not a game offset: the loader's key type, checked against the table

/// The build's offsets, after their code checked out.
#[derive(Clone, Copy, Debug)]
struct Layout {
    signal: SignalFn,
    request_size: usize,
    request_hash: usize,
    request_context: usize,
    request_offset: usize,
    ring_records: usize,
    /// How many bytes a ring's completion record takes, at most
    /// [`RECORD_ROOM`].
    ring_record_size: usize,
    /// Where a completion record holds its 16-byte context, after its code
    /// byte and inside the record.
    ring_record_context: usize,
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

/// The build's layout of the mod textures, read once; `None` turns them off
/// by their fuse, naming what the build lacks.
fn layout(loader: &Loader) -> Option<&'static Layout> {
    LAYOUT
        .get_or_init(|| {
            if let Some(symbol) = CHECKED.iter().find(|s| !loader.code_holds(**s)) {
                let _off = loader.fuse_off(&TEXTURES_FUSE, symbol.id(), UNKNOWN_CODE);
                return None;
            }
            if loader.offset(pinned::textures::KEY_SIZE) != Some(KEY) {
                let id = pinned::textures::KEY_SIZE.id();
                let _off = loader.fuse_off(&TEXTURES_FUSE, id, &format!("is not {KEY} bytes"));
                return None;
            }
            let read = read_layout(loader);
            if read.is_none() {
                let what = "are not all in this build, or a ring record runs past its room";
                let _off = loader.fuse_off(&TEXTURES_FUSE, "the texture offsets", what);
            }
            read
        })
        .as_ref()
}

/// Why a hook of the textures does not link: its code is not what the
/// table pins.
const UNKNOWN_CODE: &str = "is not the code the loader knows";

/// The build's offsets of the mod textures, once their code checked out;
/// `None` when one is missing or a ring record would run past its room.
fn read_layout(loader: &Loader) -> Option<Layout> {
    let o = |name: &str| loader.offset_by_name(&format!("textures.{name}"));
    let signal = usize::try_from(loader.address(pinned::textures::SIGNAL)?).ok()?;
    let (ring_record_size, ring_record_context) =
        (o("ring_record_size")?, o("ring_record_context")?);
    if !record_fits(ring_record_size, ring_record_context) {
        return None;
    }
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
        ring_record_size,
        ring_record_context,
        ring_head: o("ring_head")?,
        ring_last: o("ring_last")?,
        ring_tail: o("ring_tail")?,
        ring_tail_next: o("ring_tail_next")?,
        ring_event: o("ring_event")?,
        registry_listed: o("registry_listed")?,
    })
}

/// Whether a ring's completion record of `size` bytes fits the loader's
/// [`RECORD_ROOM`] and holds its 16-byte context at `context`, after the
/// record's code byte.
const fn record_fits(size: usize, context: usize) -> bool {
    size <= RECORD_ROOM
        && context > 0
        && match context.checked_add(size_of::<[u8; 16]>()) {
            Some(end) => end <= size,
            None => false,
        }
}

/// The hash a served `data` gets: its size, two FNV-1a words over it and
/// the loader's mark.
pub(crate) fn hash_of(data: &[u8]) -> ContentHash {
    [
        u32::try_from(data.len()).unwrap_or(u32::MAX),
        fnv1a32_from(BASIS32, data),
        fnv1a32_from(SECOND_SEED, data),
        MARK,
    ]
}

/// The seed of a served hash's second FNV-1a word, so two different
/// contents rarely share both words.
const SECOND_SEED: u32 = 0x2f69_3d1b;

/// Why content was not served or read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum TextureError {
    /// No bytes, or more than a `u32` counts.
    #[error("no bytes, or too many")]
    BadData,
    /// The game's archive has no such content.
    #[error("the archive has no such content")]
    NotInArchive,
    /// The content served this session reached the loader's limit, which
    /// lasts the process, as served content is never freed.
    #[error(
        "the served content reached its limit of {} entries or {} MiB",
        store::MAX_ENTRIES,
        store::MAX_BYTES >> 20
    )]
    Full,
}

/// Serves `data` as content and returns its hash; the same bytes give the
/// same hash.
pub(crate) fn serve(data: &[u8]) -> Result<ContentHash, TextureError> {
    if data.is_empty() || u32::try_from(data.len()).is_err() {
        return Err(TextureError::BadData);
    }
    let hash = hash_of(data);
    TEXTURES
        .content
        .insert(hash, data)
        .map_err(|store::Full| TextureError::Full)?;
    Ok(hash)
}

impl Textures {
    const fn new() -> Self {
        Self {
            content: store::Store::new(),
            linked: AtomicBool::new(false),
            listed: AtomicBool::new(false),
            relisted: AtomicBool::new(false),
            shown: AtomicBool::new(false),
        }
    }

    fn ready(&self) -> bool {
        self.linked.load(Ordering::Acquire)
            && self.listed.load(Ordering::Acquire)
            && self.relisted.load(Ordering::Acquire)
    }

    /// Whether the parts that wait for served UI images are to be told now:
    /// true once, at the first ask after the images show.
    fn announces(&self) -> bool {
        self.ready() && !self.shown.swap(true, Ordering::AcqRel)
    }
}

/// Whether served UI images show on this client: the links ran and the
/// registry knows them. A game build of a served key that asks before they
/// show is built again once they show.
pub(crate) fn ready() -> bool {
    if !TEXTURES.shown.load(Ordering::Acquire) && may_show() {
        crate::content::served::note_icons_read();
    }
    TEXTURES.ready()
}

/// Whether served UI images may still show: on a client, the only side
/// whose images ever show, while the mod textures are on. Otherwise no
/// rebuild comes, so no build is noted and keeps its base for one.
fn may_show() -> bool {
    !TEXTURES_FUSE.is_off()
        && crate::engine::loader::loader().is_some_and(|loader| loader.side == Side::CLIENT)
}

/// Whether this client's UI registry knows the UI image `guid`, the game's
/// or a served one, once served images show: naming any other ends the game.
pub(crate) fn knows(guid: Guid) -> bool {
    ready()
        && COMBINED
            .get()
            .is_some_and(|(_, keys)| keys.iter().any(|key| key[..16] == guid.0[..]))
}

/// Tells the parts that wait for served UI images to show, once they show.
/// Only the UI registry's fill calls it: the UI thread draws every icon, so
/// what it rewrites is never read halfway.
#[cold]
fn announce_ready() {
    if TEXTURES.announces() {
        crate::content::served::icons_shown();
    }
}

/// Links the key listing on any side that serves templates or UI images of
/// its own, so the game registers them, and the content request and the UI
/// registry's fill on a client that serves UI images.
pub(crate) fn link(loader: &'static Loader) {
    let images = loader.side == Side::CLIENT
        && !crate::content::served::new_keys_of_type(UI_IMAGE).is_empty();
    let templates = !crate::content::served::new_keys_of_type(TEMPLATE).is_empty()
        || !crate::content::served::orphan_templates().is_empty();
    if !images && !templates {
        return;
    }
    let listed = link_keys(loader);
    if !images {
        return;
    }
    if !listed {
        let why = "is off, and the game would not list the served UI images";
        let _off = loader.fuse_off(&TEXTURES_FUSE, TEMPLATES_FUSE.feature(), why);
        return;
    }
    if link_textures(loader) {
        crate::log!(INFO, "mod textures: linked for the served UI images");
    }
}

/// Links the content request and the UI registry's fill, both or none, once
/// their code and layout check out; whether they linked.
fn link_textures(loader: &Loader) -> bool {
    if layout(loader).is_none() {
        return false;
    }
    let none = HookFlags::NONE;
    let (request_at, fill_at) = (
        Target::Pinned(pinned::textures::REQUEST),
        Target::Pinned(pinned::textures::REGISTRY_FILL),
    );
    loader.link_feature(&TEXTURES_FUSE, request_at, &REQUEST_NEXT, request, none)
        && loader.link_feature(&TEXTURES_FUSE, fill_at, &FILL_NEXT, fill, none)
}

/// Links the key listing, for the served templates and UI images, once its
/// code and key size check out; whether it linked.
fn link_keys(loader: &Loader) -> bool {
    if !loader.code_holds(pinned::textures::KEYS) {
        let _off = loader.fuse_off(&TEMPLATES_FUSE, pinned::textures::KEYS.id(), UNKNOWN_CODE);
        return false;
    }
    if loader.offset(pinned::textures::KEY_SIZE) != Some(KEY) {
        let id = pinned::textures::KEY_SIZE.id();
        let _off = loader.fuse_off(&TEMPLATES_FUSE, id, &format!("is not {KEY} bytes"));
        return false;
    }
    let keys_at = Target::Pinned(pinned::textures::KEYS);
    let linked = loader.link_feature(&TEMPLATES_FUSE, keys_at, &KEYS_NEXT, keys, HookFlags::NONE);
    if linked {
        crate::log!(INFO, "mod templates: the key listing is linked");
    }
    linked
}

/// The loader's link on the table's `textures.request`: answers the
/// requests for served content and passes the rest on. Written by hand, not
/// by `chain_detour!`, since it passes the rest of the chain only the
/// requests it did not answer.
unsafe extern "system" fn request(requester: *mut u8, requests: *mut Requests) {
    TEXTURES.linked.store(true, Ordering::Release);
    let content = &TEXTURES.content;
    let layout = LAYOUT.get().copied().flatten();
    if content.is_empty() || TEXTURES_FUSE.is_off() || TEMPLATES_FUSE.is_off() {
        // SAFETY: this detour runs only as the `textures.request` link, so
        // the slot holds the next function of that signature; the game's
        // arguments pass unchanged.
        return unsafe { REQUEST_NEXT.get()(requester, requests) };
    }
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
    if !requests_read(data.addr(), count, size, crate::engine::pages::query) {
        // SAFETY: this detour runs only as the `textures.request` link, so
        // the slot holds the next function of that signature; the game's
        // arguments pass unchanged.
        return unsafe { REQUEST_NEXT.get()(requester, requests) };
    }
    let mut kept: Vec<u8> = Vec::new();
    let mut ours = 0usize;
    for index in 0..count {
        let at = data.wrapping_add(index * size).cast_const();
        // SAFETY: `at` is request `index` of the game's `count` requests,
        // each `textures.request_size` bytes long, live for the call; its
        // content hash sits at the build table's `textures.request_hash`
        // offset.
        let hash: ContentHash = unsafe { read_at(at, layout.request_hash) };
        let answered = content.get(&hash).and_then(|data| {
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

/// Whether the `count` requests of `size` bytes at `data`, counts read from
/// the game's memory, lie in one region that reads, as `query` reports it,
/// so the loop over them reads nothing else and the requests it keeps for
/// the game copy no more than the game mapped.
fn requests_read(
    data: usize,
    count: usize,
    size: usize,
    query: impl FnOnce(usize) -> Option<crate::engine::pages::Region>,
) -> bool {
    if count == 0 || data == 0 {
        return false;
    }
    let Some(end) = count
        .checked_mul(size)
        .and_then(|len| data.checked_add(len))
    else {
        return false;
    };
    query(data).is_some_and(|region| region.reads() && end <= region.end)
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
    // SAFETY: the caller guarantees a live request, which starts with the
    // span of its buffer and the count of bytes it asks for.
    let RequestSpan { buffer, wanted } = unsafe { read_at(request, 0) };
    let buffer = core::ptr::with_exposed_provenance_mut::<u8>(buffer);
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
        let (size, at) = (layout.ring_record_size, layout.ring_record_context);
        let mut record = [0u8; RECORD_ROOM];
        record[0] = code;
        record[at..at + context.len()].copy_from_slice(&context);
        // SAFETY: `records` is the live requester's ring of records of the
        // build table's `textures.ring_record_size` and `head` its current
        // head, the record the game's own post writes next; the caller's
        // requesting thread is the ring's only writer now.
        unsafe { write_bytes_at(records, head as usize * size, &record[..size]) };
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

crate::engine::chains::chain_detour! {
    /// The table's `textures.keys(out, database, type, found)`, the
    /// database's key listing of a type: the served templates join it, and
    /// so do the served UI images while the mod textures are on. It belongs
    /// to the mod templates, so a fault in the textures leaves the
    /// templates listed.
    fn keys(out: *mut KeysOut, database: *mut u8, type_hash: u32, found: *mut u32) -> *mut KeysOut;
    next KEYS_NEXT;
    fuse &TEMPLATES_FUSE, "the keys";
    |onward| {
        let images = !TEXTURES_FUSE.is_off() && REQUEST_NEXT.wired().is_some();
        // SAFETY: the game's own listing result, written by the rest of the
        // chain, on the listing's thread.
        unsafe { listed(onward, out, type_hash, images) }
    }
}

/// The listing the rest of the chain `onward` writes to `out`, with the
/// served keys of `type_hash` after the game's own when it lists them.
///
/// # Safety
///
/// - `out` is the game's own listing result, null or live and writable for
///   the call, on the listing's thread, which alone touches it meanwhile.
unsafe fn listed(
    onward: &Onward<*mut KeysOut, impl Fn() -> *mut KeysOut>,
    out: *mut KeysOut,
    type_hash: u32,
    images: bool,
) -> *mut KeysOut {
    let result = onward.call();
    if lists(type_hash, images) && !out.is_null() {
        // SAFETY: `out` is non-null by the check before it and is the
        // listing's result, which the listing has just written.
        unsafe { append(out, type_hash) };
    }
    result
}

/// Whether the listing of `type_hash` gets served keys: templates always,
/// UI images only while `images` are armed.
fn lists(type_hash: u32, images: bool) -> bool {
    type_hash == TEMPLATE || (images && type_hash == UI_IMAGE)
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

/// Hands back the listing of `type_hash` with the served keys of that type
/// after the game's own: UI images, which the client's UI registry lists, and
/// templates, which every side registers once at start from this listing
/// (server 399, checked live with a debugger).
///
/// # Safety
///
/// - `out` is the non-null result the game's key listing has just written,
///   live and writable for the call.
/// - Its keys, when not null, are `count` readable keys of [`KEY`] bytes
///   that stay alive, as the game never frees the listing.
/// - Called on the listing's thread, which alone touches `out` meanwhile.
unsafe fn append(out: *mut KeysOut, type_hash: u32) {
    let (combined, images) = if type_hash == UI_IMAGE {
        (&COMBINED, true)
    } else {
        (&TEMPLATES, false)
    };
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
    if let Some(&(from, keys)) = combined.get()
        && from == original
    {
        // SAFETY: the caller guarantees `out` is live and writable, and
        // nothing else touches it during the call.
        unsafe { hand_back(out, keys) };
        TEXTURES.listed.fetch_or(images, Ordering::AcqRel);
        return;
    }
    let extra = crate::content::served::new_keys_of_type(type_hash);
    let orphans = if images {
        Vec::new()
    } else {
        crate::content::served::orphan_templates()
    };
    if extra.is_empty() && orphans.is_empty() {
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
    with_served(&mut keys, &extra);
    with_aliases(&mut keys, &orphans);
    let keys: &'static [[u8; KEY]] = Box::leak(keys.into_boxed_slice());
    let _first = combined.set((original, keys));
    // SAFETY: the caller guarantees `out` is live and writable, and nothing
    // else touches it during the call.
    unsafe { hand_back(out, keys) };
    TEXTURES.listed.fetch_or(images, Ordering::AcqRel);
    if !images {
        crate::log!(
            INFO,
            "mod templates: {} listed for the game's template registry, {} of removed mods as vanilla ones",
            extra.len(),
            orphans.len()
        );
    }
}

/// `keys`, the game's listing, with each of `served` after them as the
/// listing lays a key out (GUID, type hash, part), once each.
fn with_served(keys: &mut Vec<[u8; KEY]>, served: &[enshwick_core::abi::ResourceKey]) {
    for key in served {
        let mut entry = [0u8; KEY];
        entry[..16].copy_from_slice(&key.guid.0);
        entry[16..20].copy_from_slice(&key.type_hash.to_le_bytes());
        entry[20..24].copy_from_slice(&key.part.to_le_bytes());
        if !keys.contains(&entry) {
            keys.push(entry);
        }
    }
}

/// `keys`, the game's listing, with each clone of `aliases` listed as its
/// vanilla template's entry with the clone's GUID, once each; a clone whose
/// vanilla template the game does not list stays out.
fn with_aliases(keys: &mut Vec<[u8; KEY]>, aliases: &[(Guid, Guid)]) {
    for (clone, vanilla) in aliases {
        let Some(mut entry) = keys
            .iter()
            .find(|key| key[..16] == vanilla.0[..] && key[16..20] == TEMPLATE.to_le_bytes())
            .copied()
        else {
            continue;
        };
        entry[..16].copy_from_slice(&clone.0);
        if !keys.contains(&entry) {
            keys.push(entry);
        }
    }
}

crate::engine::chains::chain_detour! {
    /// The table's `textures.registry_fill(registry)`, the UI registry's fill, called again and
    /// again on the UI thread: once after arming it lists again, so the served UI images join,
    /// and once they show it tells the parts that wait. Every other call costs one load.
    fn fill(registry: *mut u8) -> ();
    next FILL_NEXT;
    |onward| {
        if TEXTURES.shown.load(Ordering::Acquire) || registry.is_null() {
            return onward.call();
        }
        if TEXTURES.relisted.load(Ordering::Acquire) {
            if !TEXTURES.ready() {
                return onward.call();
            }
            return onward.run(&TEXTURES_FUSE, "the UI registry fill", |onward| {
                announce_ready();
                onward.call();
            });
        }
        onward.run(&TEXTURES_FUSE, "the UI registry fill", |onward| {
            let Some(layout) = LAYOUT.get().copied().flatten() else {
                return onward.call();
            };
            // The registry listed before the loader armed: list once more,
            // so the served UI images join; the game skips keys it has.
            // SAFETY: `registry` is non-null by the check before it and is
            // the hooked fill's argument, used on the UI thread the game
            // calls it on; its "listed" byte sits at the build table's
            // `textures.registry_listed` offset, and only the fill reads it.
            unsafe { write_at(registry, layout.registry_listed, 0u8) };
            onward.call();
            // SAFETY: `registry` is non-null by the check before it and is
            // the hooked fill's argument, live for the call; its "listed"
            // byte sits at the build table's `textures.registry_listed`
            // offset.
            let listed: u8 = unsafe { read_at(registry, layout.registry_listed) };
            if listed == 1 {
                TEXTURES.relisted.store(true, Ordering::Release);
                crate::log!(
                    INFO,
                    "mod textures: the UI registry knows the served images: {}",
                    TEXTURES.listed.load(Ordering::Acquire)
                );
                announce_ready();
            }
        });
    }
}

/// Reads the game's archive for a mod: the loader's own copy of `kfc`
/// lookups, free of the game's state.
pub(crate) fn content_of(hash: &ContentHash) -> Result<Vec<u8>, TextureError> {
    crate::engine::archive::game()
        .and_then(|archive| archive.content(hash))
        .ok_or(TextureError::NotInArchive)
}

/// Readies the content request for its benchmark with a stand-in for the
/// game's: nothing served, so each request passes on, as on a game whose
/// mods serve no pixels. The closure runs one request.
#[cfg(feature = "test")]
#[doc(hidden)]
pub fn bench_texture_request() -> impl FnMut() {
    /// Stands in for the game's content request: answers nothing.
    unsafe extern "system" fn game(_: *mut u8, _: *mut Requests) {}
    let next: RequestFn = game;
    #[expect(
        clippy::fn_to_numeric_cast_any,
        reason = "a slot holds its next function's address"
    )]
    REQUEST_NEXT.slot().store(next as usize, Ordering::Release);
    let mut requester = 0u8;
    let mut requests = Requests {
        data: core::ptr::null_mut(),
        count: 0,
    };
    move || {
        // SAFETY: `requester` and `requests` are live for the call, and the
        // slot holds the stand-in, which reads neither.
        unsafe { request(&raw mut requester, &raw mut requests) };
    }
}

/// Readies the content request's reads for their benchmark: `served`
/// contents in a store of their own and `count` requests on the heap, none
/// of them served, as in a request whose content is all the game's. The
/// closure checks the requests' memory and looks each hash up, as the
/// request does before it passes them on, and returns how many it found.
#[cfg(feature = "test")]
#[doc(hidden)]
pub fn bench_request_reads(served: u32, count: u32) -> impl FnMut() -> usize {
    let store: &'static store::Store = Box::leak(Box::new(store::Store::new()));
    for n in 0..served {
        let _kept = store.insert([4, n, n.wrapping_mul(7), MARK], &[1]);
    }
    let requests: Vec<ContentHash> = (0..count).map(|n| [4, u32::MAX - n, 0, MARK]).collect();
    move || {
        let data = requests.as_ptr().addr();
        let size = size_of::<ContentHash>();
        if !requests_read(data, requests.len(), size, crate::engine::pages::query) {
            return 0;
        }
        requests
            .iter()
            .filter(|hash| store.get(hash).is_some())
            .count()
    }
}

#[cfg(test)]
mod tests;
