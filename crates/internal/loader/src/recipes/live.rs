//! Disabled recipes in the running game: the rules in force, the lookup's
//! filter, the screen's question, the messages to clients and the command.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex, OnceLock};
use std::time::SystemTime;

use arc_swap::ArcSwap;
use enshwick::abi::{Level, RecipeRuleKind, Side, Status};
use enshwick::content::items::{ITEM_REGISTRY, ItemLayout};
use enshwick::content::recipes::{RECIPE_REGISTRY, RecipeLayout};
use enshwick::content::workshops::{WORKSHOP_REGISTRY, WorkshopLayout};
use enshwick::content::{Blob, Guid};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::catalogue::{Reflected, types};
use crate::host::Loader;
use crate::locks::Relock;
use crate::mod_index::ModIndex;
use crate::settings::LOADER_ID;
use crate::texts;

use super::{
    FILE, Index, Kind, Names, Rule, RuleFile, Source, TEMPLATE, WORLD_FILE, compose, describe,
    edit_file, find, parts, resolve,
};

/// The most ids one message to a client carries.
const PER_MESSAGE: usize = 1400;
/// The most recipes a reply lists.
const LISTED: usize = 14;

/// The recipes disabled in this process, swapped whole on each change.
#[derive(Debug, Default)]
struct Disabled {
    ids: FxHashSet<u32>,
    generation: u64,
}

/// A workshop's list the game handed out, by workshop id, address and
/// length.
type ListKey = (u32, u64, u64);

/// The lookup's filtered lists of one generation: `None` for a list with
/// nothing disabled.
#[derive(Debug, Default)]
struct Lists {
    generation: u64,
    lists: FxHashMap<ListKey, Option<Arc<[u64]>>>,
}

/// What a recipe file looked like when last seen.
type Stamp = Option<(SystemTime, u64)>;

/// The rules and what they are resolved against.
#[derive(Default)]
struct Live {
    /// Mods' rules: owner, kind, name.
    mods: Vec<(usize, Kind, String)>,
    /// On a client, the joined server's disabled recipes.
    server: Option<Vec<u32>>,
    /// Ids of the server's message being received.
    incoming: Vec<u32>,
    index: Option<Arc<Index>>,
    names: Option<Arc<Names>>,
    /// The server's file and the world's, as last seen.
    stamps: (Stamp, Option<PathBuf>, Stamp),
    /// Whether the archive's registry failed to read, logged once.
    unreadable: bool,
    /// The problems last logged, so the same ones are not logged again.
    problems: Vec<String>,
}

static DISABLED: LazyLock<ArcSwap<Disabled>> = LazyLock::new(ArcSwap::default);
static LISTS: LazyLock<ArcSwap<Lists>> = LazyLock::new(ArcSwap::default);
static LIVE: Mutex<Option<Live>> = Mutex::new(None);
static LAYOUT: OnceLock<Option<RecipeLayout>> = OnceLock::new();
/// Set when the rules or what they resolve against changed.
static DIRTY: AtomicBool = AtomicBool::new(true);

fn live<R>(work: impl FnOnce(&mut Live) -> R) -> R {
    let mut guard = LIVE.relock();
    work(guard.get_or_insert_with(Live::default))
}

fn layout() -> Option<&'static RecipeLayout> {
    LAYOUT.get().and_then(Option::as_ref)
}

/// Sets disabled recipes up: the layout, the server's recipe file when it
/// is missing, and the lookup hook. Before arming.
pub(crate) fn init(loader: &'static Loader) {
    let resolved = RecipeLayout::resolve(&Reflected(&loader.reflection));
    if let Err(off) = &resolved {
        crate::log!(WARN, "disabled recipes off: {off}");
    }
    let _first = LAYOUT.set(resolved.ok());
    if layout().is_none() {
        return;
    }
    let path = loader.configs_dir.join(FILE);
    if !path.exists()
        && let Err(e) = std::fs::write(&path, TEMPLATE)
    {
        crate::log!(WARN, "{} not written: {e}", path.display());
    }
    if !crate::code::link_recipe_lookup(loader) {
        crate::log!(
            WARN,
            "disabled recipes stay in the crafting menus: the recipe list lookup is not found"
        );
    }
    DIRTY.store(true, Ordering::Release);
}

