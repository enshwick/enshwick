//! Disabled recipes in the running game: the rules in force, the lookup's
//! filter, the screen's question, the messages to clients and the command.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex, OnceLock};
use std::time::SystemTime;

use arc_swap::ArcSwap;
use enshwick_core::abi::{Level, RecipeRuleKind, Side, Status};
use enshwick_core::content::items::{ITEM_REGISTRY, ItemLayout};
use enshwick_core::content::recipes::{RECIPE_REGISTRY, RecipeLayout};
use enshwick_core::content::workshops::{WORKSHOP_REGISTRY, WorkshopLayout};
use enshwick_core::content::{Blob, Guid};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::engine::catalogue::{Reflected, types};
use crate::engine::fuse::Fuse;
use crate::engine::loader::Loader;
use crate::engine::mod_index::ModIndex;
use crate::engine::settings::LOADER_ID;
use crate::engine::texts;
use enshwick_base::Relock;

use super::{
    Index, Kind, Names, Rule, RuleFile, Source, TEMPLATE, WORLD_FILE, compose, describe, edit_file,
    find, parts, resolve,
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
    /// Ids of the server's message being received, up to a bound.
    incoming: crate::base::wire::Gathered<u32, MAX_SERVER_RECIPES>,
    index: Option<Arc<Index>>,
    names: Option<Arc<Names>>,
    /// The world's file, as last seen.
    stamps: (Option<PathBuf>, Stamp),
    /// The problems last logged, so the same ones are not logged again.
    problems: Vec<String>,
}

/// The disabled recipes' state in this process: the recipes disabled, the
/// lookup's lists, the rules, and the layout the start resolved.
struct Recipes {
    disabled: ArcSwap<Disabled>,
    lists: ArcSwap<Lists>,
    live: Mutex<Live>,
    /// Unset until the start resolves it, `None` on a build without it.
    layout: OnceLock<Option<RecipeLayout>>,
    /// Set when the rules or what they resolve against changed.
    dirty: AtomicBool,
}

impl Recipes {
    fn new() -> Self {
        Self {
            disabled: ArcSwap::default(),
            lists: ArcSwap::default(),
            live: Mutex::default(),
            layout: OnceLock::new(),
            dirty: AtomicBool::new(true),
        }
    }

    fn live<R>(&self, work: impl FnOnce(&mut Live) -> R) -> R {
        work(&mut self.live.relock())
    }

    fn layout(&self) -> Option<&RecipeLayout> {
        self.layout.get().and_then(Option::as_ref)
    }

    /// Notes that the rules or what they resolve against changed.
    fn changed(&self) {
        self.dirty.store(true, Ordering::Release);
    }

    /// Keeps the resolved layout, or turns disabled recipes off by their
    /// fuse.
    fn keep_layout(
        &self,
        loader: &Loader,
        resolved: Result<RecipeLayout, impl core::fmt::Display>,
    ) -> bool {
        if let Err(off) = &resolved {
            let why = format!("does not resolve: {off}");
            let _off = loader.fuse_off(&DISABLED_FUSE, super::RECIPE_LAYOUT, &why);
        }
        let _first = self.layout.set(resolved.ok());
        self.layout().is_some()
    }

    /// Mod `owner`'s rule: added, or taken back.
    fn rule(&self, owner: ModIndex, kind: Kind, name: &str, disable: bool) -> Status {
        // Mods' init runs before `init` resolves the layout: keep their rules
        // until then, and refuse only on a build known to lack it.
        if self.layout.get().is_some_and(Option::is_none) {
            return Status::MISSING;
        }
        self.live(|l| {
            l.mods
                .retain(|(o, k, n)| !(*o == owner.get() && *k == kind && n == name));
            if disable {
                l.mods.push((owner.get(), kind, name.to_owned()));
            }
        });
        self.changed();
        Status::OK
    }

    /// Forgets what came from the server a client hosted or joined; the
    /// client's own rules apply again at the next poll.
    fn forget(&self) {
        self.live(|l| {
            l.server = None;
            l.incoming.clear();
            l.problems.clear();
        });
        self.changed();
    }
}

static RECIPES: LazyLock<Recipes> = LazyLock::new(Recipes::new);
/// The switch of the disabled recipes: off where the build does not reflect
/// or read the recipe registry, and the rules mods set in init then do
/// nothing.
static DISABLED_FUSE: Fuse = Fuse::new("disabled recipes");

