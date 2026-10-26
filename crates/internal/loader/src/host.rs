//! The loader's side of the API: its state and the `HostApi` table.

use crate::mod_index::ModIndex;
use crate::pinned::{Constant, Symbol};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use enshwick::abi as api_frame;
use enshwick::abi::{
    Event, EventFn, Header, HookFlags, HostApi, Level, Manifest, ManifestFlags, ModHandle, Side,
    Status, Str,
};
use enshwick_gamedata::reflect::{Fingerprint, Reflection};

use crate::builds::KnownBuild;
use crate::chains::{Chains, Link, Next};
use crate::locks::Relock;
use crate::mid::Mids;

mod entries;
mod join;
mod memory;

pub(crate) use entries::mods::check_settings;

use entries::build::{
    build, caller_frame, constant, data_dir, event_types, field_offset, log_level, symbol, system,
    type_hash, type_size,
};
use entries::content::{
    add_text, change_data, content_read, content_serve, entity_data_open, game_text, icons_ready,
    item_data_get, item_data_open, item_data_set, rebuild_resource, recipe_disabled, recipe_kind,
    recipe_rule, resource_read, serve_resource,
};
use entries::gameplay::{
    account_knowledge, affix_info, affix_open, affix_roll, altar_limit, altar_zone_filter,
    altar_zones_changed, altar_zones_control, altars, altars_open, apply_buff, base_zones,
    buff_types, buffs, build_filter, comfort_filter, craft_filter, damage_filter,
    experience_filter, grant_knowledge, interaction_filter, item_lines, knowledge,
    knowledge_input_filter, quests, raise_base_limit, rate, remove_buff, revoke_knowledge,
    set_altar_zones, set_rate, skill_override, stat_buff, stat_info, stat_modify, stat_open,
    stat_sources, stat_value, stat_watch, stats_changed, summon_filter, summon_item, summoned,
    survivor_bed_check, survivor_lookup, survivor_status, zone_check_filter,
};
use entries::hooks::{
    code_range, hook, hook_at, loop_wait, patch_code, server_wait, task_workers, workshop_recipes,
};
use entries::inventory::{inventory_open, inventory_submit, item_info};
use entries::mods::{
    config_check, config_schema, fault, join_missing, language, log, migrate, mods, replaced,
    retired_command, settings, sync, texts,
};
use entries::net::{net_channel, net_ready, net_send};
use entries::services::{
    attach_point, declare_content, define_event, define_point, publish, run_point, send_event,
    service, subscribe, subscribe_event,
};
use entries::ui::{ui_dialog, ui_frame, ui_hud, ui_markers, ui_widget};
use entries::world::{
    actions, chat, component, component_write, destroy, emit, entities, entity_template, feature,
    forget_game_settings, game_event, game_settings, hosting, keep_game_settings, notify, player,
    players, rename_filter, run_console, save_world, server_world, set_game_settings, set_label,
    set_time_of_day, set_time_speed, set_weather, spawn, status, teleport, terrain_edit,
    terrain_materials, time_of_day, time_speed_control, weather_at, weather_fronts, world_key,
};
pub(crate) use join::{
    Wanted, apply_join_values, apply_server_value, join_mods, join_mods_off, join_values,
    report_join_missing, turn_off_mod,
};
pub(crate) use memory::{find, slice};

