//! Served resources: mods add game resources or change vanilla ones, and the
//! game reads them as if they came from its archive.
//!
//! The loader links the resource database's open, resolve and key lookup.
//! Opening a served key returns a handle with the top bit set; resolving it
//! builds the resource from its base on first use and keeps it. Several mods
//! may patch one key: their patches run in load order, each on the result of
//! the one before. Every other key and handle goes to the game unchanged.
//!
//! A mod that serves a key may ask, at init, to keep its base; it can then
//! have the key built again when values its patch reads change, such as a
//! server's join values, and the bytes that differ are written over the
//! resource the game holds. The game keeps reading the first build's
//! address, so a rebuild must come out the same length.

use crate::mod_index::ModIndex;
use core::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use enshwick::abi::{HookFlags, ResourceKey, ResourcePatchFn, Side, Status};
use enshwick::raw::{read_at, write_at};

use crate::chains::Next;
use crate::host::Loader;
use crate::locks::Relock;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    "resource.key_of",
    "resource.open",
    "resource.resolve",
    "text.lookup",
    "text.lookup_english",
    "text.pool_language",
];

/// Marks a handle the loader made.
const SERVED: u32 = 0x8000_0000;
/// The `resolve` result's third word for resident data, as the game writes it.
const RESIDENT: u64 = 0x1_0000_0000;

/// One mod's patch of a served resource.
struct Patch {
    owner: ModIndex,
    apply: ResourcePatchFn,
    user: usize,
}

/// A served resource.
struct Served {
    key: ResourceKey,
    base: ResourceKey,
    patches: Vec<Patch>,
    /// Whether a mod asked to keep the base for rebuilds.
    keeps_base: AtomicBool,
    state: Mutex<State>,
    /// Held through a rebuild, so two never write over each other.
    rebuilding: Mutex<()>,
}

#[derive(Default)]
struct State {
    /// The base's handle in the game's database, once opened.
    base_handle: u32,
    /// The built resource, once built: address and length.
    built: Option<(usize, usize)>,
    /// Set when building failed; the resource then reads as missing.
    failed: bool,
    /// A copy of the base as first built from, when a mod keeps it.
    base: Option<Box<[u8]>>,
    /// The mods whose patches the first build applied: a rebuild runs the
    /// same, since the game holds what they added, even once a mod is off.
    built_by: Vec<ModIndex>,
}

/// Mod texts while mods initialise: tag id, language, text.
static PENDING_TEXTS: Mutex<Vec<(u32, u32, String)>> = Mutex::new(Vec::new());
/// Mod texts once sealed: tag id to `(language, text)`, read without a lock.
static TEXTS: OnceLock<std::collections::HashMap<u32, Vec<(u32, &'static str)>>> = OnceLock::new();
// SAFETY: `TextFn` is the signature of the build table's `text.lookup`, and
// `seal_texts` uses this slot only as that function's next.
static TEXT_NEXT: Next<TextFn> = unsafe { Next::new() };
// SAFETY: `TextFn` is the signature of the build table's
// `text.lookup_english`, and `seal_texts` uses this slot only as that
// function's next.
static ENGLISH_NEXT: Next<TextFn> = unsafe { Next::new() };
static POOL_LANGUAGE: AtomicUsize = AtomicUsize::new(usize::MAX);
/// The language the game last asked a text in.
static LANGUAGE: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(ENGLISH);
/// The English the game falls back to.
const ENGLISH: u32 = 1;

type TextFn =
    unsafe extern "system" fn(out: *mut [u64; 3], pool: *mut u8, id: u32) -> *mut [u64; 3];

/// The language the game shows its texts in, as far as the loader saw.
pub(crate) fn language() -> enshwick::i18n::Language {
    enshwick::i18n::Language(LANGUAGE.load(Ordering::Relaxed))
}

/// Registers a mod text; refused after init.
pub(crate) fn add_text(id: u32, language: u32, text: &str) -> Status {
    if TEXTS.get().is_some() {
        return Status::TOO_LATE;
    }
    PENDING_TEXTS.relock().push((id, language, text.to_owned()));
    Status::OK
}

/// The text of `id` for `language`: its own, else any-language, else English.
fn text_for(id: u32, language: u32) -> Option<&'static str> {
    if let Some(text) = crate::notices::text(id).or_else(|| crate::ui::text(id)) {
        return Some(text);
    }
    let texts = TEXTS.get()?.get(&id)?;
    let pick = |wanted: u32| texts.iter().find(|(l, _)| *l == wanted).map(|(_, t)| *t);
    pick(language).or_else(|| pick(0)).or_else(|| pick(ENGLISH))
}

