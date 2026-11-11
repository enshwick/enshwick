//! The fake world's surroundings: its clock, game settings, weather,
//! terrain and voxels.

use crate::test::table::{get_headed, on_game_thread, put_headed_list, with};
use crate::test::world::{VoxelGrid, edit_loaded, edit_outside, outside};
use enshwick_core::abi::{
    DaytimeInfo, ModHandle, Status, TerrainEdit, TerrainMaterialInfo, Voxel, WeatherFront,
};

pub(crate) extern "system" fn time_of_day(_me: ModHandle, out: *mut DaytimeInfo) -> Status {
    let Some(info) = with(None, |s| s.daytime) else {
        return Status::MISSING;
    };
    // SAFETY: the safe layer passes a headed struct for the call.
    if unsafe { enshwick_core::abi::write_headed(out, info) } {
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
    put_headed_list(&fronts, out, capacity, count)
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
    put_headed_list(&materials, out, capacity, count)
}

pub(crate) extern "system" fn terrain_edit(
    _me: ModHandle,
    edit: *const TerrainEdit,
    amount: *mut f32,
) -> Status {
    let Some(edit) = get_headed(edit) else {
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
        if edit_outside(s.host.extent, &edit) {
            return Status::OUTSIDE_WORLD;
        }
        if !edit_loaded(&s.host.voxels, &edit) {
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
    // The fake host reads the mod's buffer, not game memory.
    // ast-grep-ignore: game-memory-through-raw
    let min = unsafe { min.read_unaligned() };
    // SAFETY: `size` is non-null, checked above; the safe layer passes three
    // `u32`s for the call, and an unaligned read needs no alignment.
    // The fake host reads the mod's buffer, not game memory.
    // ast-grep-ignore: game-memory-through-raw
    let size = unsafe { size.read_unaligned() };
    if Voxel::read_count(min, size).is_none_or(|count| count > capacity) {
        return Status::BAD_ARGS;
    }
    if !on_game_thread() {
        return Status::WRONG_THREAD;
    }
    let read = with(Err(Status::MISSING), |s| {
        s.edits.voxel_reads.push((min, size));
        if outside(s.host.extent, min, size) {
            return Err(Status::OUTSIDE_WORLD);
        }
        VoxelGrid::read(&s.host.voxels, min, size).ok_or(Status::NOT_LOADED)
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
    let reach = |metres: f32| metres > 0.0 && metres <= enshwick_core::terrain::MAX_RADIUS;
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
