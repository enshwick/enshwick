//! The fake world's entries: players, entities, chat, saves, transactions,
//! altars, buffs, its surroundings, the map and build rights.

use core::ffi::c_void;

use crate::test::table::{Registered, put, text, with};
use enshwick_core::abi::{
    AccountActionType, EntityPlace, ModHandle, PlayerInfo, RenameCallFn, Side, Status, Str,
    TerrainEdit,
};

pub(crate) mod altars;
pub(crate) mod buffs;
pub(crate) mod environment;
pub(crate) mod saves;
pub(crate) mod transactions;

/// The changes of the world the mod asked for, and its voxel reads, in
/// order.
#[derive(Default)]
pub(crate) struct WorldEdits {
    /// The voxel boxes the mod read on the game thread, `(min, size)`.
    pub(crate) voxel_reads: Vec<([u32; 3], [u32; 3])>,
    /// Direct terrain edits.
    pub(crate) terrain: Vec<TerrainEdit>,
    /// Committed transactions, oldest first.
    pub(crate) transactions: Vec<crate::test::FakeTransaction>,
    /// Whether the world has stopped, as between two worlds.
    pub(crate) stopped: bool,
    /// Whether the scene's map markers are held as still being read.
    pub(crate) markers_pending: bool,
    /// The reads of the scene the mod asked for in this world; the first
    /// ask of each starts it.
    pub(crate) scene_asked: Vec<SceneRead>,
}

/// A read of the scene's data the loader runs once per world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SceneRead {
    /// The map markers.
    Markers,
    /// The map's frame.
    Frame,
}

impl WorldEdits {
    /// Notes that the mod asked for `read`; whether this was its first ask.
    pub(crate) fn first_ask(&mut self, read: SceneRead) -> bool {
        let first = !self.scene_asked.contains(&read);
        if first {
            self.scene_asked.push(read);
        }
        first
    }
}

/// Whether the entity calls cannot run here, where the loader answers
/// [`Status::UNAVAILABLE`]: no server runs, or the build lacks the commands.
pub(crate) fn entity_calls_off(s: &crate::test::table::State) -> bool {
    s.host.no_entity_commands || !(s.host.side == Side::SERVER || s.host.hosting)
}

pub(crate) extern "system" fn player(
    _me: ModHandle,
    entity: enshwick_core::Entity,
    out: *mut PlayerInfo,
) -> Status {
    let player = with(None, |s| {
        s.host.players.iter().find(|p| p.entity == entity).copied()
    });
    super::found_headed(player, out)
}

pub(crate) extern "system" fn players(
    _me: ModHandle,
    out: *mut PlayerInfo,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let players = with(Vec::new(), |s| s.host.players.clone());
    super::put_headed_list(&players, out, capacity, count)
}

pub(crate) extern "system" fn chat(
    _me: ModHandle,
    to: enshwick_core::PlayerKey,
    value: Str,
) -> Status {
    let value = text(value);
    with(Status::MISSING, |s| {
        s.chat.push((to, value));
        Status::OK
    })
}

pub(crate) extern "system" fn set_label(
    _me: ModHandle,
    entity: enshwick_core::Entity,
    value: Str,
) -> Status {
    let value = text(value);
    with(Status::MISSING, |s| {
        s.labels.push((entity.0, value));
        Status::OK
    })
}

pub(crate) extern "system" fn rename_filter(
    me: ModHandle,
    callback: RenameCallFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.rename_filters.push(Registered::new(callback, user, me));
        Status::OK
    })
}