/// Answers a text lookup with `text`, which lives for the process.
///
/// # Safety
///
/// - `out` is null or the lookup's writable out record of three `u64`,
///   which nothing else touches during the call.
unsafe fn answer(out: *mut [u64; 3], text: &'static str) -> *mut [u64; 3] {
    if !out.is_null() {
        let record = [
            0,
            text.as_ptr().expose_provenance() as u64,
            text.len() as u64,
        ];
        // SAFETY: `out` is non-null by the check before it, and the caller
        // guarantees the lookup's writable out record: error 0, the UTF-8
        // text and its length.
        unsafe { write_at(out.cast(), 0, record) };
    }
    out
}

/// The mod texts' switch.
static TEXTS_FUSE: crate::fuse::Fuse = crate::fuse::Fuse::new("mod texts");
/// The served resources' switch.
static RESOURCES_FUSE: crate::fuse::Fuse = crate::fuse::Fuse::new("served resources");

unsafe extern "system" fn lookup(out: *mut [u64; 3], pool: *mut u8, id: u32) -> *mut [u64; 3] {
    let found = TEXTS_FUSE.run("the text lookup", || {
        let offset = POOL_LANGUAGE.load(Ordering::Relaxed);
        let language = if pool.is_null() || offset == usize::MAX {
            ENGLISH
        } else {
            // SAFETY: `pool` is non-null by the check before it and is the
            // lookup's text pool; its u32 current language sits at the build
            // table's `text.pool_language` offset.
            unsafe { read_at::<u32>(pool, offset) }
        };
        LANGUAGE.store(language, Ordering::Relaxed);
        text_for(id, language)
    });
    match found.flatten() {
        // SAFETY: `out` is the lookup's own out record argument, null or
        // writable for the call.
        Some(text) => unsafe { answer(out, text) },
        // SAFETY: this detour runs only as the `text.lookup` link, so the
        // slot holds the next function of that signature; the game's
        // arguments pass unchanged.
        None => unsafe { TEXT_NEXT.get()(out, pool, id) },
    }
}

unsafe extern "system" fn lookup_english(
    out: *mut [u64; 3],
    pool: *mut u8,
    id: u32,
) -> *mut [u64; 3] {
    match TEXTS_FUSE
        .run("the English text lookup", || text_for(id, ENGLISH))
        .flatten()
    {
        // SAFETY: `out` is the lookup's own out record argument, null or
        // writable for the call.
        Some(text) => unsafe { answer(out, text) },
        // SAFETY: this detour runs only as the `text.lookup_english` link, so
        // the slot holds the next function of that signature; the game's
        // arguments pass unchanged.
        None => unsafe { ENGLISH_NEXT.get()(out, pool, id) },
    }
}

/// Seals the mod texts and links the client's text lookups when there are any.
fn seal_texts(loader: &'static Loader) {
    let pending = std::mem::take(&mut *PENDING_TEXTS.relock());
    let count = pending.len();
    let mut texts: std::collections::HashMap<u32, Vec<(u32, &'static str)>> =
        std::collections::HashMap::new();
    for (id, language, text) in pending {
        let text: &'static str = Box::leak(text.into_boxed_str());
        texts.entry(id).or_default().push((language, text));
    }
    // The client always links its lookups, for the loader's notices.
    if TEXTS.set(texts).is_err() || (count == 0 && loader.side != Side::CLIENT) {
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
    let base = loader.base;
    loader.link_feature(
        "mod texts",
        Some(base + lookup_rva),
        &TEXT_NEXT,
        lookup as TextFn,
        HookFlags::NONE,
    );
    loader.link_feature(
        "mod English texts",
        Some(base + english_rva),
        &ENGLISH_NEXT,
        lookup_english as TextFn,
        HookFlags::NONE,
    );
    crate::log!(INFO, "{count} texts from mods");
}

/// Registrations while mods initialise.
static PENDING: Mutex<Vec<Served>> = Mutex::new(Vec::new());
/// The registrations once sealed, read without a lock.
static SERVED_LIST: OnceLock<Box<[Served]>> = OnceLock::new();
/// Cloned templates ever served, clone GUID to vanilla GUID: read in place of
/// a clone no mod serves now.
static ALIASES: OnceLock<rustc_hash::FxHashMap<[u8; 16], [u8; 16]>> = OnceLock::new();
// SAFETY: `OpenFn` is the signature of the build table's `resource.open`,
// and `seal_and_link` uses this slot only as that function's next.
static OPEN_NEXT: Next<OpenFn> = unsafe { Next::new() };
// SAFETY: `ResolveFn` is the signature of the build table's
// `resource.resolve`, and `seal_and_link` uses this slot only as that
// function's next.
static RESOLVE_NEXT: Next<ResolveFn> = unsafe { Next::new() };
// SAFETY: `KeyFn` is the signature of the build table's `resource.key_of`,
// and `seal_and_link` uses this slot only as that function's next.
static KEY_NEXT: Next<KeyFn> = unsafe { Next::new() };

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
    owner: ModIndex,
    key: ResourceKey,
    base: ResourceKey,
    patch: ResourcePatchFn,
    user: *mut c_void,
) -> Status {
    if SERVED_LIST.get().is_some() {
        return Status::TOO_LATE;
    }
    let mut pending = PENDING.relock();
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
            keeps_base: AtomicBool::new(false),
            state: Mutex::new(State::default()),
            rebuilding: Mutex::new(()),
        }),
    }
    Status::OK
}

