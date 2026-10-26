//! The loader's table, answering from the installed fake.

use core::ffi::c_void;
use core::sync::atomic::Ordering;
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::abi::{
    ActiveBuff, AltarInfo, AltarLimitFn, AltarZoneSize, BedCheckFn, BuffRequest, BuffTypeInfo,
    BuildCheckFn, BuildInfo, CallerFrame, ComfortFn, ConfigCheckFn, CraftCheckFn, DamageFilterFn,
    DaytimeInfo, EntityPlace, Event, EventFn, EventSpec, ExperiencePassFn, GameEventFn, Header,
    HookSpec, HostApi, InteractionFn, InventoryRequest, InventoryResultFn, ItemData, KnowledgeInfo,
    KnowledgeInputFn, Level, MidHookFn, MidHookSpec, MigrationSet, ModEventFn, ModHandle, ModInfo,
    NetMessageFn, PlayerInfo, RenameCallFn, ResourceKey, ResourcePatchFn, ServiceFound,
    ServiceSpec, Side, SkillCheckFn, SpawnRequest, Status, StatusFn, Str, SummonFn,
    SurvivorLookupFn, SurvivorStatusFn, TeleportRequest, TerrainEdit, TerrainMaterialInfo,
    UiAnswerFn, UiDialog, UiFrameFn, UiMarker, UiWidget, WeatherFront, ZoneCheckFn,
};

use super::{FakeHost, LinkedHook, ServedResource, SubmittedRequest};

/// A submitted inventory request and where its result goes.
pub(crate) struct OpenRequest {
    pub(crate) request: SubmittedRequest,
    pub(crate) done: InventoryResultFn,
    pub(crate) user: usize,
}

/// A mod event handler: name, layout, handler and the mod's pointer.
pub(crate) type ModEventSubscription = (String, Vec<(String, u64)>, ModEventFn, usize);

/// What the installed fake answers and what it has recorded.
pub(crate) struct State {
    pub(crate) host: FakeHost,
    pub(crate) logs: Vec<(Level, String)>,
    pub(crate) faults: Vec<String>,
    pub(crate) hooks: Vec<LinkedHook>,
    /// Mid-function hooks: address, handler and the mod's pointer.
    pub(crate) mid_hooks: Vec<(u64, MidHookFn, usize)>,
    pub(crate) subscriptions: Vec<(Event, EventFn, usize)>,
    /// Game event handlers: type hash (0 for every type), handler, pointer.
    pub(crate) game_events: Vec<(u32, GameEventFn, usize)>,
    pub(crate) served: Vec<(ServedResource, ResourcePatchFn, usize)>,
    /// Every rebuild of a served resource the mod asked for, at init and
    /// after.
    pub(crate) rebuilds: Vec<ResourceKey>,
    /// The mod's recipe rules in force: kind and name.
    pub(crate) recipe_rules: Vec<(crate::abi::RecipeRuleKind, String)>,
    pub(crate) texts: Vec<(u32, u32, String)>,
    pub(crate) code_patches: Vec<(u64, Vec<u8>, Vec<u8>)>,
    pub(crate) published: Vec<super::FakeService>,
    /// Mod events the mod defined: name and layout, by id.
    pub(crate) defined_events: Vec<(String, Vec<(String, u64)>)>,
    /// Mod event handlers: name, layout, handler, pointer.
    pub(crate) mod_event_subscriptions: Vec<ModEventSubscription>,
    /// The names of the mod events the mod sent, in order.
    pub(crate) sent_events: Vec<String>,
    /// The migrations the mod declared.
    pub(crate) migrations: Option<super::FakeMigrations>,
    /// Content ids the mod declared, in order.
    pub(crate) content_ids: Vec<String>,
    pub(crate) defaults: Vec<String>,
    pub(crate) synced: Vec<String>,
    /// The mod's player actions callback, once registered.
    pub(crate) actions: Option<(crate::abi::ActionFn, usize)>,
    pub(crate) chat: Vec<(u64, String)>,
    pub(crate) labels: Vec<(u32, String)>,
    pub(crate) rename_filters: Vec<(RenameCallFn, usize)>,
    pub(crate) inventory_requests: Vec<OpenRequest>,
    pub(crate) status_sources: Vec<(StatusFn, usize)>,
    /// The mod's features as it last reported them: name and, when off,
    /// the reason.
    pub(crate) features: Vec<(String, Option<String>)>,
    /// The lines the mod ran on the console, in order.
    pub(crate) console: Vec<String>,
    /// The entities the mod asked to destroy, in order.
    pub(crate) destroyed: Vec<u32>,
    /// How many saves the mod asked for.
    pub(crate) saves: usize,
    /// The components the mod asked to write, `(entity, type hash)`.
    pub(crate) writes: Vec<(u32, u32)>,
    /// The game events the mod raised: type hash and bytes.
    pub(crate) emits: Vec<(u32, Vec<u8>)>,
    /// The teleports the mod asked for.
    pub(crate) teleports: Vec<TeleportRequest>,
    /// The spawns the mod asked for, with the ids handed out.
    pub(crate) spawns: Vec<(u32, SpawnRequest)>,
    pub(crate) craft_filters: Vec<(CraftCheckFn, usize)>,
    /// Channels the mod opened: full name, id, handler and pointer.
    pub(crate) channels: Vec<(String, u32, NetMessageFn, usize)>,
    /// Messages the mod sent: channel id, raw peer and bytes.
    pub(crate) net_sent: Vec<(u32, u64, Vec<u8>)>,
    /// Notices the mod showed: account key and text.
    pub(crate) notices: Vec<(u64, String)>,
    pub(crate) build_filters: Vec<(BuildCheckFn, usize)>,
    /// The clock `time_of_day` writes, settable while installed.
    pub(crate) daytime: Option<DaytimeInfo>,
    /// The mod's factor on the clock's speed, once it asked to change it.
    pub(crate) clock_speed: Option<f32>,
    /// The game settings' bytes and preset, settable while installed.
    pub(crate) game_settings: Option<(Vec<u8>, u32)>,
    /// The game settings' bytes the mod kept with the world, by offset.
    pub(crate) kept_settings: Vec<Option<u8>>,
    pub(crate) damage_filters: Vec<(DamageFilterFn, usize)>,
    pub(crate) knowledge_input_filters: Vec<(KnowledgeInputFn, usize)>,
    /// The buff types the fake knows.
    pub(crate) buff_types: Vec<BuffTypeInfo>,
    /// The buffs of each entity that holds buffs.
    pub(crate) buffs: std::collections::HashMap<u32, Vec<ActiveBuff>>,
    /// Knowledge values by `(character or 0, id)`.
    pub(crate) knowledge: Vec<(u32, KnowledgeInfo)>,
    /// The grants the mod asked for: character, id, kind, value.
    pub(crate) grants: Vec<(u32, u32, u32, u32)>,
    /// The revokes the mod asked for: character, id, kind, value.
    pub(crate) revokes: Vec<(u32, u32, u32, u32)>,
    /// The changes the mod asked for accounts: key, id, kind, value,
    /// whether a revoke.
    pub(crate) account_changes: Vec<(u64, u32, u32, u32, bool)>,
    /// The quest ids the fake knows.
    pub(crate) quests: Vec<u32>,
    /// The weather weights everywhere and the fronts, settable.
    pub(crate) weather: Option<([f32; 4], Vec<WeatherFront>)>,
    /// The weather the mod started: effect and duration.
    pub(crate) forecasts: Vec<(u32, i64)>,
    /// The terrain materials, once a world runs.
    pub(crate) terrain: Option<Vec<TerrainMaterialInfo>>,
    /// The terrain edits the mod asked for.
    pub(crate) terrain_edits: Vec<TerrainEdit>,
    pub(crate) config_schema: Option<String>,
    /// Frame callbacks: callback and pointer.
    pub(crate) frames: Vec<(UiFrameFn, usize)>,
    /// HUD callbacks: callback and pointer.
    pub(crate) huds: Vec<(UiFrameFn, usize)>,
    /// What the HUD drawing now drew, with each item's anchor, while one
    /// draws.
    pub(crate) hud: Option<(u64, Vec<(u32, super::Drawn)>)>,
    /// The mod's markers on the map.
    pub(crate) markers: Vec<super::FakeMarker>,
    /// The mod's check of new settings texts.
    pub(crate) config_check: Option<(ConfigCheckFn, usize)>,
    /// The texts the mod handed the loader last, as TOML.
    pub(crate) published_texts: Option<String>,
    /// The mod's answer to an update pick.
    /// The frame drawing now: its token, whether a window is open and what
    /// was drawn.
    pub(crate) frame: Option<(u64, bool, Vec<super::Drawn>)>,
    /// Inputs waiting for the widget with their text.
    pub(crate) inputs: Vec<(String, super::Input)>,
    /// Dialogs waiting for an answer, with where it goes.
    pub(crate) dialogs: Vec<(super::ShownDialog, UiAnswerFn, usize)>,
    /// The world key last handed out, alive until the next one.
    world_key: Option<Box<str>>,
    /// The altars `altars` lists, settable while installed.
    pub(crate) altars: Vec<AltarInfo>,
    /// Whether the mod opened altars.
    pub(crate) altars_opened: bool,
    /// Whether the mod sizes the altar zones.
    pub(crate) zone_control: bool,
    /// The zone sizes the mod set last.
    pub(crate) zone_sizes: Vec<AltarZoneSize>,
    pub(crate) zone_filters: Vec<(ZoneCheckFn, usize)>,
    pub(crate) survivor_lookup: Option<(SurvivorLookupFn, usize)>,
    pub(crate) bed_checks: Vec<(BedCheckFn, usize)>,
    pub(crate) summons: Vec<(SummonFn, usize)>,
    pub(crate) summoned: Vec<(SummonFn, usize)>,
    /// Item data kinds by full id, and values by kind and entity: the item
    /// and the bytes.
    pub(crate) item_kinds: Vec<String>,
    /// Content served or put in the fake archive, by hash.
    pub(crate) content: std::collections::BTreeMap<[u32; 4], Vec<u8>>,
    pub(crate) item_values: std::collections::BTreeMap<(u32, u32), (u32, Vec<u8>)>,
    pub(crate) summon_items: Vec<(u32, u32)>,
    /// Stat kinds, modifiers and worn items.
    pub(crate) stats: super::stats::FakeStats,
    /// Extension points the mod defined and links it attached.
    pub(crate) points: super::points::FakePoints,
    pub(crate) survivor_status: Option<(SurvivorStatusFn, usize)>,
    pub(crate) interaction_filters: Vec<(InteractionFn, usize)>,
    pub(crate) skills: Vec<(u32, SkillCheckFn, usize)>,
    pub(crate) comfort_filters: Vec<(ComfortFn, usize)>,
    pub(crate) experience_filters: Vec<(ExperiencePassFn, usize)>,
    pub(crate) altar_limit: Option<(AltarLimitFn, usize)>,
    pub(crate) base_limit_raised: bool,
    pub(crate) grouping: Option<(u32, u32)>,
    /// The answer `run_console` last handed out, alive until the next call.
    run_out: Option<Box<str>>,
    /// The settings texts handed out, alive while the fake is installed.
    settings: Vec<Box<str>>,
    /// The join values handed out, alive while the fake is installed.
    sync_out: Vec<Box<str>>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);
