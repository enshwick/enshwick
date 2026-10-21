//! The loader's side of the API: its state and the `HostApi` table.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use enshwick_loader_api::abi as api_frame;
use enshwick_loader_api::abi::{
    BuildInfo, Event, EventFn, Header, HookFlags, HookSpec, HostApi, Level, Manifest,
    ManifestFlags, MidHookSpec, ModHandle, ModInfo, Side, Status, Str, UpdateKind, UpdateSource,
};
use enshwick_loader_api::mods::{LOADER_FILE, Updates};
use enshwick_loader_reflect::{Fingerprint, Reflection};

use crate::builds::KnownBuild;
use crate::chains::{Chains, Link, Next};
use crate::mid::{MidLink, Mids};

/// Where the loader's own releases come from.
const LOADER_UPDATES: UpdateSource = UpdateSource::github("themaiby/enshwick", "enshwick-loader");

/// The key Enshwick's releases are signed with, the loader's among them;
/// the public key of `mods/enshwick-updater/keys/enshwick.pub`.
const LOADER_KEY: &str = "RWTjqgtfpy3lSOLC4uaUXuvvKSLQKOFf6nUnEytoiFjSzxMEhLnoAdJF";

/// A loaded mod.
#[derive(Debug)]
pub(crate) struct ModRecord {
    /// The mod id.
    pub(crate) id: String,
    /// Its version.
    pub(crate) version: String,
    /// Its DLL relative to the game's folder, with `/` separators.
    pub(crate) file: String,
    /// Where its new releases come from, checked.
    pub(crate) updates: UpdateSource,
    /// The keys its author signs releases with, checked; empty for none.
    pub(crate) signed_by: [Str; 2],
    /// Its manifest, copied out of its DLL.
    pub(crate) manifest: &'static Manifest,
    /// What its manifest tells players about it.
    pub(crate) meta: crate::manifest::Meta,
    /// Notes on its relations, such as a recommended mod that is missing.
    pub(crate) notes: Vec<String>,
    /// Set when the mod is off: refused at init or faulted later.
    pub(crate) off: AtomicBool,
    /// The settings text last handed to the mod, alive until its next call.
    pub(crate) settings: Mutex<Option<String>>,
    /// The join values the mod last reported, as its settings file holds them.
    pub(crate) synced: Mutex<Option<String>>,
    /// On a client, the joined server's join values for the mod.
    pub(crate) server_synced: Mutex<Option<String>>,
    /// The text `sync` last handed to the mod, alive until its next call.
    pub(crate) sync_out: Mutex<String>,
    /// The settings schema the mod handed the loader.
    pub(crate) schema: Mutex<Option<enshwick_loader_api::config::Schema>>,
    /// The mod's check of new settings texts: callback and pointer.
    pub(crate) check: OnceLock<(api_frame::ConfigCheckFn, usize)>,
    /// The texts the mod handed the loader, for its settings' labels.
    pub(crate) texts: Mutex<Option<enshwick_loader_api::i18n::Texts>>,
    /// Whether it is a content pack, a folder of data the loader runs.
    pub(crate) pack: bool,
}

impl ModRecord {
    /// `Name (id)`, or the id when the manifest names none.
    pub(crate) fn label(&self) -> String {
        self.meta.label(&self.id)
    }

    /// Whether the mod runs and counts in the join check.
    pub(crate) fn joins(&self) -> bool {
        !self.off.load(Ordering::Acquire) && self.manifest.flags.contains(ManifestFlags::JOIN)
    }

    /// The version the join check compares: a pack's carries the hash of its
    /// files, so other files of one version differ.
    pub(crate) fn join_version(&self) -> String {
        match self.pack.then(|| crate::packs::hash_of(&self.id)).flatten() {
            Some(hash) => format!("{}+{hash}", self.version),
            None => self.version.clone(),
        }
    }
}

/// The handle of the loader itself, which its own copy of the API uses to
/// log and read while it runs content packs.
pub(crate) const LOADER_HANDLE: ModHandle = ModHandle(0);

/// A mod's handler for one event.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Subscription {
    /// The subscribing mod's index.
    pub(crate) owner: usize,
    /// The event.
    pub(crate) event: Event,
    /// The handler, inside the mod's DLL.
    pub(crate) callback: EventFn,
    /// The mod's pointer for the handler, as an address.
    pub(crate) user: usize,
    /// The subscribing mod's reload generation; events of an older one are
    /// dropped.
    pub(crate) generation: u32,
}

/// Everything the loader knows once mods are resolved.
#[derive(Debug)]
pub(crate) struct Loader {
    /// Server or client.
    pub(crate) side: Side,
    /// The game's image base.
    pub(crate) base: u64,
    /// The game build.
    pub(crate) fingerprint: Fingerprint,
    /// The build table row.
    pub(crate) build: &'static KnownBuild,
    /// The game's reflection.
    pub(crate) reflection: Reflection<'static>,
    /// The `mods` folder.
    pub(crate) mods_dir: PathBuf,
    /// The `mod-configs` folder.
    pub(crate) configs_dir: PathBuf,
    /// The `mod-data` folder.
    pub(crate) data_dir: PathBuf,
    /// Loaded mods, in load order; handle `n` is index `n - 1`.
    pub(crate) mods: Vec<ModRecord>,
    /// Every hook chain.
    pub(crate) chains: Mutex<Chains>,
    /// Every mid-function hook.
    pub(crate) mids: Mutex<Mids>,
    /// Reads the caller frame of the running chain.
    pub(crate) caller_frame: fn() -> api_frame::CallerFrame,
    /// Subscriptions while mods initialise.
    pub(crate) subscriptions: Mutex<Vec<Subscription>>,
    /// The subscriptions once every mod is initialised, read without a lock.
    pub(crate) delivery: OnceLock<Box<[Subscription]>>,
}

static LOADER: OnceLock<Loader> = OnceLock::new();

/// Makes `loader` the one the API table answers for.
pub(crate) fn install(loader: Loader) -> Option<&'static Loader> {
    LOADER.set(loader).ok()?;
    LOADER.get()
}

/// The join values of the running join mods, by id, as their settings files
/// hold them.
pub(crate) fn join_values() -> BTreeMap<String, String> {
    let Some(loader) = loader() else {
        return BTreeMap::new();
    };
    loader
        .mods
        .iter()
        .filter(|m| m.joins())
        .filter_map(|m| Some((m.id.clone(), m.synced.lock().ok()?.clone()?)))
        .collect()
}

/// On a client joining a server: makes `values`, the server's join values by
/// mod id, the ones each join mod runs with, and raises
/// [`Event::SETTINGS_CHANGED`] for every mod whose values change, so its
/// handler reads its settings again. A mod the server sends none for runs
/// with its file's.
/// On a client, takes the joined server's new join values of mod `id`, which
/// it pushed after a change, and tells the mod.
pub(crate) fn apply_server_value(id: &str, values: &str) {
    // A hosting client's own server: its file's values are the server's.
    let Some(loader) = loader().filter(|_| !crate::world::hosting()) else {
        return;
    };
    let Some((index, record)) = loader
        .mods
        .iter()
        .enumerate()
        .find(|(_, m)| m.id == id && m.joins())
    else {
        return;
    };
    let now = Some(values.to_owned());
    if let Ok(mut held) = record.server_synced.lock() {
        if *held == now {
            return;
        }
        held.clone_from(&now);
    }
    crate::log::write(
        Level::INFO,
        &record.id,
        &format!("the server changed its join values: {values}"),
    );
    loader.emit(Event::SETTINGS_CHANGED, Some(index), core::ptr::null());
}

pub(crate) fn apply_join_values(values: &BTreeMap<String, String>) {
    let Some(loader) = loader() else {
        return;
    };
    let current: Vec<(String, Option<String>)> = loader
        .mods
        .iter()
        .map(|m| {
            let held = m.server_synced.lock().ok().and_then(|held| held.clone());
            (
                if m.joins() {
                    m.id.clone()
                } else {
                    String::new()
                },
                held,
            )
        })
        .collect();
    let changes = crate::join::changes(
        current
            .iter()
            .map(|(id, held)| (id.as_str(), held.as_deref())),
        values,
    );
    for (index, now) in changes {
        let Some(record) = loader.mods.get(index) else {
            continue;
        };
        if let Ok(mut held) = record.server_synced.lock() {
            held.clone_from(&now);
        }
        let line = match &now {
            Some(values) => format!("the server's join values apply: {values}"),
            None => "the settings file's join values apply again".to_owned(),
        };
        crate::log::write(Level::INFO, &record.id, &line);
        loader.emit(Event::SETTINGS_CHANGED, Some(index), core::ptr::null());
    }
}

/// One of the server's join mods the last refused join found missing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Missing {
    id: &'static str,
    name: &'static str,
    version: &'static str,
    file: &'static str,
    kind: UpdateKind,
    location: &'static str,
    component: &'static str,
    keys: [&'static str; 2],
}

/// The server's join mods the last refused join found missing.
static JOIN_MISSING: Mutex<Vec<Missing>> = Mutex::new(Vec::new());
/// Every string [`HostApi::join_missing`] has handed out, kept for the process.
static KEPT: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

/// `text` kept for the process; the same text is kept once.
fn keep(text: &str) -> &'static str {
    let Ok(mut kept) = KEPT.lock() else {
        return "";
    };
    if let Some(&held) = kept.iter().find(|held| **held == text) {
        return held;
    }
    let held: &'static str = Box::leak(text.into());
    kept.push(held);
    held
}

/// One of the server's join mods the join check found missing on this
/// client, as the server advertises it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Wanted {
    /// The mod id.
    pub(crate) id: String,
    /// The name players read; empty for none.
    pub(crate) name: String,
    /// The server's version.
    pub(crate) version: String,
    /// Where the server's version comes from.
    pub(crate) source: Option<Updates>,
    /// The keys its author signs releases with.
    pub(crate) keys: Vec<String>,
}

/// On a client the join check kept out: records `missing`, the server's
/// join mods this client lacks or runs in another version, and raises
/// [`Event::JOIN_MISSING`].
pub(crate) fn report_join_missing(missing: &[Wanted]) {
    let Some(loader) = loader() else {
        return;
    };
    let entries = missing
        .iter()
        .map(|wanted| {
            let Wanted {
                id,
                name,
                version,
                source,
                keys,
            } = wanted;
            let ours = loader.mods.iter().find(|m| &m.id == id);
            let file = ours.map_or("", |m| m.file.as_str());
            // SAFETY: the loader keeps every source's strings for the process.
            let own = ours.and_then(|m| unsafe { Updates::from_abi(&m.updates) });
            let (kind, location, component) = match source.clone().or(own) {
                Some(Updates::GitHub {
                    repository,
                    component,
                }) => (UpdateKind::GITHUB, keep(&repository), keep(&component)),
                Some(Updates::Url(url)) => (UpdateKind::URL, keep(&url), ""),
                None => (UpdateKind::NONE, "", ""),
            };
            let mut kept = [""; 2];
            for (slot, key) in kept.iter_mut().zip(keys) {
                *slot = keep(key);
            }
            Missing {
                id: keep(id),
                name: keep(name),
                version: keep(version),
                file: keep(file),
                kind,
                location,
                component,
                keys: kept,
            }
        })
        .collect();
    if let Ok(mut held) = JOIN_MISSING.lock() {
        *held = entries;
    }
    loader.emit(Event::JOIN_MISSING, None, core::ptr::null());
}