/// For mod `owner`, which serves `key`: during init, keeps its base when it
/// is first built; afterwards, builds it again from that base and writes the
/// bytes that differ over the resource the game holds.
pub(crate) fn rebuild(owner: ModIndex, key: ResourceKey) -> Status {
    let serves = |s: &Served| s.key == key && s.patches.iter().any(|p| p.owner == owner);
    let Some(list) = SERVED_LIST.get() else {
        let pending = PENDING.relock();
        return match pending.iter().find(|s| serves(s)) {
            Some(entry) => {
                entry.keeps_base.store(true, Ordering::Relaxed);
                Status::OK
            }
            None => Status::BAD_ARGS,
        };
    };
    let Some(entry) = list.iter().find(|s| serves(s)) else {
        return Status::BAD_ARGS;
    };
    let _one_at_a_time = entry.rebuilding.relock();
    // The game resolves the resource meanwhile: the patches run outside the
    // state's lock, on a copy of the base.
    let (data, len, base, built_by) = {
        let state = entry.state.relock();
        let Some((data, len)) = state.built else {
            // The first build takes the values in force.
            return Status::OK;
        };
        let Some(base) = state.base.clone() else {
            return Status::MISSING;
        };
        (data, len, base, state.built_by.clone())
    };
    let Some((built, built_len, _)) = run_patches(
        entry,
        base.as_ptr().expose_provenance(),
        base.len(),
        Some(&built_by),
    ) else {
        return Status::MISSING;
    };
    if built_len != len {
        crate::log!(
            WARN,
            "served resource {:08x}:{} not rebuilt: {built_len} bytes, the game holds {len}",
            entry.key.type_hash,
            hex(&entry.key.guid)
        );
        return Status::REFUSED;
    }
    let changed = write_differences(data, built, len);
    crate::log!(
        INFO,
        "served resource {:08x}:{} rebuilt, {changed} bytes changed",
        entry.key.type_hash,
        hex(&entry.key.guid)
    );
    Status::OK
}

/// Writes the bytes of the `len` bytes at `new` that differ over those at
/// `held`, which the game reads meanwhile; how many differed.
fn write_differences(held: usize, new: usize, len: usize) -> usize {
    // SAFETY: `new` is a patch's build of `len` bytes, valid for the
    // process.
    let new =
        unsafe { core::slice::from_raw_parts(core::ptr::with_exposed_provenance::<u8>(new), len) };
    let held = core::ptr::with_exposed_provenance_mut::<u8>(held);
    let mut changed = 0;
    for (at, &byte) in new.iter().enumerate() {
        let slot = held.wrapping_add(at);
        // SAFETY: `held` is the resource the game reads, `len` bytes a patch
        // built and left writable for the process (`ResourcePatchFn`), and
        // `at` is below `len`.
        let old = unsafe { slot.read_volatile() };
        if old != byte {
            // SAFETY: `held` is the patch's build of `len` bytes, writable for
            // the process (`ResourcePatchFn`), and `at` is below `len`; one
            // byte at a time, so a read meanwhile sees each byte old or new.
            unsafe { slot.write_volatile(byte) };
            changed += 1;
        }
    }
    changed
}

/// Ends registration and links the database when anything is served.
pub(crate) fn seal_and_link(loader: &'static Loader) {
    LOADED.get_or_init(|| loader);
    crate::item_kinds::seal(&loader.reflection);
    let renames = crate::migrations::renamed(loader);
    let stand_ins = {
        let keys: Vec<ResourceKey> = PENDING.relock().iter().map(|s| s.key).collect();
        let recorded = crate::clones::items(&loader.data_dir);
        let orphans = crate::standins::orphans(&recorded, &keys, |guid| renames.contains_key(guid));
        crate::standins::served(loader, &orphans)
    };
    seal_texts(loader);
    let ids = crate::ids::with(|ids| {
        ids.content()
            .iter()
            .map(|(name, _)| *name)
            .collect::<Vec<_>>()
            .join(", ")
    });
    if !ids.is_empty() {
        crate::log!(DEBUG, "content ids: {ids}");
    }
    let mut served = std::mem::take(&mut *PENDING.relock());
    let count = served.len();
    let aliases = crate::clones::update(&loader.data_dir, served.iter().map(|s| (s.key, s.base)));
    let standing = stand_ins.map_or(0, |(items, registry)| {
        serve_stand_ins(&mut served, items, registry)
    });
    let orphans = aliases
        .keys()
        .filter(|guid| !served.iter().any(|s| s.key.guid == **guid))
        .count();
    let recorded = aliases.len();
    let _first = RENAMES.set(renames);
    if SERVED_LIST.set(served.into_boxed_slice()).is_err()
        || ALIASES.set(aliases).is_err()
        || count + recorded + renames.len() + standing == 0
    {
        return;
    }
    if served_recipes() {
        crate::code::lift_workshop_limit(loader);
    }
    let none = HookFlags::NONE;
    let _linked = loader.link_feature(
        "served resources (resource.open)",
        loader.address("resource.open"),
        &OPEN_NEXT,
        open,
        none,
    ) && loader.link_feature(
        "served resources (resource.resolve)",
        loader.address("resource.resolve"),
        &RESOLVE_NEXT,
        resolve,
        none,
    ) && loader.link_feature(
        "served resources (resource.key_of)",
        loader.address("resource.key_of"),
        &KEY_NEXT,
        key_of,
        none,
    );
    crate::log!(INFO, "{count} resources served by mods");
    if !renames.is_empty() {
        crate::log!(
            INFO,
            "{} renamed resources read under their new GUIDs",
            renames.len()
        );
    }
    if orphans > 0 {
        crate::log!(
            INFO,
            "{orphans} cloned templates no mod serves now read as their vanilla source"
        );
    }
    if standing > 0 {
        crate::log!(
            INFO,
            "{standing} items of removed mods stand in until their mods are back"
        );
    }
}

