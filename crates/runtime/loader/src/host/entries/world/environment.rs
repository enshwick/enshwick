//! Host functions for the world's time, game settings, weather, terrain and
//! voxels.

use enshwick_core::abi::{
    DaytimeInfo, Status, TerrainEdit, TerrainMaterialAdd, TerrainMaterialInfo, Voxel, WeatherFront,
    WeatherSample,
};

use crate::engine::conflicts::Mode;
use crate::host::layer::guards::declared_content;
use crate::host::layer::{
    Asker, Buffer, Caller, Headed, HeadedOut, In, Mod, Out, SizedListOut, entries,
};
use crate::host::memory::slice;

entries! {
    fn time_of_day(loader as Caller, out as HeadedOut<DaytimeInfo>) -> Status {
        out.put(crate::world::daytime::info(loader)?)
    }

    fn set_time_of_day(loader as Caller, time: i64) -> Status {
        crate::world::daytime::set_time(loader, time)?;
        Ok(())
    }

    fn request_time_speed(me as Mod) -> Status {
        me.claim(
            "time speed",
            Mode::Chained,
            crate::world::daytime::register_speed(me.loader, me.index),
        )
    }

    fn set_time_speed(me as Mod, factor: f32) -> Status {
        crate::world::daytime::set_speed(me.loader, me.index, factor)?;
        Ok(())
    }

    fn game_settings(loader as Caller, [out, size] as Buffer<u8>, preset as Out<u32>) -> Status {
        preset.put(u32::from(crate::world::game_settings::read(loader, out)?))
    }

    fn set_game_settings(
        loader as Caller,
        data: *const u8,
        mask: *const u8,
        size: u32,
    ) -> Status {
        if data.is_null() {
            if !mask.is_null() || size != 0 {
                return Err(Status::BAD_ARGS.into());
            }
            crate::world::game_settings::forget(loader)?;
            return Ok(());
        }
        let len = usize::try_from(size).map_err(|_| Status::BAD_ARGS)?;
        // SAFETY: `data` is non-null, checked at the top; the ABI has the mod
        // pass `size` readable bytes there, unchanged for the call.
        let bytes = unsafe { slice(data, len) };
        if mask.is_null() {
            crate::world::game_settings::write(loader, bytes)?;
            return Ok(());
        }
        // SAFETY: `mask` is non-null, checked just before; the ABI has the mod
        // pass `size` readable bytes there, unchanged for the call.
        let mask = unsafe { slice(mask, len) };
        crate::world::game_settings::keep(loader, bytes, mask)?;
        Ok(())
    }

    fn weather_at(loader as Caller, position as In<[f64; 3]>, out as HeadedOut<WeatherSample>) -> Status {
        let position = enshwick_core::ecs::checked_fixed(position).ok_or(Status::BAD_ARGS)?;
        let [clear, rain, snow, blizzard] = crate::world::weather::sample(loader, position)?;
        out.put(enshwick_abi::headed!(WeatherSample { clear, rain, snow, blizzard }))
    }

    fn weather_fronts(loader as Caller, [out, entry_size, capacity, count] as SizedListOut<WeatherFront>) -> Status {
        out.put(&crate::world::weather::fronts(loader)?)
    }

    fn set_weather(loader as Caller, effect: u32, duration: i64) -> Status {
        crate::world::weather::set(loader, effect, duration)?;
        Ok(())
    }

    fn terrain_materials(
        loader as Caller,
        [out, entry_size, capacity, count] as SizedListOut<TerrainMaterialInfo>,
    ) -> Status {
        out.put(&crate::world::terrain::materials(loader)?)
    }

    fn terrain_material_add(
        me as Mod,
        material as Headed<TerrainMaterialAdd>,
        id as Out<u8>,
    ) -> Status {
        let owner = (me.index, me.record.joins_when_on());
        crate::content::terrain_materials::in_time()?;
        declared_content(me.record, material.guid)?;
        id.put(crate::content::terrain_materials::add(me.loader, owner, &material)?)
    }

    fn terrain_edit(me as Asker, edit as Headed<TerrainEdit>, amount as Out<f32>) -> Status {
        let changed = crate::world::voxels::changes::by(me.handle, || {
            crate::world::terrain::edit(me.loader, &edit)
        })?;
        amount.put(changed)
    }

    fn voxel_read(
        loader as Caller,
        min as In<[u32; 3]>,
        size as In<[u32; 3]>,
        [out, capacity] as Buffer<Voxel>,
    ) -> Status {
        let request = crate::world::voxels::ReadBox::new(min, size, capacity_of(out))?;
        let words = crate::world::voxels::read(loader, &request)?;
        for (slot, &word) in out.iter_mut().zip(&words) {
            *slot = crate::world::voxels::decode(word);
        }
        Ok(())
    }
}

/// The capacity of a mod's voxel buffer, as the ABI counts it.
fn capacity_of(out: &[Voxel]) -> u64 {
    u64::try_from(out.len()).unwrap_or(u64::MAX)
}