/// Held by the installed fake, so one test at a time owns the table.
static TURN: Mutex<()> = Mutex::new(());

fn state() -> MutexGuard<'static, Option<State>> {
    STATE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Waits for the table to be free, then answers from `host`.
pub(crate) fn install(host: FakeHost) -> MutexGuard<'static, ()> {
    let turn = TURN.lock().unwrap_or_else(PoisonError::into_inner);
    *state() = Some(State {
        host,
        logs: Vec::new(),
        faults: Vec::new(),
        hooks: Vec::new(),
        mid_hooks: Vec::new(),
        subscriptions: Vec::new(),
        game_events: Vec::new(),
        served: Vec::new(),
        rebuilds: Vec::new(),
        recipe_rules: Vec::new(),
        texts: Vec::new(),
        code_patches: Vec::new(),
        published: Vec::new(),
        defined_events: Vec::new(),
        mod_event_subscriptions: Vec::new(),
        sent_events: Vec::new(),
        migrations: None,
        content_ids: Vec::new(),
        defaults: Vec::new(),
        synced: Vec::new(),
        inventory_requests: Vec::new(),
        sync_out: Vec::new(),
        actions: None,
        chat: Vec::new(),
        labels: Vec::new(),
        rename_filters: Vec::new(),
        status_sources: Vec::new(),
        features: Vec::new(),
        console: Vec::new(),
        destroyed: Vec::new(),
        saves: 0,
        writes: Vec::new(),
        emits: Vec::new(),
        teleports: Vec::new(),
        spawns: Vec::new(),
        craft_filters: Vec::new(),
        daytime: None,
        clock_speed: None,
        game_settings: None,
        kept_settings: Vec::new(),
        damage_filters: Vec::new(),
        knowledge_input_filters: Vec::new(),
        buff_types: Vec::new(),
        buffs: std::collections::HashMap::new(),
        knowledge: Vec::new(),
        grants: Vec::new(),
        revokes: Vec::new(),
        account_changes: Vec::new(),
        quests: Vec::new(),
        weather: None,
        forecasts: Vec::new(),
        terrain: None,
        terrain_edits: Vec::new(),
        config_schema: None,
        channels: Vec::new(),
        net_sent: Vec::new(),
        notices: Vec::new(),
        frames: Vec::new(),
        huds: Vec::new(),
        hud: None,
        markers: Vec::new(),
        config_check: None,
        published_texts: None,
        frame: None,
        inputs: Vec::new(),
        dialogs: Vec::new(),
        build_filters: Vec::new(),
        world_key: None,
        altars: Vec::new(),
        altars_opened: false,
        zone_control: false,
        zone_sizes: Vec::new(),
        zone_filters: Vec::new(),
        survivor_lookup: None,
        bed_checks: Vec::new(),
        summons: Vec::new(),
        summoned: Vec::new(),
        item_kinds: Vec::new(),
        content: std::collections::BTreeMap::new(),
        item_values: std::collections::BTreeMap::new(),
        summon_items: Vec::new(),
        stats: super::stats::FakeStats::default(),
        points: super::points::FakePoints::default(),
        survivor_status: None,
        interaction_filters: Vec::new(),
        skills: Vec::new(),
        comfort_filters: Vec::new(),
        experience_filters: Vec::new(),
        altar_limit: None,
        base_limit_raised: false,
        grouping: None,
        run_out: None,
        settings: Vec::new(),
    });
    turn
}

pub(crate) fn uninstall() {
    *state() = None;
}

/// `f` on the installed fake's state, or `default` when none is installed.
pub(crate) fn with<R>(default: R, f: impl FnOnce(&mut State) -> R) -> R {
    state().as_mut().map_or(default, f)
}

/// Writes `value` through `out`, which the safe layer passes valid or null.
pub(crate) fn put<T>(out: *mut T, value: T) -> Status {
    // SAFETY: the safe layer passes a valid out pointer; `as_mut` turns away null.
    match unsafe { out.as_mut() } {
        Some(slot) => {
            *slot = value;
            Status::OK
        }
        None => Status::BAD_ARGS,
    }
}

fn found<T>(value: Option<T>, out: *mut T) -> Status {
    value.map_or(Status::MISSING, |value| put(out, value))
}

fn text(s: Str) -> String {
    // SAFETY: strings are borrowed for the call.
    unsafe { s.as_str() }.map_or_else(|| "<not UTF-8>".to_owned(), str::to_owned)
}

extern "system" fn build(out: *mut BuildInfo) {
    let host = with(FakeHost::new(), |s| s.host.clone());
    let _written = put(
        out,
        BuildInfo {
            header: Header::of::<BuildInfo>(),
            side: host.side,
            base: host.base,
            fingerprint: host.fingerprint,
            table: host.table,
        },
    );
}

extern "system" fn symbol(_me: ModHandle, name: Str, out: *mut u64) -> Status {
    let name = text(name);
    found(with(None, |s| s.host.symbols.get(&name).copied()), out)
}

extern "system" fn system(name: Str, out: *mut u64) -> Status {
    let name = text(name);
    found(with(None, |s| s.host.systems.get(&name).copied()), out)
}

extern "system" fn field_offset(type_name: Str, field: Str, out: *mut u64) -> Status {
    let key = (text(type_name), text(field));
    found(with(None, |s| s.host.field_offsets.get(&key).copied()), out)
}

extern "system" fn type_size(type_name: Str, out: *mut u32) -> Status {
    let name = text(type_name);
    found(with(None, |s| s.host.type_sizes.get(&name).copied()), out)
}

extern "system" fn hook(_me: ModHandle, spec: *const HookSpec) -> Status {
    // SAFETY: the safe layer passes a valid spec for the call; `as_ref` turns away null.
    let Some(spec) = (unsafe { spec.as_ref() }) else {
        return Status::BAD_ARGS;
    };
    let linked = with(Status::MISSING, |s| {
        if s.host.refused_hooks.contains(&spec.target) {
            return Status::REFUSED;
        }
        s.hooks.push(LinkedHook {
            target: spec.target,
            detour: spec.detour as u64,
            priority: spec.priority,
            flags: spec.flags,
        });
        Status::OK
    });
    if linked.is_ok() {
        // The chain is armed at once, and its rest is the target itself.
        // SAFETY: `next` points at the hook's static slot; `as_ref` turns away null.
        if let Some(next) = unsafe { spec.next.as_ref() } {
            next.store(usize::try_from(spec.target).unwrap_or(0), Ordering::Release);
        }
    }
    linked
}

extern "system" fn hook_at(_me: ModHandle, spec: *const MidHookSpec) -> Status {
    // SAFETY: the safe layer passes a valid spec for the call; `as_ref` turns away null.
    let Some(spec) = (unsafe { spec.as_ref() }) else {
        return Status::BAD_ARGS;
    };
    with(Status::MISSING, |s| {
        if s.host.refused_hooks.contains(&spec.address) {
            return Status::REFUSED;
        }
        s.mid_hooks
            .push((spec.address, spec.handler, spec.user.expose_provenance()));
        Status::OK
    })
}

extern "system" fn caller_frame(out: *mut CallerFrame) {
    let frame = with(CallerFrame::default(), |s| s.host.caller_frame);
    let _written = put(out, frame);
}

extern "system" fn fault(_me: ModHandle, reason: Str) {
    let reason = text(reason);
    with((), |s| s.faults.push(reason));
}

extern "system" fn log(_me: ModHandle, level: Level, message: Str) {
    let message = text(message);
    with((), |s| s.logs.push((level, message)));
}

extern "system" fn settings(_me: ModHandle, defaults: Str, out: *mut Str) -> Status {
    let defaults = text(defaults);
    let served = with(None, |s| {
        let served: Box<str> = s
            .host
            .settings
            .clone()
            .unwrap_or_else(|| defaults.clone())
            .into();
        s.defaults.push(defaults);
        // The heap text stays where it is while the box lives in the state.
        let view = Str::new(&served);
        s.settings.push(served);
        Some(view)
    });
    found(served, out)
}

/// The `(name, hash)` fields of a layout the safe layer passed.
fn layout_of(layout: &crate::abi::Layout) -> Option<Vec<(String, u64)>> {
    // SAFETY: the safe layer passes the layout of a declared type.
    unsafe { crate::services::read_layout(layout) }
}

/// Borrowed `(name, hash)` pairs.
pub(crate) fn view(fields: &[(String, u64)]) -> Vec<(&str, u64)> {
    fields.iter().map(|(n, h)| (n.as_str(), *h)).collect()
}