/// The world's own recipe file, while a world runs.
fn world_path(loader: &Loader) -> Option<PathBuf> {
    crate::worlds::current().map(|key| {
        loader
            .data_dir
            .join(LOADER_ID)
            .join("worlds")
            .join(key)
            .join(WORLD_FILE)
    })
}

fn stamp(path: &Path) -> Stamp {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// Looks at the recipe files and applies the rules when anything changed;
/// on the settings thread, once a second.
pub(crate) fn poll(loader: &Loader) {
    if layout().is_none() {
        return;
    }
    let global = stamp(&loader.configs_dir.join(FILE));
    let world = world_path(loader);
    let world_stamp = world.as_deref().and_then(stamp);
    let seen = (global, world, world_stamp);
    let moved = live(|l| {
        let moved = l.stamps != seen;
        l.stamps = seen;
        moved
    });
    if (moved | DIRTY.swap(false, Ordering::AcqRel))
        && let Some(index) = ensure_index(loader)
    {
        apply(loader, &index);
    }
}

/// The index of the recipe registry: the served one once the game built
/// it, else the archive's, read once.
fn ensure_index(loader: &Loader) -> Option<Arc<Index>> {
    if let Some(index) = live(|l| l.index.clone()) {
        return Some(index);
    }
    if crate::content::served_recipes() {
        // Mods change the registry: wait for the game's build of it, and
        // look again next poll, as the build may fail or its mods turn off.
        DIRTY.store(true, Ordering::Release);
        return None;
    }
    let read = crate::archive::game()
        .and_then(|archive| archive.resource(&RECIPE_REGISTRY.key_by_hash(types::RECIPE_REGISTRY)))
        .and_then(|registry| Index::read(layout()?, &registry))
        .map(|index| index.with_content(&content_names()));
    let Some(index) = read.map(Arc::new) else {
        live(|l| {
            if !l.unreadable {
                l.unreadable = true;
                crate::log!(
                    WARN,
                    "disabled recipes off: the recipe registry does not read"
                );
            }
        });
        return None;
    };
    let _ = loader;
    live(|l| l.index = Some(Arc::clone(&index)));
    Some(index)
}

/// The names of the game's items and workshops, from its archive, read
/// once; empty when they do not read.
fn names(loader: &Loader) -> Arc<Names> {
    if let Some(names) = live(|l| l.names.clone()) {
        return names;
    }
    let names = read_names(loader).unwrap_or_else(|| {
        crate::log!(
            WARN,
            "disabled recipes: item names do not read; name items and workshops by GUID"
        );
        Names::default()
    });
    let names = Arc::new(names.with_content(&content_names()));
    live(|l| l.names = Some(Arc::clone(&names)));
    names
}

/// The mods' content ids by the game id of their GUID.
fn content_names() -> FxHashMap<u32, String> {
    crate::ids::with(|ids| {
        ids.content()
            .into_iter()
            .filter_map(|(name, _)| Some((Guid::try_named(name)?.id(), name.to_owned())))
            .collect()
    })
}

fn read_names(loader: &Loader) -> Option<Names> {
    let items = ItemLayout::resolve(&Reflected(&loader.reflection)).ok()?;
    let workshops = WorkshopLayout::resolve(&Reflected(&loader.reflection)).ok()?;
    let archive = crate::archive::game()?;
    let registry = archive.resource(&ITEM_REGISTRY.key_by_hash(types::ITEM_REGISTRY))?;
    let blob = Blob::new(&registry);
    let (refs, count) = blob.array(items.refs)?;
    let (texts, texts_count) = blob.array(items.names)?;
    if texts_count != count {
        return None;
    }
    let count = usize::try_from(count).ok()?;
    let mut named = Vec::with_capacity(count.min(registry.len() / 16));
    for i in 0..count {
        let guid: [u8; 16] = registry
            .get(refs + i * 16..refs + i * 16 + 16)?
            .try_into()
            .ok()?;
        let (text, len) = blob.array(texts + i * 8)?;
        let name = registry.get(text..text + usize::try_from(len).ok()?)?;
        named.push((String::from_utf8_lossy(name).into_owned(), Guid(guid)));
    }
    let registry = archive.resource(&WORKSHOP_REGISTRY.key_by_hash(types::WORKSHOP_REGISTRY))?;
    let blob = Blob::new(&registry);
    let (start, count) = blob.array(workshops.workshops)?;
    let stations = (0..usize::try_from(count).ok()?)
        .map(|i| start + i * workshops.entry)
        .map_while(|at| {
            Some((
                blob.read_u32(at + workshops.entry_id)?,
                blob.read_u32(at + workshops.entry_item)?,
            ))
        })
        .collect::<Vec<_>>();
    Some(Names::new(named, stations))
}

/// Reads a recipe file; a problem when it does not read.
fn read_file(path: &Path, shown: &str) -> Result<Option<RuleFile>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => RuleFile::parse(shown, &text).map(Some),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{shown}: {e}")),
    }
}