fn live<R>(work: impl FnOnce(&mut Live) -> R) -> R {
    RECIPES.live(work)
}

fn layout() -> Option<&'static RecipeLayout> {
    LazyLock::force(&RECIPES).layout()
}

/// Sets disabled recipes up: the layout and the lookup hook. Before arming.
pub(crate) fn init(loader: &'static Loader) {
    if !RECIPES.keep_layout(
        loader,
        RecipeLayout::resolve(&Reflected(&loader.reflection)),
    ) {
        return;
    }
    // Without the lookup, which turns itself off where players and admins
    // see it, disabled recipes stay in the crafting menus and the rest of
    // them holds.
    let _linked = crate::engine::code::link_recipe_lookup(loader);
    RECIPES.changed();
}

/// Turns disabled recipes off: the recipe registry, the game's or the one
/// mods served it, does not read with the layout the build reflects. The
/// rules in force go too, since nothing applies new ones once it is off,
/// and the crafting menus and the craft veto must not keep an old world's.
#[cold]
fn unreadable(loader: &Loader, registry: &str) {
    let _off = loader.fuse_off(
        &DISABLED_FUSE,
        registry,
        "does not read with the recipe layout",
    );
    let generation = RECIPES.disabled.load().generation + 1;
    RECIPES.disabled.store(Arc::new(Disabled {
        ids: FxHashSet::default(),
        generation,
    }));
}

/// The world's own recipe file, while a world runs.
fn world_path() -> Option<PathBuf> {
    crate::base::worlds::current().map(|key| {
        crate::base::world_folders::settings_of(&key)
            .join(LOADER_ID)
            .join(WORLD_FILE)
    })
}

