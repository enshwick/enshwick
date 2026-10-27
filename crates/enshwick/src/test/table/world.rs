//! The fake world: players, entities, chat, time, settings, weather, terrain, voxels and build rights.

use core::ffi::c_void;

use crate::abi::{
    DaytimeInfo, EntityPlace, EventTypeInfo, GameEventFn, Header, ModHandle, PlayerInfo,
    RenameCallFn, Side, SpawnRequest, Status, StatusFn, Str, TeleportRequest, TerrainEdit,
    TerrainMaterialInfo, Voxel, WeatherFront,
};
use crate::test::table::{found, put, put_list, text, with};

pub(crate) extern "system" fn component(
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

pub(crate) extern "system" fn entity_template(
    _me: ModHandle,
    entity: u32,
    out: *mut [u8; 16],
) -> Status {
    found(with(None, |s| s.host.templates.get(&entity).copied()), out)
}

pub(crate) extern "system" fn player(_me: ModHandle, entity: u32, out: *mut PlayerInfo) -> Status {
    let player = with(None, |s| {
        s.host.players.iter().find(|p| p.entity == entity).copied()
    });
    found(player, out)
}

pub(crate) extern "system" fn players(
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

pub(crate) extern "system" fn hosting() -> u8 {
    with(0, |s| {
        u8::from(s.host.side == Side::SERVER || s.host.hosting)
    })
}

pub(crate) extern "system" fn server_world() -> u64 {
    with(0, |s| s.host.server_world)
}

pub(crate) extern "system" fn actions(
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

pub(crate) extern "system" fn chat(_me: ModHandle, to: u64, value: Str) -> Status {
    let value = text(value);
    with(Status::MISSING, |s| {
        s.chat.push((to, value));
        Status::OK
    })
}

pub(crate) extern "system" fn set_label(_me: ModHandle, entity: u32, value: Str) -> Status {
    let value = text(value);
    with(Status::MISSING, |s| {
        s.labels.push((entity, value));
        Status::OK
    })
}

pub(crate) extern "system" fn rename_filter(
    _me: ModHandle,
    callback: RenameCallFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.rename_filters.push((callback, user.expose_provenance()));
        Status::OK
    })
}

pub(crate) extern "system" fn feature(_me: ModHandle, name: Str, off: Str) -> Status {
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

pub(crate) extern "system" fn status(
    _me: ModHandle,
    callback: StatusFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.status_sources.push((callback, user.expose_provenance()));
        Status::OK
    })
}