/// Resolves the rules in force and swaps the disabled recipes in.
fn apply(loader: &Loader, index: &Index) {
    let names = names(loader);
    let server = live(|l| l.server.clone()).filter(|_| loader.side == Side::CLIENT);
    let ours = server.is_none();
    let (ids, problems) = if let Some(ids) = server {
        (ids.into_iter().collect(), Vec::new())
    } else {
        let mods: Vec<Rule> = live(|l| l.mods.clone())
            .into_iter()
            .filter(|(owner, ..)| {
                loader
                    .mods
                    .get(*owner)
                    .is_some_and(|m| !m.off.load(Ordering::Acquire))
            })
            .filter_map(|(owner, kind, name)| {
                Some(Rule {
                    kind,
                    name,
                    source: Source::Mod(loader.mods.get(owner)?.id.clone()),
                })
            })
            .collect();
        let mut problems = Vec::new();
        let mut files = Vec::new();
        let global = format!("mod-configs/{FILE}");
        let world = world_path(loader);
        let world_shown = crate::worlds::current()
            .map(|key| format!("mod-data/{LOADER_ID}/worlds/{key}/{WORLD_FILE}"));
        for (path, shown) in [
            (Some(loader.configs_dir.join(FILE)), Some(global)),
            (world, world_shown),
        ] {
            let (Some(path), Some(shown)) = (path, shown) else {
                continue;
            };
            match read_file(&path, &shown) {
                Ok(Some(file)) => files.push(file),
                Ok(None) => {}
                Err(why) => problems.push(format!("{why}; its rules are skipped")),
            }
        }
        let files: Vec<&RuleFile> = files.iter().collect();
        let (ids, found) = compose(index, &names, &mods, &files);
        problems.extend(found);
        (ids, problems)
    };
    let fresh = live(|l| {
        let fresh = l.problems != problems;
        l.problems.clone_from(&problems);
        fresh
    });
    for problem in problems.iter().filter(|_| fresh) {
        crate::log!(WARN, "disabled recipes: {problem}");
        if let Some(id) = problem
            .strip_prefix("mod ")
            .and_then(|rest| rest.split_once(':'))
            .map(|(id, _)| id)
        {
            crate::log::write(Level::WARN, id, &format!("recipe rule: {problem}"));
        }
    }
    let ids: FxHashSet<u32> = ids.into_iter().collect();
    let old = DISABLED.load();
    if old.ids == ids {
        return;
    }
    let generation = old.generation + 1;
    let count = ids.len();
    let mut sorted: Vec<u32> = ids.iter().copied().collect();
    sorted.sort_unstable();
    DISABLED.store(Arc::new(Disabled { ids, generation }));
    crate::log!(
        INFO,
        "disabled recipes: {count}{}",
        if ours { "" } else { ", the server's" }
    );
    if ours {
        for message in messages(&sorted) {
            crate::net::broadcast_control(&message);
        }
    }
}

fn messages(ids: &[u32]) -> Vec<crate::remote::Message> {
    parts(ids, PER_MESSAGE)
        .into_iter()
        .map(|(first, last, ids)| crate::remote::Message::Recipes { first, last, ids })
        .collect()
}