pub(crate) extern "system" fn entities(
    _me: ModHandle,
    type_hash: u32,
    out: *mut EntityPlace,
    capacity: u32,
    count: *mut u32,
) -> Status {
    if !crate::test::table::on_game_thread() {
        return Status::WRONG_THREAD;
    }
    if with(true, |s| s.edits.stopped) {
        return Status::MISSING;
    }
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

pub(crate) extern "system" fn world_quiet(_me: ModHandle) -> Status {
    if !crate::test::table::on_game_thread() {
        return Status::WRONG_THREAD;
    }
    with(Status::MISSING, |s| {
        if s.edits.stopped {
            Status::MISSING
        } else {
            Status::OK
        }
    })
}

pub(crate) extern "system" fn destroy(_me: ModHandle, entity: enshwick_core::Entity) -> Status {
    with(Status::MISSING, |s| {
        if entity_calls_off(s) {
            return Status::UNAVAILABLE;
        }
        if s.host.players.iter().any(|p| p.entity == entity) {
            return Status::REFUSED;
        }
        if !s.host.placed.iter().any(|(_, p)| p.entity == entity)
            && !s.host.templates.contains_key(&entity.0)
        {
            return Status::MISSING;
        }
        s.destroyed.push(entity.0);
        Status::OK
    })
}

pub(crate) extern "system" fn save_world(_me: ModHandle) -> Status {
    with(Status::MISSING, |s| {
        if s.save.held {
            return Status::REFUSED;
        }
        s.saves += 1;
        Status::OK
    })
}

pub(crate) extern "system" fn world_key(_me: ModHandle, out: *mut Str) -> Status {
    with(Status::MISSING, |s| {
        let Some(key) = s.host.world_key.clone() else {
            return Status::MISSING;
        };
        let kept = s.world_key.insert(key.into_boxed_str());
        put(out, Str::new(kept))
    })
}

pub(crate) extern "system" fn component_write(
    _me: ModHandle,
    entity: enshwick_core::Entity,
    type_hash: u32,
    out: *mut *mut c_void,
) -> Status {
    let address = with(Err(Status::MISSING), |s| {
        if entity_calls_off(s) {
            return Err(Status::UNAVAILABLE);
        }
        if s.host.refused_writes.contains(&type_hash) {
            return Err(Status::REFUSED);
        }
        if s.host.static_halves.contains(&type_hash) {
            return Err(Status::STATIC_HALF);
        }
        let address = s
            .host
            .components
            .get(&(entity.0, type_hash))
            .map(|c| c.0)
            .ok_or(Status::MISSING)?;
        s.writes.push((entity.0, type_hash));
        Ok(address)
    });
    match address {
        Ok(address) => put(out, core::ptr::with_exposed_provenance_mut(address)),
        Err(status) => status,
    }
}

pub(crate) extern "system" fn item_info(
    _me: ModHandle,
    item: enshwick_core::ItemId,
    out: *mut enshwick_core::abi::ItemData,
) -> Status {
    crate::test::table::found_headed(with(None, |s| s.host.items.get(&item.get()).copied()), out)
}

pub(crate) extern "system" fn notify(
    _me: ModHandle,
    key: enshwick_core::PlayerKey,
    value: Str,
) -> Status {
    let value = text(value);
    with(Status::MISSING, |s| {
        if !s.host.net_peers.contains(&key.get()) {
            return Status::MISSING;
        }
        s.notices.push((key, value));
        Status::OK
    })
}

pub(crate) extern "system" fn language_of(_me: ModHandle, key: enshwick_core::PlayerKey) -> u32 {
    with(0, |s| {
        s.host
            .player_languages
            .get(&key.get())
            .copied()
            .unwrap_or(0)
    })
}

/// The first entity id the fake hands out for a spawn.
pub(crate) const FIRST_SPAWNED: u32 = 0x10_0000;

pub(crate) extern "system" fn progress_write(
    _me: ModHandle,
    entity: enshwick_core::Entity,
    what: u32,
    value: u32,
) -> Status {
    use enshwick_core::abi::{
        PROGRESS_ADD_EXPERIENCE, PROGRESS_OPEN, PROGRESS_SET_EXPERIENCE, PROGRESS_SET_LEVEL,
    };
    with(Status::MISSING, |s| {
        if what == PROGRESS_OPEN {
            if entity.0 != 0 {
                return Status::BAD_ARGS;
            }
            if s.registration == super::Registration::Closed {
                return Status::TOO_LATE;
            }
            s.progress_writes.get_or_insert_with(Vec::new);
            return Status::OK;
        }
        if ![
            PROGRESS_SET_LEVEL,
            PROGRESS_SET_EXPERIENCE,
            PROGRESS_ADD_EXPERIENCE,
        ]
        .contains(&what)
        {
            return Status::BAD_ARGS;
        }
        // The loader's level step links after `init`.
        if s.registration != super::Registration::Closed {
            return Status::MISSING;
        }
        let in_game = s.host.players.iter().any(|p| p.entity == entity);
        let step = s.ticks.step;
        // The loader holds at most 256 writes for its next level step.
        let waiting = s.progress_steps.iter().filter(|&&at| at == step).count();
        match s.progress_writes.as_mut() {
            Some(writes) if in_game && waiting < 256 => {
                writes.push((entity, what, value));
                s.progress_steps.push(step);
                Status::OK
            }
            _ => Status::REFUSED,
        }
    })
}

pub(crate) extern "system" fn server_stop(_me: ModHandle) -> Status {
    with(Status::UNAVAILABLE, |s| {
        if s.host.side != Side::SERVER || s.host.hosting {
            return Status::UNAVAILABLE;
        }
        if s.stops > 0 {
            return Status::REFUSED;
        }
        s.stops = 1;
        Status::OK
    })
}

pub(crate) extern "system" fn fog_uncover(
    _me: ModHandle,
    player: enshwick_core::PlayerKey,
    x: f32,
    z: f32,
    radius: f32,
) -> Status {
    with(Status::UNAVAILABLE, |s| {
        if !serving(s) {
            return Status::UNAVAILABLE;
        }
        if !(x.is_finite()
            && z.is_finite()
            && radius > 0.0
            && radius <= enshwick_core::abi::MAX_UNCOVER_RADIUS)
        {
            return Status::BAD_ARGS;
        }
        // As the loader: only a client that takes its messages is reached.
        if !s.host.net_peers.contains(&player.get()) {
            return Status::UNAVAILABLE;
        }
        s.uncovers.push((player, [x, z], radius));
        Status::OK
    })
}

pub(crate) extern "system" fn build_rights(
    _me: ModHandle,
    check: *mut enshwick_core::abi::BuildRights,
) -> Status {
    use enshwick_core::abi::BuildRights;
    let Some(mut request) = super::get_headed::<BuildRights>(check.cast_const()) else {
        return Status::BAD_ARGS;
    };
    let finite = request
        .min
        .iter()
        .chain(&request.max)
        .all(|v| v.is_finite());
    if !finite || (0..3).any(|a| request.max[a] < request.min[a]) {
        return Status::BAD_ARGS;
    }
    if !crate::test::table::on_game_thread() {
        return Status::WRONG_THREAD;
    }
    let refusal = with(None, |s| {
        let permissions = s
            .host
            .players
            .iter()
            .find(|p| p.entity == request.player)
            .map_or(0, |p| p.permissions);
        Some(s.host.zones.refusal(permissions, request.min, request.max))
    });
    let Some(refusal) = refusal else {
        return Status::MISSING;
    };
    request.refusal = refusal;
    // SAFETY: the safe layer passes `check` non-null (read above) and
    // writable for the size its header states, untouched by other threads.
    if unsafe { enshwick_core::abi::write_headed(check, request) } {
        Status::OK
    } else {
        Status::BAD_ARGS
    }
}

pub(crate) extern "system" fn map_markers(
    _me: ModHandle,
    out: *mut enshwick_core::abi::MapMarkerInfo,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    if !crate::test::table::on_game_thread() {
        return Status::WRONG_THREAD;
    }
    let Some(markers) = with(None, |s| {
        let first = s.edits.first_ask(SceneRead::Markers);
        (!s.edits.stopped)
            .then(|| (!first && !s.edits.markers_pending).then(|| s.host.map_markers.clone()))
    }) else {
        return Status::MISSING;
    };
    let Some(markers) = markers else {
        return Status::NOT_LOADED;
    };
    crate::test::table::base::io::list(&markers, out, entry_size, capacity, count)
}

pub(crate) extern "system" fn map_frame(
    _me: ModHandle,
    out: *mut enshwick_core::abi::MapFrameInfo,
) -> Status {
    if !crate::test::table::on_game_thread() {
        return Status::WRONG_THREAD;
    }
    let Some(frame) = with(None, |s| {
        let first = s.edits.first_ask(SceneRead::Frame);
        (!s.edits.stopped).then(|| (!first && !s.edits.markers_pending).then_some(s.host.map_frame))
    }) else {
        return Status::MISSING;
    };
    let Some(frame) = frame else {
        return Status::NOT_LOADED;
    };
    let Some(frame) = frame else {
        return Status::MISSING;
    };
    // SAFETY: the safe layer passes a headed struct for the call.
    if unsafe { enshwick_core::abi::write_headed(out, frame) } {
        Status::OK
    } else {
        Status::BAD_ARGS
    }
}

pub(crate) extern "system" fn world_persists(
    _me: ModHandle,
    min: *const [f64; 3],
    max: *const [f64; 3],
    kept: *mut u8,
) -> Status {
    if min.is_null() || max.is_null() || kept.is_null() {
        return Status::BAD_ARGS;
    }
    // SAFETY: the safe layer passes `min` pointing at three live `f64` for
    // the call, non-null by the check before it.
    // The fake host reads the mod's buffer, not game memory.
    // ast-grep-ignore: game-memory-through-raw
    let low = unsafe { min.read_unaligned() };
    // SAFETY: the safe layer passes `max` pointing at three live `f64` for
    // the call, non-null by the check before it.
    // The fake host reads the mod's buffer, not game memory.
    // ast-grep-ignore: game-memory-through-raw
    let high = unsafe { max.read_unaligned() };
    let finite = low.iter().chain(&high).all(|v| v.is_finite());
    if !finite || (0..3).any(|a| high[a] < low[a]) {
        return Status::BAD_ARGS;
    }
    if !crate::test::table::on_game_thread() {
        return Status::WRONG_THREAD;
    }
    let saved = with(false, |s| s.host.zones.kept(low, high));
    // SAFETY: the safe layer passes `kept` writable for one byte for the
    // call, non-null by the check before it.
    unsafe { kept.write(u8::from(saved)) };
    Status::OK
}

pub(crate) extern "system" fn account_action(
    _me: ModHandle,
    action: AccountActionType,
    account: enshwick_core::PlayerKey,
) -> Status {
    if account == enshwick_core::PlayerKey::new(0) || action.0 > AccountActionType::UN_BAN.0 {
        return Status::BAD_ARGS;
    }
    if !crate::test::table::on_game_thread() {
        return Status::WRONG_THREAD;
    }
    with(Status::MISSING, |s| {
        if s.edits.stopped {
            return Status::MISSING;
        }
        let online = s.host.players.iter().position(|p| p.key == account);
        let admin = s
            .host
            .players
            .iter()
            .any(|p| p.key == account && p.permissions & PlayerInfo::CAN_KICK_BAN != 0);
        match action {
            AccountActionType::UN_BAN => s.banned.retain(|&banned| banned != account),
            _ if admin || s.host.host_key == Some(account) => return Status::REFUSED,
            _ if online.is_none() => return Status::REFUSED,
            _ => {
                if action == AccountActionType::BAN && !s.banned.contains(&account) {
                    s.banned.push(account);
                }
                if let Some(at) = online {
                    s.host.players.remove(at);
                    s.removed.push(account.get());
                }
            }
        }
        Status::OK
    })
}

/// Whether the fake runs a server, the dedicated one or one a client hosts.
fn serving(s: &super::State) -> bool {
    s.host.side == Side::SERVER || s.host.hosting
}

pub(crate) extern "system" fn chat_mute_set(
    _me: ModHandle,
    account: enshwick_core::PlayerKey,
    muted: u8,
) -> Status {
    if account == enshwick_core::PlayerKey::new(0) || muted > 1 {
        return Status::BAD_ARGS;
    }
    with(Status::MISSING, |s| {
        if !serving(s) || s.host.world_key.is_none() {
            return Status::MISSING;
        }
        let admin = s
            .host
            .players
            .iter()
            .any(|p| p.key == account && p.permissions & PlayerInfo::CAN_KICK_BAN != 0);
        let at = s.muted.iter().position(|&k| k == account);
        match (muted == 1, at) {
            (true, _) if admin || s.host.host_key == Some(account) => return Status::REFUSED,
            (true, None) => s.muted.push(account),
            (false, Some(at)) => {
                s.muted.remove(at);
            }
            _ => {}
        }
        Status::OK
    })
}

pub(crate) extern "system" fn chat_mute_has(
    _me: ModHandle,
    account: enshwick_core::PlayerKey,
    muted: *mut u8,
) -> Status {
    if account == enshwick_core::PlayerKey::new(0) || muted.is_null() {
        return Status::BAD_ARGS;
    }
    let has = with(None, |s| serving(s).then(|| s.muted.contains(&account)));
    match has {
        Some(has) => put(muted, u8::from(has)),
        None => Status::MISSING,
    }
}