/// Adds the loader's stand-ins for the items of removed mods to `served`,
/// and the patch that lists them to the item registry; how many.
fn serve_stand_ins(
    served: &mut Vec<Served>,
    items: Vec<crate::standins::StandIn>,
    registry: crate::standins::Builder,
) -> usize {
    fn patch(builder: crate::standins::Builder) -> Patch {
        Patch {
            owner: ModIndex::LOADER,
            apply: build_stand_in,
            user: Box::into_raw(Box::new(builder)).expose_provenance(),
        }
    }
    let count = items.len();
    for (key, base, builder) in items {
        served.push(Served {
            key,
            base,
            patches: vec![patch(builder)],
            keeps_base: AtomicBool::new(false),
            state: Mutex::new(State::default()),
            rebuilding: Mutex::new(()),
        });
    }
    let key =
        enshwick::content::items::ITEM_REGISTRY.key_by_hash(crate::catalogue::types::ITEM_REGISTRY);
    match served.iter_mut().find(|s| s.key == key) {
        Some(listed) => listed.patches.push(patch(registry)),
        None => served.push(Served {
            key,
            base: key,
            patches: vec![patch(registry)],
            keeps_base: AtomicBool::new(false),
            state: Mutex::new(State::default()),
            rebuilding: Mutex::new(()),
        }),
    }
    count
}

/// The loader's own patch: a stand-in built by the builder at `user`.
unsafe extern "system" fn build_stand_in(
    base: *const u8,
    base_len: usize,
    user: *mut c_void,
    out: *mut *const u8,
    out_len: *mut usize,
) -> Status {
    // SAFETY: `user` is the builder `serve_stand_ins` leaked for it, which
    // lives for the process.
    let builder = unsafe { &*user.cast::<crate::standins::Builder>() };
    let mut copy = vec![0; base_len];
    // SAFETY: `ResourcePatchFn` hands `base_len` readable bytes at `base`,
    // unchanged for the call.
    unsafe { enshwick::raw::read_bytes_at(base, 0, &mut copy) };
    let base = copy;
    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| builder(&base)));
    let Ok(Some(bytes)) = built else {
        return Status::REFUSED;
    };
    // The game reads the resource for the life of the process.
    let bytes: &'static [u8] = Box::leak(bytes.into_boxed_slice());
    // SAFETY: `run_patches` passes `out` pointing at a local
    // `*const u8`, writable for the call.
    unsafe { out.write(bytes.as_ptr()) };
    // SAFETY: `run_patches` passes `out_len` pointing at a local `usize`,
    // writable for the call.
    unsafe { out_len.write(bytes.len()) };
    Status::OK
}

/// The served resources of type `type_hash` that are new, not a vanilla
/// one changed, whose mods are on.
pub(crate) fn new_keys_of_type(type_hash: u32) -> Vec<ResourceKey> {
    SERVED_LIST.get().map_or_else(Vec::new, |served| {
        served
            .iter()
            .filter(|s| s.key.type_hash == type_hash && s.key != s.base && owner_on(s))
            .map(|s| s.key)
            .collect()
    })
}

/// Every served resource several mods patch: its key in words and the
/// patching mods, in the order their patches run.
pub(crate) fn shared() -> Vec<(String, Vec<ModIndex>)> {
    let served = SERVED_LIST.get().map_or(&[][..], |s| s);
    served
        .iter()
        .filter_map(|entry| {
            let mut owners: Vec<ModIndex> = Vec::new();
            for patch in &entry.patches {
                if !owners.contains(&patch.owner) {
                    owners.push(patch.owner);
                }
            }
            (owners.len() >= 2).then(|| (key_name(&entry.key), owners))
        })
        .collect()
}