extern "system" fn publish(_me: ModHandle, spec: *const ServiceSpec) -> Status {
    // SAFETY: the safe layer passes a valid spec.
    let Some(spec) = (unsafe { spec.as_ref() }) else {
        return Status::BAD_ARGS;
    };
    let (name, Some(fields)) = (text(spec.name), layout_of(&spec.layout)) else {
        return Status::BAD_ARGS;
    };
    with(Status::MISSING, |s| {
        let taken = s
            .published
            .iter()
            .any(|p| p.name == name && p.major == spec.major)
            || s.host
                .services
                .iter()
                .any(|p| p.name == name && p.major == spec.major);
        if taken {
            return Status::REFUSED;
        }
        s.published.push(super::FakeService {
            name,
            major: spec.major,
            fields,
            table: spec.table.expose_provenance(),
        });
        Status::OK
    })
}

extern "system" fn service(
    _me: ModHandle,
    wanted: *const ServiceSpec,
    out: *mut ServiceFound,
) -> Status {
    // SAFETY: the safe layer passes a valid spec.
    let Some(wanted) = (unsafe { wanted.as_ref() }) else {
        return Status::BAD_ARGS;
    };
    let (name, Some(fields)) = (text(wanted.name), layout_of(&wanted.layout)) else {
        return Status::BAD_ARGS;
    };
    let answer = with(Err("no fake installed".to_owned()), |s| {
        let offered = s
            .host
            .services
            .iter()
            .chain(&s.published)
            .find(|p| p.name == name && p.major == wanted.major)
            .ok_or_else(|| "no running mod publishes it".to_owned())?;
        crate::services::compare(&view(&fields), &view(&offered.fields))
            .map_err(|e| e.to_string())?;
        Ok((offered.table, offered.fields.len()))
    });
    let mut found = ServiceFound::EMPTY;
    let status = match answer {
        Ok((table, count)) => {
            found.table = core::ptr::with_exposed_provenance(table);
            found.fields = u32::try_from(count).unwrap_or(u32::MAX);
            found.owner = Str::new(super::MOD_ID);
            Status::OK
        }
        Err(reason) => {
            found.reason = Str::new(Box::leak(reason.into_boxed_str()));
            Status::REFUSED
        }
    };
    match put(out, found) {
        Status::OK => status,
        other => other,
    }
}

extern "system" fn define_event(_me: ModHandle, spec: *const EventSpec, out: *mut u32) -> Status {
    // SAFETY: the safe layer passes a valid spec.
    let Some(spec) = (unsafe { spec.as_ref() }) else {
        return Status::BAD_ARGS;
    };
    let (name, Some(fields)) = (text(spec.name), layout_of(&spec.layout)) else {
        return Status::BAD_ARGS;
    };
    let id = with(None, |s| {
        if s.defined_events.iter().any(|d| d.0 == name) {
            return None;
        }
        s.defined_events.push((name, fields));
        u32::try_from(s.defined_events.len() - 1).ok()
    });
    match id {
        Some(id) => put(out, id),
        None => Status::REFUSED,
    }
}

extern "system" fn subscribe_event(
    _me: ModHandle,
    spec: *const EventSpec,
    callback: ModEventFn,
    user: *mut c_void,
) -> Status {
    // SAFETY: the safe layer passes a valid spec.
    let Some(spec) = (unsafe { spec.as_ref() }) else {
        return Status::BAD_ARGS;
    };
    let (name, Some(fields)) = (text(spec.name), layout_of(&spec.layout)) else {
        return Status::BAD_ARGS;
    };
    with(Status::MISSING, |s| {
        s.mod_event_subscriptions
            .push((name, fields, callback, user.expose_provenance()));
        Status::OK
    })
}

extern "system" fn send_event(_me: ModHandle, event: u32, _data: *const c_void) -> Status {
    with(Status::MISSING, |s| {
        let Some(name) = s.defined_events.get(event as usize).map(|d| d.0.clone()) else {
            return Status::BAD_ARGS;
        };
        s.sent_events.push(name);
        Status::OK
    })
}

extern "system" fn declare_content(_me: ModHandle, id: Str, out: *mut [u8; 16]) -> Status {
    let id = text(id);
    let Some(guid) = crate::content::Guid::try_named(&id) else {
        return Status::BAD_ARGS;
    };
    with(Status::MISSING, |s| {
        let clash = s.content_ids.iter().any(|known| {
            *known != id
                && crate::content::Guid::try_named(known).is_some_and(|g| g.id() == guid.id())
        });
        if clash {
            return Status::REFUSED;
        }
        if !s.content_ids.contains(&id) {
            s.content_ids.push(id);
        }
        put(out, guid.0)
    })
}

extern "system" fn migrate(_me: ModHandle, set: *const MigrationSet) -> Status {
    // SAFETY: the safe layer passes a valid set.
    let Some(set) = (unsafe { set.as_ref() }) else {
        return Status::BAD_ARGS;
    };
    let Some(map) = set.map else {
        return Status::BAD_ARGS;
    };
    let mut migrations = Vec::new();
    for index in 0..set.count as usize {
        // SAFETY: the set holds `count` migrations.
        let migration = unsafe { set.migrations.add(index) };
        // SAFETY: `index` is below the set's `count`, so `migration` points at
        // one of its live, aligned entries.
        let migration = unsafe { &*migration };
        let steps = (0..migration.count as usize)
            .map(|i| {
                // SAFETY: the migration holds `count` steps.
                let step = unsafe { migration.steps.add(i) };
                // SAFETY: `i` is below the migration's `count`, so `step`
                // points at one of its live, aligned steps.
                let step = unsafe { &*step };
                (step.kind, text(step.from), text(step.to))
            })
            .collect();
        migrations.push((text(migration.id), steps));
    }
    with(Status::MISSING, |s| {
        if s.migrations.is_some() {
            return Status::REFUSED;
        }
        s.migrations = Some(super::FakeMigrations {
            list: migrations,
            map,
            user: set.user.expose_provenance(),
        });
        Status::OK
    })
}

extern "system" fn constant(_me: ModHandle, name: Str, out: *mut u64) -> Status {
    let name = text(name);
    found(with(None, |s| s.host.constants.get(&name).copied()), out)
}

extern "system" fn data_dir(_me: ModHandle, out: *mut Str) -> Status {
    let dir = with(None, |s| Some(s.host.data_dir.clone()));
    // The loader's path lives for the process; a test leaks a few bytes for it.
    found(
        dir.map(|dir| Str::new(Box::leak(dir.into_boxed_str()))),
        out,
    )
}

extern "system" fn log_level(_me: ModHandle) -> Level {
    with(Level::INFO, |s| s.host.log_level)
}

extern "system" fn subscribe(
    _me: ModHandle,
    event: Event,
    callback: EventFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.subscriptions
            .push((event, callback, user.expose_provenance()));
        Status::OK
    })
}

extern "system" fn type_hash(type_name: Str, out: *mut u32) -> Status {
    let name = text(type_name);
    found(with(None, |s| s.host.type_hashes.get(&name).copied()), out)
}

extern "system" fn component(
    _me: ModHandle,
    entity: u32,
    type_hash: u32,
    out: *mut *const c_void,
) -> Status {
    let address = with(None, |s| {
        s.host.components.get(&(entity, type_hash)).map(|c| c.0)
    });
    found(address.map(core::ptr::with_exposed_provenance), out)
}

extern "system" fn entity_template(_me: ModHandle, entity: u32, out: *mut [u8; 16]) -> Status {
    found(with(None, |s| s.host.templates.get(&entity).copied()), out)
}

extern "system" fn player(_me: ModHandle, entity: u32, out: *mut PlayerInfo) -> Status {
    let player = with(None, |s| {
        s.host.players.iter().find(|p| p.entity == entity).copied()
    });
    found(player, out)
}

extern "system" fn players(
    _me: ModHandle,
    out: *mut PlayerInfo,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let players = with(Vec::new(), |s| s.host.players.clone());
    let capacity = usize::try_from(capacity).unwrap_or(0);
    for (i, player) in players.iter().take(capacity).enumerate() {
        // SAFETY: the safe layer passes room for `capacity` players.
        unsafe { out.wrapping_add(i).write(*player) };
    }
    put(count, u32::try_from(players.len()).unwrap_or(u32::MAX))
}

extern "system" fn serve_resource(
    _me: ModHandle,
    key: *const ResourceKey,
    base: *const ResourceKey,
    patch: ResourcePatchFn,
    user: *mut c_void,
) -> Status {
    // SAFETY: the safe layer passes valid keys for the call; `as_ref` turns away null.
    let Some(key) = (unsafe { key.as_ref() }).copied() else {
        return Status::BAD_ARGS;
    };
    // SAFETY: the safe layer passes `base` null or pointing at a key valid
    // for the call; `as_ref` turns away null.
    let Some(base) = (unsafe { base.as_ref() }).copied() else {
        return Status::BAD_ARGS;
    };
    with(Status::MISSING, |s| {
        if s.served
            .iter()
            .any(|served| served.0.key == key && served.0.base != base)
        {
            return Status::REFUSED;
        }
        s.served.push((
            ServedResource { key, base },
            patch,
            user.expose_provenance(),
        ));
        Status::OK
    })
}

extern "system" fn rebuild_resource(_me: ModHandle, key: *const ResourceKey) -> Status {
    // SAFETY: the safe layer passes a valid key for the call; `as_ref` turns away null.
    let Some(key) = (unsafe { key.as_ref() }).copied() else {
        return Status::BAD_ARGS;
    };
    with(Status::MISSING, |s| {
        if !s.served.iter().any(|served| served.0.key == key) {
            return Status::BAD_ARGS;
        }
        s.rebuilds.push(key);
        Status::OK
    })
}