fn stamp(path: &Path) -> Stamp {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// Looks at the world's recipe file and applies the rules when anything
/// changed; on the settings thread, once a second.
pub(crate) fn poll(loader: &Loader) {
    if layout().is_none() || DISABLED_FUSE.is_off() {
        return;
    }
    let world = world_path();
    let world_stamp = world.as_deref().and_then(stamp);
    let seen = (world, world_stamp);
    let moved = live(|l| {
        let moved = l.stamps != seen;
        l.stamps = seen;
        moved
    });
    if (moved | RECIPES.dirty.swap(false, Ordering::AcqRel))
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
    if crate::content::served::served_recipes() {
        // Mods change the registry: wait for the game's build of it, and
        // look again next poll, as the build may fail or its mods turn off.
        RECIPES.changed();
        return None;
    }
    let read = crate::engine::archive::game()
        .and_then(|archive| archive.resource(&RECIPE_REGISTRY.key_by_hash(types::RECIPE_REGISTRY)))
        .and_then(|registry| Index::read(layout()?, &registry))
        .map(|index| index.with_content(&content_names()));
    let Some(index) = read.map(Arc::new) else {
        unreadable(loader, "the game's recipe registry");
        return None;
    };
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
    crate::base::ids::with(|ids| {
        ids.content()
            .into_iter()
            .filter_map(|(name, _)| Some((Guid::try_named(name)?.fnv1a(), name.to_owned())))
            .collect()
    })
}

fn read_names(loader: &Loader) -> Option<Names> {
    let items = ItemLayout::resolve(&Reflected(&loader.reflection)).ok()?;
    let workshops = WorkshopLayout::resolve(&Reflected(&loader.reflection)).ok()?;
    let archive = crate::engine::archive::game()?;
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
fn read_file(path: &Path, shown: &str) -> Result<Option<RuleFile>, ReadRulesError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(RuleFile::parse(shown, &text)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(ReadRulesError::Io {
            shown: shown.to_owned(),
            source,
        }),
    }
}

/// Why a recipe file that is there does not read.
#[derive(Debug, thiserror::Error)]
enum ReadRulesError {
    /// The system refused it.
    #[error("{shown}: {source}")]
    Io {
        shown: String,
        #[source]
        source: std::io::Error,
    },
    /// It is no recipe file.
    #[error(transparent)]
    Parse(#[from] crate::content::recipes::ParseRulesError),
}

/// Why a rule was not written into a recipe file.
#[derive(Debug, thiserror::Error)]
enum WriteRuleError {
    /// The file was not read or written.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// The file's lists did not take the rule.
    #[error(transparent)]
    Edit(#[from] crate::content::recipes::EditRulesError),
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
        if let Some(path) = world_path() {
            match read_file(&path, &path.display().to_string()) {
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
            crate::engine::log::write(Level::WARN, id, &format!("recipe rule: {problem}"));
        }
    }
    let ids: FxHashSet<u32> = ids.into_iter().collect();
    let old = RECIPES.disabled.load();
    if old.ids == ids {
        return;
    }
    let generation = old.generation + 1;
    let count = ids.len();
    let mut sorted: Vec<u32> = ids.iter().copied().collect();
    sorted.sort_unstable();
    RECIPES
        .disabled
        .store(Arc::new(Disabled { ids, generation }));
    crate::log!(
        INFO,
        "disabled recipes: {count}{}",
        if ours { "" } else { ", the server's" }
    );
    if ours {
        for message in messages(&sorted) {
            crate::base::net::broadcast_control(&message);
        }
    }
}

fn messages(ids: &[u32]) -> Vec<crate::base::remote::Message> {
    parts(ids, PER_MESSAGE)
        .into_iter()
        .map(|(first, last, ids)| crate::base::remote::Message::Recipes { first, last, ids })
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
        if let Some(loader) = crate::engine::loader::loader() {
            unreadable(loader, "the served recipe registry");
        }
        return;
    };
    live(|l| l.index = Some(Arc::new(index.with_content(&content_names()))));
    RECIPES.changed();
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
    RECIPES.rule(owner, kind, name, disable)
}

/// Whether recipe `id` is disabled in this process now.
pub(crate) fn is_disabled(id: u32) -> bool {
    RECIPES.disabled.load().ids.contains(&id)
}

/// The item with game id `id` in words, by the item names, read from the
/// archive at the first call; the number alone when they do not read. Off
/// the game thread.
pub(crate) fn item_words(loader: &Loader, id: u32) -> String {
    names(loader).item_words(id)
}

/// The recipe with id `id` in words, its full id and the number, once the
/// recipe registry is read; the number alone before.
pub(crate) fn recipe_words(id: u32) -> String {
    live(|l| l.index.clone())
        .and_then(|index| {
            let recipe = index.recipes().iter().find(|r| r.id == id)?;
            Some(format!("{} ({id})", super::full_id(&recipe.name)))
        })
        .unwrap_or_else(|| id.to_string())
}

/// Disables exactly `ids`, for tests.
#[cfg(test)]
pub(crate) fn disable_for_test(ids: &[u32]) {
    RECIPES.disabled.store(Arc::new(Disabled {
        ids: ids.iter().copied().collect(),
        generation: RECIPES.disabled.load().generation + 1,
    }));
}

/// A world loaded or stopped: its file, written from the template when it
/// is missing, applies or stops applying, and a client that hosts it runs
/// its own rules.
pub(crate) fn world_changed(loader: &Loader) {
    if loader.side == Side::CLIENT {
        live(|l| l.server = None);
    }
    if layout().is_some()
        && let Some(path) = world_path().filter(|path| !path.exists())
        && let Err(e) = crate::base::worlds::write_target(&path, TEMPLATE)
    {
        crate::log!(WARN, "{} not written: {e}", path.display());
    }
    RECIPES.changed();
}

/// The client joins a server: nothing is disabled until the server says.
pub(crate) fn joining() {
    live(|l| {
        l.server = Some(Vec::new());
        l.incoming.clear();
    });
    RECIPES.changed();
}

/// The client's hosted game ended: its own rules apply again.
pub(crate) fn forget() {
    RECIPES.forget();
}

/// Disabled recipe ids a client takes from its server at most, far past the
/// game's recipes.
const MAX_SERVER_RECIPES: usize = 1 << 14;

/// A part of the server's disabled recipes, on the client.
pub(crate) fn received(first: bool, last: bool, ids: &[u32]) {
    live(|l| match l.incoming.add(first, last, ids.iter().copied()) {
        Some(ids) => l.server = Some(ids),
        None if last => crate::log!(
            WARN,
            "recipes: the server's disabled recipes are over {MAX_SERVER_RECIPES}; kept the last list"
        ),
        None => {}
    });
    if last {
        RECIPES.changed();
    }
}

/// A client said hello: it gets the disabled recipes. False when the line
/// queue dropped them.
pub(crate) fn hello(key: u64) -> bool {
    if live(|l| l.server.is_some()) {
        return true;
    }
    let mut ids: Vec<u32> = RECIPES.disabled.load().ids.iter().copied().collect();
    ids.sort_unstable();
    messages(&ids)
        .iter()
        .all(|message| crate::base::net::to_player(key, message))
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
    let recipes = &*RECIPES;
    let disabled = recipes.disabled.load();
    if disabled.ids.is_empty() {
        return;
    }
    let Some(layout) = recipes.layout() else {
        return;
    };
    // SAFETY: the caller's contract makes `out` readable for a `[u64; 2]`
    // span for the call.
    let [address, len] = unsafe { enshwick_core::raw::read_at::<[u64; 2]>(out.cast(), 0) };
    if address == 0 || len == 0 {
        return;
    }
    let key = (workshop, address, len);
    let list = recipes.cached(key, disabled.generation, || {
        // SAFETY: the caller's contract makes the non-zero `address` (checked
        // earlier) the start of `len` readable recipe addresses, each 0 or a
        // live recipe, unchanged during the call; `layout.id` is the
        // reflected offset of a recipe's `u32` id.
        unsafe { filtered(address, len, layout.id, &disabled.ids) }
    });
    if let Some(list) = list {
        // SAFETY: the caller's contract makes `out` writable for a `[u64; 2]`
        // span for the call. The list it now names stays in the state's
        // `lists` while its
        // generation is current and is retired after, freed only once the
        // world has stepped `retire::GRACE` times and `retire::MIN_AGE` has
        // passed; the game's callers read it before they return.
        unsafe {
            enshwick_core::raw::write_at(
                out.cast(),
                0,
                [list.as_ptr().expose_provenance() as u64, list.len() as u64],
            );
        };
    }
}

impl Recipes {
    /// The filtered list of `key` in disable generation `generation`, made by
    /// `make` the first time it is asked for. A list handed out stays alive
    /// until the world has stepped past the caller that got it: it stays in
    /// `lists` while its generation is current, the lists of an older
    /// generation are retired, and a fresh list is retired too, in case
    /// another thread cached its own copy of it first.
    fn cached(
        &self,
        key: ListKey,
        generation: u64,
        make: impl FnOnce() -> Option<Arc<[u64]>>,
    ) -> Option<Arc<[u64]>> {
        let lists = self.lists.load();
        if lists.generation == generation
            && let Some(list) = lists.lists.get(&key)
        {
            return list.clone();
        }
        let list = make();
        let replaced = self.lists.rcu(|old| {
            let mut lists = if old.generation == generation {
                old.lists.clone()
            } else {
                FxHashMap::default()
            };
            lists.entry(key).or_insert_with(|| list.clone());
            Lists { generation, lists }
        });
        if replaced.generation != generation {
            crate::engine::retire::retire(replaced);
        }
        if let Some(list) = &list {
            crate::engine::retire::retire(Arc::clone(list));
        }
        list
    }
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
        Some(unsafe { enshwick_core::raw::read_at(recipe, id) })
    };
    let mut kept = Vec::new();
    for index in 0..len {
        // SAFETY: the caller's contract makes `address` the start of `len`
        // readable `u64` entries, and `index` is below `len`.
        let recipe: u64 = unsafe { enshwick_core::raw::read_at(list, index * size_of::<u64>()) };
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
                    change(&index, &names, kind, name.trim(), word == "disable")
                }
                _ => texts::fill("recipes.usage", &[]),
            }
        }
        _ => texts::fill("recipes.usage", &[]),
    }
}