/// Notes that the game built the recipe registry, `len` bytes at `data`,
/// which lives for the process.
pub(crate) fn registry_built(data: usize, len: usize) {
    let Some(layout) = layout() else {
        return;
    };
    // SAFETY: the one caller, the content build of the recipe registry,
    // passes the registry it served the game: `len` readable bytes at `data`
    // that live for the process and are not written again.
    let registry = unsafe { core::slice::from_raw_parts(data as *const u8, len) };
    let Some(index) = Index::read(layout, registry) else {
        crate::log!(
            WARN,
            "disabled recipes off: the served recipe registry does not read"
        );
        return;
    };
    live(|l| l.index = Some(Arc::new(index.with_content(&content_names()))));
    DIRTY.store(true, Ordering::Release);
}

/// Mod `owner`'s rule: added, or taken back.
pub(crate) fn rule(owner: ModIndex, kind: RecipeRuleKind, name: &str, disable: bool) -> Status {
    let Some(kind) = Kind::from_abi(kind) else {
        return Status::BAD_ARGS;
    };
    let name = name.trim();
    if name.is_empty() {
        return Status::BAD_ARGS;
    }
    if layout().is_none() {
        return Status::MISSING;
    }
    live(|l| {
        l.mods
            .retain(|(o, k, n)| !(*o == owner.get() && *k == kind && n == name));
        if disable {
            l.mods.push((owner.get(), kind, name.to_owned()));
        }
    });
    DIRTY.store(true, Ordering::Release);
    Status::OK
}

/// Whether recipe `id` is disabled in this process now.
pub(crate) fn is_disabled(id: u32) -> bool {
    DISABLED.load().ids.contains(&id)
}

/// Disables exactly `ids`, for tests.
#[cfg(test)]
pub(crate) fn disable_for_test(ids: &[u32]) {
    DISABLED.store(Arc::new(Disabled {
        ids: ids.iter().copied().collect(),
        generation: DISABLED.load().generation + 1,
    }));
}

/// A world loaded or stopped: its own file applies or stops applying, and
/// a client that hosts it runs its own rules.
pub(crate) fn world_changed(loader: &Loader) {
    if loader.side == Side::CLIENT {
        live(|l| l.server = None);
    }
    DIRTY.store(true, Ordering::Release);
}

/// The client joins a server: nothing is disabled until the server says.
pub(crate) fn joining() {
    live(|l| {
        l.server = Some(Vec::new());
        l.incoming.clear();
    });
    DIRTY.store(true, Ordering::Release);
}

/// The client's hosted game ended: its own rules apply again.
pub(crate) fn left() {
    live(|l| l.server = None);
    DIRTY.store(true, Ordering::Release);
}

/// A part of the server's disabled recipes, on the client.
pub(crate) fn received(first: bool, last: bool, ids: &[u32]) {
    live(|l| {
        if first {
            l.incoming.clear();
        }
        l.incoming.extend_from_slice(ids);
        if last {
            l.server = Some(std::mem::take(&mut l.incoming));
        }
    });
    if last {
        DIRTY.store(true, Ordering::Release);
    }
}

/// A client said hello: it gets the disabled recipes.
pub(crate) fn hello(key: u64) {
    if live(|l| l.server.is_some()) {
        return;
    }
    let mut ids: Vec<u32> = DISABLED.load().ids.iter().copied().collect();
    ids.sort_unstable();
    for message in messages(&ids) {
        crate::net::to_player(key, &message);
    }
}

/// The workshop list at `out`, `[address, length]` of recipe addresses the
/// game's lookup wrote for `workshop`, without the disabled recipes.
///
/// # Safety
///
/// - `out` is readable and writable for a `[u64; 2]` span `[address, length]`
///   for the call, and nothing else touches it during the call.
/// - The span's `address` is 0 or the address of `length` readable `u64`
///   recipe addresses, each 0 or a recipe of the game's live registry,
///   unchanged during the call.
pub(crate) unsafe fn filter(workshop: u32, out: *mut [u64; 2]) {
    let disabled = DISABLED.load();
    if disabled.ids.is_empty() {
        return;
    }
    let Some(layout) = layout() else {
        return;
    };
    // SAFETY: the caller's contract makes `out` readable for a `[u64; 2]`
    // span for the call.
    let [address, len] = unsafe { enshwick::raw::read_at::<[u64; 2]>(out.cast(), 0) };
    if address == 0 || len == 0 {
        return;
    }
    let key = (workshop, address, len);
    let list = cached(key, disabled.generation, || {
        // SAFETY: the caller's contract makes the non-zero `address` (checked
        // earlier) the start of `len` readable recipe addresses, each 0 or a
        // live recipe, unchanged during the call; `layout.id` is the
        // reflected offset of a recipe's `u32` id.
        unsafe { filtered(address, len, layout.id, &disabled.ids) }
    });
    if let Some(list) = list {
        // SAFETY: the caller's contract makes `out` writable for a `[u64; 2]`
        // span for the call. The list it now names stays in `LISTS` while its
        // generation is current and is retired after, freed only once the
        // world has stepped `retire::GRACE` times and `retire::MIN_AGE` has
        // passed; the game's callers read it before they return.
        unsafe {
            enshwick::raw::write_at(
                out.cast(),
                0,
                [list.as_ptr().expose_provenance() as u64, list.len() as u64],
            );
        };
    }
}