/// Turns off the loaded mod `id`, if there is one, with `reason`.
pub(crate) fn turn_off_mod(id: &str, reason: &str) {
    if let Some(loader) = loader()
        && let Some(index) = loader.mods.iter().position(|m| m.id == id)
    {
        loader.turn_off(index, reason);
    }
}

pub(crate) fn loader() -> Option<&'static Loader> {
    LOADER.get()
}

/// A mod's callback with the pointer it registered beside it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Registered<F> {
    /// The owning mod's index.
    pub(crate) owner: usize,
    /// The callback, inside the mod's DLL, which stays loaded.
    pub(crate) callback: F,
    /// The pointer the callback takes, as an address.
    pub(crate) user: usize,
}

impl<F> Registered<F> {
    pub(crate) fn new(owner: usize, callback: F, user: *mut core::ffi::c_void) -> Self {
        Self {
            owner,
            callback,
            user: user.expose_provenance(),
        }
    }

    /// The pointer the mod registered with the callback.
    pub(crate) fn user(&self) -> *mut core::ffi::c_void {
        core::ptr::with_exposed_provenance_mut(self.user)
    }

    /// Whether its mod still runs.
    pub(crate) fn live(&self) -> bool {
        !is_off(self.owner)
    }
}

/// Takes what mods registered during init, for sealing.
pub(crate) fn take<T>(list: &Mutex<Vec<T>>) -> Vec<T> {
    std::mem::take(
        &mut *list
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    )
}

/// Whether the mod at index `owner` is off; the loader's own links never are.
pub(crate) fn is_off(owner: usize) -> bool {
    owner != crate::chains::LOADER
        && loader().is_none_or(|loader| {
            loader
                .mods
                .get(owner)
                .is_none_or(|m| m.off.load(Ordering::Acquire))
        })
}

impl Loader {
    /// The address of the build table's symbol `name` in the running game.
    pub(crate) fn address(&self, name: &str) -> Option<u64> {
        self.build.symbol(name).map(|rva| self.base + rva)
    }

    /// The build table's constant `name` as an offset or a size.
    pub(crate) fn offset(&self, name: &str) -> Option<usize> {
        self.build
            .constant(name)
            .and_then(|value| usize::try_from(value).ok())
    }

    /// Links one of the loader's own detours on `target`, ahead of every
    /// mod's link there.
    pub(crate) fn link<F: Copy>(
        &self,
        target: u64,
        next: &'static Next<F>,
        detour: F,
        flags: HookFlags,
    ) -> Status {
        let link = Link {
            owner: crate::chains::LOADER,
            detour: Next::<F>::address_of(detour),
            next: next.slot(),
            priority: i32::MIN,
            flags,
            hop: None,
        };
        self.chains
            .lock()
            .map_or(Status::REFUSED, |mut chains| chains.add(target, link))
    }

    /// [`Loader::link`] for a feature, logging why it stays off: a target
    /// this build lacks, or a refused link. True when linked.
    pub(crate) fn link_feature<F: Copy>(
        &self,
        feature: &str,
        target: Option<u64>,
        next: &'static Next<F>,
        detour: F,
        flags: HookFlags,
    ) -> bool {
        let Some(target) = target.filter(|&t| t != 0) else {
            crate::log!(WARN, "{feature} off: not in this build");
            return false;
        };
        let status = self.link(target, next, detour, flags);
        if !status.is_ok() {
            crate::log!(WARN, "{feature} off: refused ({})", status.0);
        }
        status.is_ok()
    }

    fn record(&self, me: ModHandle) -> Option<(usize, &ModRecord)> {
        let index = me.0.checked_sub(1)?;
        self.mods.get(index).map(|record| (index, record))
    }

    /// Turns a mod off: unlinks its hooks and logs why.
    pub(crate) fn turn_off(&self, index: usize, reason: &str) {
        let Some(record) = self.mods.get(index) else {
            return;
        };
        if record.off.swap(true, Ordering::AcqRel) {
            return;
        }
        crate::log::write(Level::ERROR, &record.id, &format!("off: {reason}"));
        if record.pack {
            crate::packs::forget(&record.id);
        }
        crate::diag::turned_off(&record.id, reason);
        if let Ok(chains) = self.chains.lock() {
            chains.rewire(|owner| {
                owner != crate::chains::LOADER
                    && self
                        .mods
                        .get(owner)
                        .is_none_or(|m| m.off.load(Ordering::Acquire))
            });
        }
    }

    /// Ends subscribing: from now on events go to the subscriptions made so far.
    pub(crate) fn seal_subscriptions(&self) {
        let made = self
            .subscriptions
            .lock()
            .map(|mut s| std::mem::take(&mut *s))
            .unwrap_or_default();
        let _first = self.delivery.set(made.into_boxed_slice());
    }

    /// Whether a mod subscribed to `event`.
    pub(crate) fn subscribed(&self, event: Event) -> bool {
        let item_data = matches!(
            event,
            Event::WORLD_LOADED | Event::WORLD_SAVED | Event::SHUTDOWN
        ) && (crate::item_data::wanted() || crate::stats::wanted());
        item_data
            || self
                .delivery
                .get()
                .is_some_and(|d| d.iter().any(|s| s.event == event))
            || crate::reload::late().is_some_and(|late| late.iter().any(|s| s.event == event))
    }

    /// Raises `event` with `data` for every mod still on, or for mod `only`.
    pub(crate) fn emit(&self, event: Event, only: Option<usize>, data: *const core::ffi::c_void) {
        if only.is_none() {
            crate::item_data::world_event(self, event);
            crate::stats::world_event(self, event);
        }
        let Some(delivery) = self.delivery.get() else {
            return;
        };
        let late = crate::reload::late();
        let late = late.as_ref().map_or(&[][..], |late| late.as_slice());
        for subscription in delivery.iter().chain(late).filter(|s| {
            s.event == event
                && only.is_none_or(|only| only == s.owner)
                && self
                    .mods
                    .get(s.owner)
                    .is_some_and(|m| !m.off.load(Ordering::Acquire))
                && s.generation == crate::reload::generation(s.owner)
        }) {
            // SAFETY: the handler lives in the mod's DLL, which stays loaded;
            // the safe layer catches its panics.
            crate::profile::call(
                subscription.owner,
                crate::profile::Feature::Events,
                || unsafe {
                    (subscription.callback)(
                        event,
                        data,
                        core::ptr::with_exposed_provenance_mut(subscription.user),
                    );
                },
            );
        }
    }

    /// The mod's own build rows that match the running build.
    fn own_rows(
        &self,
        index: usize,
    ) -> impl Iterator<Item = &'static enshwick_loader_api::abi::BuildRow> + '_ {
        let manifest = self.mods.get(index).map(|record| record.manifest);
        let rows: &'static [enshwick_loader_api::abi::BuildRow] = match manifest {
            Some(m)
                if m.header
                    .covers(core::mem::offset_of!(Manifest, build_count) + 8)
                    && !m.builds.is_null() =>
            {
                // SAFETY: the manifest points at `build_count` static rows.
                unsafe { core::slice::from_raw_parts(m.builds, m.build_count) }
            }
            _ => &[],
        };
        rows.iter().filter(|row| {
            row.side == self.side
                && row.fingerprint.guid == self.fingerprint.guid()
                && row.fingerprint.age == self.fingerprint.age()
        })
    }

    fn resolve_symbol(&self, index: usize, name: &str) -> Option<u64> {
        let own = self.own_rows(index).find_map(|row| {
            // SAFETY: the row points at `symbol_count` static symbols.
            let symbols = unsafe { slice(row.symbols, row.symbol_count) };
            find(symbols, name)
        });
        own.or_else(|| self.build.symbol(name))
            .map(|rva| self.base + rva)
    }

    fn resolve_constant(&self, index: usize, name: &str) -> Option<u64> {
        let own = self.own_rows(index).find_map(|row| {
            // SAFETY: the row points at `constant_count` static constants.
            let constants = unsafe { slice(row.constants, row.constant_count) };
            find(constants, name)
        });
        own.or_else(|| self.build.constant(name))
    }
}

/// The table handed to every mod.
pub(crate) static API: HostApi = HostApi {
    header: Header::of::<HostApi>(),
    host_version: Str::new(env!("CARGO_PKG_VERSION")),
    build,
    symbol,
    system,
    field_offset,
    type_size,
    hook,
    caller_frame,
    fault,
    log,
    settings,
    publish,
    service,
    constant,
    data_dir,
    log_level,
    subscribe,
    type_hash,
    component,
    entity_template,
    player,
    players,
    serve_resource,
    add_text,
    mods,
    sync,
    code_range,
    patch_code,
    workshop_recipes,
    hosting,
    server_world,
    command,
    chat,
    set_label,
    rename_filter,
    join_missing,
    hook_at,
    inventory_open,
    item_info,
    inventory_submit,
    status,
    run_command,
    altars_open,
    altars,
    altar_zones_control,
    set_altar_zones,
    zone_check_filter,
    survivor_lookup,
    survivor_bed_check,
    summon_filter,
    summon_item,
    survivor_status,
    interaction_filter,
    skill_override,
    comfort_filter,
    experience_filter,
    altar_limit,
    raise_base_limit,
    base_zones,
    game_event,
    entities,
    destroy,
    save_world,
    world_key,
    component_write,
    emit,
    teleport,
    spawn,
    craft_filter,
    build_filter,
    net_channel,
    net_send,
    net_ready,
    notify,
    time_of_day,
    set_time_of_day,
    time_speed_control,
    set_time_speed,
    game_settings,
    set_game_settings,
    damage_filter,
    buff_types,
    buffs,
    apply_buff,
    remove_buff,
    knowledge,
    grant_knowledge,
    quests,
    weather_at,
    weather_fronts,
    set_weather,
    terrain_materials,
    terrain_edit,
    config_schema,
    summoned,
    item_data_open,
    item_data_get,
    item_data_set,
    stat_open,
    stat_info,
    stat_value,
    stat_modify,
    stat_buff,
    stats_changed,
    stat_sources,
    stat_watch,
    affix_open,
    affix_roll,
    item_lines,
    affix_info,
    ui_frame,
    ui_widget,
    ui_dialog,
    ui_hud,
    ui_markers,
    config_check,
    texts,
    content_serve,
    content_read,
    icons_ready,
    define_event,
    subscribe_event,
    send_event,
    migrate,
    declare_content,
    entity_data_open,
    replaced,
    knowledge_input_filter,
};

/// A static slice from a pointer and a count, empty for a null pointer.
///
/// # Safety
///
/// A non-null `ptr` points at `count` static values.
unsafe fn slice<T>(ptr: *const T, count: usize) -> &'static [T] {
    if ptr.is_null() || count == 0 {
        return &[];
    }
    // SAFETY: forwarded from the caller.
    unsafe { core::slice::from_raw_parts(ptr, count) }
}

/// The value of the symbol named `name`.
fn find(symbols: &[enshwick_loader_api::abi::Symbol], name: &str) -> Option<u64> {
    symbols
        .iter()
        .find_map(|symbol| (text(symbol.name) == Some(name)).then_some(symbol.rva))
}

/// Writes `value`, a struct that starts with a [`Header`], through `out`, as
/// far as the mod's header says its struct reaches.
fn put_headed<T: Copy>(out: *mut T, value: T) -> Status {
    // SAFETY: a non-null out pointer from a mod points at its own version of
    // the struct, its header saying how far it reaches.
    if unsafe { enshwick_loader_api::abi::write_headed(out, value) } {
        Status::OK
    } else {
        Status::BAD_ARGS
    }
}