/// A resource key in words: the registries by name, others as type and
/// GUID.
fn key_name(key: &ResourceKey) -> String {
    use crate::catalogue::types;
    let kind = match key.type_hash {
        types::ITEM_REGISTRY => return "the item registry".to_owned(),
        types::RECIPE_REGISTRY => return "the recipe registry".to_owned(),
        types::WORKSHOP_REGISTRY => return "the workshop registry".to_owned(),
        types::UI_BUNDLE => return "the UI bundle".to_owned(),
        types::TEMPLATE_COLLECTION => "template collection",
        types::ITEM_INFO => "item",
        types::TEMPLATE => "template",
        types::UI_IMAGE => "image",
        _ => "resource",
    };
    format!("{kind} {}", enshwick::content::Guid(key.guid))
}

/// What mods serve, for the status summary.
pub(crate) fn summary() -> crate::diag::Served {
    let served = SERVED_LIST.get().map_or(&[][..], |s| s);
    let (mut built, mut failed) = (0, 0);
    for entry in served {
        let state = entry.state.relock();
        built += usize::from(state.built.is_some());
        failed += usize::from(state.failed);
    }
    crate::diag::Served {
        resources: served.len(),
        built,
        failed,
        texts: TEXTS.get().map_or(0, |t| t.values().map(Vec::len).sum()),
    }
}

/// The vanilla template to read for `key`, a cloned template no mod serves.
fn alias(key: &ResourceKey) -> Option<ResourceKey> {
    if key.type_hash != crate::catalogue::types::TEMPLATE {
        return None;
    }
    let vanilla = ALIASES.get()?.get(&key.guid)?;
    Some(ResourceKey {
        guid: *vanilla,
        ..*key
    })
}

static LOADED: OnceLock<&'static Loader> = OnceLock::new();
/// Content mods renamed, old GUID to new, from their migrations.
static RENAMES: OnceLock<&'static rustc_hash::FxHashMap<[u8; 16], [u8; 16]>> = OnceLock::new();

/// The key `key` was renamed to by a mod's migration.
fn renamed(key: &ResourceKey) -> Option<ResourceKey> {
    let new = RENAMES.get()?.get(&key.guid)?;
    Some(ResourceKey { guid: *new, ..*key })
}

/// The served resource at `key` whose mods are on, with its index.
fn find(key: &ResourceKey) -> Option<(usize, &'static Served)> {
    SERVED_LIST
        .get()?
        .iter()
        .enumerate()
        .find(|(_, s)| s.key == *key && owner_on(s))
}

/// The game's recipe registry.
fn recipe_registry() -> ResourceKey {
    use enshwick::content::recipes::RECIPE_REGISTRY;

    use crate::catalogue::types;
    RECIPE_REGISTRY.key_by_hash(types::RECIPE_REGISTRY)
}

/// Whether mods that are on serve the recipe registry and its build has
/// not failed; otherwise the game reads the vanilla one.
pub(crate) fn served_recipes() -> bool {
    SERVED_LIST
        .get()
        .is_some_and(|served| serves(served, &recipe_registry(), mod_on))
}

/// Whether `served` holds `key` with a patch of a mod `on` names and a build
/// that has not failed.
fn serves(served: &[Served], key: &ResourceKey, on: impl Fn(ModIndex) -> bool) -> bool {
    served
        .iter()
        .any(|s| s.key == *key && s.patches.iter().any(|p| on(p.owner)) && !s.state.relock().failed)
}

fn served(handle: u32) -> Option<&'static Served> {
    if handle & SERVED == 0 {
        return None;
    }
    let index = usize::try_from((handle & !SERVED).checked_sub(1)?).ok()?;
    SERVED_LIST.get()?.get(index)
}

fn mod_on(owner: ModIndex) -> bool {
    owner == ModIndex::LOADER
        || LOADED
            .get()
            .and_then(|loader| loader.mods.get(owner.get()))
            .is_some_and(|m| !m.off.load(Ordering::Acquire))
}

fn owner_on(served: &Served) -> bool {
    served.patches.iter().any(|p| mod_on(p.owner))
}

/// Copies the game's resource key at `key`.
///
/// # Safety
///
/// - `key` is non-null and readable for `size_of::<ResourceKey>()` bytes,
///   with no writer during the call.
unsafe fn read_key(key: *const ResourceKey) -> ResourceKey {
    let key = key.cast::<u8>();
    ResourceKey {
        // SAFETY: the caller guarantees a readable `ResourceKey`, whose GUID
        // sits at its `offset_of!`.
        guid: unsafe { read_at(key, core::mem::offset_of!(ResourceKey, guid)) },
        // SAFETY: the caller guarantees a readable `ResourceKey`, whose u32
        // type hash sits at its `offset_of!`.
        type_hash: unsafe { read_at(key, core::mem::offset_of!(ResourceKey, type_hash)) },
        // SAFETY: the caller guarantees a readable `ResourceKey`, whose u32
        // part sits at its `offset_of!`.
        part: unsafe { read_at(key, core::mem::offset_of!(ResourceKey, part)) },
    }
}