/// The filtered list of `key` in disable generation `generation`, made by
/// `make` the first time it is asked for. A list handed out stays alive until
/// the world has stepped past the caller that got it: it stays in
/// [`LISTS`] while its generation is current, the lists of an older
/// generation are retired, and a fresh list is retired too, in case another
/// thread cached its own copy of it first.
fn cached(
    key: ListKey,
    generation: u64,
    make: impl FnOnce() -> Option<Arc<[u64]>>,
) -> Option<Arc<[u64]>> {
    let lists = LISTS.load();
    if lists.generation == generation
        && let Some(list) = lists.lists.get(&key)
    {
        return list.clone();
    }
    let list = make();
    let replaced = LISTS.rcu(|old| {
        let mut lists = if old.generation == generation {
            old.lists.clone()
        } else {
            FxHashMap::default()
        };
        lists.entry(key).or_insert_with(|| list.clone());
        Lists { generation, lists }
    });
    if replaced.generation != generation {
        crate::retire::retire(replaced);
    }
    if let Some(list) = &list {
        crate::retire::retire(Arc::clone(list));
    }
    list
}

/// The recipes of the `len` at `address` that are not disabled; `None` when
/// none is disabled.
///
/// # Safety
///
/// - `address` is the start of `len` readable `u64` recipe addresses,
///   unchanged during the call.
/// - Each entry is 0 or a live recipe readable for a `u32` at `id`.
#[cold]
unsafe fn filtered(
    address: u64,
    len: u64,
    id: usize,
    disabled: &FxHashSet<u32>,
) -> Option<Arc<[u64]>> {
    let len = usize::try_from(len).ok()?;
    let list = core::ptr::with_exposed_provenance::<u8>(usize::try_from(address).ok()?);
    let recipe_id = |recipe: u64| -> Option<u32> {
        let recipe = core::ptr::with_exposed_provenance::<u8>(usize::try_from(recipe).ok()?);
        // SAFETY: the caller's contract makes each non-zero entry (the filter
        // below passes 0 by) a live recipe readable for a `u32` at `id`.
        Some(unsafe { enshwick::raw::read_at(recipe, id) })
    };
    let mut kept = Vec::new();
    for index in 0..len {
        // SAFETY: the caller's contract makes `address` the start of `len`
        // readable `u64` entries, and `index` is below `len`.
        let recipe: u64 = unsafe { enshwick::raw::read_at(list, index * size_of::<u64>()) };
        if recipe == 0 || recipe_id(recipe).is_none_or(|id| !disabled.contains(&id)) {
            kept.push(recipe);
        }
    }
    (kept.len() != len).then(|| Arc::from(kept))
}

/// `enshwick recipes ...` on the server's console.
pub(crate) fn command(loader: &Loader, args: &str) -> String {
    if layout().is_none() {
        return texts::fill("recipes.off", &[]);
    }
    let Some(index) = ensure_index(loader) else {
        return texts::fill("recipes.waiting", &[]);
    };
    let names = names(loader);
    let (first, rest) = args.split_once(' ').unwrap_or((args, ""));
    let rest = rest.trim();
    match first.to_ascii_lowercase().as_str() {
        "" => disabled_list(&index, &names),
        "find" if !rest.is_empty() => found_list(&index, &names, rest),
        word @ ("disable" | "enable") => {
            let (kind, name) = rest.split_once(' ').unwrap_or((rest, ""));
            match Kind::from_word(kind) {
                Some(kind) if !name.trim().is_empty() => {
                    change(loader, &index, &names, kind, name.trim(), word == "disable")
                }
                _ => texts::fill("recipes.usage", &[]),
            }
        }
        _ => texts::fill("recipes.usage", &[]),
    }
}