fn disabled_list(index: &Index, names: &Names) -> String {
    let disabled = RECIPES.disabled.load();
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
    let disabled = RECIPES.disabled.load();
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

/// Writes a rule into the running world's recipe file.
fn change(index: &Index, names: &Names, kind: Kind, name: &str, disable: bool) -> String {
    let Some(path) = world_path() else {
        return texts::fill("recipes.no_world", &[]);
    };
    let probe = Rule {
        kind,
        name: name.to_owned(),
        source: Source::Mod(String::new()),
    };
    if let Err(why) = resolve(&probe, index, names) {
        return texts::fill("recipes.unwritten", &[("why", &why.to_string())]);
    }
    let shown = path.display().to_string();
    // A file that does not read is never replaced by one with this rule alone.
    let written = enshwick_core::files::read_to_string_if_present(&path)
        .map_err(WriteRuleError::from)
        .and_then(|text| Ok(edit_file(&text.unwrap_or_default(), kind, name, disable)?))
        .and_then(|text| Ok(crate::base::worlds::write_target(&path, &text)?));
    match written {
        Ok(()) => {
            RECIPES.changed();
            texts::fill(
                if disable {
                    "recipes.disabled"
                } else {
                    "recipes.enabled"
                },
                &[("file", &shown), ("kind", kind.key()), ("name", name)],
            )
        }
        Err(why) => texts::fill("recipes.unwritten", &[("why", &why.to_string())]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_is_made_once_per_generation_and_kept_until_the_next() {
        let recipes = Recipes::new();
        let (first, next) = (1, 2);
        let key = (7, 0x1000, 3);
        let made = recipes.cached(key, first, || Some(Arc::from([1_u64, 2])));
        let again = recipes.cached(key, first, || panic!("made twice in one generation"));
        assert!(Arc::ptr_eq(made.as_ref().unwrap(), again.as_ref().unwrap()));
        let none = recipes.cached((7, 0x2000, 3), first, || None);
        assert_eq!(none, None);
        assert_eq!(
            recipes.cached((7, 0x2000, 3), first, || panic!("made twice")),
            None
        );
        let renewed = recipes.cached(key, next, || Some(Arc::from([1_u64])));
        assert_eq!(renewed.as_deref(), Some(&[1_u64][..]));
        assert_eq!(
            made.as_deref(),
            Some(&[1_u64, 2][..]),
            "the old list lives on"
        );
    }

    #[test]
    fn a_mod_s_rule_from_init_is_kept_until_the_layout_resolves_and_shows_off_without_it() {
        let recipes = Recipes::new();
        let owner = ModIndex::new(3);
        assert_eq!(
            recipes.rule(owner, Kind::Output, "a-guid", true),
            Status::OK
        );
        assert!(recipes.live(|l| l.mods.contains(&(3, Kind::Output, "a-guid".to_owned()))));
        let loader = crate::start::tests::bare_loader();
        assert!(!recipes.keep_layout(&loader, Err("no recipe layout")));
        assert!(
            DISABLED_FUSE.is_off(),
            "off by its fuse, not only in the log"
        );
        assert!(
            crate::engine::diag::parts_off().iter().any(|off| {
                off.part == DISABLED_FUSE.feature() && off.reason.contains("no recipe layout")
            }),
            "the rules kept from init show as off"
        );
        assert_eq!(
            recipes.rule(owner, Kind::Output, "a-guid", true),
            Status::MISSING,
            "a build without the layout refuses"
        );
    }

    #[test]
    fn a_mod_s_rule_with_a_blank_name_or_an_unknown_kind_is_refused() {
        let owner = ModIndex::new(3);
        assert_eq!(
            rule(owner, RecipeRuleKind::OUTPUT, "  ", true),
            Status::BAD_ARGS
        );
        assert_eq!(
            rule(owner, RecipeRuleKind(u32::MAX), "a-guid", true),
            Status::BAD_ARGS
        );
    }

    #[test]
    fn the_end_of_a_hosted_game_forgets_its_server_s_recipes_and_applies_the_client_s_own() {
        let recipes = Recipes::new();
        recipes.dirty.store(false, Ordering::Relaxed);
        recipes.live(|l| {
            l.server = Some(vec![5]);
            assert!(l.incoming.add(true, false, [6].into_iter()).is_none());
            l.problems.push("a problem of the last world".to_owned());
        });
        recipes.forget();
        recipes.live(|l| {
            assert_eq!(l.server, None);
            assert!(l.problems.is_empty(), "the next world logs its own");
            assert_eq!(
                l.incoming.add(false, true, [7].into_iter()),
                Some(vec![7]),
                "the last server's part is gone"
            );
        });
        assert!(
            recipes.dirty.load(Ordering::Acquire),
            "applied at the next poll"
        );
    }

    #[test]
    fn a_registry_that_does_not_read_turns_the_rules_in_force_off_with_the_feature() {
        disable_for_test(&[5]);
        assert!(is_disabled(5));
        let loader = crate::start::tests::bare_loader();
        unreadable(&loader, "the served recipe registry");
        assert!(DISABLED_FUSE.is_off());
        assert!(
            !is_disabled(5),
            "an off feature disables nothing, not the last world's rules"
        );
    }

    #[test]
    fn a_rule_from_the_console_needs_a_running_world() {
        assert_eq!(world_path(), None);
        let answer = change(
            &Index::default(),
            &Names::default(),
            Kind::Workshop,
            super::super::HAND,
            true,
        );
        assert_eq!(answer, texts::fill("recipes.no_world", &[]));
    }
}
