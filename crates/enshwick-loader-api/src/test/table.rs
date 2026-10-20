//! The loader's table, answering from the installed fake.

use core::ffi::c_void;
use core::sync::atomic::Ordering;
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::abi::{
    AltarInfo, AltarLimitFn, AltarZoneSize, BedCheckFn, BuildCheckFn, BuildInfo, CallerFrame,
    ComfortFn, CommandCallFn, CraftCheckFn, EntityPlace, Event, EventFn, ExperiencePassFn,
    GameEventFn, Header, HookSpec, HostApi, InteractionFn, InventoryRequest, InventoryResultFn,
    ItemData, Level, MidHookFn, MidHookSpec, ModHandle, ModInfo, NetMessageFn, PlayerInfo,
    RenameCallFn, ResourceKey, ResourcePatchFn, Side, SkillCheckFn, SpawnRequest, Status, StatusFn,
    Str, SummonFn, SurvivorLookupFn, SurvivorStatusFn, TeleportRequest, ZoneCheckFn,
};

use super::{FakeHost, LinkedHook, ServedResource, SubmittedRequest};

/// A submitted inventory request and where its result goes.
pub(crate) struct OpenRequest {
    pub(crate) request: SubmittedRequest,
    pub(crate) done: InventoryResultFn,
    pub(crate) user: usize,
}

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
    pub(crate) texts: Vec<(u32, u32, String)>,
    pub(crate) code_patches: Vec<(u64, Vec<u8>, Vec<u8>)>,
    pub(crate) published: Vec<(String, u32, usize)>,
    pub(crate) defaults: Vec<String>,
    pub(crate) synced: Vec<String>,
    pub(crate) commands: Vec<(String, String, CommandCallFn, usize)>,
    pub(crate) chat: Vec<(u64, String)>,
    pub(crate) labels: Vec<(u32, String)>,
    pub(crate) rename_filters: Vec<(RenameCallFn, usize)>,
    pub(crate) inventory_requests: Vec<OpenRequest>,
    pub(crate) status_sources: Vec<(StatusFn, usize)>,
    /// The command lines the mod ran as the console, in order.
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
    pub(crate) summon_items: Vec<(u32, u32)>,
    pub(crate) survivor_status: Option<(SurvivorStatusFn, usize)>,
    pub(crate) interaction_filters: Vec<(InteractionFn, usize)>,
    pub(crate) skills: Vec<(u32, SkillCheckFn, usize)>,
    pub(crate) comfort_filters: Vec<(ComfortFn, usize)>,
    pub(crate) experience_filters: Vec<(ExperiencePassFn, usize)>,
    pub(crate) altar_limit: Option<(AltarLimitFn, usize)>,
    pub(crate) base_limit_raised: bool,
    pub(crate) grouping: Option<(u32, u32)>,
    /// The reply `run_command` last handed out, alive until the next call.
    run_out: Option<Box<str>>,
    /// The settings text last handed out, alive until the next call.
    settings: Option<Box<str>>,
    /// The join values last handed out, alive until the next call.
    sync_out: Option<Box<str>>,
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
        texts: Vec::new(),
        code_patches: Vec::new(),
        published: Vec::new(),
        defaults: Vec::new(),
        synced: Vec::new(),
        inventory_requests: Vec::new(),
        sync_out: None,
        commands: Vec::new(),
        chat: Vec::new(),
        labels: Vec::new(),
        rename_filters: Vec::new(),
        status_sources: Vec::new(),
        console: Vec::new(),
        destroyed: Vec::new(),
        saves: 0,
        writes: Vec::new(),
        emits: Vec::new(),
        teleports: Vec::new(),
        spawns: Vec::new(),
        craft_filters: Vec::new(),
        channels: Vec::new(),
        net_sent: Vec::new(),
        notices: Vec::new(),
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
        summon_items: Vec::new(),
        survivor_status: None,
        interaction_filters: Vec::new(),
        skills: Vec::new(),
        comfort_filters: Vec::new(),
        experience_filters: Vec::new(),
        altar_limit: None,
        base_limit_raised: false,
        grouping: None,
        run_out: None,
        settings: None,
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
fn put<T>(out: *mut T, value: T) -> Status {
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
        s.settings = Some(served);
        Some(view)
    });
    found(served, out)
}

extern "system" fn publish(
    _me: ModHandle,
    name: Str,
    version: u32,
    table: *const c_void,
) -> Status {
    let name = text(name);
    with(Status::MISSING, |s| {
        if s.published.iter().any(|p| p.0 == name && p.1 == version) {
            return Status::REFUSED;
        }
        s.published.push((name, version, table.expose_provenance()));
        Status::OK
    })
}

extern "system" fn service(
    _me: ModHandle,
    name: Str,
    version: u32,
    out: *mut *const c_void,
) -> Status {
    let key = (text(name), version);
    let table = with(None, |s| {
        s.host.services.get(&key).copied().or_else(|| {
            s.published
                .iter()
                .find(|p| p.0 == key.0 && p.1 == key.1)
                .map(|p| p.2)
        })
    });
    found(table.map(core::ptr::with_exposed_provenance), out)
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
        s.host.components.get(&(entity, type_hash)).copied()
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
    // SAFETY: as for `key`.
    let Some(base) = (unsafe { base.as_ref() }).copied() else {
        return Status::BAD_ARGS;
    };
    with(Status::MISSING, |s| {
        if s.served.iter().any(|served| served.0.key == key) {
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
        s.sync_out = Some(served);
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
    // SAFETY: the safe layer passes `len` bytes behind each pointer.
    let expected = unsafe { core::slice::from_raw_parts(expected, len) }.to_vec();
    // SAFETY: as above.
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

extern "system" fn command(
    _me: ModHandle,
    name: Str,
    help: Str,
    callback: CommandCallFn,
    user: *mut c_void,
) -> Status {
    let (name, help) = (text(name), text(help));
    with(Status::MISSING, |s| {
        if s.commands.iter().any(|c| c.0 == name) || name == "enshwick" {
            return Status::REFUSED;
        }
        s.commands
            .push((name, help, callback, user.expose_provenance()));
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

extern "system" fn status(_me: ModHandle, callback: StatusFn, user: *mut c_void) -> Status {
    with(Status::MISSING, |s| {
        s.status_sources.push((callback, user.expose_provenance()));
        Status::OK
    })
}

extern "system" fn run_command(_me: ModHandle, line: Str, out: *mut Str) -> Status {
    let line = text(line);
    let Some((name, args)) = crate::commands::parse_line(&line) else {
        return Status::BAD_ARGS;
    };
    let args = args.to_owned();
    with((), |s| s.console.push(line.clone()));
    let own = with(None, |s| {
        s.commands.iter().find(|c| c.0 == name).map(|c| (c.2, c.3))
    });
    let reply = match own {
        // The mod's own command, run with the table unlocked: it may call back.
        Some((callback, user)) => {
            super::run_handler(callback, user, &name, &super::console(), &args)
        }
        None => match with(None, |s| s.host.foreign_commands.get(&name).cloned()) {
            Some(reply) => reply,
            None => return Status::MISSING,
        },
    };
    with(Status::MISSING, |s| {
        let kept = s.run_out.insert(reply.into_boxed_str());
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
            .copied()
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

/// table it sees for the process, so it is one static.
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
};