/// Reads a borrowed string, or `None` when it is not UTF-8.
fn text<'a>(value: Str) -> Option<&'a str> {
    // SAFETY: every `Str` a mod passes is borrowed for the call, which outlives
    // this use.
    unsafe { value.as_str() }
}

/// Writes `value` through `out` when `out` is not null.
fn put<T>(out: *mut T, value: T) -> Status {
    if out.is_null() {
        return Status::BAD_ARGS;
    }
    // SAFETY: a non-null out pointer from a mod points at a writable `T`.
    unsafe { out.write(value) };
    Status::OK
}

extern "system" fn build(out: *mut BuildInfo) {
    entry("build", || {
        let Some(loader) = loader() else {
            return;
        };
        let _written = put_headed(
            out,
            BuildInfo {
                header: Header::of::<BuildInfo>(),
                side: loader.side,
                base: loader.base,
                fingerprint: enshwick_loader_api::abi::Fingerprint {
                    guid: loader.fingerprint.guid(),
                    age: loader.fingerprint.age(),
                },
                table: match loader.build.kind {
                    crate::builds::Kind::Exact => enshwick_loader_api::abi::BuildTable::EXACT,
                    crate::builds::Kind::Fallback => enshwick_loader_api::abi::BuildTable::PATTERNS,
                },
            },
        );
    });
}

extern "system" fn symbol(me: ModHandle, name: Str, out: *mut u64) -> Status {
    entry("symbol", || {
        let (Some(loader), Some(name)) = (loader(), text(name)) else {
            return Status::BAD_ARGS;
        };
        let Some((index, _)) = loader.record(me) else {
            return Status::BAD_ARGS;
        };
        loader
            .resolve_symbol(index, name)
            .map_or(Status::MISSING, |address| {
                crate::diag::name(address, name);
                put(out, address)
            })
    })
}

fn lookup(error: &enshwick_loader_reflect::LookupError) -> Status {
    match error {
        enshwick_loader_reflect::LookupError::Ambiguous(_) => Status::AMBIGUOUS,
        _ => Status::MISSING,
    }
}

extern "system" fn system(name: Str, out: *mut u64) -> Status {
    entry("system", || {
        let (Some(loader), Some(name)) = (loader(), text(name)) else {
            return Status::BAD_ARGS;
        };
        match loader.reflection.system(name) {
            Ok(found) if found.entry() != 0 => {
                crate::diag::name(found.entry(), name);
                put(out, found.entry())
            }
            Ok(_) => Status::MISSING,
            Err(error) => lookup(&error),
        }
    })
}

extern "system" fn field_offset(type_name: Str, field: Str, out: *mut u64) -> Status {
    entry("field_offset", || {
        let (Some(loader), Some(type_name), Some(field)) = (loader(), text(type_name), text(field))
        else {
            return Status::BAD_ARGS;
        };
        match loader.reflection.field_offset(type_name, field) {
            Ok(offset) => put(out, offset),
            Err(error) => lookup(&error),
        }
    })
}

extern "system" fn type_size(type_name: Str, out: *mut u32) -> Status {
    entry("type_size", || {
        let (Some(loader), Some(type_name)) = (loader(), text(type_name)) else {
            return Status::BAD_ARGS;
        };
        match loader.reflection.type_info(type_name) {
            Ok(info) => put(out, info.size()),
            Err(error) => lookup(&error),
        }
    })
}

extern "system" fn hook(me: ModHandle, spec: *const HookSpec) -> Status {
    entry("hook", || {
        let (loader, index, _) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        // SAFETY: a non-null spec from a mod points at a valid `HookSpec`.
        let Some(spec) = (unsafe { spec.as_ref() }) else {
            return Status::BAD_ARGS;
        };
        // SAFETY: `next` is a static inside the mod's DLL, which is never unloaded.
        let Some(next) = (unsafe { spec.next.as_ref::<'static>() }) else {
            return Status::BAD_ARGS;
        };
        // A pack's links are the API's own code in the loader, shared by
        // every pack: they stay when one pack turns off.
        let owner = if loader.mods[index].pack {
            crate::chains::LOADER
        } else {
            index
        };
        let link = Link {
            owner,
            detour: spec.detour,
            next,
            priority: spec.priority,
            flags: spec.flags,
            hop: None,
        };
        match loader.chains.lock() {
            Ok(mut chains) if crate::reload::reinitialising(index) => {
                chains.relink(spec.target, link)
            }
            Ok(mut chains) => chains.add(spec.target, link),
            Err(_) => Status::REFUSED,
        }
    })
}

extern "system" fn hook_at(me: ModHandle, spec: *const MidHookSpec) -> Status {
    entry("hook_at", || {
        let (loader, index, _) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        // SAFETY: a non-null spec from a mod points at a valid `MidHookSpec`.
        let Some(spec) = (unsafe { spec.as_ref() }) else {
            return Status::BAD_ARGS;
        };
        let link = MidLink {
            owner: index,
            handler: spec.handler,
            user: spec.user.expose_provenance(),
            priority: spec.priority,
            flags: spec.flags,
        };
        match loader.mids.lock() {
            Ok(mut mids) => mids.add(spec.address, link),
            Err(_) => Status::REFUSED,
        }
    })
}

extern "system" fn caller_frame(out: *mut api_frame::CallerFrame) {
    entry("caller_frame", || {
        let frame = loader()
            .map(|loader| (loader.caller_frame)())
            .unwrap_or_default();
        let _written = put(out, frame);
    });
}

extern "system" fn fault(me: ModHandle, reason: Str) {
    entry("fault", || {
        let Some(loader) = loader() else {
            return;
        };
        if let Some((index, _)) = loader.record(me) {
            loader.turn_off(index, text(reason).unwrap_or("a fault"));
        }
    });
}

extern "system" fn log(me: ModHandle, level: Level, message: Str) {
    entry("log", || {
        let Some(loader) = loader() else {
            return;
        };
        let id = if me == LOADER_HANDLE {
            Some(crate::settings::LOADER_ID)
        } else {
            loader.record(me).map(|(_, record)| record.id.as_str())
        };
        if let (Some(id), Some(message)) = (id, text(message)) {
            crate::log::write(level, id, message);
        }
    });
}

extern "system" fn settings(me: ModHandle, defaults: Str, out: *mut Str) -> Status {
    entry("settings", || {
        let (Some(loader), Some(defaults)) = (loader(), text(defaults)) else {
            return Status::BAD_ARGS;
        };
        let Some((_, record)) = loader.record(me) else {
            return Status::BAD_ARGS;
        };
        let path = loader.configs_dir.join(format!("{}.toml", record.id));
        let old = loader.mods_dir.join(format!("{}.toml", record.id));
        if !path.exists() && old.exists() && std::fs::rename(&old, &path).is_ok() {
            crate::log::write(Level::INFO, &record.id, "settings moved to mod-configs");
        }
        let content = std::fs::read_to_string(&path).unwrap_or_else(|_| {
            if std::fs::write(&path, defaults).is_err() {
                crate::log::write(
                    Level::WARN,
                    &record.id,
                    "could not write the default settings",
                );
            }
            defaults.to_owned()
        });
        let content = crate::worlds::effective(loader, record, &content);
        let Ok(mut slot) = record.settings.lock() else {
            return Status::REFUSED;
        };
        let kept = slot.insert(content);
        // The text stays in the record until the loader drops it after init.
        put(out, Str::new(kept))
    })
}

impl crate::services::Names for Loader {
    fn name(&self, index: usize) -> (&str, &str) {
        self.mods
            .get(index)
            .map_or(("?", "?"), |m| (m.id.as_str(), m.version.as_str()))
    }

    fn off(&self, index: usize) -> bool {
        self.mods
            .get(index)
            .is_none_or(|m| m.off.load(Ordering::Acquire))
    }
}

/// The name and layout of a service or event spec the safe layer passed.
///
/// # Safety
///
/// `name` and `layout` come from a spec valid for the call.
unsafe fn spec_parts(
    name: Str,
    layout: &api_frame::Layout,
) -> Option<(&str, crate::services::Fields)> {
    let name = text(name)?;
    // SAFETY: forwarded from the caller.
    let fields = unsafe { enshwick_loader_api::services::read_layout(layout) }?;
    Some((name, fields))
}

extern "system" fn publish(me: ModHandle, spec: *const api_frame::ServiceSpec) -> Status {
    entry("publish", || {
        let (loader, index, record) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        // SAFETY: the safe layer passes a valid spec or null.
        let Some(spec) = (unsafe { spec.as_ref() }) else {
            return Status::BAD_ARGS;
        };
        // SAFETY: the spec is valid for the call.
        let Some((name, fields)) = (unsafe { spec_parts(spec.name, &spec.layout) }) else {
            return Status::BAD_ARGS;
        };
        if spec.table.is_null() {
            return Status::BAD_ARGS;
        }
        let published = crate::services::registry().publish(
            index,
            name,
            spec.major,
            fields,
            spec.table.expose_provenance(),
            loader,
        );
        match published {
            Ok(()) => Status::OK,
            Err(reason) => {
                crate::log::write(Level::WARN, &record.id, &reason);
                Status::REFUSED
            }
        }
    })
}

extern "system" fn service(
    me: ModHandle,
    wanted: *const api_frame::ServiceSpec,
    out: *mut api_frame::ServiceFound,
) -> Status {
    entry("service", || {
        let loader = match caller(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        // SAFETY: the safe layer passes a valid spec or null.
        let Some(wanted) = (unsafe { wanted.as_ref() }) else {
            return Status::BAD_ARGS;
        };
        // SAFETY: the spec is valid for the call.
        let Some((name, fields)) = (unsafe { spec_parts(wanted.name, &wanted.layout) }) else {
            return Status::BAD_ARGS;
        };
        let found = crate::services::registry().find(name, wanted.major, &fields, loader);
        let mut answer = api_frame::ServiceFound::EMPTY;
        let status = match found {
            Ok(found) => {
                let (id, version) = crate::services::Names::name(loader, found.owner);
                answer.table = core::ptr::with_exposed_provenance(found.table);
                answer.fields = found.fields;
                answer.owner = Str::new(keep(id));
                answer.owner_version = Str::new(keep(version));
                Status::OK
            }
            Err((status, reason)) => {
                answer.reason = Str::new(keep(&reason));
                status
            }
        };
        // SAFETY: the safe layer passes a valid out pointer or null.
        match unsafe { out.as_mut() } {
            Some(slot) => {
                *slot = answer;
                status
            }
            None => Status::BAD_ARGS,
        }
    })
}

extern "system" fn define_event(
    me: ModHandle,
    spec: *const api_frame::EventSpec,
    out: *mut u32,
) -> Status {
    entry("define_event", || {
        let (_, index, record) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if crate::services::sealed() {
            return Status::TOO_LATE;
        }
        // SAFETY: the safe layer passes a valid spec or null.
        let Some(spec) = (unsafe { spec.as_ref() }) else {
            return Status::BAD_ARGS;
        };
        // SAFETY: the spec is valid for the call.
        let Some((name, fields)) = (unsafe { spec_parts(spec.name, &spec.layout) }) else {
            return Status::BAD_ARGS;
        };
        let defined = crate::services::registry().define(index, &record.id, name, fields);
        match defined {
            Ok(id) => put(out, id),
            Err(reason) => {
                crate::log::write(Level::WARN, &record.id, &reason);
                Status::REFUSED
            }
        }
    })
}

extern "system" fn subscribe_event(
    me: ModHandle,
    spec: *const api_frame::EventSpec,
    callback: api_frame::ModEventFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("subscribe_event", || {
        let index = match caller_index(me) {
            Ok((_, index)) => index,
            Err(status) => return status,
        };
        if crate::services::sealed() {
            return Status::TOO_LATE;
        }
        // SAFETY: the safe layer passes a valid spec or null.
        let Some(spec) = (unsafe { spec.as_ref() }) else {
            return Status::BAD_ARGS;
        };
        // SAFETY: the spec is valid for the call.
        let Some((name, fields)) = (unsafe { spec_parts(spec.name, &spec.layout) }) else {
            return Status::BAD_ARGS;
        };
        crate::services::registry().subscribe(
            index,
            name,
            fields,
            callback,
            user.expose_provenance(),
        );
        Status::OK
    })
}

extern "system" fn migrate(me: ModHandle, set: *const api_frame::MigrationSet) -> Status {
    entry("migrate", || {
        let (loader, index, _) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if loader.delivery.get().is_some() {
            return Status::TOO_LATE;
        }
        // SAFETY: the safe layer passes a valid set or null.
        match unsafe { set.as_ref() } {
            Some(set) => crate::migrations::register(loader, index, set),
            None => Status::BAD_ARGS,
        }
    })
}

extern "system" fn declare_content(me: ModHandle, id: Str, out: *mut [u8; 16]) -> Status {
    entry("declare_content", || {
        let (loader, index, record) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if loader.delivery.get().is_some() {
            return Status::TOO_LATE;
        }
        let Some(id) = text(id) else {
            return Status::BAD_ARGS;
        };
        match crate::ids::declare_content(&record.id, id) {
            Ok(guid) => put(out, guid.0),
            Err(why) => {
                loader.turn_off(index, &why);
                Status::REFUSED
            }
        }
    })
}

extern "system" fn send_event(me: ModHandle, event: u32, data: *const core::ffi::c_void) -> Status {
    entry("send_event", || match caller_index(me) {
        Ok((_, index)) => crate::services::send(index, event, data),
        Err(status) => status,
    })
}

extern "system" fn constant(me: ModHandle, name: Str, out: *mut u64) -> Status {
    entry("constant", || {
        let (Some(loader), Some(name)) = (loader(), text(name)) else {
            return Status::BAD_ARGS;
        };
        let Some((index, _)) = loader.record(me) else {
            return Status::BAD_ARGS;
        };
        loader
            .resolve_constant(index, name)
            .map_or(Status::MISSING, |value| put(out, value))
    })
}

extern "system" fn data_dir(me: ModHandle, out: *mut Str) -> Status {
    entry("data_dir", || {
        let Some(loader) = loader() else {
            return Status::BAD_ARGS;
        };
        let Some((_, record)) = loader.record(me) else {
            return Status::BAD_ARGS;
        };
        let dir = loader.data_dir.join(&record.id);
        if std::fs::create_dir_all(&dir).is_err() {
            return Status::REFUSED;
        }
        // The path lives for the process: the mod keeps it as long as it likes.
        let path: &'static str = Box::leak(dir.to_string_lossy().into_owned().into_boxed_str());
        put(out, Str::new(path))
    })
}

extern "system" fn log_level(me: ModHandle) -> Level {
    entry("log_level", || {
        loader()
            .and_then(|loader| loader.record(me))
            .map_or(Level::INFO, |(_, record)| crate::log::level(&record.id))
    })
}

extern "system" fn subscribe(
    me: ModHandle,
    event: Event,
    callback: EventFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("subscribe", || {
        let (loader, index, _) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if !(Event::SETTINGS_CHANGED.0..=Event::WORLD_SETTINGS.0).contains(&event.0) {
            return Status::MISSING;
        }
        let subscription = Subscription {
            owner: index,
            event,
            callback,
            user: user.expose_provenance(),
            generation: crate::reload::generation(index),
        };
        if loader.delivery.get().is_some() {
            if crate::reload::reinitialising(index) {
                crate::reload::subscribe_late(subscription);
                return Status::OK;
            }
            return Status::TOO_LATE;
        }
        let Ok(mut subscriptions) = loader.subscriptions.lock() else {
            return Status::REFUSED;
        };
        subscriptions.push(subscription);
        Status::OK
    })
}

extern "system" fn type_hash(type_name: Str, out: *mut u32) -> Status {
    entry("type_hash", || {
        let (Some(loader), Some(type_name)) = (loader(), text(type_name)) else {
            return Status::BAD_ARGS;
        };
        match loader.reflection.type_info(type_name) {
            Ok(info) => {
                crate::world::type_hash(loader, info.va()).map_or(Status::MISSING, |h| put(out, h))
            }
            Err(error) => lookup(&error),
        }
    })
}

/// The loader and the calling mod, when the mod is on.
/// What an entry point answers when the loader's own code in it panicked.
trait Fallback {
    const FALLBACK: Self;
}

impl Fallback for Status {
    const FALLBACK: Self = Self::REFUSED;
}

impl Fallback for () {
    const FALLBACK: Self = ();
}

impl Fallback for Level {
    const FALLBACK: Self = Self::INFO;
}

impl Fallback for u8 {
    const FALLBACK: Self = 0;
}

impl Fallback for u32 {
    const FALLBACK: Self = 0;
}

impl Fallback for u64 {
    const FALLBACK: Self = 0;
}

/// Runs the body of the entry point `name`, so a panic in the loader's own
/// code never unwinds into the mod that called it: the call is refused and
/// the fault logged.
fn entry<R: Fallback>(name: &'static str, body: impl FnOnce() -> R) -> R {
    crate::reload::note(name);
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(body))
        .unwrap_or_else(|_| entry_fault(name))
}

#[cold]
fn entry_fault<R: Fallback>(name: &str) -> R {
    crate::log!(ERROR, "{name}: the loader panicked; the call is refused");
    R::FALLBACK
}

/// The calling mod of an entry point, with its index and record: refused
/// with [`Status::BAD_ARGS`] for a handle the loader never gave out and with
/// [`Status::FAULTED`] for a mod that is off, the one answer every entry
/// point gives a mod that is off.
fn calling(me: ModHandle) -> Result<(&'static Loader, usize, &'static ModRecord), Status> {
    let loader = loader().ok_or(Status::BAD_ARGS)?;
    let (index, record) = loader.record(me).ok_or(Status::BAD_ARGS)?;
    if record.off.load(Ordering::Acquire) {
        return Err(Status::FAULTED);
    }
    Ok((loader, index, record))
}

/// [`calling`], for an entry point that needs only the loader; the
/// loader's own handle passes too.
fn caller(me: ModHandle) -> Result<&'static Loader, Status> {
    if me == LOADER_HANDLE {
        return loader().ok_or(Status::BAD_ARGS);
    }
    calling(me).map(|(loader, _, _)| loader)
}