/// A loaded mod.
#[derive(Debug)]
pub(crate) struct ModRecord {
    /// The mod id.
    pub(crate) id: String,
    /// Its version.
    pub(crate) version: String,
    /// Its DLL relative to the game's folder, with `/` separators.
    pub(crate) file: String,
    /// Its manifest, copied out of its DLL.
    pub(crate) manifest: &'static Manifest,
    /// What its manifest tells players about it.
    pub(crate) meta: crate::manifest::Meta,
    /// The ids it had before a rename.
    pub(crate) former: Vec<String>,
    /// Notes on its relations, such as a recommended mod that is missing.
    pub(crate) notes: Vec<String>,
    /// Set when the mod is off: refused at init or faulted later.
    pub(crate) off: AtomicBool,
    /// The settings texts handed to the mod.
    pub(crate) settings: Kept,
    /// The join values the mod last reported, as its settings file holds them.
    pub(crate) synced: Mutex<Option<String>>,
    /// On a client, the joined server's join values for the mod.
    pub(crate) server_synced: Mutex<Option<String>>,
    /// The texts `sync` handed to the mod.
    pub(crate) sync_out: Kept,
    /// The settings schema the mod handed the loader.
    pub(crate) schema: Mutex<Option<enshwick::config::Schema>>,
    /// The mod's check of new settings texts: callback and pointer.
    pub(crate) check: OnceLock<(api_frame::ConfigCheckFn, usize)>,
    /// The texts the mod handed the loader, for its settings' labels.
    pub(crate) texts: Mutex<Option<enshwick::i18n::Texts>>,
    /// Whether it is a data pack, a folder of data the loader runs.
    pub(crate) pack: bool,
    /// The features it reported on or off, in the order it named them.
    pub(crate) features: Mutex<Vec<crate::features::Feature>>,
}

impl ModRecord {
    /// The namespace its bare stat and item-data names take: the first id
    /// it had before a rename, where its data lives, or else its id.
    pub(crate) fn home(&self) -> &str {
        self.former.first().map_or(self.id.as_str(), String::as_str)
    }

    /// Its id, then the ids it had before a rename: every namespace its
    /// content, stats, points, services and events may take.
    pub(crate) fn names(&self) -> Vec<&str> {
        std::iter::once(self.id.as_str())
            .chain(self.former.iter().map(String::as_str))
            .collect()
    }

    /// Whether `namespace` is its id or one it had before a rename.
    pub(crate) fn owns(&self, namespace: &str) -> bool {
        self.id == namespace || self.former.iter().any(|f| f == namespace)
    }

    /// The name players read, from the mod's texts in their language, else
    /// from its manifest; empty when neither names it.
    pub(crate) fn name(&self) -> String {
        let texts = self.texts.lock().ok();
        self.meta.name_in(
            texts.as_ref().and_then(|t| t.as_ref()),
            crate::content::language(),
        )
    }

    /// The name players read, or the id when nothing names the mod.
    pub(crate) fn label(&self) -> String {
        enshwick::mods::label(&self.name(), &self.id)
    }

    /// What the mod does, from its texts in the player's language, else
    /// from its manifest.
    pub(crate) fn description(&self) -> String {
        let texts = self.texts.lock().ok();
        self.meta.description_in(
            texts.as_ref().and_then(|t| t.as_ref()),
            crate::content::language(),
        )
    }