extern "system" fn recipe_rule(
    _me: ModHandle,
    kind: crate::abi::RecipeRuleKind,
    name: Str,
    disable: u32,
) -> Status {
    let name = text(name);
    with(Status::MISSING, |s| {
        s.recipe_rules.retain(|(k, n)| !(*k == kind && *n == name));
        if disable != 0 {
            s.recipe_rules.push((kind, name));
        }
        Status::OK
    })
}

extern "system" fn recipe_disabled(_me: ModHandle, recipe: u32, out: *mut u32) -> Status {
    let disabled = with(false, |s| s.host.disabled_recipes.contains(&recipe));
    if !out.is_null() {
        // SAFETY: the safe layer passes its out value for the call.
        unsafe { out.write(u32::from(disabled)) };
    }
    Status::OK
}

extern "system" fn add_text(_me: ModHandle, id: u32, language: u32, value: Str) -> Status {
    let value = text(value);
    with(Status::MISSING, |s| {
        s.texts.push((id, language, value));
        Status::OK
    })
}

extern "system" fn mods(
    _me: ModHandle,
    out: *mut ModInfo,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let mods = with(Vec::new(), |s| s.host.mods.clone());
    list(&mods, out, entry_size, capacity, count)
}

extern "system" fn join_missing(
    _me: ModHandle,
    out: *mut ModInfo,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let mods = with(Vec::new(), |s| s.host.join_missing.clone());
    list(&mods, out, entry_size, capacity, count)
}

extern "system" fn replaced(
    _me: ModHandle,
    out: *mut ModInfo,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let mods = with(Vec::new(), |s| s.host.replaced.clone());
    list(&mods, out, entry_size, capacity, count)
}

/// Copies up to `capacity` of `items` to `out`, `entry_size` bytes apart, and
/// writes how many there are.
fn list<T: Copy>(
    items: &[T],
    out: *mut T,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let stride = usize::try_from(entry_size).unwrap_or(0);
    let shared = stride.min(size_of::<T>());
    let capacity = usize::try_from(capacity).unwrap_or(0);
    if !out.is_null() {
        for (i, info) in items.iter().take(capacity).enumerate() {
            let entry = out.cast::<u8>().wrapping_add(i * stride);
            // SAFETY: the safe layer passes room for `capacity` entries of
            // `stride` bytes; the fake fills what both sides know.
            unsafe {
                core::ptr::copy_nonoverlapping(core::ptr::from_ref(info).cast(), entry, shared);
            }
        }
    }
    put(count, u32::try_from(items.len()).unwrap_or(u32::MAX))
}

extern "system" fn sync(_me: ModHandle, synced: Str, out: *mut Str) -> Status {
    let synced = text(synced);
    let served = with(Str::EMPTY, |s| {
        s.synced.push(synced);
        let served: Box<str> = s.host.server_synced.clone().unwrap_or_default().into();
        // The heap text stays where it is while the box lives in the state.
        let view = Str::new(&served);
        s.sync_out.push(served);
        view
    });
    put(out, served)
}

extern "system" fn code_range(start: *mut u64, len: *mut u64) -> Status {
    let (address, size) = with((0, 0), |s| s.host.code);
    if address == 0 {
        return Status::MISSING;
    }
    let status = put(start, address);
    if !status.is_ok() {
        return status;
    }
    put(len, size)
}

extern "system" fn patch_code(
    _me: ModHandle,
    address: u64,
    expected: *const u8,
    replacement: *const u8,
    len: usize,
) -> Status {
    if expected.is_null() || replacement.is_null() || len == 0 {
        return Status::BAD_ARGS;
    }
    // SAFETY: `expected` is non-null, checked at the top; the safe layer
    // passes `len` readable bytes there for the call.
    let expected = unsafe { core::slice::from_raw_parts(expected, len) }.to_vec();
    // SAFETY: `replacement` is non-null, checked at the top; the safe layer
    // passes `len` readable bytes there for the call.
    let replacement = unsafe { core::slice::from_raw_parts(replacement, len) }.to_vec();
    with(Status::MISSING, |s| {
        s.code_patches.push((address, expected, replacement));
        Status::OK
    })
}

extern "system" fn workshop_recipes() -> u32 {
    with(512, |s| s.host.workshop_recipes)
}

extern "system" fn hosting() -> u8 {
    with(0, |s| {
        u8::from(s.host.side == Side::SERVER || s.host.hosting)
    })
}

extern "system" fn server_world() -> u64 {
    with(0, |s| s.host.server_world)
}

extern "system" fn retired_command(_me: ModHandle) -> Status {
    Status::MISSING
}

extern "system" fn actions(
    _me: ModHandle,
    callback: crate::abi::ActionFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        if s.actions.is_some() {
            return Status::REFUSED;
        }
        s.actions = Some((callback, user.expose_provenance()));
        Status::OK
    })
}

extern "system" fn chat(_me: ModHandle, to: u64, value: Str) -> Status {
    let value = text(value);
    with(Status::MISSING, |s| {
        s.chat.push((to, value));
        Status::OK
    })
}

extern "system" fn set_label(_me: ModHandle, entity: u32, value: Str) -> Status {
    let value = text(value);
    with(Status::MISSING, |s| {
        s.labels.push((entity, value));
        Status::OK
    })
}

extern "system" fn rename_filter(
    _me: ModHandle,
    callback: RenameCallFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.rename_filters.push((callback, user.expose_provenance()));
        Status::OK
    })
}

extern "system" fn feature(_me: ModHandle, name: Str, off: Str) -> Status {
    let name = text(name);
    if name.is_empty() {
        return Status::BAD_ARGS;
    }
    let off = Some(text(off)).filter(|off| !off.is_empty());
    with(Status::MISSING, |s| {
        match s.features.iter_mut().find(|(have, _)| *have == name) {
            Some(kept) => kept.1 = off,
            None => s.features.push((name, off)),
        }
        Status::OK
    })
}

extern "system" fn status(_me: ModHandle, callback: StatusFn, user: *mut c_void) -> Status {
    with(Status::MISSING, |s| {
        s.status_sources.push((callback, user.expose_provenance()));
        Status::OK
    })
}

extern "system" fn run_console(_me: ModHandle, line: Str, out: *mut Str) -> Status {
    let line = text(line);
    let (word, rest) = line.split_once(' ').unwrap_or((&line, ""));
    let (word, rest) = (word.to_owned(), rest.trim().to_owned());
    with((), |s| s.console.push(line.clone()));
    let answer = match with(None, |s| s.host.console_answers.get(&word).cloned()) {
        Some(answer) => answer,
        // The mod's own actions, asked with the table unlocked: they may call
        // back.
        None => match with(None, |s| s.actions) {
            Some((callback, user)) => {
                let reply = super::ask(
                    callback,
                    user,
                    crate::abi::ActionCall::CONSOLE,
                    &super::console(),
                    0,
                    &rest,
                    "",
                );
                reply.get(1..).unwrap_or_default().to_owned()
            }
            None => return Status::MISSING,
        },
    };
    with(Status::MISSING, |s| {
        let kept = s.run_out.insert(answer.into_boxed_str());
        put(out, Str::new(kept))
    })
}

/// The table every installed fake answers through; a mod keeps the first
extern "system" fn inventory_open(_me: ModHandle) -> Status {
    with(Status::MISSING, |s| {
        if s.host.no_inventories {
            Status::MISSING
        } else {
            Status::OK
        }
    })
}

extern "system" fn item_info(_me: ModHandle, item: u32, out: *mut ItemData) -> Status {
    found(with(None, |s| s.host.items.get(&item).copied()), out)
}

extern "system" fn inventory_submit(
    _me: ModHandle,
    request: *const InventoryRequest,
    done: InventoryResultFn,
    user: *mut c_void,
) -> Status {
    // SAFETY: the safe layer passes a valid request for the call; `as_ref`
    // turns away null.
    let Some(request) = (unsafe { request.as_ref() }) else {
        return Status::BAD_ARGS;
    };
    if request.ops.is_null() || request.op_count == 0 {
        return Status::BAD_ARGS;
    }
    // SAFETY: the request holds `op_count` operations, read during the call.
    let ops = unsafe { core::slice::from_raw_parts(request.ops, request.op_count as usize) };
    with(Status::MISSING, |s| {
        if s.host.no_inventories {
            return Status::MISSING;
        }
        s.inventory_requests.push(OpenRequest {
            request: SubmittedRequest {
                ops: ops.to_vec(),
                flags: request.flags,
                actor: request.actor,
            },
            done,
            user: user.expose_provenance(),
        });
        Status::OK
    })
}

extern "system" fn altars_open(_me: ModHandle) -> Status {
    with(Status::MISSING, |s| {
        s.altars_opened = true;
        Status::OK
    })
}

extern "system" fn altars(
    _me: ModHandle,
    out: *mut AltarInfo,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let Some(altars) = with(None, |s| s.altars_opened.then(|| s.altars.clone())) else {
        return Status::REFUSED;
    };
    list(&altars, out, entry_size, capacity, count)
}

extern "system" fn altar_zones_control(_me: ModHandle) -> Status {
    with(Status::MISSING, |s| {
        s.altars_opened = true;
        s.zone_control = true;
        Status::OK
    })
}

extern "system" fn set_altar_zones(
    _me: ModHandle,
    sizes: *const AltarZoneSize,
    count: usize,
) -> Status {
    let sizes = if count == 0 || sizes.is_null() {
        Vec::new()
    } else {
        // SAFETY: the safe layer passes `count` sizes, valid for the call.
        unsafe { core::slice::from_raw_parts(sizes, count) }.to_vec()
    };
    with(Status::MISSING, |s| {
        if !s.zone_control {
            return Status::REFUSED;
        }
        s.zone_sizes = sizes;
        Status::OK
    })
}

extern "system" fn zone_check_filter(
    _me: ModHandle,
    callback: ZoneCheckFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.zone_filters.push((callback, user.expose_provenance()));
        Status::OK
    })
}