/// [`calling`], for an entry point that needs the loader and the index.
fn caller_index(me: ModHandle) -> Result<(&'static Loader, usize), Status> {
    calling(me).map(|(loader, index, _)| (loader, index))
}

extern "system" fn component(
    me: ModHandle,
    entity: u32,
    type_hash: u32,
    out: *mut *const core::ffi::c_void,
) -> Status {
    entry("component", || {
        match caller(me).and_then(|loader| crate::world::component(loader, entity, type_hash)) {
            Ok(data) => put(out, data),
            Err(status) => status,
        }
    })
}

extern "system" fn entity_template(me: ModHandle, entity: u32, out: *mut [u8; 16]) -> Status {
    entry("entity_template", || {
        match caller(me).and_then(|loader| crate::world::template(loader, entity)) {
            Ok(guid) => put(out, guid),
            Err(status) => status,
        }
    })
}

extern "system" fn player(me: ModHandle, entity: u32, out: *mut api_frame::PlayerInfo) -> Status {
    entry("player", || {
        let found = caller(me)
            .and_then(crate::world::players)
            .and_then(|players| {
                players
                    .into_iter()
                    .find(|p| p.entity == entity)
                    .ok_or(Status::MISSING)
            });
        match found {
            Ok(info) => put(out, info),
            Err(status) => status,
        }
    })
}

extern "system" fn players(
    me: ModHandle,
    out: *mut api_frame::PlayerInfo,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("players", || {
        let players = match caller(me).and_then(crate::world::players) {
            Ok(players) => players,
            Err(status) => return status,
        };
        let total = u32::try_from(players.len()).unwrap_or(u32::MAX);
        if !out.is_null() {
            for (index, info) in players.into_iter().take(capacity as usize).enumerate() {
                // SAFETY: the caller passes room for `capacity` entries.
                let entry = unsafe { out.add(index) };
                // SAFETY: as above.
                unsafe { entry.write(info) };
            }
        }
        put(count, total)
    })
}

extern "system" fn serve_resource(
    me: ModHandle,
    key: *const api_frame::ResourceKey,
    base: *const api_frame::ResourceKey,
    patch: api_frame::ResourcePatchFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("serve_resource", || {
        let (_, index, _) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        // SAFETY: non-null keys from a mod point at valid keys for the call.
        let (Some(key), Some(base)) = (unsafe { key.as_ref() }, unsafe { base.as_ref() }) else {
            return Status::BAD_ARGS;
        };
        crate::content::register(index, *key, *base, patch, user)
    })
}

extern "system" fn add_text(me: ModHandle, id: u32, language: u32, text: Str) -> Status {
    entry("add_text", || {
        let Some(text) = self::text(text) else {
            return Status::BAD_ARGS;
        };
        match caller(me) {
            Ok(_) => crate::content::add_text(id, language, text),
            Err(status) => status,
        }
    })
}

extern "system" fn code_range(start: *mut u64, len: *mut u64) -> Status {
    entry("code_range", || {
        let Some((address, size)) = loader().and_then(|loader| crate::code::text(loader.base))
        else {
            return Status::MISSING;
        };
        let status = put(start, address as u64);
        if !status.is_ok() {
            return status;
        }
        put(len, size as u64)
    })
}