    /// Whether it takes part in the join check while it runs.
    pub(crate) fn joins_when_on(&self) -> bool {
        self.manifest.flags.contains(ManifestFlags::JOIN)
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

/// Texts handed to a mod, kept for the life of the process: the mod may still
/// read one on a thread of its own while another thread asks for a newer one.
/// An equal text handed out before is handed out again, so a file read over
/// and over keeps one copy.
#[derive(Debug, Default)]
pub(crate) struct Kept(Mutex<Vec<Box<str>>>);

impl Kept {
    /// `text`, kept for good.
    pub(crate) fn keep(&self, text: &str) -> Str {
        let mut kept = self.0.relock();
        let at = kept.iter().position(|k| **k == *text).unwrap_or_else(|| {
            kept.push(text.into());
            kept.len() - 1
        });
        // The box's heap text never moves or drops while the store lives,
        // whatever the vector does with the boxes.
        Str::new(&kept[at])
    }
}

/// The handle of the loader itself, which its own copy of the API uses to
/// log and read while it runs data packs.
pub(crate) const LOADER_HANDLE: ModHandle = ModHandle(0);

/// A mod's handler for one event.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Subscription {
    /// The subscribing mod's index.
    pub(crate) owner: ModIndex,
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
    /// The contract of the stub that started the loader; 0 in a dry run.
    pub(crate) stub: u32,
    /// The `mod-configs` folder.
    pub(crate) configs_dir: PathBuf,
    /// The `mod-data` folder.
    pub(crate) data_dir: PathBuf,
    /// Loaded mods, in load order; handle `n` is index `n - 1`.
    pub(crate) mods: Vec<ModRecord>,
    /// The DLLs that run beside the mods without the API.
    pub(crate) others: Vec<crate::others::OtherDll>,
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
/// The events some mod handles, a bit per event id.
static HANDLED: AtomicU64 = AtomicU64::new(0);

/// Whether a mod's handler takes `event`, so work that only raises it may
/// be skipped otherwise. One load, for the game's hot paths.
pub(crate) fn handles(event: Event) -> bool {
    1u64.checked_shl(event.0)
        .is_none_or(|bit| HANDLED.load(Ordering::Relaxed) & bit != 0)
}

fn mark_handled(event: Event) {
    if let Some(bit) = 1u64.checked_shl(event.0) {
        HANDLED.fetch_or(bit, Ordering::Relaxed);
    }
}

/// Makes `loader` the one the API table answers for.
pub(crate) fn install(loader: Loader) -> Option<&'static Loader> {
    LOADER.set(loader).ok()?;
    LOADER.get()
}

pub(crate) fn loader() -> Option<&'static Loader> {
    LOADER.get()
}

/// A mod's callback with the pointer it registered beside it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Registered<F> {
    /// The owning mod's index.
    pub(crate) owner: ModIndex,
    /// The callback, inside the mod's DLL, which stays loaded.
    pub(crate) callback: F,
    /// The pointer the callback takes, as an address.
    pub(crate) user: usize,
}

impl<F> Registered<F> {
    pub(crate) fn new(owner: ModIndex, callback: F, user: *mut core::ffi::c_void) -> Self {
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
    std::mem::take(&mut *list.relock())
}

/// Whether the mod at index `owner` is off; the loader's own links never are.
pub(crate) fn is_off(owner: ModIndex) -> bool {
    owner != ModIndex::LOADER
        && loader().is_none_or(|loader| {
            loader
                .mods
                .get(owner.get())
                .is_none_or(|m| m.off.load(Ordering::Acquire))
        })
}

impl Loader {
    /// The address of the build table's symbol `id` in the running game.
    pub(crate) fn address(&self, id: Symbol) -> Option<u64> {
        self.address_by_name(id.id())
    }

    /// The build table's constant `id` as an offset or a size.
    pub(crate) fn offset(&self, id: Constant) -> Option<usize> {
        self.offset_by_name(id.id())
    }

    /// The address of the symbol `name`, an id put together at run time.
    pub(crate) fn address_by_name(&self, name: &str) -> Option<u64> {
        self.build.symbol(name).map(|rva| self.base + rva)
    }

    /// The constant `name`, an id put together at run time, as an offset or
    /// a size.
    pub(crate) fn offset_by_name(&self, name: &str) -> Option<usize> {
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
            owner: ModIndex::LOADER,
            detour: Next::<F>::address_of(detour),
            next: next.slot(),
            priority: i32::MIN,
            flags,
            hop: None,
        };
        self.chains.relock().add(target, link)
    }

