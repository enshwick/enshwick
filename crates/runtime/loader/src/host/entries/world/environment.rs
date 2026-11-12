//! Host functions for the world's time, game settings, weather, terrain and
//! voxels.

use crate::host::memory::{put, put_headed, put_headed_list};
use crate::host::{caller, caller_index, entry};
use enshwick_core::abi::{ModHandle, Status};

pub(crate) extern "system" fn time_of_day(
    me: ModHandle,
    out: *mut enshwick_core::abi::DaytimeInfo,
) -> Status {
    entry("time_of_day", || {
        match caller(me).and_then(crate::world::daytime::info) {
            // SAFETY: `HostApi::time_of_day` has the mod pass `out` null or
            // pointing at its version of `DaytimeInfo`, writable for the size
            // its header states, that no other thread touches during the call.
            Ok(info) => unsafe { put_headed(out, info) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn set_time_of_day(me: ModHandle, time: i64) -> Status {
    entry("set_time_of_day", || {
        match caller(me).and_then(|loader| crate::world::daytime::set_time(loader, time)) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn time_speed_control(me: ModHandle) -> Status {
    entry("time_speed_control", || match caller_index(me) {
        Ok((loader, index)) => crate::engine::conflicts::note(
            "time speed",
            index,
            crate::engine::conflicts::Mode::Exclusive,
            crate::world::daytime::register_speed(loader, index),
        ),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn set_time_speed(me: ModHandle, factor: f32) -> Status {
    entry("set_time_speed", || {
        match caller_index(me)
            .and_then(|(loader, index)| crate::world::daytime::set_speed(loader, index, factor))
        {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn game_settings(
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
        match caller(me).and_then(|loader| crate::world::game_settings::read(loader, out)) {
            // SAFETY: `HostApi::game_settings` has the mod pass `preset` null
            // or pointing at a writable, aligned `u32` that no other thread
            // touches during the call.
            Ok(value) => unsafe { put(preset, u32::from(value)) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn set_game_settings(
    me: ModHandle,
    data: *const u8,
    size: u32,
) -> Status {
    entry("set_game_settings", || {
        let Ok(len) = usize::try_from(size) else {
            return Status::BAD_ARGS;
        };
        if data.is_null() {
            return Status::BAD_ARGS;
        }
        // SAFETY: the caller passes `size` bytes at `data`, valid for the call.
        let bytes = unsafe { core::slice::from_raw_parts(data, len) };
        match caller(me).and_then(|loader| crate::world::game_settings::write(loader, bytes)) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn keep_game_settings(
    me: ModHandle,
    data: *const u8,
    mask: *const u8,
    size: u32,
) -> Status {
    entry("keep_game_settings", || {
        if data.is_null() || mask.is_null() {
            return Status::BAD_ARGS;
        }
        let Ok(len) = usize::try_from(size) else {
            return Status::BAD_ARGS;
        };
        // SAFETY: `data` is non-null, checked at the top; the ABI has the mod
        // pass `size` readable bytes there, unchanged for the call.
        let bytes = unsafe { core::slice::from_raw_parts(data, len) };
        // SAFETY: `mask` is non-null, checked at the top; the ABI has the mod
        // pass `size` readable bytes there, unchanged for the call.
        let mask = unsafe { core::slice::from_raw_parts(mask, len) };
        match caller(me).and_then(|loader| crate::world::game_settings::keep(loader, bytes, mask)) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn forget_game_settings(me: ModHandle) -> Status {
    entry("forget_game_settings", || {
        match caller(me).and_then(crate::world::game_settings::forget) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn weather_at(
    me: ModHandle,
    position: *const [i64; 3],
    out: *mut [f32; 4],
) -> Status {
    entry("weather_at", || {
        // SAFETY: the caller passes null or a position valid for the call.
        let Some(position) = (unsafe { position.as_ref() }).copied() else {
            return Status::BAD_ARGS;
        };
        match caller(me).and_then(|loader| crate::world::weather::sample(loader, position)) {
            // SAFETY: `HostApi::weather_at` has the mod pass `out` null or
            // pointing at a writable, aligned `[f32; 4]` that no other thread
            // touches during the call.
            Ok(weights) => unsafe { put(out, weights) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn weather_fronts(
    me: ModHandle,
    out: *mut enshwick_core::abi::WeatherFront,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("weather_fronts", || {
        match caller(me).and_then(crate::world::weather::fronts) {
            // SAFETY: `HostApi::weather_fronts` has the mod pass `out` null or
            // pointing at `capacity` entries of its version of `WeatherFront`,
            // each writable for the size the first entry's header states, and
            // `count` null or pointing at a writable, aligned `u32`, neither
            // touched by another thread during the call.
            Ok(fronts) => unsafe { put_headed_list(&fronts, out, capacity, count) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn set_weather(me: ModHandle, effect: u32, duration: i64) -> Status {
    entry("set_weather", || {
        match caller(me).and_then(|loader| crate::world::weather::set(loader, effect, duration)) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn terrain_materials(
    me: ModHandle,
    out: *mut enshwick_core::abi::TerrainMaterialInfo,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("terrain_materials", || {
        match caller(me).and_then(crate::world::terrain::materials) {
            // SAFETY: `HostApi::terrain_materials` has the mod pass `out` null
            // or pointing at `capacity` entries of its version of
            // `TerrainMaterialInfo`, each writable for the size the first
            // entry's header states, and `count` null or pointing at a
            // writable, aligned `u32`, neither touched by another thread during
            // the call.
            Ok(materials) => unsafe { put_headed_list(&materials, out, capacity, count) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn terrain_edit(
    me: ModHandle,
    edit: *const enshwick_core::abi::TerrainEdit,
    amount: *mut f32,
) -> Status {
    entry("terrain_edit", || {
        // SAFETY: the caller passes null or an edit with its header; an
        // all-zero edit is valid.
        let Some(edit) = (unsafe {
            enshwick_core::abi::read_headed(
                edit,
                <enshwick_core::abi::TerrainEdit as enshwick_core::abi::Headed>::BORN,
            )
        }) else {
            return Status::BAD_ARGS;
        };
        let edited = caller(me).and_then(|loader| {
            crate::world::voxels::changes::by(me, || crate::world::terrain::edit(loader, &edit))
        });
        match edited {
            // SAFETY: `HostApi::terrain_edit` has the mod pass `amount` null or
            // pointing at a writable, aligned `f32` that no other thread
            // touches during the call.
            Ok(changed) => unsafe { put(amount, changed) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn voxel_read(
    me: ModHandle,
    min: *const [u32; 3],
    size: *const [u32; 3],
    out: *mut enshwick_core::abi::Voxel,
    capacity: u64,
) -> Status {
    entry("voxel_read", || {
        if min.is_null() || size.is_null() || out.is_null() {
            return Status::BAD_ARGS;
        }
        // SAFETY: `min` is non-null, checked above; `HostApi::voxel_read` has
        // the mod pass it pointing at three `u32`s borrowed for the call, and
        // an unaligned read needs no alignment.
        // The mod's buffer, not game memory.
        // ast-grep-ignore: game-memory-through-raw
        let min = unsafe { min.read_unaligned() };
        // SAFETY: `size` is non-null, checked above; `HostApi::voxel_read`
        // has the mod pass it pointing at three `u32`s borrowed for the call,
        // and an unaligned read needs no alignment.
        // The mod's buffer, not game memory.
        // ast-grep-ignore: game-memory-through-raw
        let size = unsafe { size.read_unaligned() };
        let request = match crate::world::voxels::ReadBox::new(min, size, capacity) {
            Ok(request) => request,
            Err(status) => return status,
        };
        let words = match caller(me).and_then(|loader| crate::world::voxels::read(loader, &request))
        {
            Ok(words) => words,
            Err(status) => return status,
        };
        for (i, &word) in words.iter().enumerate() {
            // SAFETY: `out` is non-null, checked above, and `i` is below
            // `request.count`, which `ReadBox::new` keeps within `capacity`;
            // `HostApi::voxel_read` has the mod pass `capacity` entries at
            // `out`, so the offset stays inside that array.
            let slot = unsafe { out.add(i) };
            // SAFETY: `slot` is inside the `capacity` writable entries at
            // `out` that `HostApi::voxel_read` has the mod pass, untouched by
            // other threads during the call; `Voxel` has alignment 1.
            unsafe { slot.write(crate::world::voxels::decode(word)) };
        }
        Status::OK
    })
}