extern "system" fn survivor_lookup(
    _me: ModHandle,
    callback: SurvivorLookupFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        if s.survivor_lookup.is_some() {
            return Status::REFUSED;
        }
        s.survivor_lookup = Some((callback, user.expose_provenance()));
        Status::OK
    })
}

extern "system" fn survivor_bed_check(
    _me: ModHandle,
    callback: BedCheckFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.bed_checks.push((callback, user.expose_provenance()));
        Status::OK
    })
}

extern "system" fn summon_filter(_me: ModHandle, callback: SummonFn, user: *mut c_void) -> Status {
    with(Status::MISSING, |s| {
        s.summons.push((callback, user.expose_provenance()));
        Status::OK
    })
}

extern "system" fn summoned(_me: ModHandle, callback: SummonFn, user: *mut c_void) -> Status {
    with(Status::MISSING, |s| {
        s.summoned.push((callback, user.expose_provenance()));
        Status::OK
    })
}

extern "system" fn item_data_open(_me: ModHandle, id: Str, out: *mut u32) -> Status {
    let id = text(id);
    let full = if id.contains(':') {
        id
    } else {
        format!("{}:{id}", super::MOD_ID)
    };
    let kind = with(None, |s| {
        let index = s
            .item_kinds
            .iter()
            .position(|k| *k == full)
            .unwrap_or_else(|| {
                s.item_kinds.push(full.clone());
                s.item_kinds.len() - 1
            });
        u32::try_from(index).ok()
    });
    found(kind, out)
}

extern "system" fn entity_data_open(
    me: ModHandle,
    id: Str,
    _flags: crate::abi::EntityDataFlags,
    out: *mut u32,
) -> Status {
    item_data_open(me, id, out)
}

extern "system" fn item_data_get(
    _me: ModHandle,
    kind: u32,
    pide: u32,
    item: u32,
    out: *mut u8,
    capacity: u32,
    len: *mut u32,
) -> Status {
    let value = with(None, |s| {
        s.item_values
            .get(&(kind, pide))
            .filter(|(held, _)| *held == item)
            .map(|(_, bytes)| bytes.clone())
    });
    let Some(value) = value else {
        return Status::MISSING;
    };
    let size = u32::try_from(value.len()).unwrap_or(u32::MAX);
    if !len.is_null() {
        // SAFETY: `len` is non-null, checked just before; the safe layer
        // passes it pointing at a writable, aligned `u32` for the call.
        unsafe { len.write(size) };
    }
    if size > capacity || out.is_null() {
        return Status::BAD_ARGS;
    }
    // SAFETY: `out` is non-null and the copy is at most `capacity` bytes, both
    // checked just before; the safe layer passes `capacity` writable bytes at
    // `out`, apart from the fake's own buffer.
    unsafe { out.copy_from_nonoverlapping(value.as_ptr(), value.len()) };
    Status::OK
}

extern "system" fn item_data_set(
    _me: ModHandle,
    kind: u32,
    pide: u32,
    item: u32,
    data: *const u8,
    len: u32,
) -> Status {
    if pide == 0 {
        return Status::BAD_ARGS;
    }
    let value = if len == 0 || data.is_null() {
        Vec::new()
    } else {
        // SAFETY: the safe layer passes `len` bytes at `data`.
        unsafe { core::slice::from_raw_parts(data, len as usize) }.to_vec()
    };
    with(Status::MISSING, |s| {
        if value.is_empty() {
            s.item_values.remove(&(kind, pide));
        } else {
            s.item_values.insert((kind, pide), (item, value));
        }
        Status::OK
    })
}

extern "system" fn content_serve(
    _me: ModHandle,
    data: *const u8,
    len: u32,
    out: *mut [u32; 4],
) -> Status {
    if data.is_null() || len == 0 {
        return Status::BAD_ARGS;
    }
    // SAFETY: the safe layer passes `len` bytes at `data`.
    let bytes = unsafe { core::slice::from_raw_parts(data, len as usize) }.to_vec();
    let hash = [
        len,
        u32::try_from(bytes.len()).unwrap_or(0) ^ 0x5a5a,
        bytes.iter().map(|&b| u32::from(b)).sum(),
        1,
    ];
    with(Status::MISSING, |s| {
        s.content.insert(hash, bytes);
        Status::OK
    });
    found(Some(hash), out)
}

extern "system" fn content_read(
    _me: ModHandle,
    hash: *const [u32; 4],
    out: *mut u8,
    capacity: u32,
    len: *mut u32,
) -> Status {
    // SAFETY: the safe layer passes a hash for the call.
    let Some(hash) = (unsafe { hash.as_ref() }).copied() else {
        return Status::BAD_ARGS;
    };
    let Some(data) = with(None, |s| {
        s.content
            .get(&hash)
            .or_else(|| s.host.archive.get(&hash))
            .cloned()
    }) else {
        return Status::MISSING;
    };
    let size = u32::try_from(data.len()).unwrap_or(u32::MAX);
    if !len.is_null() {
        // SAFETY: `len` is non-null, checked just before; the safe layer
        // passes it pointing at a writable, aligned `u32` for the call.
        unsafe { len.write(size) };
    }
    if size > capacity || out.is_null() {
        return Status::BAD_ARGS;
    }
    // SAFETY: `out` is non-null and the copy is at most `capacity` bytes, both
    // checked just before; the safe layer passes `capacity` writable bytes at
    // `out`, apart from the fake's own buffer.
    unsafe { out.copy_from_nonoverlapping(data.as_ptr(), data.len()) };
    Status::OK
}

extern "system" fn icons_ready(_me: ModHandle) -> Status {
    if with(false, |s| s.host.icons_ready) {
        Status::OK
    } else {
        Status::MISSING
    }
}

extern "system" fn summon_item(_me: ModHandle, item: u32, survivor: u32) -> Status {
    with(Status::MISSING, |s| {
        s.summon_items.push((item, survivor));
        Status::OK
    })
}

extern "system" fn survivor_status(
    _me: ModHandle,
    callback: SurvivorStatusFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        if s.host.side != Side::CLIENT || s.survivor_status.is_some() {
            return Status::REFUSED;
        }
        s.survivor_status = Some((callback, user.expose_provenance()));
        Status::OK
    })
}

extern "system" fn interaction_filter(
    _me: ModHandle,
    callback: InteractionFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.interaction_filters
            .push((callback, user.expose_provenance()));
        Status::OK
    })
}

extern "system" fn skill_override(
    _me: ModHandle,
    node: u32,
    callback: SkillCheckFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.skills.push((node, callback, user.expose_provenance()));
        Status::OK
    })
}

extern "system" fn comfort_filter(
    _me: ModHandle,
    callback: ComfortFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.comfort_filters.push((callback, user.expose_provenance()));
        Status::OK
    })
}

extern "system" fn experience_filter(
    _me: ModHandle,
    callback: ExperiencePassFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.experience_filters
            .push((callback, user.expose_provenance()));
        Status::OK
    })
}

extern "system" fn altar_limit(
    _me: ModHandle,
    callback: AltarLimitFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        if s.altar_limit.is_some() {
            return Status::REFUSED;
        }
        s.altar_limit = Some((callback, user.expose_provenance()));
        Status::OK
    })
}

extern "system" fn raise_base_limit(_me: ModHandle) -> Status {
    with(Status::MISSING, |s| {
        s.base_limit_raised = true;
        Status::OK
    })
}

extern "system" fn base_zones(_me: ModHandle, zones: *mut u32, bases: *mut u32) -> Status {
    let Some((zone_count, base_count)) = with(None, |s| s.grouping) else {
        return Status::MISSING;
    };
    let status = put(zones, zone_count);
    if !status.is_ok() {
        return status;
    }
    put(bases, base_count)
}

extern "system" fn game_event(
    _me: ModHandle,
    type_hash: u32,
    callback: GameEventFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.game_events
            .push((type_hash, callback, user.expose_provenance()));
        Status::OK
    })
}

extern "system" fn entities(
    _me: ModHandle,
    type_hash: u32,
    out: *mut EntityPlace,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let found: Vec<EntityPlace> = with(Vec::new(), |s| {
        s.host
            .placed
            .iter()
            .filter(|(ty, _)| *ty == type_hash)
            .map(|(_, place)| *place)
            .collect()
    });
    for (i, place) in found.iter().take(capacity as usize).enumerate() {
        // SAFETY: the safe layer passes room for `capacity` entries.
        unsafe { out.wrapping_add(i).write(*place) };
    }
    put(count, u32::try_from(found.len()).unwrap_or(u32::MAX))
}

extern "system" fn destroy(_me: ModHandle, entity: u32) -> Status {
    with(Status::MISSING, |s| {
        if s.host.players.iter().any(|p| p.entity == entity) {
            return Status::REFUSED;
        }
        if !s.host.placed.iter().any(|(_, p)| p.entity == entity)
            && !s.host.templates.contains_key(&entity)
        {
            return Status::MISSING;
        }
        s.destroyed.push(entity);
        Status::OK
    })
}

extern "system" fn save_world(_me: ModHandle) -> Status {
    with(Status::MISSING, |s| {
        s.saves += 1;
        Status::OK
    })
}

extern "system" fn world_key(_me: ModHandle, out: *mut Str) -> Status {
    with(Status::MISSING, |s| {
        let Some(key) = s.host.world_key.clone() else {
            return Status::MISSING;
        };
        let kept = s.world_key.insert(key.into_boxed_str());
        put(out, Str::new(kept))
    })
}

extern "system" fn component_write(
    _me: ModHandle,
    entity: u32,
    type_hash: u32,
    out: *mut *mut c_void,
) -> Status {
    let address = with(Err(Status::MISSING), |s| {
        if s.host.refused_writes.contains(&type_hash) {
            return Err(Status::REFUSED);
        }
        let address = s
            .host
            .components
            .get(&(entity, type_hash))
            .map(|c| c.0)
            .ok_or(Status::MISSING)?;
        s.writes.push((entity, type_hash));
        Ok(address)
    });
    match address {
        Ok(address) => put(out, core::ptr::with_exposed_provenance_mut(address)),
        Err(status) => status,
    }
}