unsafe extern "system" fn open(
    out: *mut u32,
    database: *mut u8,
    key: *const ResourceKey,
) -> *mut u32 {
    let wanted = if key.is_null() {
        None
    } else {
        // SAFETY: `key` is non-null by the check before it and is the key the
        // game passes the open, which it owns for the call.
        Some(unsafe { read_key(key) })
    };
    let decided = RESOURCES_FUSE.run("a resource open", || {
        let mut found = wanted.as_ref().and_then(find);
        let mut new = None;
        if found.is_none() {
            new = wanted.as_ref().and_then(renamed);
            found = new.as_ref().and_then(find);
        }
        let aliased = if found.is_none() && new.is_none() {
            wanted.as_ref().and_then(alias)
        } else {
            None
        };
        (found, new, aliased)
    });
    let Some((found, new, aliased)) = decided else {
        // SAFETY: this detour runs only as the `resource.open` link, so the
        // slot holds the next function of that signature; the game's
        // arguments pass unchanged.
        return unsafe { OPEN_NEXT.get()(out, database, key) };
    };
    let Some((index, entry)) = found else {
        let opened = if let Some(new) = new {
            // SAFETY: this detour runs only as the `resource.open` link, so
            // the slot holds the next function of that signature; the game's
            // out and database pass unchanged with a key on this stack.
            unsafe { OPEN_NEXT.get()(out, database, &raw const new) }
        } else if let Some(vanilla) = aliased {
            // SAFETY: this detour runs only as the `resource.open` link, so
            // the slot holds the next function of that signature; the game's
            // out and database pass unchanged with a key on this stack.
            unsafe { OPEN_NEXT.get()(out, database, &raw const vanilla) }
        } else {
            // SAFETY: this detour runs only as the `resource.open` link, so
            // the slot holds the next function of that signature; the game's
            // arguments pass unchanged.
            unsafe { OPEN_NEXT.get()(out, database, key) }
        };
        if wanted
            .as_ref()
            .is_some_and(|key| key.type_hash == crate::catalogue::types::ITEM_INFO)
            && !out.is_null()
        {
            // SAFETY: `out` is non-null by the check before it and is the
            // open's u32 out handle, which the game's open just wrote; zero
            // when the open failed, which `opened` skips.
            crate::item_kinds::ITEMS.opened(unsafe { read_at(out.cast_const().cast(), 0) });
        }
        return opened;
    };
    let mut base_handle = 0u32;
    // SAFETY: this detour runs only as the `resource.open` link, so the slot
    // holds the next function of that signature; the game's database passes
    // unchanged, with an out handle on this stack and the served entry's
    // base key, which lives for the process.
    unsafe { OPEN_NEXT.get()(&raw mut base_handle, database, &raw const entry.base) };
    if base_handle == 0 {
        // No base in this archive: the served key stays missing.
        // SAFETY: this detour runs only as the `resource.open` link, so the
        // slot holds the next function of that signature; the game's
        // arguments pass unchanged.
        return unsafe { OPEN_NEXT.get()(out, database, key) };
    }
    entry.state.relock().base_handle = base_handle;
    let handle = u32::try_from(index + 1).unwrap_or(0) | SERVED;
    if !out.is_null() {
        // SAFETY: `out` is non-null by the check before it and is the open's
        // writable u32 out handle.
        unsafe { write_at(out.cast(), 0, handle) };
    }
    out
}

unsafe extern "system" fn resolve(
    out: *mut [u64; 3],
    database: *mut u8,
    handle: u32,
) -> *mut [u64; 3] {
    let Some(entry) = served(handle) else {
        // SAFETY: this detour runs only as the `resource.resolve` link, so
        // the slot holds the next function of that signature; the game's
        // arguments pass unchanged.
        let resolved = unsafe { RESOLVE_NEXT.get()(out, database, handle) };
        if crate::item_kinds::ITEMS.pending() && !out.is_null() {
            // SAFETY: `out` is non-null by the check before it and is the
            // resolve's out triple of three u64, which the game's resolve
            // just wrote.
            let [data, len, _] = unsafe { read_at::<[u64; 3]>(out.cast_const().cast(), 0) };
            if data != 0 && crate::item_kinds::ITEMS.take_opened(handle) {
                record_item(data, len);
            }
        }
        return resolved;
    };
    // A served handle never reaches the game's resolve, even once the
    // resources are off: it would read past the game's table.
    let result = RESOURCES_FUSE
        .run("a resource build", || build(entry, database))
        .flatten()
        .map_or([0; 3], |(data, len)| [data as u64, len as u64, RESIDENT]);
    if !out.is_null() {
        // SAFETY: `out` is non-null by the check before it and is the resolve's
        // writable out triple of three u64.
        unsafe { write_at(out.cast(), 0, result) };
    }
    out
}

