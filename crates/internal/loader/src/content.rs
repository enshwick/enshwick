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
//!
//! Which reads pass the served path, from a live check on dedicated server
//! build 26319703 (a server copy, a world loaded, no player):
//!
//! | Side | Reads |
//! | --- | --- |
//! | Server | Every type it was checked for goes through open and resolve: the balancing table, game setting presets, loot tables, skill tree, perks, buff types, item infos, recipes, workshops and templates at start; model hierarchies and joint animations on demand from world load. The server links the served path before its own code reads a resource. |
//! | Client | Links the served path once its window shows, about 1.6 s after start; a resource it resolves before that stays vanilla for the run. Which types those are is open until an in-game check. |

use core::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use enshwick::abi::{HookFlags, ResourceKey, ResourcePatchFn, Status};
use enshwick::raw::{read_at, write_at};
use patches::{Build, run_patches};
pub(crate) use patches::{rebuild, scratch, served_now};
use texts::{TEXTS, seal_texts};

use crate::chains::Next;
use crate::host::{Loader, Target};
use crate::locks::Relock;
use crate::mod_index::ModIndex;
use crate::pinned;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::resource::KEY_OF.id(),
    pinned::resource::OPEN.id(),
    pinned::resource::RESOLVE.id(),
    pinned::text::FORMAT.id(),
    pinned::text::LOOKUP.id(),
    pinned::text::LOOKUP_ENGLISH.id(),
    pinned::text::POOL_LANGUAGE.id(),
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

/// The served resources' switch.
static RESOURCES_FUSE: crate::fuse::Fuse = crate::fuse::Fuse::new("served resources");

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
    let patch = Patch {
        owner,
        apply: patch,
        user: user.expose_provenance(),
    };
    add(&mut PENDING.relock(), key, base, patch)
}