    /// Whether the code at symbol `id` is what the table expects, checked
    /// before a hook patches it or a detour relies on it, by
    /// [`crate::builds::code_holds`]: on the build the table was made for,
    /// the symbol's pattern matches where its code starts, when its pattern
    /// finds it directly, and its first eight bytes are the table's
    /// `<id>.bytes`, when the table pins them. A table carried over already
    /// checked both while it found the symbol, so there only the presence of
    /// a check counts. False for a symbol the build lacks.
    pub(crate) fn code_holds(&self, id: Symbol) -> bool {
        let Some(address) = self.address(id) else {
            return false;
        };
        let exact = self.build.kind == crate::builds::Kind::Exact;
        let head = self.build.head(id.id()).map(|(pattern, at)| {
            !exact || {
                let Some(start) = address
                    .checked_sub(at)
                    .and_then(|a| usize::try_from(a).ok())
                else {
                    return false;
                };
                let mut code = vec![0u8; pattern.len()];
                // SAFETY: `start` is the table's `id` less the distance its
                // pattern starts before it, code inside the game's image,
                // which stays mapped and readable for the life of the
                // process; a pattern spans code of the function it was made
                // from, which the table checks against its build.
                unsafe { crate::memory::read_bytes(start, 0, &mut code) };
                pattern.matches(&code)
            }
        });
        let bytes = self.build.constant(&format!("{id}.bytes")).map(|expected| {
            !exact || {
                let Ok(code) = usize::try_from(address) else {
                    return false;
                };
                // SAFETY: `code` is the table's `id`, eight bytes of code
                // inside the game's image, which stays mapped and
                // readable for the life of the process and is not
                // patched before this check.
                let found: u64 = unsafe { crate::memory::read(code, 0) };
                found == expected
            }
        });
        crate::builds::code_holds(head, bytes)
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

    /// The mod named `id`, or the mod that had `id` before a rename, as a
    /// player or an administrator names it.
    pub(crate) fn find_mod(&self, id: &str) -> Option<(usize, &ModRecord)> {
        let mut mods = self.mods.iter().enumerate();
        mods.clone()
            .find(|(_, m)| m.id == id)
            .or_else(|| mods.find(|(_, m)| m.owns(id)))
    }

    fn record(&self, me: ModHandle) -> Option<(ModIndex, &ModRecord)> {
        let index = ModIndex::from_handle(me)?;
        self.mods.get(index.get()).map(|record| (index, record))
    }

    /// Turns a mod off: unlinks its hooks and logs why.
    pub(crate) fn turn_off(&self, index: ModIndex, reason: &str) {
        let Some(record) = self.mods.get(index.get()) else {
            return;
        };
        if record.off.swap(true, Ordering::AcqRel) {
            return;
        }
        crate::log::write(
            Level::ERROR,
            &record.id,
            &format!("off: {reason} ({})", crate::errors::MOD_OFF),
        );
        if record.pack {
            crate::packs::forget(&record.id);
        }
        crate::diag::turned_off(&record.id, reason);
        // A poisoned lock still holds the chains: the mod must be wired past.
        let chains = self.chains.relock();
        let faulted = |owner: ModIndex| {
            owner != ModIndex::LOADER
                && self
                    .mods
                    .get(owner.get())
                    .is_none_or(|m| m.off.load(Ordering::Acquire))
        };
        chains.rewire(faulted);
        drop(chains);
        crate::points::rearm(faulted);
        if record.joins_when_on() {
            crate::advertise::wake();
        }
    }

    /// Ends subscribing: from now on events go to the subscriptions made so
    /// far, grouped by event in subscription order, so a raise walks its own.
    pub(crate) fn seal_subscriptions(&self) {
        let mut made = std::mem::take(&mut *self.subscriptions.relock());
        made.sort_by_key(|s| s.event.0);
        for subscription in &made {
            mark_handled(subscription.event);
        }
        let _first = self.delivery.set(made.into_boxed_slice());
    }

    /// Whether a mod subscribed to `event`.
    pub(crate) fn subscribed(&self, event: Event) -> bool {
        let item_data = matches!(
            event,
            Event::WORLD_LOADED | Event::WORLD_SAVED | Event::SHUTDOWN_STARTED
        ) && (crate::item_data::wanted()
            || crate::stats::wanted()
            || crate::data_changes::wanted(self)
            || crate::rate_links::recipes_rated());
        item_data
            || self
                .delivery
                .get()
                .is_some_and(|d| d.iter().any(|s| s.event == event))
            || crate::reload::late().is_some_and(|late| late.iter().any(|s| s.event == event))
    }

    /// Raises `event` with `data` for every mod still on, or for mod `only`.
    pub(crate) fn emit(&self, event: Event, only: Option<usize>, data: *const core::ffi::c_void) {
        // The world followers act on these three events alone.
        if only.is_none()
            && matches!(
                event,
                Event::WORLD_LOADED | Event::WORLD_SAVED | Event::SHUTDOWN_STARTED
            )
        {
            crate::game_settings::world_event(self, event);
            crate::knowledge::changes::world_event(self, event);
            crate::item_data::world_event(self, event);
            crate::stats::world_event(self, event);
            crate::data_changes::world_event(self, event);
            crate::rate_links::world_event(event);
        }
        let Some(delivery) = self.delivery.get() else {
            return;
        };
        let late = crate::reload::late();
        let late = late.as_ref().map_or(&[][..], |late| late.as_slice());
        for subscription in of_event(delivery, event).iter().chain(late).filter(|s| {
            s.event == event
                && only.is_none_or(|only| only == s.owner.get())
                && self
                    .mods
                    .get(s.owner.get())
                    .is_some_and(|m| !m.off.load(Ordering::Acquire))
                && s.generation == crate::reload::generation(s.owner.get())
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
    ) -> impl Iterator<Item = &'static enshwick::abi::BuildRow> + '_ {
        let manifest = self.mods.get(index).map(|record| record.manifest);
        let rows: &'static [enshwick::abi::BuildRow] = match manifest {
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
            // SAFETY: a manifest's build row points at `symbol_count` symbols in
            // the mod's statics, and the mod's DLL stays loaded for the process.
            let symbols = unsafe { slice(row.symbols, row.symbol_count) };
            find(symbols, name)
        });
        own.or_else(|| self.build.symbol(name))
            .map(|rva| self.base + rva)
    }

    fn resolve_constant(&self, index: usize, name: &str) -> Option<u64> {
        let own = self.own_rows(index).find_map(|row| {
            // SAFETY: a manifest's build row points at `constant_count`
            // constants in the mod's statics, and the mod's DLL stays loaded for
            // the process.
            let constants = unsafe { slice(row.constants, row.constant_count) };
            find(constants, name)
        });
        own.or_else(|| self.build.constant(name))
            .or_else(|| crate::catalogue::constant(name, &self.reflection))
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
    retired_command,
    chat,
    set_label,
    rename_filter,
    join_missing,
    hook_at,
    inventory_open,
    item_info,
    inventory_submit,
    status,
    run_console,
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
    language,
    keep_game_settings,
    forget_game_settings,
    revoke_knowledge,
    account_knowledge,
    resource_read,
    game_text,
    rebuild_resource,
    recipe_rule,
    recipe_disabled,
    define_point,
    attach_point,
    run_point,
    actions,
    event_types,
    recipe_kind,
    feature,
    rate,
    set_rate,
    change_data,
    loop_wait,
    server_wait,
    task_workers,
    altar_zone_filter,
    altar_zones_changed,
};

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
fn calling(me: ModHandle) -> Result<(&'static Loader, ModIndex, &'static ModRecord), Status> {
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
fn caller_index(me: ModHandle) -> Result<(&'static Loader, ModIndex), Status> {
    calling(me).map(|(loader, index, _)| (loader, index))
}

/// The handle of the mod at `index`.
#[must_use]
pub(crate) fn handle(index: ModIndex) -> ModHandle {
    index.handle()
}

impl core::ops::Index<ModIndex> for [ModRecord] {
    type Output = ModRecord;

    fn index(&self, index: ModIndex) -> &ModRecord {
        &self[index.get()]
    }
}

impl core::ops::Index<ModIndex> for Vec<ModRecord> {
    type Output = ModRecord;

    fn index(&self, index: ModIndex) -> &ModRecord {
        &self[index.get()]
    }
}

#[cfg(test)]
mod headed_tests;

/// The subscriptions to `event` among `sealed`, which are sorted by event.
fn of_event(sealed: &[Subscription], event: Event) -> &[Subscription] {
    let first = sealed.partition_point(|s| s.event.0 < event.0);
    let end = sealed.partition_point(|s| s.event.0 <= event.0);
    sealed.get(first..end).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::join::{SHOWN_VALUES, shown};
    use super::*;

    #[test]
    fn a_raise_walks_its_event_s_subscriptions_in_their_order() {
        unsafe extern "system" fn handler(
            _: Event,
            _: *const core::ffi::c_void,
            _: *mut core::ffi::c_void,
        ) {
        }
        let made = |owner, event| Subscription {
            owner,
            event,
            callback: handler,
            user: 0,
            generation: 0,
        };
        let mut sealed = vec![
            made(ModIndex::new(2), Event::TICKED),
            made(ModIndex::new(0), Event::WORLD_SAVED),
            made(ModIndex::new(1), Event::TICKED),
            made(ModIndex::new(0), Event::TICKED),
            made(ModIndex::new(1), Event::SETTINGS_CHANGED),
        ];
        sealed.sort_by_key(|s| s.event.0);
        let owners = |event| {
            of_event(&sealed, event)
                .iter()
                .map(|s| s.owner)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            owners(Event::TICKED),
            [ModIndex::new(2), ModIndex::new(1), ModIndex::new(0)]
        );
        assert_eq!(owners(Event::WORLD_SAVED), [ModIndex::new(0)]);
        assert_eq!(owners(Event::SETTINGS_CHANGED), [ModIndex::new(1)]);
        assert!(owners(Event::SHUTDOWN_STARTED).is_empty());
    }

    fn read(text: Str) -> Option<&'static str> {
        // SAFETY: the tests read texts of stores that live to their end.
        unsafe { text.as_str() }
    }

    #[test]
    fn a_server_s_join_values_stay_on_one_log_line() {
        assert_eq!(shown("{ a = \"Сейф\" }"), "{ a = \"Сейф\" }");
        assert_eq!(
            shown("{ a = 1 }\n12:00:00.000Z ERROR [enshwick] forged"),
            "{ a = 1 }\\n12:00:00.000Z ERROR [enshwick] forged"
        );
        let long = shown(&"x".repeat(SHOWN_VALUES * 4));
        assert_eq!(long.len(), SHOWN_VALUES + 3);
    }

    #[test]
    fn a_poisoned_lock_is_taken_over_with_its_state() {
        let mutex = std::sync::Arc::new(Mutex::new(vec![1]));
        let held = std::sync::Arc::clone(&mutex);
        let panicked = std::thread::spawn(move || {
            let mut guard = held.lock().unwrap();
            guard.push(2);
            panic!("poisons the lock");
        })
        .join();
        assert!(panicked.is_err() && mutex.is_poisoned());
        mutex.relock().push(3);
        assert!(!mutex.is_poisoned());
        assert_eq!(*mutex.relock(), [1, 2, 3]);
    }

    #[test]
    fn a_kept_text_outlives_newer_ones_and_an_equal_text_is_kept_once() {
        let kept = Kept::default();
        let first = kept.keep("limit = 1");
        for n in 2..100 {
            kept.keep(&format!("limit = {n}"));
        }
        let again = kept.keep("limit = 1");
        assert_eq!(read(first), Some("limit = 1"));
        assert_eq!(read(again), read(first));
        assert_eq!(kept.0.lock().unwrap().len(), 99);
    }

    #[test]
    fn texts_kept_on_other_threads_leave_earlier_ones_readable() {
        let kept = std::sync::Arc::new(Kept::default());
        let first = kept.keep("a = 0");
        let writers: Vec<_> = (0..4)
            .map(|t| {
                let kept = std::sync::Arc::clone(&kept);
                std::thread::spawn(move || {
                    for n in 0..200 {
                        kept.keep(&format!("a = {t}{n}"));
                    }
                })
            })
            .collect();
        for _ in 0..200 {
            assert_eq!(read(first), Some("a = 0"));
        }
        for writer in writers {
            writer.join().unwrap();
        }
        assert_eq!(read(first), Some("a = 0"));
    }
}