/// The served resource, built from its base on first use.
fn build(entry: &Served, database: *mut u8) -> Option<(usize, usize)> {
    let mut state = entry.state.relock();
    if let Some(built) = state.built {
        return Some(built);
    }
    if state.failed || state.base_handle == 0 {
        return None;
    }
    let mut base = [0u64; 3];
    // SAFETY: `build` runs only inside the `resource.resolve` detour, so the
    // slot holds the next function of that signature; the game's database
    // passes with the base's own handle from its open and an out triple on
    // this stack.
    unsafe { RESOLVE_NEXT.get()(&raw mut base, database, state.base_handle) };
    let (data, len) = (
        usize::try_from(base[0]).ok()?,
        usize::try_from(base[1]).ok()?,
    );
    if data == 0 {
        // The base is not resident yet; the game asks again.
        return None;
    }
    let Some((current, current_len, applied)) = run_patches(entry, data, len, None) else {
        state.failed = true;
        return None;
    };
    if entry.keeps_base.load(Ordering::Relaxed) {
        let mut base = vec![0; len];
        // SAFETY: the game resolved its base to `len` resident bytes at
        // `data`, non-zero by the check before it, unchanged for the call.
        unsafe { crate::memory::read_bytes(data, 0, &mut base) };
        state.base = Some(base.into_boxed_slice());
    }
    let result = (current, current_len);
    state.built = Some(result);
    state.built_by.clone_from(&applied);
    if entry.key.type_hash == crate::catalogue::types::ITEM_INFO {
        record_item(current as u64, current_len as u64);
    }
    if entry.key == recipe_registry() {
        crate::code::recipes_served(current, current_len);
        crate::recipes::registry_built(current, current_len);
    }
    if let Some(loader) = LOADED.get() {
        let by: Vec<&str> = applied
            .iter()
            .filter_map(|&owner| {
                if owner == ModIndex::new((ModIndex::LOADER).get()) {
                    Some("the loader")
                } else {
                    loader.mods.get(owner.get()).map(|m| m.id.as_str())
                }
            })
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

/// Records the equipment slot of the item whose resolved `ItemInfo` is the
/// `len` bytes at `data`; nothing when it is not resident or too short.
fn record_item(data: u64, len: u64) {
    let (Ok(data), Ok(len)) = (usize::try_from(data), usize::try_from(len)) else {
        return;
    };
    let Some((id_at, slot_at)) = crate::item_kinds::ITEMS.offsets() else {
        return;
    };
    if data == 0 || id_at.saturating_add(4) > len || slot_at >= len {
        return;
    }
    let info = core::ptr::with_exposed_provenance::<u8>(data);
    // SAFETY: the game resolved this `ItemInfo` to `len` resident bytes at
    // `data`, non-zero by the check before it, and its u32 item id at the
    // reflected `id_at` ends within them, checked before it.
    let item: u32 = unsafe { read_at(info, id_at) };
    // SAFETY: as for the item id: the reflected `slot_at`, below `len` by the
    // check before it, holds the one-byte equipment slot `seal` checked.
    let slot: u8 = unsafe { read_at(info, slot_at) };
    crate::item_kinds::ITEMS.record(item, slot);
}

/// Runs the patches of the mods still on, or of the mods `only` names,
/// over the `len` bytes at `data`, each on the result of the one before; a
/// failed one turns its mod off and is skipped. The bytes built, their
/// length and who built them; `None` when no patch did.
fn run_patches(
    entry: &Served,
    data: usize,
    len: usize,
    only: Option<&[ModIndex]>,
) -> Option<(usize, usize, Vec<ModIndex>)> {
    let (mut current, mut current_len) = (data, len);
    let mut applied = Vec::new();
    let runs = |owner: ModIndex| only.map_or_else(|| mod_on(owner), |only| only.contains(&owner));
    for patch in entry.patches.iter().filter(|p| runs(p.owner)) {
        let (mut built, mut built_len) = (core::ptr::null::<u8>(), 0usize);
        // SAFETY: `patch.apply` is a live mod's or the loader's
        // `ResourcePatchFn`, given the `current_len` bytes so far at
        // `current`, valid for the call, the user pointer it registered and
        // out pointers to locals on this stack.
        let mut apply = || unsafe {
            (patch.apply)(
                core::ptr::with_exposed_provenance(current),
                current_len,
                core::ptr::with_exposed_provenance_mut(patch.user),
                &raw mut built,
                &raw mut built_len,
            )
        };
        let status = if patch.owner == ModIndex::LOADER {
            apply()
        } else {
            crate::profile::call(patch.owner, crate::profile::Feature::Resources, apply)
        };
        if status != Status::OK || built.is_null() {
            if patch.owner == ModIndex::LOADER {
                crate::log!(
                    WARN,
                    "a stand-in for an item of a removed mod failed to build"
                );
            } else if let Some(loader) = LOADED.get() {
                loader.turn_off(
                    patch.owner,
                    &format!("a served resource failed to build ({})", status.0),
                );
            }
            continue;
        }
        (current, current_len) = (built.expose_provenance(), built_len);
        applied.push(patch.owner);
    }
    (!applied.is_empty()).then_some((current, current_len, applied))
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
        // SAFETY: this detour runs only as the `resource.key_of` link, so the
        // slot holds the next function of that signature; the game's
        // arguments pass unchanged.
        return unsafe { KEY_NEXT.get()(out, database, handle) };
    };
    if !out.is_null() {
        // Status byte, a word, then the key padded to 32 bytes, as the game writes it.
        let mut bytes = [0u8; 0x28];
        bytes[8..24].copy_from_slice(&entry.key.guid);
        bytes[24..28].copy_from_slice(&entry.key.type_hash.to_le_bytes());
        bytes[28..32].copy_from_slice(&entry.key.part.to_le_bytes());
        // SAFETY: `out` is non-null by the check before it and is the key
        // lookup's writable out record of 0x28 bytes.
        unsafe { write_at(out, 0, bytes) };
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "system" fn add_one(
        base: *const u8,
        base_len: usize,
        _user: *mut c_void,
        out: *mut *const u8,
        out_len: *mut usize,
    ) -> Status {
        // SAFETY: `base` holds `base_len` bytes for the call.
        let mut built = unsafe { core::slice::from_raw_parts(base, base_len) }.to_vec();
        built.push(1);
        let built: &'static [u8] = Box::leak(built.into_boxed_slice());
        // SAFETY: `run_patches` passes `out` pointing at a local
        // `*const u8`, writable for the call.
        unsafe { out.write(built.as_ptr()) };
        // SAFETY: `run_patches` passes `out_len` pointing at a local
        // `usize`, writable for the call.
        unsafe { out_len.write(built.len()) };
        Status::OK
    }

    #[test]
    fn a_rebuild_runs_the_mods_of_the_first_build_although_they_are_off() {
        let key = ResourceKey {
            guid: [0; 16],
            type_hash: 1,
            part: 0,
        };
        let entry = Served {
            key,
            base: key,
            patches: [3, 4]
                .map(|owner| Patch {
                    owner: ModIndex::new(owner),
                    apply: add_one,
                    user: 0,
                })
                .into(),
            keeps_base: AtomicBool::new(true),
            state: Mutex::new(State::default()),
            rebuilding: Mutex::new(()),
        };
        let base = [7u8; 4];
        let at = base.as_ptr().expose_provenance();
        // No loader runs here, so every mod reads as off.
        assert!(run_patches(&entry, at, base.len(), None).is_none());
        let both = [ModIndex::new(3), ModIndex::new(4)];
        let (_, len, by) = run_patches(&entry, at, base.len(), Some(&both)).unwrap();
        assert_eq!((len, by), (6, both.to_vec()));
    }

    unsafe extern "system" fn no_patch(
        _: *const u8,
        _: usize,
        _: *mut c_void,
        _: *mut *const u8,
        _: *mut usize,
    ) -> Status {
        Status::OK
    }

    fn served_by(key: ResourceKey, owners: &[usize]) -> Served {
        Served {
            key,
            base: key,
            patches: owners
                .iter()
                .map(|&owner| Patch {
                    owner: ModIndex::new(owner),
                    apply: no_patch,
                    user: 0,
                })
                .collect(),
            keeps_base: AtomicBool::new(false),
            state: Mutex::new(State::default()),
            rebuilding: Mutex::new(()),
        }
    }

    #[test]
    fn a_resource_counts_as_served_only_while_a_mod_is_on_and_it_built() {
        let key = recipe_registry();
        let served = [served_by(key, &[1, 2])];
        assert!(serves(&served, &key, |owner| owner == ModIndex::new(2)));
        assert!(!serves(&served, &key, |_| false));
        served[0].state.relock().failed = true;
        assert!(!serves(&served, &key, |_| true));
        let other = ResourceKey { part: 1, ..key };
        assert!(!serves(&[served_by(other, &[1])], &key, |_| true));
    }

    #[test]
    fn a_rebuild_writes_only_the_bytes_that_differ() {
        let mut held = vec![1u8, 2, 3, 4, 5];
        let new = [1u8, 9, 3, 4, 7];
        let changed = write_differences(
            held.as_mut_ptr().expose_provenance(),
            new.as_ptr().expose_provenance(),
            new.len(),
        );
        assert_eq!(changed, 2);
        assert_eq!(held, new);
        let again = write_differences(
            held.as_mut_ptr().expose_provenance(),
            new.as_ptr().expose_provenance(),
            new.len(),
        );
        assert_eq!(again, 0);
    }
}