pub(crate) extern "system" fn run_console(_me: ModHandle, line: Str, out: *mut Str) -> Status {
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
                let reply = crate::test::ask(
                    callback,
                    user,
                    crate::abi::ActionCall::CONSOLE,
                    &crate::test::console(),
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

pub(crate) extern "system" fn game_event(
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

pub(crate) extern "system" fn event_types(
    _me: ModHandle,
    out: *mut EventTypeInfo,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let found: Vec<EventTypeInfo> = with(Vec::new(), |s| {
        s.host
            .event_types
            .iter()
            .map(|&(name, hash, size)| EventTypeInfo {
                header: Header::of::<EventTypeInfo>(),
                name: Str::new(name),
                hash,
                size,
            })
            .collect()
    });
    put_list(&found, out, capacity, count)
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

pub(crate) extern "system" fn destroy(_me: ModHandle, entity: u32) -> Status {
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

pub(crate) extern "system" fn save_world(_me: ModHandle) -> Status {
    with(Status::MISSING, |s| {
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
    entity: u32,
    type_hash: u32,
    out: *mut *mut c_void,
) -> Status {
    let address = with(Err(Status::MISSING), |s| {
        if s.host.refused_writes.contains(&type_hash) {
            return Err(Status::REFUSED);
        }
        if s.host.static_halves.contains(&type_hash) {
            return Err(Status::STATIC_HALF);
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

pub(crate) extern "system" fn emit(
    _me: ModHandle,
    type_hash: u32,
    data: *const u8,
    size: u32,
) -> Status {
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

pub(crate) extern "system" fn teleport(_me: ModHandle, request: *const TeleportRequest) -> Status {
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

pub(crate) extern "system" fn notify(_me: ModHandle, key: u64, value: Str) -> Status {
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
pub(crate) const FIRST_SPAWNED: u32 = 0x10_0000;

pub(crate) extern "system" fn spawn(
    _me: ModHandle,
    request: *const SpawnRequest,
    out: *mut u32,
) -> Status {
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

pub(crate) extern "system" fn time_of_day(_me: ModHandle, out: *mut DaytimeInfo) -> Status {
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

pub(crate) extern "system" fn set_time_of_day(_me: ModHandle, time: i64) -> Status {
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

pub(crate) extern "system" fn time_speed_control(_me: ModHandle) -> Status {
    with(Status::MISSING, |s| {
        s.clock_speed.get_or_insert(1.0);
        Status::OK
    })
}

pub(crate) extern "system" fn set_time_speed(_me: ModHandle, factor: f32) -> Status {
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

pub(crate) extern "system" fn game_settings(
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

pub(crate) extern "system" fn set_game_settings(
    _me: ModHandle,
    data: *const u8,
    size: u32,
) -> Status {
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

pub(crate) extern "system" fn keep_game_settings(
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

pub(crate) extern "system" fn forget_game_settings(_me: ModHandle) -> Status {
    with(Status::MISSING, |s| {
        s.kept_settings.clear();
        Status::OK
    })
}

pub(crate) extern "system" fn weather_at(
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

pub(crate) extern "system" fn weather_fronts(
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

pub(crate) extern "system" fn set_weather(_me: ModHandle, effect: u32, duration: i64) -> Status {
    with(Status::MISSING, |s| {
        if s.weather.is_none() {
            return Status::MISSING;
        }
        s.forecasts.push((effect, duration));
        Status::OK
    })
}

pub(crate) extern "system" fn terrain_materials(
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

pub(crate) extern "system" fn terrain_edit(
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
    if !terrain_edit_valid(&edit) {
        return Status::BAD_ARGS;
    }
    with(Status::MISSING, |s| {
        if s.terrain.is_none() {
            return Status::MISSING;
        }
        if crate::test::world::edit_outside(s.host.extent, &edit) {
            return Status::OUTSIDE_WORLD;
        }
        if !crate::test::world::edit_loaded(&s.host.voxels, &edit) {
            return Status::NOT_LOADED;
        }
        s.edits.terrain.push(edit);
        // SAFETY: `amount` is non-null, checked after the edit is read; the
        // safe layer passes it pointing at a writable, aligned `f32`.
        unsafe { amount.write(1.0) };
        Status::OK
    })
}

pub(crate) extern "system" fn voxel_read(
    _me: ModHandle,
    min: *const [u32; 3],
    size: *const [u32; 3],
    out: *mut Voxel,
    capacity: u64,
) -> Status {
    if min.is_null() || size.is_null() || out.is_null() {
        return Status::BAD_ARGS;
    }
    // SAFETY: `min` is non-null, checked above; the safe layer passes three
    // `u32`s for the call, and an unaligned read needs no alignment.
    let min = unsafe { min.read_unaligned() };
    // SAFETY: `size` is non-null, checked above; the safe layer passes three
    // `u32`s for the call, and an unaligned read needs no alignment.
    let size = unsafe { size.read_unaligned() };
    if Voxel::read_count(min, size).is_none_or(|count| count > capacity) {
        return Status::BAD_ARGS;
    }
    if !crate::test::table::on_game_thread() {
        return Status::WRONG_THREAD;
    }
    let read = with(Err(Status::MISSING), |s| {
        s.edits.voxel_reads.push((min, size));
        if crate::test::world::outside(s.host.extent, min, size) {
            return Err(Status::OUTSIDE_WORLD);
        }
        crate::test::world::VoxelGrid::read(&s.host.voxels, min, size).ok_or(Status::NOT_LOADED)
    });
    let voxels = match read {
        Ok(voxels) => voxels,
        Err(status) => return status,
    };
    for (i, voxel) in voxels.into_iter().enumerate() {
        // SAFETY: `out` is non-null, checked above, and `i` is below the
        // box's count, which is at most `capacity`; the safe layer passes
        // `capacity` entries at `out`, so the offset stays inside them.
        let slot = unsafe { out.add(i) };
        // SAFETY: `slot` is inside the `capacity` writable entries the safe
        // layer passes at `out`; `Voxel` has alignment 1.
        unsafe { slot.write(voxel) };
    }
    Status::OK
}

/// Whether the loader takes `edit`: a known kind and shape, a radius above
/// 0 and up to 16, a position at 0 or more, and for an add or a flatten a
/// terrain material, for an add a height above 0 and up to 16.
pub(crate) fn terrain_edit_valid(edit: &TerrainEdit) -> bool {
    let reach = |metres: f32| metres > 0.0 && metres <= crate::terrain::MAX_RADIUS;
    let material = (1..=0x7f).contains(&edit.material);
    reach(edit.radius)
        && edit
            .position
            .iter()
            .all(|axis| axis.is_finite() && *axis >= 0.0)
        && edit.shape <= TerrainEdit::SQUARE
        && match edit.kind {
            TerrainEdit::ADD => material && reach(edit.height),
            TerrainEdit::DIG => true,
            TerrainEdit::FLATTEN => material,
            _ => false,
        }
}

pub(crate) extern "system" fn build_rights(
    _me: ModHandle,
    check: *mut crate::abi::BuildRights,
) -> Status {
    use crate::abi::BuildRights;
    // SAFETY: the safe layer passes `check` null or pointing at a live
    // `BuildRights` for the call, readable for the size its header states.
    let Some(mut request) = (unsafe { crate::abi::read_headed(check, size_of::<BuildRights>()) })
    else {
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
    if unsafe { crate::abi::write_headed(check, request) } {
        Status::OK
    } else {
        Status::BAD_ARGS
    }
}

pub(crate) extern "system" fn map_markers(
    _me: ModHandle,
    out: *mut crate::abi::MapMarkerInfo,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    if !crate::test::table::on_game_thread() {
        return Status::WRONG_THREAD;
    }
    let Some(markers) = with(None, |s| {
        let first = !core::mem::replace(&mut s.edits.markers_asked, true);
        (!s.edits.stopped)
            .then(|| (!first && !s.edits.markers_pending).then(|| s.host.map_markers.clone()))
    }) else {
        return Status::MISSING;
    };
    let Some(markers) = markers else {
        return Status::NOT_LOADED;
    };
    crate::test::table::mods::list(&markers, out, entry_size, capacity, count)
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
    let low = unsafe { min.read_unaligned() };
    // SAFETY: as for `min`, `max` points at three live `f64`.
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