/// Adds `patch` of `key` from `base` to `pending`. A change, served from
/// its own key, chains onto the base the key's maker serves it from, and a
/// maker that comes after changes goes first, so mods change each other's
/// new resources in either load order.
fn add(pending: &mut Vec<Served>, key: ResourceKey, base: ResourceKey, patch: Patch) -> Status {
    match pending.iter_mut().find(|s| s.key == key) {
        Some(served) if served.base == key && base != key => {
            served.base = base;
            served.patches.insert(0, patch);
        }
        Some(served) if base != key && served.base != base => return Status::REFUSED,
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

/// Ends registration and links the database when anything is served.
pub(crate) fn seal_and_link(loader: &'static Loader) {
    LOADED.get_or_init(|| loader);
    crate::item_kinds::seal(&loader.reflection);
    let renames = crate::content_renames::renamed(|owner| {
        loader
            .mods
            .get(owner.get())
            .is_some_and(|m| !m.off.load(Ordering::Acquire))
    });
    let stand_ins = {
        let keys: Vec<ResourceKey> = PENDING.relock().iter().map(|s| s.key).collect();
        let recorded = crate::clones::items(&loader.mods_dir);
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
    let aliases = crate::clones::update(&loader.mods_dir, served.iter().map(|s| (s.key, s.base)));
    let standing = stand_ins.map_or(0, |(items, registry)| {
        serve_stand_ins(&mut served, items, registry)
    });
    let changed = serve_changes(
        &mut served,
        crate::data_changes::serve(loader),
        crate::data_changes::has_live(),
    );
    let rated = serve_rates(&mut served, crate::rate_links::recipe_patch(loader));
    let orphans = aliases
        .keys()
        .filter(|guid| !served.iter().any(|s| s.key.guid == **guid))
        .count();
    let recorded = aliases.len();
    let _first = RENAMES.set(renames);
    if SERVED_LIST.set(served.into_boxed_slice()).is_err()
        || ALIASES.set(aliases).is_err()
        || count + recorded + renames.len() + standing + changed + rated == 0
    {
        return;
    }
    if mods_serve_recipes() {
        crate::code::lift_workshop_limit(loader);
    }
    let none = HookFlags::NONE;
    let _linked = loader.link_feature(
        "served resources (resource.open)",
        Target::Pinned(pinned::resource::OPEN),
        &OPEN_NEXT,
        open,
        none,
    ) && loader.link_feature(
        "served resources (resource.resolve)",
        Target::Pinned(pinned::resource::RESOLVE),
        &RESOLVE_NEXT,
        resolve,
        none,
    ) && loader.link_feature(
        "served resources (resource.key_of)",
        Target::Pinned(pinned::resource::KEY_OF),
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

/// Adds the loader's patch for the recipe output rates to `served`, last on
/// the recipe registry, after every mod's and every data change, and keeps
/// the registry's base, since a changed factor rebuilds it; how many keys.
fn serve_rates(served: &mut Vec<Served>, patch: Option<ResourcePatchFn>) -> usize {
    let Some(apply) = patch else {
        return 0;
    };
    let key = recipe_registry();
    let patch = Patch {
        owner: ModIndex::LOADER,
        apply,
        user: 0,
    };
    match served.iter_mut().find(|s| s.key == key) {
        Some(entry) => {
            entry.patches.push(patch);
            entry.keeps_base.store(true, Ordering::Relaxed);
        }
        None => served.push(Served {
            key,
            base: key,
            patches: vec![patch],
            keeps_base: AtomicBool::new(true),
            state: Mutex::new(State::default()),
            rebuilding: Mutex::new(()),
        }),
    }
    1
}

/// Adds the loader's patches of changed vanilla values to `served`: first
/// on a vanilla key, so mods patch the changed values; last on a mod's new
/// resource, after the mod built it. How many keys.
fn serve_changes(
    served: &mut Vec<Served>,
    patches: Vec<(ResourceKey, ResourcePatchFn, *mut c_void)>,
    keep: bool,
) -> usize {
    let count = patches.len();
    for (key, apply, user) in patches {
        let patch = Patch {
            owner: ModIndex::LOADER,
            apply,
            user: user.expose_provenance(),
        };
        match served.iter_mut().find(|s| s.key == key) {
            Some(entry) if crate::data_changes::goes_first(&key, &entry.base) => {
                entry.patches.insert(0, patch);
                entry.keeps_base.fetch_or(keep, Ordering::Relaxed);
            }
            Some(entry) => {
                entry.patches.push(patch);
                entry.keeps_base.fetch_or(keep, Ordering::Relaxed);
            }
            None => served.push(Served {
                key,
                base: key,
                patches: vec![patch],
                keeps_base: AtomicBool::new(keep),
                state: Mutex::new(State::default()),
                rebuilding: Mutex::new(()),
            }),
        }
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

/// Cloned templates no mod that is on serves now, each with the vanilla
/// template read in its place: the game registers them from its listing, so
/// a world's placed clones of a removed mod load as the vanilla one.
pub(crate) fn orphan_templates() -> Vec<([u8; 16], [u8; 16])> {
    let served: Vec<[u8; 16]> = new_keys_of_type(crate::catalogue::types::TEMPLATE)
        .iter()
        .map(|key| key.guid)
        .collect();
    ALIASES.get().map_or_else(Vec::new, |aliases| {
        aliases
            .iter()
            .filter(|(clone, _)| !served.contains(clone))
            .map(|(clone, vanilla)| (*clone, *vanilla))
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
/// Content mods renamed, old GUID to new.
static RENAMES: OnceLock<&'static rustc_hash::FxHashMap<[u8; 16], [u8; 16]>> = OnceLock::new();

/// The key a mod renamed `key` to.
fn renamed(key: &ResourceKey) -> Option<ResourceKey> {
    let new = RENAMES.get()?.get(&key.guid)?;
    Some(ResourceKey { guid: *new, ..*key })
}

/// A served resource with its index, a renamed key and a template's
/// vanilla source, as [`route`] finds them.
type Route = (
    Option<(usize, &'static Served)>,
    Option<ResourceKey>,
    Option<ResourceKey>,
);

/// Where an open of `key` leads: the served resource of `key` or of the key
/// a mod renamed it to; else that renamed key, or the vanilla source of a
/// cloned template no mod serves, to open from the archive.
fn route(key: &ResourceKey) -> Route {
    if let Some(found) = find(key) {
        return (Some(found), None, None);
    }
    match renamed(key) {
        Some(new) => (find(&new), Some(new), None),
        None => (None, None, alias(key)),
    }
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
pub(crate) fn recipe_registry() -> ResourceKey {
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

/// Whether mods, not the loader's own changes of values alone, serve the
/// recipe registry: only they add recipes past a workshop's vanilla count.
fn mods_serve_recipes() -> bool {
    SERVED_LIST.get().is_some_and(|served| {
        serves(served, &recipe_registry(), |owner| {
            !owner.is_loader() && mod_on(owner)
        })
    })
}

/// Whether `served` holds `key` with a patch of a mod `on` names and a build
/// that has not failed.
fn serves(served: &[Served], key: &ResourceKey, on: impl Fn(ModIndex) -> bool) -> bool {
    served
        .iter()
        .any(|s| s.key == *key && s.patches.iter().any(|p| on(p.owner)) && !s.state.relock().failed)
}

/// Whether mods that are on serve `key` and its build has not failed, as
/// for a resource a mod added.
pub(crate) fn mods_serve(key: &ResourceKey) -> bool {
    SERVED_LIST
        .get()
        .is_some_and(|served| serves(served, key, mod_on))
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
        wanted.as_ref().map_or((None, None, None), route)
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
    let Some((current, current_len, applied)) = run_patches(entry, data, len, None, Build::Game)
    else {
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
    if crate::log::enabled(enshwick::abi::Level::DEBUG, crate::settings::LOADER_ID)
        && let Some(loader) = LOADED.get()
    {
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
            DEBUG,
            "served resource {:08x}:{} built by {}, {current_len} bytes",
            entry.key.type_hash,
            hex(&entry.key.guid),
            by.join(", ")
        );
    }
    Some(result)
}

fn record_item(data: u64, len: u64) {
    let (Ok(data), Ok(len)) = (usize::try_from(data), usize::try_from(len)) else {
        return;
    };
    let Some(sealed) = crate::item_kinds::ITEMS.sealed() else {
        return;
    };
    if data == 0 {
        return;
    }
    let read = |at: usize, out: &mut [u8]| {
        // SAFETY: the game resolved this `ItemInfo` to `len` resident bytes
        // at `data`, non-zero by the check before it, unchanged for the
        // call; `ItemKindLayout::item` reads only bytes within `len`.
        unsafe { crate::memory::read_bytes(data, at, out) };
    };
    if let Some((item, kind)) = sealed.item(len, read) {
        crate::item_kinds::ITEMS.record(item, kind);
    }
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

mod patches;
#[cfg(test)]
mod tests;
mod texts;

pub(crate) use texts::{Handed, add_text, hash as text_hash, language};