extern "system" fn emit(_me: ModHandle, type_hash: u32, data: *const u8, size: u32) -> Status {
    if data.is_null() {
        return Status::BAD_ARGS;
    }
    // SAFETY: the safe layer passes `size` bytes at `data` for the call.
    let bytes = unsafe { core::slice::from_raw_parts(data, size as usize) }.to_vec();
    with(Status::MISSING, |s| {
        s.emits.push((type_hash, bytes));
        Status::OK
    })
}

extern "system" fn teleport(_me: ModHandle, request: *const TeleportRequest) -> Status {
    // SAFETY: the safe layer passes a valid request for the call.
    let Some(request) = (unsafe { request.as_ref() }).copied() else {
        return Status::BAD_ARGS;
    };
    with(Status::MISSING, |s| {
        if !s.host.templates.contains_key(&request.entity)
            && !s
                .host
                .placed
                .iter()
                .any(|(_, p)| p.entity == request.entity)
        {
            return Status::MISSING;
        }
        s.teleports.push(request);
        Status::OK
    })
}

extern "system" fn craft_filter(
    _me: ModHandle,
    callback: CraftCheckFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.craft_filters.push((callback, user.expose_provenance()));
        Status::OK
    })
}

extern "system" fn build_filter(
    _me: ModHandle,
    callback: BuildCheckFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.build_filters.push((callback, user.expose_provenance()));
        Status::OK
    })
}

extern "system" fn net_channel(
    _me: ModHandle,
    name: Str,
    callback: NetMessageFn,
    user: *mut c_void,
    out: *mut u32,
) -> Status {
    let name = text(name);
    let id = with(None, |s| {
        if s.channels.iter().any(|c| c.0 == name) {
            return None;
        }
        let id = u32::try_from(s.channels.len() + 1).unwrap_or(u32::MAX);
        s.channels
            .push((name, id, callback, user.expose_provenance()));
        Some(id)
    });
    match id {
        Some(id) => put(out, id),
        None => Status::REFUSED,
    }
}

extern "system" fn net_send(
    _me: ModHandle,
    channel: u32,
    peer: u64,
    data: *const u8,
    size: u32,
) -> Status {
    let bytes = if size == 0 || data.is_null() {
        Vec::new()
    } else {
        // SAFETY: the safe layer passes `size` bytes at `data` for the call.
        unsafe { core::slice::from_raw_parts(data, size as usize) }.to_vec()
    };
    with(Status::MISSING, |s| {
        if peer != 0 && !s.host.net_peers.contains(&peer) {
            return Status::MISSING;
        }
        s.net_sent.push((channel, peer, bytes));
        Status::OK
    })
}

extern "system" fn net_ready(_me: ModHandle, peer: u64) -> Status {
    with(Status::MISSING, |s| {
        if s.host.net_peers.contains(&peer) {
            Status::OK
        } else {
            Status::MISSING
        }
    })
}

extern "system" fn notify(_me: ModHandle, key: u64, value: Str) -> Status {
    let value = text(value);
    with(Status::MISSING, |s| {
        if !s.host.net_peers.contains(&key) {
            return Status::MISSING;
        }
        s.notices.push((key, value));
        Status::OK
    })
}

/// The first entity id the fake hands out for a spawn.
const FIRST_SPAWNED: u32 = 0x10_0000;

extern "system" fn spawn(_me: ModHandle, request: *const SpawnRequest, out: *mut u32) -> Status {
    // SAFETY: the safe layer passes a valid request for the call.
    let Some(request) = (unsafe { request.as_ref() }).copied() else {
        return Status::BAD_ARGS;
    };
    if request.template == [0; 16] {
        return Status::BAD_ARGS;
    }
    let entity = with(None, |s| {
        let entity = FIRST_SPAWNED + u32::try_from(s.spawns.len()).unwrap_or(0);
        s.spawns.push((entity, request));
        Some(entity)
    });
    match entity {
        Some(entity) => put(out, entity),
        None => Status::MISSING,
    }
}

extern "system" fn time_of_day(_me: ModHandle, out: *mut DaytimeInfo) -> Status {
    let Some(info) = with(None, |s| s.daytime) else {
        return Status::MISSING;
    };
    // SAFETY: the safe layer passes a headed struct for the call.
    if unsafe { crate::abi::write_headed(out, info) } {
        Status::OK
    } else {
        Status::BAD_ARGS
    }
}

extern "system" fn set_time_of_day(_me: ModHandle, time: i64) -> Status {
    const DAY: i64 = 24 * 3_600_000_000_000;
    with(Status::MISSING, |s| match s.daytime.as_mut() {
        Some(info) => {
            info.time = time.rem_euclid(DAY);
            let day = (info.day_start..info.day_end).contains(&info.time);
            info.flags = if day { DaytimeInfo::DAY } else { 0 };
            Status::OK
        }
        None => Status::MISSING,
    })
}

extern "system" fn time_speed_control(_me: ModHandle) -> Status {
    with(Status::MISSING, |s| {
        s.clock_speed.get_or_insert(1.0);
        Status::OK
    })
}

extern "system" fn set_time_speed(_me: ModHandle, factor: f32) -> Status {
    if !(0.0..=1000.0).contains(&factor) {
        return Status::BAD_ARGS;
    }
    with(Status::MISSING, |s| match s.clock_speed.as_mut() {
        Some(speed) => {
            *speed = factor;
            Status::OK
        }
        None => Status::REFUSED,
    })
}

extern "system" fn game_settings(
    _me: ModHandle,
    out: *mut u8,
    size: u32,
    preset: *mut u32,
) -> Status {
    let Some((bytes, value)) = with(None, |s| s.game_settings.clone()) else {
        return Status::MISSING;
    };
    if out.is_null() || preset.is_null() || size as usize != bytes.len() {
        return Status::BAD_ARGS;
    }
    // SAFETY: `out` is non-null and `size` equals `bytes.len()`, both checked
    // just before; the safe layer passes `size` writable bytes at `out`, apart
    // from the fake's own copy.
    unsafe {
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len());
    };
    // SAFETY: `preset` is non-null, checked just before; the safe layer passes
    // it pointing at a writable, aligned `u32` for the call.
    unsafe { preset.write(value) };
    Status::OK
}

extern "system" fn set_game_settings(_me: ModHandle, data: *const u8, size: u32) -> Status {
    if data.is_null() {
        return Status::BAD_ARGS;
    }
    // SAFETY: `data` is non-null, checked just before; the safe layer passes
    // `size` readable bytes there for the call.
    let bytes = unsafe { core::slice::from_raw_parts(data, size as usize) }.to_vec();
    with(Status::MISSING, |s| match s.game_settings.as_mut() {
        Some(current) if current.0.len() == bytes.len() => {
            *current = (bytes, 4);
            Status::OK
        }
        Some(_) => Status::BAD_ARGS,
        None => Status::MISSING,
    })
}

extern "system" fn keep_game_settings(
    me: ModHandle,
    data: *const u8,
    mask: *const u8,
    size: u32,
) -> Status {
    if mask.is_null() {
        return Status::BAD_ARGS;
    }
    let status = set_game_settings(me, data, size);
    if status != Status::OK {
        return status;
    }
    // SAFETY: `data` is non-null, since `set_game_settings` refuses null and
    // returned `OK`; the safe layer passes `size` readable bytes there.
    let data = unsafe { core::slice::from_raw_parts(data, size as usize) };
    // SAFETY: `mask` is non-null, checked at the top; the safe layer passes
    // `size` readable bytes there for the call.
    let mask = unsafe { core::slice::from_raw_parts(mask, size as usize) };
    with(Status::MISSING, |s| {
        s.kept_settings.resize(data.len(), None);
        for ((kept, &byte), &set) in s.kept_settings.iter_mut().zip(data).zip(mask) {
            if set != 0 {
                *kept = Some(byte);
            }
        }
        Status::OK
    })
}

extern "system" fn forget_game_settings(_me: ModHandle) -> Status {
    with(Status::MISSING, |s| {
        s.kept_settings.clear();
        Status::OK
    })
}

extern "system" fn revoke_knowledge(
    _me: ModHandle,
    entity: u32,
    id: u32,
    kind: u32,
    value: u32,
) -> Status {
    with(Status::MISSING, |s| {
        s.revokes.push((entity, id, kind, value));
        Status::OK
    })
}

extern "system" fn account_knowledge(
    _me: ModHandle,
    key: u64,
    id: u32,
    kind: u32,
    value: u32,
    revoke: u32,
) -> Status {
    with(Status::MISSING, |s| {
        s.account_changes.push((key, id, kind, value, revoke != 0));
        Status::OK
    })
}

extern "system" fn resource_read(
    _me: ModHandle,
    key: *const ResourceKey,
    out: *mut u8,
    capacity: u32,
    len: *mut u32,
) -> Status {
    // SAFETY: the safe layer passes a key for the call.
    let Some(key) = (unsafe { key.as_ref() }).copied() else {
        return Status::BAD_ARGS;
    };
    let Some(data) = with(None, |s| {
        s.host
            .resources
            .get(&(key.guid, key.type_hash, key.part))
            .cloned()
    }) else {
        return Status::MISSING;
    };
    let size = u32::try_from(data.len()).unwrap_or(u32::MAX);
    if !len.is_null() {
        // SAFETY: `len` is non-null, checked just before; the safe layer
        // passes it pointing at a writable, aligned `u32` for the call.
        unsafe { len.write(size) };
    }
    if size > capacity || out.is_null() {
        return Status::BAD_ARGS;
    }
    // SAFETY: `out` is non-null and the copy is at most `capacity` bytes, both
    // checked just before; the safe layer passes `capacity` writable bytes at
    // `out`, apart from the fake's own buffer.
    unsafe { out.copy_from_nonoverlapping(data.as_ptr(), data.len()) };
    Status::OK
}

