//! The fake world's surroundings: its clock, game settings, weather,
//! terrain and voxels.

use crate::test::table::base::args;
use crate::test::table::entries;
use crate::test::table::{found_headed, get_headed, list, on_game_thread, with};
use crate::test::world::{VoxelGrid, edit_loaded, edit_outside, outside};
use enshwick_core::abi::{
    DaytimeInfo, Header, Status, TerrainEdit, TerrainMaterialAdd, TerrainMaterialInfo, Voxel,
    WeatherFront, WeatherSample,
};

entries! {
    pub(crate) fn time_of_day(me as Caller, out: *mut DaytimeInfo as args::HeadedOut) -> Status {
        let Some(info) = with(None, |s| s.world.daytime) else {
            return Status::UNAVAILABLE;
        };
        // SAFETY: the safe layer passes a headed struct for the call.
        if unsafe { enshwick_core::abi::write_headed(out, info) } {
            Status::OK
        } else {
            Status::BAD_ARGS
        }
    }

    pub(crate) fn set_time_of_day(me as Caller, time: i64) -> Status {
        const DAY: i64 = 24 * 3_600_000_000_000;
        with(Status::MISSING, |s| match s.world.daytime.as_mut() {
            Some(info) => {
                info.time_ns = time.rem_euclid(DAY);
                let day = (info.day_start_ns..info.day_end_ns).contains(&info.time_ns);
                info.flags = if day { DaytimeInfo::DAY } else { 0 };
                Status::OK
            }
            None => Status::UNAVAILABLE,
        })
    }

    pub(crate) fn request_time_speed(me as Mod) -> Status {
        crate::test::table::with_open(|s| {
            if !s.world.clock_speeds.iter().any(|(owner, _)| *owner == me) {
                s.world.clock_speeds.push((me, 1.0));
            }
            Status::OK
        })
    }

    pub(crate) fn set_time_speed(me as Mod, factor: f32) -> Status {
        if !(0.0..=1000.0).contains(&factor) {
            return Status::BAD_ARGS;
        }
        with(Status::MISSING, |s| {
            match s
                .world
                .clock_speeds
                .iter_mut()
                .find(|(owner, _)| *owner == me)
            {
                Some((_, speed)) => {
                    *speed = factor;
                    Status::OK
                }
                None => Status::REFUSED,
            }
        })
    }

    pub(crate) fn game_settings(
        me as Caller,
        [out: *mut u8, size: u32] as args::Buffer,
        preset: *mut u32 as args::Out,
    ) -> Status {
        let Some((bytes, value)) = with(None, |s| s.world.game_settings.clone()) else {
            return Status::UNAVAILABLE;
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

    /// As the loader's: `data` alone sets, with `mask` keeps the masked
    /// fields too, and null `data` forgets what is kept.
    pub(crate) fn set_game_settings(
        me as Caller,
        data: *const u8,
        mask: *const u8,
        size: u32,
    ) -> Status {
        if data.is_null() {
            if !mask.is_null() || size != 0 {
                return Status::BAD_ARGS;
            }
            return forget();
        }
        let status = set(data, size);
        if status != Status::OK || mask.is_null() {
            return status;
        }
        keep(data, mask, size)
    }
}

fn set(data: *const u8, size: u32) -> Status {
    // SAFETY: `data` is non-null, checked just before; the safe layer passes
    // `size` readable bytes there for the call.
    let bytes = unsafe { core::slice::from_raw_parts(data, size as usize) }.to_vec();
    with(Status::MISSING, |s| match s.world.game_settings.as_mut() {
        Some(current) if current.0.len() == bytes.len() => {
            *current = (bytes, 4);
            Status::OK
        }
        Some(_) => Status::BAD_ARGS,
        None => Status::UNAVAILABLE,
    })
}

fn keep(data: *const u8, mask: *const u8, size: u32) -> Status {
    // SAFETY: `data` is non-null, since `set_game_settings` checked it and
    // `set` returned `OK`; the safe layer passes `size` readable bytes there.
    let data = unsafe { core::slice::from_raw_parts(data, size as usize) };
    // SAFETY: `mask` is non-null, checked by `set_game_settings`; the safe layer passes
    // `size` readable bytes there for the call.
    let mask = unsafe { core::slice::from_raw_parts(mask, size as usize) };
    with(Status::MISSING, |s| {
        s.world.kept_settings.resize(data.len(), None);
        for ((kept, &byte), &set) in s.world.kept_settings.iter_mut().zip(data).zip(mask) {
            if set != 0 {
                *kept = Some(byte);
            }
        }
        Status::OK
    })
}

fn forget() -> Status {
    with(Status::MISSING, |s| {
        s.world.kept_settings.clear();
        Status::OK
    })
}

entries! {
    pub(crate) fn weather_at(
        me as Caller,
        position: *const [f64; 3] as args::In,
        out: *mut WeatherSample as args::HeadedOut,
    ) -> Status {
        if position.is_null() {
            return Status::BAD_ARGS;
        }
        let sample = with(None, |s| s.world.weather.as_ref().map(|w| w.0)).map(
            |[clear, rain, snow, blizzard]| WeatherSample {
                header: Header::of::<WeatherSample>(),
                clear,
                rain,
                snow,
                blizzard,
            },
        );
        found_headed(sample, out)
    }

    pub(crate) fn weather_fronts(
        me as Caller,
        [out: *mut WeatherFront, entry_size: u32, capacity: u32, count: *mut u32] as args::SizedListOut,
    ) -> Status {
        let Some(fronts) = with(None, |s| s.world.weather.as_ref().map(|w| w.1.clone())) else {
            return Status::UNAVAILABLE;
        };
        list(&fronts, out, entry_size, capacity, count)
    }

    pub(crate) fn set_weather(me as Caller, effect: u32, duration: i64) -> Status {
        with(Status::MISSING, |s| {
            if s.world.weather.is_none() {
                return Status::UNAVAILABLE;
            }
            s.world.forecasts.push((effect, duration));
            Status::OK
        })
    }

    pub(crate) fn terrain_materials(
        me as Caller,
        [out: *mut TerrainMaterialInfo, entry_size: u32, capacity: u32, count: *mut u32] as args::SizedListOut,
    ) -> Status {
        let Some(mut materials) = with(None, |s| s.world.terrain.clone()) else {
            return Status::UNAVAILABLE;
        };
        let added = with(Vec::new(), |s| s.world.terrain_added.clone());
        for (material, id) in added.iter().zip(GAME_FREE_TERRAIN) {
            if let Some(like) = materials.iter().find(|m| m.id == material.like).copied() {
                materials.push(added_info(like, material, id));
            }
        }
        list(&materials, out, entry_size, capacity, count)
    }
}

/// The terrain ids the game's palette leaves free (data 1287369), which the
/// loader gives mods' materials in load order.
const GAME_FREE_TERRAIN: core::ops::RangeInclusive<u8> = 117..=127;

/// The game's own terrain ids, below the free ones.
const GAME_TERRAIN: core::ops::RangeInclusive<u8> = 1..=116;

/// What the game lists for `material` at terrain id `id`: `like` with the
/// rules it changes.
fn added_info(
    like: TerrainMaterialInfo,
    material: &TerrainMaterialAdd,
    id: u8,
) -> TerrainMaterialInfo {
    let set = |bit| material.changes & bit != 0;
    let mut info = TerrainMaterialInfo { id, ..like };
    if set(TerrainMaterialAdd::ITEM) {
        info.item = enshwick_core::ItemId::new(material.item.fnv1a());
    }
    if set(TerrainMaterialAdd::HARDNESS) {
        info.hardness = material.hardness;
    }
    if set(TerrainMaterialAdd::HEALTH) {
        info.health = material.health;
    }
    if set(TerrainMaterialAdd::FLATTENS) {
        info.flags = material.flags & TerrainMaterialInfo::FLATTENS;
    }
    info
}

entries! {
    pub(crate) fn terrain_material_add(
        me as Mod,
        material: *const TerrainMaterialAdd as args::Headed,
        id: *mut u8 as args::Out,
    ) -> Status {
        let Some(material) = get_headed(material) else {
            return Status::BAD_ARGS;
        };
        if id.is_null() {
            return Status::BAD_ARGS;
        }
        let vanilla = |id| GAME_TERRAIN.contains(&id);
        with(Status::MISSING, |s| {
            // In the loader's order: the seal, the declared id, the mod's side,
            // the request; the fake's palette always reads.
            if s.base.closed() {
                return Status::TOO_LATE;
            }
            if !crate::test::table::content::declared(s, me, material.guid)
                || !crate::test::table::base::mods::joins(s, me)
            {
                return Status::REFUSED;
            }
            if !vanilla(material.like)
                || !vanilla(material.fallback)
                || !enshwick_core::__fake::rules::terrain_material_changes_valid(&material)
            {
                return Status::BAD_ARGS;
            }
            // The fake game names its terrain ids by no GUID, so it cannot
            // refuse one of the game's as the loader does.
            if material.guid == enshwick_core::content::Guid::NIL
                || s.world
                    .terrain_added
                    .iter()
                    .any(|m| m.guid == material.guid)
            {
                return Status::REFUSED;
            }
            let Some(placed) = GAME_FREE_TERRAIN.clone().nth(s.world.terrain_added.len()) else {
                return Status::REFUSED;
            };
            s.world.terrain_added.push(material);
            // SAFETY: `id` is non-null (checked above) and the safe layer passes
            // a byte for the call.
            unsafe { id.write(placed) };
            Status::OK
        })
    }

    pub(crate) fn terrain_edit(
        me as Caller,
        edit: *const TerrainEdit as args::Headed,
        amount: *mut f32 as args::Out,
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
            if s.world.terrain.is_none() {
                return Status::UNAVAILABLE;
            }
            if edit_outside(s.host.world.extent, &edit) {
                return Status::OUTSIDE_WORLD;
            }
            if !edit_loaded(&s.host.world.voxels, &edit) {
                return Status::NOT_LOADED;
            }
            s.world.edits.terrain.push(edit);
            // SAFETY: `amount` is non-null, checked after the edit is read; the
            // safe layer passes it pointing at a writable, aligned `f32`.
            unsafe { amount.write(1.0) };
            Status::OK
        })
    }

    pub(crate) fn voxel_read(
        me as Caller,
        min: *const [u32; 3] as args::In,
        size: *const [u32; 3] as args::In,
        [out: *mut Voxel, capacity: u32] as args::Buffer,
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
        if Voxel::read_count(min, size).is_none_or(|count| count > u64::from(capacity)) {
            return Status::BAD_ARGS;
        }
        if !on_game_thread() {
            return Status::WRONG_THREAD;
        }
        let read = with(Err(Status::MISSING), |s| {
            s.world.edits.voxel_reads.push((min, size));
            if outside(s.host.world.extent, min, size) {
                return Err(Status::OUTSIDE_WORLD);
            }
            VoxelGrid::read(&s.host.world.voxels, min, size).ok_or(Status::NOT_LOADED)
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
}

pub(crate) use enshwick_core::__fake::rules::terrain_edit_valid;