extern "system" fn patch_code(
    me: ModHandle,
    address: u64,
    expected: *const u8,
    replacement: *const u8,
    len: usize,
) -> Status {
    entry("patch_code", || {
        let (loader, index) = match caller_index(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        let (Ok(address), Some(text)) = (usize::try_from(address), crate::code::text(loader.base))
        else {
            return Status::BAD_ARGS;
        };
        // SAFETY: the mod passes `len` readable bytes behind each pointer,
        // copied before the call returns.
        let expected = unsafe { slice(expected, len) };
        // SAFETY: as above.
        let replacement = unsafe { slice(replacement, len) };
        crate::code::register(
            crate::code::CodePatch {
                owner: index,
                address,
                expected: expected.into(),
                replacement: replacement.into(),
                needs: Box::default(),
            },
            text,
        )
    })
}

extern "system" fn workshop_recipes() -> u32 {
    entry("workshop_recipes", crate::code::workshop_recipes)
}

extern "system" fn inventory_open(me: ModHandle) -> Status {
    entry("inventory_open", || match caller(me) {
        Ok(loader) => crate::inventory::open(loader),
        Err(status) => status,
    })
}

extern "system" fn item_info(me: ModHandle, item: u32, out: *mut api_frame::ItemData) -> Status {
    entry("item_info", || {
        match caller(me).and_then(|loader| crate::inventory::item_info(loader, item)) {
            Ok(data) => put_headed(out, data),
            Err(status) => status,
        }
    })
}

extern "system" fn inventory_submit(
    me: ModHandle,
    request: *const api_frame::InventoryRequest,
    done: api_frame::InventoryResultFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("inventory_submit", || {
        let (loader, index) = match caller_index(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        let required = core::mem::offset_of!(api_frame::InventoryRequest, op_count) + 4;
        // SAFETY: a non-null request from a mod points at a valid request for the
        // call; an all-zero request is valid.
        let Some(request) = (unsafe { enshwick_loader_api::abi::read_headed(request, required) })
        else {
            return Status::BAD_ARGS;
        };
        crate::inventory::submit(loader, index, &request, done, user)
    })
}

/// The listing entry of the loader itself.
fn loader_info() -> ModInfo {
    ModInfo {
        header: Header::of::<ModInfo>(),
        id: Str::new(crate::settings::LOADER_ID),
        version: Str::new(env!("CARGO_PKG_VERSION")),
        side: Side::BOTH,
        flags: ManifestFlags::NONE,
        off: 0,
        file: Str::new(LOADER_FILE),
        updates: LOADER_UPDATES,
        signed_by: [Str::new(LOADER_KEY), Str::EMPTY],
        name: Str::new("Enshwick"),
        description: Str::new("The mod loader and modding API for Enshrouded"),
        authors: LOADER_AUTHORS.as_ptr(),
        author_count: LOADER_AUTHORS.len(),
        license: Str::new(env!("CARGO_PKG_LICENSE")),
        homepage: Str::new(env!("CARGO_PKG_REPOSITORY")),
        replaced_by: Str::EMPTY,
    }
}

/// The loader's authors, as its listing names them.
static LOADER_AUTHORS: [Str; 1] = [Str::new("themaiby")];

/// The listing entry of a loaded mod; its strings live as long as the loader.
fn mod_info(record: &'static ModRecord) -> ModInfo {
    ModInfo {
        header: Header::of::<ModInfo>(),
        id: Str::new(&record.id),
        version: Str::new(&record.version),
        side: record.manifest.side,
        flags: record.manifest.flags,
        off: u8::from(record.off.load(Ordering::Acquire)),
        file: Str::new(&record.file),
        updates: record.updates,
        signed_by: record.signed_by,
        name: record.manifest.name,
        description: record.manifest.description,
        authors: record.manifest.authors,
        author_count: record.manifest.author_count,
        license: record.manifest.license,
        homepage: record.manifest.homepage,
        replaced_by: Str::EMPTY,
    }
}

extern "system" fn mods(
    me: ModHandle,
    out: *mut ModInfo,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("mods", || {
        let Some(loader) = loader() else {
            return Status::BAD_ARGS;
        };
        if loader.record(me).is_none() {
            return Status::BAD_ARGS;
        }
        let infos: Vec<ModInfo> = core::iter::once(loader_info())
            .chain(loader.mods.iter().map(mod_info))
            .collect();
        // SAFETY: forwarded from the mod's call.
        unsafe { list(&infos, out, entry_size, capacity, count) }
    })
}

extern "system" fn join_missing(
    me: ModHandle,
    out: *mut ModInfo,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("join_missing", || {
        let Some(loader) = loader() else {
            return Status::BAD_ARGS;
        };
        if loader.record(me).is_none() {
            return Status::BAD_ARGS;
        }
        let held = JOIN_MISSING
            .lock()
            .map(|held| held.clone())
            .unwrap_or_default();
        let infos: Vec<ModInfo> = held
            .iter()
            .map(|missing| ModInfo {
                header: Header::of::<ModInfo>(),
                id: Str::new(missing.id),
                version: Str::new(missing.version),
                side: Side::BOTH,
                flags: ManifestFlags::JOIN,
                off: 1,
                file: Str::new(missing.file),
                updates: UpdateSource {
                    kind: missing.kind,
                    location: Str::new(missing.location),
                    component: Str::new(missing.component),
                    ..UpdateSource::NONE
                },
                signed_by: missing.keys.map(Str::new),
                name: Str::new(missing.name),
                ..ModInfo::EMPTY
            })
            .collect();
        // SAFETY: forwarded from the mod's call.
        unsafe { list(&infos, out, entry_size, capacity, count) }
    })
}

extern "system" fn replaced(
    me: ModHandle,
    out: *mut ModInfo,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    /// The listing entries, built once: the mods replaced at start never
    /// change.
    static INFOS: OnceLock<Vec<ModInfo>> = OnceLock::new();
    entry("replaced", || {
        let Some(loader) = loader() else {
            return Status::BAD_ARGS;
        };
        if loader.record(me).is_none() {
            return Status::BAD_ARGS;
        }
        let infos = INFOS.get_or_init(|| {
            let text = |s: String| Str::new(Box::leak(s.into_boxed_str()));
            crate::modlist::replaced()
                .iter()
                .map(|old| ModInfo {
                    header: Header::of::<ModInfo>(),
                    id: Str::new(&old.id),
                    version: Str::new(&old.version),
                    off: 1,
                    file: text(format!("mods/{}", old.file)),
                    replaced_by: Str::new(&old.by),
                    ..ModInfo::EMPTY
                })
                .collect()
        });
        // SAFETY: forwarded from the mod's call.
        unsafe { list(infos, out, entry_size, capacity, count) }
    })
}

/// Copies up to `capacity` of `infos` to `out`, `entry_size` bytes apart, and
/// writes how many there are.
///
/// # Safety
///
/// A non-null `out` has room for `capacity` entries of `entry_size` bytes.
unsafe fn list<T: Copy>(
    infos: &[T],
    out: *mut T,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    if !out.is_null() {
        let stride = entry_size as usize;
        if stride < size_of::<Header>() {
            return Status::BAD_ARGS;
        }
        let copied = stride.min(size_of::<T>());
        for (index, info) in infos.iter().take(capacity as usize).enumerate() {
            // SAFETY: the caller passes room for `capacity` entries of
            // `entry_size` bytes each.
            let entry = unsafe { out.cast::<u8>().add(index * stride) };
            // SAFETY: as above; `copied` bytes fit both the entry and `info`.
            unsafe {
                core::ptr::copy_nonoverlapping((&raw const *info).cast::<u8>(), entry, copied);
            };
        }
    }
    put(count, u32::try_from(infos.len()).unwrap_or(u32::MAX))
}

extern "system" fn config_schema(me: ModHandle, schema: Str) -> Status {
    entry("config_schema", || {
        let (Some(loader), Some(schema)) = (loader(), text(schema)) else {
            return Status::BAD_ARGS;
        };
        let Some((_, record)) = loader.record(me) else {
            return Status::BAD_ARGS;
        };
        let Ok(schema) = enshwick_loader_api::config::Schema::parse(schema) else {
            return Status::BAD_ARGS;
        };
        let Ok(mut slot) = record.schema.lock() else {
            return Status::REFUSED;
        };
        *slot = Some(schema);
        Status::OK
    })
}

extern "system" fn sync(me: ModHandle, synced: Str, out: *mut Str) -> Status {
    entry("sync", || {
        let (Some(loader), Some(synced)) = (loader(), text(synced)) else {
            return Status::BAD_ARGS;
        };
        let Some((_, record)) = loader.record(me) else {
            return Status::BAD_ARGS;
        };
        if let Ok(mut reported) = record.synced.lock() {
            let now = (!synced.is_empty()).then(|| synced.to_owned());
            // A server whose join values changed tells the joined clients at
            // once; the first report, at init, finds none joined.
            if reported.is_some()
                && *reported != now
                && (loader.side == Side::SERVER || crate::world::hosting())
            {
                crate::remote::push_sync(&record.id, now.as_deref().unwrap_or("{}"));
            }
            *reported = now;
        }
        let server = record
            .server_synced
            .lock()
            .ok()
            .and_then(|held| held.clone())
            .unwrap_or_default();
        let Ok(mut slot) = record.sync_out.lock() else {
            return Status::REFUSED;
        };
        *slot = server;
        // The text stays in the record until the mod's next call.
        put(out, Str::new(slot.as_str()))
    })
}

extern "system" fn hosting() -> u8 {
    entry("hosting", || {
        loader().map_or(0, |loader| {
            u8::from(loader.side == Side::SERVER || crate::world::hosting())
        })
    })
}

extern "system" fn server_world() -> u64 {
    entry("server_world", crate::world::server_world)
}

extern "system" fn command(
    me: ModHandle,
    name: Str,
    help: Str,
    callback: api_frame::CommandCallFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("command", || {
        let (Some(name), Some(help)) = (text(name), text(help)) else {
            return Status::BAD_ARGS;
        };
        let (_, index, record) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        let status = crate::conflicts::note(
            format!("command /{name}"),
            index,
            crate::conflicts::Mode::Exclusive,
            crate::chat::register(index, name, help, callback, user),
        );
        if status.is_ok() {
            crate::log::write(Level::DEBUG, &record.id, &format!("command /{name}"));
        }
        status
    })
}

extern "system" fn chat(me: ModHandle, to: u64, message: Str) -> Status {
    entry("chat", || {
        let Some(message) = text(message) else {
            return Status::BAD_ARGS;
        };
        match caller(me) {
            Ok(_) => crate::chat::send(to, message),
            Err(status) => status,
        }
    })
}

extern "system" fn set_label(me: ModHandle, entity: u32, label: Str) -> Status {
    entry("set_label", || {
        let Some(label) = text(label) else {
            return Status::BAD_ARGS;
        };
        match caller(me) {
            Ok(loader) => crate::label::set(loader, entity, label),
            Err(status) => status,
        }
    })
}

extern "system" fn rename_filter(
    me: ModHandle,
    callback: api_frame::RenameCallFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("rename_filter", || {
        let (_, index, _) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        crate::conflicts::note(
            "rename filter",
            index,
            crate::conflicts::Mode::Chained,
            crate::label::register(index, callback, user),
        )
    })
}

extern "system" fn status(
    me: ModHandle,
    callback: api_frame::StatusFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("status", || {
        let (_, index, _) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        crate::status::register(index, callback, user)
    })
}

thread_local! {
    /// The reply `run_command` last handed out on this thread.
    static CONSOLE_REPLY: core::cell::RefCell<String> = const { core::cell::RefCell::new(String::new()) };
}

extern "system" fn run_command(me: ModHandle, line: Str, out: *mut Str) -> Status {
    entry("run_command", || {
        let Some(line) = text(line) else {
            return Status::BAD_ARGS;
        };
        let loader = match caller(me) {
            Ok(loader) => loader,
            Err(status) => return status,
        };
        match crate::chat::run_console(loader, line) {
            Ok(reply) => CONSOLE_REPLY.with_borrow_mut(|kept| {
                *kept = reply;
                // The text stays in this thread's buffer until its next call.
                put(out, Str::new(kept))
            }),
            Err(status) => status,
        }
    })
}

extern "system" fn altars_open(me: ModHandle) -> Status {
    entry("altars_open", || {
        let (loader, index) = match caller_index(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        crate::altars::open(loader, index)
    })
}

extern "system" fn altars(
    me: ModHandle,
    out: *mut api_frame::AltarInfo,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("altars", || {
        let (_, index) = match caller_index(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if !crate::altars::opened(index) {
            return Status::REFUSED;
        }
        let altars = crate::altars::list();
        // SAFETY: forwarded from the mod's call.
        unsafe { list(&altars, out, entry_size, capacity, count) }
    })
}

extern "system" fn altar_zones_control(me: ModHandle) -> Status {
    entry("altar_zones_control", || {
        let (loader, index) = match caller_index(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        crate::conflicts::note(
            "altar zones",
            index,
            crate::conflicts::Mode::Exclusive,
            crate::altars::control(loader, index),
        )
    })
}

extern "system" fn set_altar_zones(
    me: ModHandle,
    sizes: *const api_frame::AltarZoneSize,
    count: usize,
) -> Status {
    entry("set_altar_zones", || {
        let (_, index) = match caller_index(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        let sizes = if count == 0 {
            &[][..]
        } else if sizes.is_null() {
            return Status::BAD_ARGS;
        } else {
            // SAFETY: the mod passes `count` sizes, valid for the call.
            unsafe { core::slice::from_raw_parts(sizes, count) }
        };
        crate::altars::set_sizes(index, sizes)
    })
}

extern "system" fn zone_check_filter(
    me: ModHandle,
    callback: api_frame::ZoneCheckFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("zone_check_filter", || {
        let (_, index) = match caller_index(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        crate::conflicts::note(
            "altar zone check",
            index,
            crate::conflicts::Mode::Chained,
            crate::altars::register(index, callback, user),
        )
    })
}

extern "system" fn survivor_lookup(
    me: ModHandle,
    callback: api_frame::SurvivorLookupFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("survivor_lookup", || match caller_index(me) {
        Ok((loader, index)) => crate::conflicts::note(
            "survivor lookup",
            index,
            crate::conflicts::Mode::Exclusive,
            crate::survivors::register_lookup(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

extern "system" fn survivor_bed_check(
    me: ModHandle,
    callback: api_frame::BedCheckFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("survivor_bed_check", || match caller_index(me) {
        Ok((loader, index)) => crate::conflicts::note(
            "bed check",
            index,
            crate::conflicts::Mode::Chained,
            crate::survivors::register_bed_check(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

extern "system" fn summon_filter(
    me: ModHandle,
    callback: api_frame::SummonFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("summon_filter", || match caller_index(me) {
        Ok((loader, index)) => crate::conflicts::note(
            "summon filter",
            index,
            crate::conflicts::Mode::Chained,
            crate::survivors::register_summon(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

extern "system" fn summoned(
    me: ModHandle,
    callback: api_frame::SummonFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("summoned", || match caller_index(me) {
        Ok((loader, index)) => crate::survivors::register_summoned(loader, index, callback, user),
        Err(status) => status,
    })
}

extern "system" fn item_data_open(me: ModHandle, id: Str, out: *mut u32) -> Status {
    entry("item_data_open", || {
        let Some(id) = text(id) else {
            return Status::BAD_ARGS;
        };
        match caller_index(me).and_then(|(loader, index)| crate::item_data::open(loader, index, id))
        {
            Ok(kind) => put(out, kind),
            Err(status) => status,
        }
    })
}

extern "system" fn entity_data_open(
    me: ModHandle,
    id: Str,
    flags: api_frame::EntityDataFlags,
    out: *mut u32,
) -> Status {
    entry("entity_data_open", || {
        let Some(id) = text(id) else {
            return Status::BAD_ARGS;
        };
        let sent = flags.0 & api_frame::EntityDataFlags::SENT.0 != 0;
        match caller_index(me)
            .and_then(|(loader, index)| crate::item_data::open_entity(loader, index, id, sent))
        {
            Ok(kind) => put(out, kind),
            Err(status) => status,
        }
    })
}

extern "system" fn item_data_get(
    me: ModHandle,
    kind: u32,
    pide: u32,
    item: u32,
    out: *mut u8,
    capacity: u32,
    len: *mut u32,
) -> Status {
    entry("item_data_get", || {
        if let Err(status) = caller(me) {
            return status;
        }
        let value = match crate::item_data::get(kind, pide, item) {
            Ok(value) => value,
            Err(status) => return status,
        };
        let size = u32::try_from(value.len()).unwrap_or(u32::MAX);
        if !len.is_null() {
            // SAFETY: the caller's length out pointer.
            unsafe { len.write(size) };
        }
        if size > capacity || out.is_null() {
            return Status::BAD_ARGS;
        }
        // SAFETY: the caller passes room for `capacity` bytes at `out`.
        unsafe { out.copy_from_nonoverlapping(value.as_ptr(), value.len()) };
        Status::OK
    })
}

extern "system" fn item_data_set(
    me: ModHandle,
    kind: u32,
    pide: u32,
    item: u32,
    data: *const u8,
    len: u32,
) -> Status {
    entry("item_data_set", || {
        if let Err(status) = caller(me) {
            return status;
        }
        let Ok(len) = usize::try_from(len) else {
            return Status::BAD_ARGS;
        };
        let value = if len == 0 {
            &[][..]
        } else if data.is_null() {
            return Status::BAD_ARGS;
        } else {
            // SAFETY: the caller passes `len` bytes at `data` for the call.
            unsafe { core::slice::from_raw_parts(data, len) }
        };
        match crate::item_data::set(kind, pide, item, value) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

extern "system" fn content_serve(
    me: ModHandle,
    data: *const u8,
    len: u32,
    out: *mut [u32; 4],
) -> Status {
    entry("content_serve", || {
        if let Err(status) = caller(me) {
            return status;
        }
        let Ok(len) = usize::try_from(len) else {
            return Status::BAD_ARGS;
        };
        if data.is_null() || len == 0 {
            return Status::BAD_ARGS;
        }
        // SAFETY: the caller passes `len` bytes at `data` for the call.
        let bytes = unsafe { core::slice::from_raw_parts(data, len) };
        match crate::textures::serve(bytes) {
            Ok(hash) => put(out, hash),
            Err(status) => status,
        }
    })
}

extern "system" fn content_read(
    me: ModHandle,
    hash: *const [u32; 4],
    out: *mut u8,
    capacity: u32,
    len: *mut u32,
) -> Status {
    entry("content_read", || {
        if let Err(status) = caller(me) {
            return status;
        }
        // SAFETY: the caller passes a hash for the call; `as_ref` turns away null.
        let Some(hash) = (unsafe { hash.as_ref() }).copied() else {
            return Status::BAD_ARGS;
        };
        let data = match crate::textures::content_of(&hash) {
            Ok(data) => data,
            Err(status) => return status,
        };
        let size = u32::try_from(data.len()).unwrap_or(u32::MAX);
        if !len.is_null() {
            // SAFETY: the caller's length out pointer.
            unsafe { len.write(size) };
        }
        if size > capacity || out.is_null() {
            return Status::BAD_ARGS;
        }
        // SAFETY: the caller passes room for `capacity` bytes at `out`.
        unsafe { out.copy_from_nonoverlapping(data.as_ptr(), data.len()) };
        Status::OK
    })
}

extern "system" fn icons_ready(me: ModHandle) -> Status {
    entry("icons_ready", || match caller(me) {
        Ok(_) if crate::textures::ready() => Status::OK,
        Ok(_) => Status::MISSING,
        Err(status) => status,
    })
}

extern "system" fn stat_open(
    me: ModHandle,
    id: Str,
    def: *const api_frame::StatDefData,
    out: *mut api_frame::StatRef,
) -> Status {
    entry("stat_open", || {
        let Some(id) = text(id) else {
            return Status::BAD_ARGS;
        };
        // SAFETY: the caller passes a valid definition for the call, or null.
        let def = match unsafe { def.as_ref() } {
            // SAFETY: the definition's text is borrowed for the call.
            Some(data) => match unsafe { enshwick_loader_api::stats::StatDef::from_data(data) } {
                Some(def) => Some(def),
                None => return Status::BAD_ARGS,
            },
            None => None,
        };
        match caller_index(me)
            .and_then(|(loader, index)| crate::stats::open(loader, index, id, def))
        {
            Ok(stat) => put(out, stat),
            Err(status) => status,
        }
    })
}

extern "system" fn stat_info(
    me: ModHandle,
    stat: u32,
    def: *mut api_frame::StatDefData,
    id: *mut Str,
) -> Status {
    entry("stat_info", || {
        if let Err(status) = caller(me) {
            return status;
        }
        match crate::stats::info(stat) {
            Ok((data, name)) => {
                let status = put(def, data);
                if status.is_ok() {
                    put(id, name)
                } else {
                    status
                }
            }
            Err(status) => status,
        }
    })
}

extern "system" fn stat_value(me: ModHandle, entity: u32, stat: u32, out: *mut f32) -> Status {
    entry("stat_value", || {
        if let Err(status) = caller(me) {
            return status;
        }
        match crate::stats::value(entity, stat) {
            Ok(value) => put(out, value),
            Err(status) => status,
        }
    })
}

extern "system" fn stat_modify(me: ModHandle, request: *const api_frame::StatModify) -> Status {
    entry("stat_modify", || {
        // SAFETY: the caller passes a valid request for the call.
        let Some(request) = (unsafe { request.as_ref() }) else {
            return Status::BAD_ARGS;
        };
        match caller_index(me)
            .and_then(|(loader, index)| crate::stats::modify(loader, index, request))
        {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

extern "system" fn stat_buff(
    me: ModHandle,
    buff: u32,
    modifiers: *const api_frame::StatModifierData,
    count: u32,
) -> Status {
    entry("stat_buff", || {
        if let Err(status) = caller(me) {
            return status;
        }
        let list = if count == 0 {
            &[][..]
        } else if modifiers.is_null() {
            return Status::BAD_ARGS;
        } else {
            // SAFETY: the caller passes `count` modifiers for the call.
            unsafe { core::slice::from_raw_parts(modifiers, count as usize) }
        };
        match crate::stats::buff(buff, list) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

extern "system" fn stats_changed(
    me: ModHandle,
    callback: api_frame::StatChangeFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("stats_changed", || match caller_index(me) {
        Ok((_, index)) => match crate::stats::on_changed(index, callback, user) {
            Ok(()) => Status::OK,
            Err(status) => status,
        },
        Err(status) => status,
    })
}

extern "system" fn stat_sources(
    me: ModHandle,
    entity: u32,
    stat: u32,
    out: *mut api_frame::StatSource,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("stat_sources", || {
        let loader = match caller(me) {
            Ok(loader) => loader,
            Err(status) => return status,
        };
        match crate::stats::sources(loader, entity, stat) {
            Ok(list) => put_list(&list, out, capacity, count),
            Err(status) => status,
        }
    })
}

extern "system" fn stat_watch(me: ModHandle, entity: u32, watch: u8) -> Status {
    entry("stat_watch", || {
        if let Err(status) = caller(me) {
            return status;
        }
        match crate::stats::watch(entity, watch != 0) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

extern "system" fn affix_open(
    me: ModHandle,
    id: Str,
    def: *const api_frame::AffixDefData,
    out: *mut api_frame::StatRef,
) -> Status {
    entry("affix_open", || {
        let Some(id) = text(id) else {
            return Status::BAD_ARGS;
        };
        // SAFETY: the caller passes a valid definition for the call.
        let Some(def) = (unsafe { def.as_ref() }) else {
            return Status::BAD_ARGS;
        };
        match caller_index(me)
            .and_then(|(loader, index)| crate::stats::open_affix(loader, index, id, def))
        {
            Ok(affix) => put(out, affix),
            Err(status) => status,
        }
    })
}

extern "system" fn affix_roll(
    me: ModHandle,
    roll: *const api_frame::AffixRoll,
    count: *mut u32,
) -> Status {
    entry("affix_roll", || {
        let loader = match caller(me) {
            Ok(loader) => loader,
            Err(status) => return status,
        };
        // SAFETY: the caller passes a valid request for the call.
        let Some(roll) = (unsafe { roll.as_ref() }) else {
            return Status::BAD_ARGS;
        };
        match crate::stats::roll(loader, roll) {
            Ok(rolled) => put(count, rolled),
            Err(status) => status,
        }
    })
}

extern "system" fn item_lines(
    me: ModHandle,
    pide: u32,
    item: u32,
    out: *mut api_frame::ItemLine,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("item_lines", || {
        if let Err(status) = caller(me) {
            return status;
        }
        match crate::stats::item_lines(pide, item) {
            Ok(lines) => put_list(&lines, out, capacity, count),
            Err(status) => status,
        }
    })
}

extern "system" fn affix_info(
    me: ModHandle,
    affix: u32,
    text: *mut Str,
    rarity: *mut u8,
) -> Status {
    entry("affix_info", || {
        if let Err(status) = caller(me) {
            return status;
        }
        match crate::stats::affix_info(affix) {
            Ok((name, rare)) => {
                let status = put(text, name);
                if status.is_ok() {
                    put(rarity, rare)
                } else {
                    status
                }
            }
            Err(status) => status,
        }
    })
}

extern "system" fn summon_item(me: ModHandle, item: u32, survivor: u32) -> Status {
    entry("summon_item", || match caller_index(me) {
        Ok((loader, _)) => crate::survivors::register_item(loader, item, survivor),
        Err(status) => status,
    })
}

extern "system" fn survivor_status(
    me: ModHandle,
    callback: api_frame::SurvivorStatusFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("survivor_status", || match caller_index(me) {
        Ok((loader, index)) => crate::conflicts::note(
            "survivor status",
            index,
            crate::conflicts::Mode::Exclusive,
            crate::survivors::register_status(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

extern "system" fn interaction_filter(
    me: ModHandle,
    callback: api_frame::InteractionFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("interaction_filter", || match caller_index(me) {
        Ok((loader, index)) => crate::conflicts::note(
            "interaction filter",
            index,
            crate::conflicts::Mode::Chained,
            crate::interactions::register(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

extern "system" fn skill_override(
    me: ModHandle,
    node: u32,
    callback: api_frame::SkillCheckFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("skill_override", || match caller_index(me) {
        Ok((loader, index)) => crate::conflicts::note(
            "skill override",
            index,
            crate::conflicts::Mode::FirstAnswers,
            crate::progress::register_skill(loader, index, node, callback, user),
        ),
        Err(status) => status,
    })
}

extern "system" fn comfort_filter(
    me: ModHandle,
    callback: api_frame::ComfortFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("comfort_filter", || match caller_index(me) {
        Ok((loader, index)) => crate::conflicts::note(
            "comfort filter",
            index,
            crate::conflicts::Mode::Chained,
            crate::progress::register_comfort(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

extern "system" fn experience_filter(
    me: ModHandle,
    callback: api_frame::ExperiencePassFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("experience_filter", || match caller_index(me) {
        Ok((loader, index)) => crate::conflicts::note(
            "experience filter",
            index,
            crate::conflicts::Mode::Chained,
            crate::progress::register_experience(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

extern "system" fn altar_limit(
    me: ModHandle,
    callback: api_frame::AltarLimitFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("altar_limit", || match caller_index(me) {
        Ok((loader, index)) => crate::conflicts::note(
            "altar limit",
            index,
            crate::conflicts::Mode::Exclusive,
            crate::bases::register_limit(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

extern "system" fn raise_base_limit(me: ModHandle) -> Status {
    entry("raise_base_limit", || match caller_index(me) {
        Ok((loader, index)) => crate::bases::raise(loader, index),
        Err(status) => status,
    })
}

extern "system" fn base_zones(me: ModHandle, zones: *mut u32, bases: *mut u32) -> Status {
    entry("base_zones", || {
        if let Err(status) = caller(me) {
            return status;
        }
        let Some((zone_count, base_count)) = crate::bases::zones() else {
            return Status::MISSING;
        };
        let status = put(zones, zone_count);
        if !status.is_ok() {
            return status;
        }
        put(bases, base_count)
    })
}

extern "system" fn game_event(
    me: ModHandle,
    type_hash: u32,
    callback: api_frame::GameEventFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("game_event", || {
        let (_, index, _) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        crate::game_events::register(index, type_hash, callback, user)
    })
}

extern "system" fn entities(
    me: ModHandle,
    type_hash: u32,
    out: *mut api_frame::EntityPlace,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("entities", || {
        let found = match caller(me)
            .and_then(|loader| crate::entities::with_component(loader, type_hash))
        {
            Ok(found) => found,
            Err(status) => return status,
        };
        if !out.is_null() {
            for (index, place) in found.iter().take(capacity as usize).enumerate() {
                // SAFETY: the caller passes room for `capacity` entries.
                unsafe { out.wrapping_add(index).write(*place) };
            }
        }
        put(count, u32::try_from(found.len()).unwrap_or(u32::MAX))
    })
}

extern "system" fn destroy(me: ModHandle, entity: u32) -> Status {
    entry("destroy", || {
        match caller(me).and_then(|loader| crate::entities::destroy(loader, entity)) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

extern "system" fn save_world(me: ModHandle) -> Status {
    entry("save_world", || {
        match caller(me).and_then(crate::saves::request) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

extern "system" fn world_key(me: ModHandle, out: *mut Str) -> Status {
    entry("world_key", || match caller(me) {
        Ok(loader) => {
            crate::saves::world_key(loader).map_or(Status::MISSING, |key| put(out, Str::new(key)))
        }
        Err(status) => status,
    })
}

extern "system" fn component_write(
    me: ModHandle,
    entity: u32,
    type_hash: u32,
    out: *mut *mut core::ffi::c_void,
) -> Status {
    entry("component_write", || {
        match caller(me).and_then(|loader| crate::writes::component_mut(loader, entity, type_hash))
        {
            Ok(data) => put(out, data),
            Err(status) => status,
        }
    })
}

extern "system" fn emit(me: ModHandle, type_hash: u32, data: *const u8, size: u32) -> Status {
    entry("emit", || {
        if data.is_null() {
            return Status::BAD_ARGS;
        }
        let Ok(len) = usize::try_from(size) else {
            return Status::BAD_ARGS;
        };
        // SAFETY: the caller passes `size` bytes at `data`, valid for the call.
        let bytes = unsafe { core::slice::from_raw_parts(data, len) };
        match caller(me).and_then(|loader| crate::emits::emit(loader, type_hash, bytes)) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

extern "system" fn teleport(
    me: ModHandle,
    request: *const enshwick_loader_api::abi::TeleportRequest,
) -> Status {
    entry("teleport", || {
        // SAFETY: the caller passes null or a request with its header; an
        // all-zero request is valid.
        let Some(request) = (unsafe {
            enshwick_loader_api::abi::read_headed(
                request,
                size_of::<enshwick_loader_api::abi::TeleportRequest>(),
            )
        }) else {
            return Status::BAD_ARGS;
        };
        match caller(me).and_then(|loader| crate::emits::teleport(loader, &request)) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

extern "system" fn spawn(
    me: ModHandle,
    request: *const enshwick_loader_api::abi::SpawnRequest,
    out: *mut u32,
) -> Status {
    entry("spawn", || {
        // SAFETY: the caller passes null or a request with its header; an
        // all-zero request is valid.
        let Some(request) = (unsafe {
            enshwick_loader_api::abi::read_headed(
                request,
                size_of::<enshwick_loader_api::abi::SpawnRequest>(),
            )
        }) else {
            return Status::BAD_ARGS;
        };
        match caller(me).and_then(|loader| crate::spawns::spawn(loader, &request)) {
            Ok(entity) => put(out, entity),
            Err(status) => status,
        }
    })
}

extern "system" fn craft_filter(
    me: ModHandle,
    callback: api_frame::CraftCheckFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("craft_filter", || match caller_index(me) {
        Ok((loader, index)) => crate::conflicts::note(
            "crafting veto",
            index,
            crate::conflicts::Mode::Chained,
            crate::vetoes::register_craft(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

extern "system" fn build_filter(
    me: ModHandle,
    callback: api_frame::BuildCheckFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("build_filter", || match caller_index(me) {
        Ok((loader, index)) => crate::conflicts::note(
            "building veto",
            index,
            crate::conflicts::Mode::Chained,
            crate::vetoes::register_build(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

extern "system" fn net_channel(
    me: ModHandle,
    name: Str,
    callback: api_frame::NetMessageFn,
    user: *mut core::ffi::c_void,
    out: *mut u32,
) -> Status {
    entry("net_channel", || {
        let (loader, index) = match caller_index(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        // SAFETY: the name is borrowed for the call.
        let Some(name) = (unsafe { name.as_str() }) else {
            return Status::BAD_ARGS;
        };
        match crate::net::open(loader, index, name, callback, user) {
            Ok(id) => put(out, id),
            Err(status) => status,
        }
    })
}

extern "system" fn net_send(
    me: ModHandle,
    channel: u32,
    peer: u64,
    data: *const u8,
    size: u32,
) -> Status {
    entry("net_send", || {
        let (loader, index) = match caller_index(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        let Ok(len) = usize::try_from(size) else {
            return Status::BAD_ARGS;
        };
        if data.is_null() && len != 0 {
            return Status::BAD_ARGS;
        }
        let bytes = if len == 0 {
            &[][..]
        } else {
            // SAFETY: the caller passes `size` bytes at `data` for the call.
            unsafe { core::slice::from_raw_parts(data, len) }
        };
        match crate::net::send(loader, index, channel, peer, bytes) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

extern "system" fn net_ready(me: ModHandle, peer: u64) -> Status {
    entry("net_ready", || match caller(me) {
        Ok(_) if crate::net::ready(peer) => Status::OK,
        Ok(_) => Status::MISSING,
        Err(status) => status,
    })
}

extern "system" fn notify(me: ModHandle, key: u64, text: Str) -> Status {
    entry("notify", || {
        // SAFETY: the text is borrowed for the call.
        let Some(text) = (unsafe { text.as_str() }) else {
            return Status::BAD_ARGS;
        };
        match caller(me).and_then(|loader| crate::notices::send(loader, key, text)) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

extern "system" fn time_of_day(
    me: ModHandle,
    out: *mut enshwick_loader_api::abi::DaytimeInfo,
) -> Status {
    entry("time_of_day", || {
        match caller(me).and_then(crate::daytime::info) {
            Ok(info) => put_headed(out, info),
            Err(status) => status,
        }
    })
}

extern "system" fn set_time_of_day(me: ModHandle, time: i64) -> Status {
    entry("set_time_of_day", || {
        match caller(me).and_then(|loader| crate::daytime::set_time(loader, time)) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

extern "system" fn time_speed_control(me: ModHandle) -> Status {
    entry("time_speed_control", || match caller_index(me) {
        Ok((loader, index)) => crate::conflicts::note(
            "time speed",
            index,
            crate::conflicts::Mode::Exclusive,
            crate::daytime::register_speed(loader, index),
        ),
        Err(status) => status,
    })
}

extern "system" fn set_time_speed(me: ModHandle, factor: f32) -> Status {
    entry("set_time_speed", || {
        match caller_index(me)
            .and_then(|(loader, index)| crate::daytime::set_speed(loader, index, factor))
        {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

extern "system" fn game_settings(
    me: ModHandle,
    out: *mut u8,
    size: u32,
    preset: *mut u32,
) -> Status {
    entry("game_settings", || {
        let Ok(len) = usize::try_from(size) else {
            return Status::BAD_ARGS;
        };
        if out.is_null() {
            return Status::BAD_ARGS;
        }
        // SAFETY: the caller passes `size` writable bytes at `out` for the call.
        let out = unsafe { core::slice::from_raw_parts_mut(out, len) };
        match caller(me).and_then(|loader| crate::game_settings::read(loader, out)) {
            Ok(value) => put(preset, u32::from(value)),
            Err(status) => status,
        }
    })
}

extern "system" fn set_game_settings(me: ModHandle, data: *const u8, size: u32) -> Status {
    entry("set_game_settings", || {
        let Ok(len) = usize::try_from(size) else {
            return Status::BAD_ARGS;
        };
        if data.is_null() {
            return Status::BAD_ARGS;
        }
        // SAFETY: the caller passes `size` bytes at `data`, valid for the call.
        let bytes = unsafe { core::slice::from_raw_parts(data, len) };
        match caller(me).and_then(|loader| crate::game_settings::write(loader, bytes)) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

extern "system" fn damage_filter(
    me: ModHandle,
    callback: enshwick_loader_api::abi::DamageFilterFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("damage_filter", || match caller_index(me) {
        Ok((loader, index)) => crate::conflicts::note(
            "damage filter",
            index,
            crate::conflicts::Mode::Chained,
            crate::damage::register(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

extern "system" fn knowledge_input_filter(
    me: ModHandle,
    callback: enshwick_loader_api::abi::KnowledgeInputFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("knowledge_input_filter", || match caller_index(me) {
        Ok((loader, index)) => crate::conflicts::note(
            "knowledge input filter",
            index,
            crate::conflicts::Mode::Chained,
            crate::knowledge::guard::register(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

/// Writes up to `capacity` of `items` to `out` and their number to `count`.
fn put_list<T: Copy>(items: &[T], out: *mut T, capacity: u32, count: *mut u32) -> Status {
    let Ok(total) = u32::try_from(items.len()) else {
        return Status::BAD_ARGS;
    };
    if !out.is_null() {
        for (i, item) in items.iter().take(capacity as usize).enumerate() {
            // SAFETY: the caller passes room for `capacity` entries at `out`.
            let slot = unsafe { out.add(i) };
            // SAFETY: as above.
            unsafe { slot.write(*item) };
        }
    }
    put(count, total)
}

extern "system" fn buff_types(
    me: ModHandle,
    out: *mut enshwick_loader_api::abi::BuffTypeInfo,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("buff_types", || {
        match caller(me).and_then(crate::buffs::types) {
            Ok(types) => put_list(&types, out, capacity, count),
            Err(status) => status,
        }
    })
}

extern "system" fn buffs(
    me: ModHandle,
    entity: u32,
    out: *mut enshwick_loader_api::abi::ActiveBuff,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("buffs", || {
        match caller(me).and_then(|loader| crate::buffs::of(loader, entity)) {
            Ok(list) => put_list(&list, out, capacity, count),
            Err(status) => status,
        }
    })
}

extern "system" fn apply_buff(
    me: ModHandle,
    request: *const enshwick_loader_api::abi::BuffRequest,
) -> Status {
    entry("apply_buff", || {
        // SAFETY: the caller passes null or a request with its header; an
        // all-zero request is valid.
        let Some(request) = (unsafe {
            enshwick_loader_api::abi::read_headed(
                request,
                size_of::<enshwick_loader_api::abi::BuffRequest>(),
            )
        }) else {
            return Status::BAD_ARGS;
        };
        match caller(me).and_then(|loader| crate::buffs::apply(loader, &request)) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

extern "system" fn remove_buff(me: ModHandle, entity: u32, id: u32, removed: *mut u8) -> Status {
    entry("remove_buff", || {
        match caller(me).and_then(|loader| crate::buffs::remove(loader, entity, id)) {
            Ok(had) => put(removed, u8::from(had)),
            Err(status) => status,
        }
    })
}

extern "system" fn knowledge(
    me: ModHandle,
    entity: u32,
    id: u32,
    out: *mut enshwick_loader_api::abi::KnowledgeInfo,
) -> Status {
    entry("knowledge", || {
        match caller(me).and_then(|loader| crate::knowledge::value(loader, entity, id)) {
            Ok(info) => put_headed(out, info),
            Err(status) => status,
        }
    })
}

extern "system" fn grant_knowledge(
    me: ModHandle,
    entity: u32,
    id: u32,
    kind: u32,
    value: u32,
) -> Status {
    entry("grant_knowledge", || {
        match caller(me).and_then(|loader| crate::knowledge::grant(loader, entity, id, kind, value))
        {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

extern "system" fn quests(me: ModHandle, out: *mut u32, capacity: u32, count: *mut u32) -> Status {
    entry("quests", || {
        match caller(me).and_then(crate::knowledge::quests) {
            Ok(ids) => {
                if !out.is_null() {
                    for (i, id) in ids.iter().take(capacity as usize).enumerate() {
                        // SAFETY: the caller passes room for `capacity` ids at `out`.
                        let slot = unsafe { out.add(i) };
                        // SAFETY: as above.
                        unsafe { slot.write(*id) };
                    }
                }
                put(count, u32::try_from(ids.len()).unwrap_or(u32::MAX))
            }
            Err(status) => status,
        }
    })
}

extern "system" fn weather_at(
    me: ModHandle,
    position: *const [i64; 3],
    out: *mut [f32; 4],
) -> Status {
    entry("weather_at", || {
        // SAFETY: the caller passes null or a position valid for the call.
        let Some(position) = (unsafe { position.as_ref() }).copied() else {
            return Status::BAD_ARGS;
        };
        match caller(me).and_then(|loader| crate::weather::sample(loader, position)) {
            Ok(weights) => put(out, weights),
            Err(status) => status,
        }
    })
}

extern "system" fn weather_fronts(
    me: ModHandle,
    out: *mut enshwick_loader_api::abi::WeatherFront,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("weather_fronts", || {
        match caller(me).and_then(crate::weather::fronts) {
            Ok(fronts) => {
                if !out.is_null() {
                    for (i, front) in fronts.iter().take(capacity as usize).enumerate() {
                        // SAFETY: the caller passes room for `capacity` fronts.
                        let slot = unsafe { out.add(i) };
                        // SAFETY: as above.
                        unsafe { slot.write(*front) };
                    }
                }
                put(count, u32::try_from(fronts.len()).unwrap_or(u32::MAX))
            }
            Err(status) => status,
        }
    })
}

extern "system" fn set_weather(me: ModHandle, effect: u32, duration: i64) -> Status {
    entry("set_weather", || {
        match caller(me).and_then(|loader| crate::weather::set(loader, effect, duration)) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

extern "system" fn terrain_materials(
    me: ModHandle,
    out: *mut enshwick_loader_api::abi::TerrainMaterialInfo,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("terrain_materials", || {
        match caller(me).and_then(crate::terrain::materials) {
            Ok(materials) => {
                if !out.is_null() {
                    for (i, material) in materials.iter().take(capacity as usize).enumerate() {
                        // SAFETY: the caller passes room for `capacity` entries.
                        let slot = unsafe { out.add(i) };
                        // SAFETY: as above.
                        unsafe { slot.write(*material) };
                    }
                }
                put(count, u32::try_from(materials.len()).unwrap_or(u32::MAX))
            }
            Err(status) => status,
        }
    })
}

extern "system" fn terrain_edit(
    me: ModHandle,
    edit: *const enshwick_loader_api::abi::TerrainEdit,
    amount: *mut f32,
) -> Status {
    entry("terrain_edit", || {
        // SAFETY: the caller passes null or an edit with its header; an
        // all-zero edit is valid.
        let Some(edit) = (unsafe {
            enshwick_loader_api::abi::read_headed(
                edit,
                size_of::<enshwick_loader_api::abi::TerrainEdit>(),
            )
        }) else {
            return Status::BAD_ARGS;
        };
        match caller(me).and_then(|loader| crate::terrain::edit(loader, &edit)) {
            Ok(changed) => put(amount, changed),
            Err(status) => status,
        }
    })
}

extern "system" fn ui_frame(
    me: ModHandle,
    callback: api_frame::UiFrameFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("ui_frame", || match caller_index(me) {
        Ok((loader, index)) => crate::ui::register_frame(loader, index, callback, user),
        Err(status) => status,
    })
}

extern "system" fn ui_widget(
    me: ModHandle,
    frame: u64,
    widget: *mut api_frame::UiWidget,
) -> Status {
    entry("ui_widget", || {
        let (loader, index) = match caller_index(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        // SAFETY: a non-null widget from a mod is its own, valid for the call.
        let Some(widget) = (unsafe { widget.as_mut() }) else {
            return Status::BAD_ARGS;
        };
        crate::ui::widget(loader, index, frame, widget)
    })
}

extern "system" fn ui_hud(
    me: ModHandle,
    callback: api_frame::UiFrameFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("ui_hud", || match caller_index(me) {
        Ok((loader, index)) => crate::ui::register_hud(loader, index, callback, user),
        Err(status) => status,
    })
}

extern "system" fn ui_markers(
    me: ModHandle,
    markers: *const api_frame::UiMarker,
    count: u32,
) -> Status {
    entry("ui_markers", || {
        let (loader, index) = match caller_index(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if loader.side != Side::CLIENT {
            return Status::MISSING;
        }
        let given = if count == 0 {
            &[][..]
        } else if markers.is_null() {
            return Status::BAD_ARGS;
        } else {
            // SAFETY: the mod passes `count` markers, valid for the call.
            unsafe { core::slice::from_raw_parts(markers, count as usize) }
        };
        crate::ui::set_markers(index, given)
    })
}

extern "system" fn ui_dialog(
    me: ModHandle,
    dialog: *const api_frame::UiDialog,
    callback: api_frame::UiAnswerFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("ui_dialog", || {
        let (loader, index) = match caller_index(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        // SAFETY: a non-null dialog from a mod is valid for the call.
        let Some(dialog) = (unsafe { dialog.as_ref() }) else {
            return Status::BAD_ARGS;
        };
        crate::ui::mod_dialog(loader, index, dialog, callback, user)
    })
}

extern "system" fn config_check(
    me: ModHandle,
    callback: api_frame::ConfigCheckFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("config_check", || match calling(me) {
        Ok((_, _, record)) => {
            if record
                .check
                .set((callback, user.expose_provenance()))
                .is_ok()
            {
                Status::OK
            } else {
                Status::REFUSED
            }
        }
        Err(status) => status,
    })
}

/// Runs mod `index`'s check of the new settings text `text`; `Ok` for a mod
/// without one.
pub(crate) fn check_settings(index: usize, text: &str) -> Result<(), String> {
    let Some(&(callback, user)) = loader()
        .and_then(|loader| loader.mods.get(index))
        .and_then(|record| record.check.get())
    else {
        return Ok(());
    };
    let mut why = Str::EMPTY;
    // SAFETY: the mod's check with its own pointer; the text lives for the
    // call, and the safe layer catches its panics.
    let status = unsafe {
        callback(
            Str::new(text),
            &raw mut why,
            core::ptr::with_exposed_provenance_mut(user),
        )
    };
    if status.is_ok() {
        return Ok(());
    }
    // SAFETY: the reason stays valid until the mod's next check.
    Err(unsafe { why.as_str() }
        .unwrap_or("refused by the mod")
        .to_owned())
}

extern "system" fn texts(me: ModHandle, toml: Str) -> Status {
    entry("texts", || {
        let (_, _, record) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        let Some(toml) = text(toml) else {
            return Status::BAD_ARGS;
        };
        let Ok(parsed) = enshwick_loader_api::i18n::Texts::parse(toml) else {
            return Status::BAD_ARGS;
        };
        let Ok(mut slot) = record.texts.lock() else {
            return Status::REFUSED;
        };
        *slot = Some(parsed);
        Status::OK
    })
}

/// Mod handles start at 1; 0 is no mod.
#[must_use]
pub(crate) fn handle(index: usize) -> ModHandle {
    ModHandle(index + 1)
}