extern "system" fn game_text(_me: ModHandle, id: u32, language: u32, out: *mut Str) -> Status {
    let english = crate::i18n::Language::EN_US.0;
    let found = with(None, |s| {
        let texts = &s.host.game_texts;
        let language = if language == 0 { english } else { language };
        texts
            .get(&(id, language))
            .or_else(|| texts.get(&(id, english)))
            .copied()
    });
    match found {
        Some(text) => put(out, Str::new(text)),
        None => Status::MISSING,
    }
}

extern "system" fn config_schema(_me: ModHandle, schema: Str) -> Status {
    // SAFETY: the mod passes text valid for the call.
    let Some(text) = (unsafe { schema.as_str() }) else {
        return Status::BAD_ARGS;
    };
    if crate::config::Schema::parse(text).is_err() {
        return Status::BAD_ARGS;
    }
    with(Status::REFUSED, |s| {
        s.config_schema = Some(text.to_owned());
        Status::OK
    })
}

extern "system" fn damage_filter(
    _me: ModHandle,
    callback: DamageFilterFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.damage_filters.push((callback, user.expose_provenance()));
        Status::OK
    })
}

extern "system" fn knowledge_input_filter(
    _me: ModHandle,
    callback: KnowledgeInputFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.knowledge_input_filters
            .push((callback, user.expose_provenance()));
        Status::OK
    })
}

/// Writes up to `capacity` of `items` to `out` and their number to `count`.
pub(crate) fn put_list<T: Copy>(
    items: &[T],
    out: *mut T,
    capacity: u32,
    count: *mut u32,
) -> Status {
    if count.is_null() {
        return Status::BAD_ARGS;
    }
    if !out.is_null() {
        for (i, item) in items.iter().take(capacity as usize).enumerate() {
            // SAFETY: `i` is below `capacity` and the safe layer passes a
            // non-null `out` with room for `capacity` entries, so the offset
            // stays inside that array.
            let slot = unsafe { out.add(i) };
            // SAFETY: `slot` is inside the `capacity` writable, aligned
            // entries the safe layer passes at `out`.
            unsafe { slot.write(*item) };
        }
    }
    // SAFETY: `count` is non-null, checked at the top; the safe layer passes
    // it pointing at a writable, aligned `u32` for the call.
    unsafe { count.write(u32::try_from(items.len()).unwrap_or(u32::MAX)) };
    Status::OK
}

extern "system" fn buff_types(
    _me: ModHandle,
    out: *mut BuffTypeInfo,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let types = with(Vec::new(), |s| s.buff_types.clone());
    put_list(&types, out, capacity, count)
}

extern "system" fn buffs(
    _me: ModHandle,
    entity: u32,
    out: *mut ActiveBuff,
    capacity: u32,
    count: *mut u32,
) -> Status {
    match with(None::<Vec<ActiveBuff>>, |s| s.buffs.get(&entity).cloned()) {
        Some(on) => put_list(&on, out, capacity, count),
        None => Status::MISSING,
    }
}

extern "system" fn apply_buff(_me: ModHandle, request: *const BuffRequest) -> Status {
    // SAFETY: the safe layer passes a valid request for the call.
    let Some(request) = (unsafe { request.as_ref() }).copied() else {
        return Status::BAD_ARGS;
    };
    with(Status::MISSING, |s| {
        let Some(ty) = s.buff_types.iter().find(|ty| ty.id == request.id).copied() else {
            return Status::BAD_ARGS;
        };
        let Some(on) = s.buffs.get_mut(&request.entity) else {
            return Status::MISSING;
        };
        let duration = if request.duration == 0 {
            ty.duration
        } else {
            request.duration
        };
        if let Some(buff) = on.iter_mut().find(|buff| buff.id == request.id) {
            buff.duration = duration;
            buff.remaining = duration;
        } else {
            on.push(ActiveBuff {
                header: Header::of::<ActiveBuff>(),
                id: request.id,
                entity: 0x8000 + u32::try_from(on.len()).unwrap_or(0),
                slot: u32::from(ty.slot),
                level: request.level,
                duration,
                remaining: duration,
            });
        }
        Status::OK
    })
}

extern "system" fn remove_buff(_me: ModHandle, entity: u32, id: u32, removed: *mut u8) -> Status {
    if removed.is_null() {
        return Status::BAD_ARGS;
    }
    with(Status::MISSING, |s| {
        let Some(on) = s.buffs.get_mut(&entity) else {
            return Status::MISSING;
        };
        let before = on.len();
        on.retain(|buff| buff.id != id);
        // SAFETY: `removed` is non-null, checked at the top; the safe layer
        // passes it pointing at a writable byte for the call.
        unsafe { removed.write(u8::from(on.len() != before)) };
        Status::OK
    })
}

extern "system" fn knowledge(
    _me: ModHandle,
    entity: u32,
    id: u32,
    out: *mut KnowledgeInfo,
) -> Status {
    let found = with(None, |s| {
        s.knowledge
            .iter()
            .find(|(e, k)| k.id == id && (k.scope == KnowledgeInfo::WORLD || *e == entity))
            .map(|(_, k)| *k)
    });
    let Some(info) = found else {
        return Status::MISSING;
    };
    // SAFETY: the safe layer passes a headed struct for the call.
    if unsafe { crate::abi::write_headed(out, info) } {
        Status::OK
    } else {
        Status::BAD_ARGS
    }
}

extern "system" fn grant_knowledge(
    _me: ModHandle,
    entity: u32,
    id: u32,
    kind: u32,
    value: u32,
) -> Status {
    with(Status::MISSING, |s| {
        let Some(info) = s
            .knowledge
            .iter()
            .find(|(_, k)| k.id == id)
            .map(|(_, k)| *k)
        else {
            return Status::BAD_ARGS;
        };
        if u32::from(info.kind) != kind {
            return Status::BAD_ARGS;
        }
        s.grants.push((entity, id, kind, value));
        Status::OK
    })
}

extern "system" fn quests(_me: ModHandle, out: *mut u32, capacity: u32, count: *mut u32) -> Status {
    let ids = with(Vec::new(), |s| s.quests.clone());
    if count.is_null() {
        return Status::BAD_ARGS;
    }
    if !out.is_null() {
        for (i, id) in ids.iter().take(capacity as usize).enumerate() {
            // SAFETY: `i` is below `capacity` and the safe layer passes a
            // non-null `out` with room for `capacity` ids, so the offset
            // stays inside that array.
            let slot = unsafe { out.add(i) };
            // SAFETY: `slot` is inside the `capacity` writable, aligned
            // ids the safe layer passes at `out`.
            unsafe { slot.write(*id) };
        }
    }
    // SAFETY: `count` is non-null, checked at the top; the safe layer passes
    // it pointing at a writable, aligned `u32` for the call.
    unsafe { count.write(u32::try_from(ids.len()).unwrap_or(u32::MAX)) };
    Status::OK
}

extern "system" fn weather_at(
    _me: ModHandle,
    position: *const [i64; 3],
    out: *mut [f32; 4],
) -> Status {
    if position.is_null() || out.is_null() {
        return Status::BAD_ARGS;
    }
    match with(None, |s| s.weather.as_ref().map(|w| w.0)) {
        Some(weights) => {
            // SAFETY: `out` is non-null, checked at the top; the safe layer
            // passes it pointing at a writable, aligned `[f32; 4]`.
            unsafe { out.write(weights) };
            Status::OK
        }
        None => Status::MISSING,
    }
}

extern "system" fn weather_fronts(
    _me: ModHandle,
    out: *mut WeatherFront,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let Some(fronts) = with(None, |s| s.weather.as_ref().map(|w| w.1.clone())) else {
        return Status::MISSING;
    };
    if count.is_null() {
        return Status::BAD_ARGS;
    }
    if !out.is_null() {
        for (i, front) in fronts.iter().take(capacity as usize).enumerate() {
            // SAFETY: `i` is below `capacity` and the safe layer passes a
            // non-null `out` with room for `capacity` fronts, so the offset
            // stays inside that array.
            let slot = unsafe { out.add(i) };
            // SAFETY: `slot` is inside the `capacity` writable, aligned
            // fronts the safe layer passes at `out`.
            unsafe { slot.write(*front) };
        }
    }
    // SAFETY: `count` is non-null, checked at the top; the safe layer passes
    // it pointing at a writable, aligned `u32` for the call.
    unsafe { count.write(u32::try_from(fronts.len()).unwrap_or(u32::MAX)) };
    Status::OK
}

extern "system" fn set_weather(_me: ModHandle, effect: u32, duration: i64) -> Status {
    with(Status::MISSING, |s| {
        if s.weather.is_none() {
            return Status::MISSING;
        }
        s.forecasts.push((effect, duration));
        Status::OK
    })
}

extern "system" fn terrain_materials(
    _me: ModHandle,
    out: *mut TerrainMaterialInfo,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let Some(materials) = with(None, |s| s.terrain.clone()) else {
        return Status::MISSING;
    };
    if count.is_null() {
        return Status::BAD_ARGS;
    }
    if !out.is_null() {
        for (i, material) in materials.iter().take(capacity as usize).enumerate() {
            // SAFETY: `i` is below `capacity` and the safe layer passes a
            // non-null `out` with room for `capacity` entries, so the offset
            // stays inside that array.
            let slot = unsafe { out.add(i) };
            // SAFETY: `slot` is inside the `capacity` writable, aligned
            // entries the safe layer passes at `out`.
            unsafe { slot.write(*material) };
        }
    }
    // SAFETY: `count` is non-null, checked at the top; the safe layer passes
    // it pointing at a writable, aligned `u32` for the call.
    unsafe { count.write(u32::try_from(materials.len()).unwrap_or(u32::MAX)) };
    Status::OK
}