fn disabled_list(index: &Index, names: &Names) -> String {
    let disabled = DISABLED.load();
    if disabled.ids.is_empty() {
        return texts::fill("recipes.none", &[]);
    }
    let count = disabled.ids.len().to_string();
    let mut lines = vec![texts::fill("recipes.list", &[("count", &count)])];
    let mut shown: Vec<String> = index
        .recipes()
        .iter()
        .filter(|r| disabled.ids.contains(&r.id))
        .map(|r| describe(r, names))
        .collect();
    shown.sort();
    let more = shown.len().saturating_sub(LISTED);
    lines.extend(shown.into_iter().take(LISTED));
    if more > 0 {
        lines.push(texts::fill("recipes.more", &[("count", &more.to_string())]));
    }
    lines.join("\n")
}

fn found_list(index: &Index, names: &Names, text: &str) -> String {
    let found = find(index, names, text);
    if found.is_empty() {
        return texts::fill("recipes.found.none", &[("text", text)]);
    }
    let disabled = DISABLED.load();
    let mut lines = vec![texts::fill(
        "recipes.found",
        &[("count", &found.len().to_string()), ("text", text)],
    )];
    let mark = texts::fill("recipes.mark", &[]);
    lines.extend(found.iter().take(LISTED).map(|r| {
        let line = describe(r, names);
        if disabled.ids.contains(&r.id) {
            format!("{line} {mark}")
        } else {
            line
        }
    }));
    let more = found.len().saturating_sub(LISTED);
    if more > 0 {
        lines.push(texts::fill("recipes.more", &[("count", &more.to_string())]));
    }
    lines.join("\n")
}

/// Writes a rule into the running world's recipe file, or the server's
/// when no world runs.
fn change(
    loader: &Loader,
    index: &Index,
    names: &Names,
    kind: Kind,
    name: &str,
    disable: bool,
) -> String {
    let probe = Rule {
        kind,
        name: name.to_owned(),
        source: Source::Mod(String::new()),
    };
    if let Err(why) = resolve(&probe, index, names) {
        return texts::fill("recipes.unwritten", &[("why", &why)]);
    }
    let (path, shown) = match (world_path(loader), crate::worlds::current()) {
        (Some(path), Some(key)) => (
            path,
            format!("mod-data/{LOADER_ID}/worlds/{key}/{WORLD_FILE}"),
        ),
        _ => (loader.configs_dir.join(FILE), format!("mod-configs/{FILE}")),
    };
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    // Through a rename: the poll never reads a half-written file.
    let written = edit_file(&text, kind, name, disable)
        .and_then(|text| crate::worlds::write_target(&path, &text).map_err(|e| e.to_string()));
    match written {
        Ok(()) => {
            DIRTY.store(true, Ordering::Release);
            texts::fill(
                if disable {
                    "recipes.disabled"
                } else {
                    "recipes.enabled"
                },
                &[("file", &shown), ("kind", kind.key()), ("name", name)],
            )
        }
        Err(why) => texts::fill("recipes.unwritten", &[("why", &why)]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_is_made_once_per_generation_and_kept_until_the_next() {
        // Generations far from any other test's.
        let (first, next) = (u64::MAX - 10, u64::MAX - 9);
        let key = (7, 0x1000, 3);
        let made = cached(key, first, || Some(Arc::from([1_u64, 2])));
        let again = cached(key, first, || panic!("made twice in one generation"));
        assert!(Arc::ptr_eq(made.as_ref().unwrap(), again.as_ref().unwrap()));
        let none = cached((7, 0x2000, 3), first, || None);
        assert_eq!(none, None);
        assert_eq!(cached((7, 0x2000, 3), first, || panic!("made twice")), None);
        let renewed = cached(key, next, || Some(Arc::from([1_u64])));
        assert_eq!(renewed.as_deref(), Some(&[1_u64][..]));
        assert_eq!(
            made.as_deref(),
            Some(&[1_u64, 2][..]),
            "the old list lives on"
        );
    }
}