extern "system" fn terrain_edit(
    _me: ModHandle,
    edit: *const TerrainEdit,
    amount: *mut f32,
) -> Status {
    // SAFETY: the safe layer passes a valid edit for the call.
    let Some(edit) = (unsafe { edit.as_ref() }).copied() else {
        return Status::BAD_ARGS;
    };
    if amount.is_null() {
        return Status::BAD_ARGS;
    }
    if !(edit.radius > 0.0 && edit.radius <= 16.0) || edit.position.iter().any(|a| *a < 0.0) {
        return Status::BAD_ARGS;
    }
    with(Status::MISSING, |s| {
        if s.terrain.is_none() {
            return Status::MISSING;
        }
        s.terrain_edits.push(edit);
        // SAFETY: `amount` is non-null, checked after the edit is read; the
        // safe layer passes it pointing at a writable, aligned `f32`.
        unsafe { amount.write(1.0) };
        Status::OK
    })
}

/// table it sees for the process, so it is one static.
extern "system" fn ui_frame(_me: ModHandle, callback: UiFrameFn, user: *mut c_void) -> Status {
    with(Status::MISSING, |s| {
        if s.host.side == Side::SERVER {
            return Status::MISSING;
        }
        s.frames.push((callback, user.expose_provenance()));
        Status::OK
    })
}

/// The input waiting for the widget labelled `text`, taken.
fn input(s: &mut State, text: &str) -> Option<super::Input> {
    let at = s.inputs.iter().position(|(t, _)| t == text)?;
    Some(s.inputs.remove(at).1)
}

/// A progress bar's filled part, as the loader draws it; `None` for a bar
/// the loader refuses.
fn fraction(w: &UiWidget) -> Option<f32> {
    let fine = w.min.is_finite() && w.max.is_finite() && w.value.is_finite() && w.min < w.max;
    fine.then(|| (w.value.clamp(w.min, w.max) - w.min) / (w.max - w.min))
}

/// Draws a slider row as the loader does, taking its waiting input.
fn slider(
    s: &mut State,
    w: &mut UiWidget,
    label: String,
    read_only: bool,
) -> Result<super::Drawn, Status> {
    if !(w.min.is_finite() && w.max.is_finite() && w.value.is_finite()) {
        return Err(Status::BAD_ARGS);
    }
    let (min, max) = (w.min.min(w.max), w.max.max(w.min));
    w.value = w.value.clamp(min, max);
    if let (false, Some(super::Input::Value(value))) = (read_only, input(s, &label))
        && value.is_finite()
    {
        w.value = value.clamp(min, max);
        w.result = 1;
    }
    Ok(super::Drawn::Slider {
        label,
        value: w.value,
        min: w.min,
        max: w.max,
        steps: w.steps,
        read_only,
    })
}

/// Records a HUD callback's widget; `None` outside a HUD callback.
fn hud_widget(s: &mut State, frame: u64, w: &UiWidget, label: &str) -> Option<Status> {
    let (token, list) = s.hud.as_mut()?;
    if *token != frame {
        return None;
    }
    let drawn = match w.kind {
        UiWidget::LABEL => super::Drawn::Label {
            text: label.to_owned(),
            style: w.style,
        },
        UiWidget::PROGRESS => match fraction(w) {
            Some(fraction) => super::Drawn::Progress { fraction },
            None => return Some(Status::BAD_ARGS),
        },
        _ => return Some(Status::REFUSED),
    };
    list.push((w.anchor, drawn));
    Some(Status::OK)
}

extern "system" fn ui_widget(_me: ModHandle, frame: u64, widget: *mut UiWidget) -> Status {
    use super::{Drawn, Input};
    // SAFETY: the safe layer passes its own widget, valid for the call.
    let Some(w) = (unsafe { widget.as_mut() }) else {
        return Status::BAD_ARGS;
    };
    let label = text(w.text);
    let options: Vec<String> = if w.options.is_null() {
        Vec::new()
    } else {
        // SAFETY: the safe layer passes `option_count` texts.
        unsafe { core::slice::from_raw_parts(w.options, w.option_count as usize) }
            .iter()
            .map(|o| text(*o))
            .collect()
    };
    with(Status::MISSING, |s| {
        if let Some(status) = hud_widget(s, frame, w, &label) {
            return status;
        }
        let open = match s.frame.as_ref() {
            Some((token, open, _)) if *token == frame => *open,
            _ => return Status::WRONG_THREAD,
        };
        let read_only = w.flags & UiWidget::READ_ONLY != 0;
        w.result = 0;
        let drawn = match w.kind {
            UiWidget::WINDOW if !open => Drawn::Window {
                id: label,
                title: text(w.detail),
            },
            UiWidget::END if open => Drawn::End,
            UiWidget::SOUND => Drawn::Sound(w.style),
            UiWidget::WINDOW | UiWidget::END => return Status::REFUSED,
            _ if !open => return Status::REFUSED,
            UiWidget::LABEL => Drawn::Label {
                text: label,
                style: w.style,
            },
            UiWidget::BUTTON => {
                if !read_only && matches!(input(s, &label), Some(Input::Click)) {
                    w.result = 1;
                }
                Drawn::Button {
                    text: label,
                    enabled: !read_only,
                }
            }
            UiWidget::TOGGLE => {
                if let (false, Some(Input::Toggle(on))) = (read_only, input(s, &label)) {
                    w.index = u32::from(on);
                    w.result = 1;
                }
                Drawn::Toggle {
                    label,
                    on: w.index != 0,
                    read_only,
                }
            }
            UiWidget::SLIDER => match slider(s, w, label, read_only) {
                Ok(drawn) => drawn,
                Err(status) => return status,
            },
            UiWidget::DROPDOWN => {
                if let (false, Some(Input::Pick(index))) = (read_only, input(s, &label))
                    && index < options.len()
                {
                    w.index = u32::try_from(index).unwrap_or(0);
                    w.result = 1;
                }
                Drawn::Dropdown {
                    label,
                    options,
                    index: w.index as usize,
                    read_only,
                }
            }
            UiWidget::PROGRESS => match fraction(w) {
                Some(fraction) => Drawn::Progress { fraction },
                None => return Status::BAD_ARGS,
            },
            _ => return Status::MISSING,
        };
        if let Some((_, open, list)) = s.frame.as_mut() {
            match drawn {
                Drawn::Window { .. } => *open = true,
                Drawn::End => *open = false,
                _ => {}
            }
            list.push(drawn);
        }
        Status::OK
    })
}

extern "system" fn ui_hud(_me: ModHandle, callback: UiFrameFn, user: *mut c_void) -> Status {
    with(Status::MISSING, |s| {
        if s.host.side == Side::SERVER {
            return Status::MISSING;
        }
        s.huds.push((callback, user.expose_provenance()));
        Status::OK
    })
}

extern "system" fn ui_markers(_me: ModHandle, markers: *const UiMarker, count: u32) -> Status {
    if count as usize > crate::ui::MAX_MARKERS || (markers.is_null() && count > 0) {
        return Status::BAD_ARGS;
    }
    let given = if count == 0 {
        &[][..]
    } else {
        // SAFETY: the safe layer passes `count` markers, valid for the call.
        unsafe { core::slice::from_raw_parts(markers, count as usize) }
    };
    let markers: Vec<super::FakeMarker> = given
        .iter()
        .map(|m| super::FakeMarker {
            position: m.position,
            label: text(m.label),
            color: m.color,
        })
        .collect();
    with(Status::MISSING, |s| {
        if s.host.side == Side::SERVER {
            return Status::MISSING;
        }
        s.markers = markers;
        Status::OK
    })
}

extern "system" fn ui_dialog(
    _me: ModHandle,
    dialog: *const UiDialog,
    callback: UiAnswerFn,
    user: *mut c_void,
) -> Status {
    // SAFETY: the safe layer passes its own dialog, valid for the call.
    let Some(d) = (unsafe { dialog.as_ref() }) else {
        return Status::BAD_ARGS;
    };
    let shown = super::ShownDialog {
        title: text(d.title),
        message: text(d.message),
        detail: text(d.detail),
        buttons: d.buttons,
    };
    with(Status::MISSING, |s| {
        if s.host.side == Side::SERVER {
            return Status::MISSING;
        }
        if s.dialogs.len() >= 16 {
            return Status::REFUSED;
        }
        s.dialogs.push((shown, callback, user.expose_provenance()));
        Status::OK
    })
}

extern "system" fn config_check(
    _me: ModHandle,
    callback: ConfigCheckFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        if s.config_check.is_some() {
            return Status::REFUSED;
        }
        s.config_check = Some((callback, user.expose_provenance()));
        Status::OK
    })
}

extern "system" fn texts(_me: ModHandle, toml: Str) -> Status {
    let toml = text(toml);
    with(Status::MISSING, |s| {
        s.published_texts = Some(toml);
        Status::OK
    })
}

extern "system" fn language(_me: ModHandle) -> u32 {
    with(0, |s| s.host.language)
}

pub(crate) static TABLE: HostApi = HostApi {
    header: Header::of::<HostApi>(),
    host_version: Str::new("0.0.0-test"),
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
    stat_open: super::stats::stat_open,
    stat_info: super::stats::stat_info,
    stat_value: super::stats::stat_value,
    stat_modify: super::stats::stat_modify,
    stat_buff: super::stats::stat_buff,
    stats_changed: super::stats::stats_changed,
    stat_sources: super::stats::stat_sources,
    stat_watch: super::stats::stat_watch,
    define_event,
    subscribe_event,
    send_event,
    affix_open: super::stats::affix_open,
    affix_roll: super::stats::affix_roll,
    item_lines: super::stats::item_lines,
    affix_info: super::stats::affix_info,
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
    define_point: super::points::define_point,
    attach_point: super::points::attach_point,
    run_point: super::points::run_point,
    actions,
    feature,
};
