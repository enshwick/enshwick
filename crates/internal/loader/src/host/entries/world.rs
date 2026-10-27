//! Host functions for the world: players, entities, chat, time, settings, weather, terrain, voxels and build rights.

use enshwick::Entity;
use enshwick::abi as api_frame;
use enshwick::abi::{Level, ModHandle, Side, Status, Str};

use crate::host::memory::{put, put_headed, put_headed_list, text};
use crate::host::{caller, caller_index, calling, entry, loader};
use crate::locks::Relock;

pub(crate) extern "system" fn component(
    me: ModHandle,
    entity: u32,
    type_hash: u32,
    out: *mut *const core::ffi::c_void,
) -> Status {
    entry("component", || {
        match caller(me)
            .and_then(|loader| crate::world::component(loader, Entity(entity), type_hash))
        {
            // SAFETY: `HostApi::component` has the mod pass `out` null or
            // pointing at a writable, aligned `c_void` that no other thread
            // touches during the call.
            Ok(data) => unsafe { put(out, data) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn entity_template(
    me: ModHandle,
    entity: u32,
    out: *mut [u8; 16],
) -> Status {
    entry("entity_template", || {
        match caller(me).and_then(|loader| crate::world::template(loader, Entity(entity))) {
            // SAFETY: `HostApi::entity_template` has the mod pass `out` null or
            // pointing at a writable, aligned `[u8; 16]` that no other thread
            // touches during the call.
            Ok(guid) => unsafe { put(out, guid) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn player(
    me: ModHandle,
    entity: u32,
    out: *mut api_frame::PlayerInfo,
) -> Status {
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
            // SAFETY: `HostApi::player` has the mod pass `out` null or pointing
            // at its version of `PlayerInfo`, writable for the size its header
            // states, that no other thread touches during the call.
            Ok(info) => unsafe { put_headed(out, info) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn players(
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
        // SAFETY: `HostApi::players` has the mod pass `out` null or pointing at
        // `capacity` entries of its version of `PlayerInfo`, each writable for
        // the size the first entry's header states, and `count` null or
        // pointing at a writable, aligned `u32`, neither touched by another
        // thread during the call.
        unsafe { put_headed_list(&players, out, capacity, count) }
    })
}

pub(crate) extern "system" fn hosting() -> u8 {
    entry("hosting", || {
        loader().map_or(0, |loader| {
            u8::from(loader.side == Side::SERVER || crate::world::hosting())
        })
    })
}

pub(crate) extern "system" fn server_world() -> u64 {
    entry("server_world", crate::world::server_world)
}

pub(crate) extern "system" fn chat(me: ModHandle, to: u64, message: Str) -> Status {
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

pub(crate) extern "system" fn set_label(me: ModHandle, entity: u32, label: Str) -> Status {
    entry("set_label", || {
        let Some(label) = text(label) else {
            return Status::BAD_ARGS;
        };
        match caller(me) {
            Ok(loader) => crate::label::set(loader, Entity(entity), label),
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn rename_filter(
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

pub(crate) extern "system" fn feature(me: ModHandle, name: Str, off: Str) -> Status {
    entry("feature", || {
        let (Some(name), Some(off)) = (text(name), text(off)) else {
            return Status::BAD_ARGS;
        };
        if name.is_empty() {
            return Status::BAD_ARGS;
        }
        let record = match calling(me) {
            Ok((_, _, record)) => record,
            Err(status) => return status,
        };
        let changed = crate::features::report(
            &mut record.features.relock(),
            name,
            Some(off).filter(|off| !off.is_empty()),
        );
        if let Some(line) = changed {
            let level = if off.is_empty() {
                Level::INFO
            } else {
                Level::WARN
            };
            crate::log::write(level, &record.id, &line);
        }
        Status::OK
    })
}

pub(crate) extern "system" fn status(
    me: ModHandle,
    callback: api_frame::StatusFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("status", || {
        let (_, index, _) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        // A source added later would outlive a reload of the mod unseen.
        if !crate::reload::in_init(index.get()) {
            return Status::TOO_LATE;
        }
        crate::status::register(index, callback, user)
    })
}

pub(crate) extern "system" fn actions(
    me: ModHandle,
    callback: api_frame::ActionFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("actions", || {
        let (_, index, record) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if !crate::reload::in_init(index.get()) {
            return Status::TOO_LATE;
        }
        let again = crate::reload::reinitialising(index.get());
        let status = crate::actions::register(index, callback, user, again);
        if status.is_ok() {
            crate::log::write(Level::DEBUG, &record.id, "player actions");
        }
        status
    })
}

thread_local! {
    /// The answer `run_console` last handed out on this thread.
    static CONSOLE_REPLY: core::cell::RefCell<String> = const { core::cell::RefCell::new(String::new()) };
}

pub(crate) extern "system" fn run_console(me: ModHandle, line: Str, out: *mut Str) -> Status {
    entry("run_console", || {
        let Some(line) = text(line) else {
            return Status::BAD_ARGS;
        };
        let loader = match caller(me) {
            Ok(loader) => loader,
            Err(status) => return status,
        };
        match crate::console::run(loader, line) {
            Ok(reply) => CONSOLE_REPLY.with_borrow_mut(|kept| {
                *kept = reply;
                // The text stays in this thread's buffer until its next call.
                // SAFETY: `HostApi::run_console` has the mod pass `out` null or
                // pointing at a writable, aligned `Str` that no other thread
                // touches during the call.
                unsafe { put(out, Str::new(kept)) }
            }),
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn game_event(
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

pub(crate) extern "system" fn entities(
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
                // SAFETY: `out` is non-null, checked just before, and `index` is
                // below `capacity`; the ABI has the mod pass `capacity` writable,
                // aligned entries at `out`.
                unsafe { out.wrapping_add(index).write(*place) };
            }
        }
        // SAFETY: `HostApi::entities` has the mod pass `count` null or pointing
        // at a writable, aligned `u32` that no other thread touches during the
        // call.
        unsafe { put(count, u32::try_from(found.len()).unwrap_or(u32::MAX)) }
    })
}

pub(crate) extern "system" fn destroy(me: ModHandle, entity: u32) -> Status {
    entry("destroy", || {
        match caller(me).and_then(|loader| crate::entities::destroy(loader, Entity(entity))) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn save_world(me: ModHandle) -> Status {
    entry("save_world", || {
        match caller(me).and_then(crate::saves::request) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn world_key(me: ModHandle, out: *mut Str) -> Status {
    entry("world_key", || match caller(me) {
        Ok(loader) => {
            // SAFETY: `HostApi::world_key` has the mod pass `out` null or
            // pointing at a writable, aligned `Str` that no other thread
            // touches during the call.
            crate::saves::world_key(loader)
                .map_or(Status::MISSING, |key| unsafe { put(out, Str::new(key)) })
        }
        Err(status) => status,
    })
}

pub(crate) extern "system" fn world_quiet(me: ModHandle) -> Status {
    entry("world_quiet", || {
        match caller(me).and_then(crate::world::quiet_world) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn map_markers(
    me: ModHandle,
    out: *mut enshwick::abi::MapMarkerInfo,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("map_markers", || {
        let markers = match caller(me).and_then(crate::scene::markers) {
            Ok(markers) => markers,
            Err(status) => return status,
        };
        // SAFETY: the ABI has the mod pass `out` null or writable for
        // `capacity` entries of `entry_size` bytes and `count` null or a
        // writable `u32`, untouched by the mod during the call.
        unsafe { crate::host::entries::mods::list(&markers, out, entry_size, capacity, count) }
    })
}

pub(crate) extern "system" fn component_write(
    me: ModHandle,
    entity: u32,
    type_hash: u32,
    out: *mut *mut core::ffi::c_void,
) -> Status {
    entry("component_write", || {
        match caller_index(me).and_then(|(loader, index)| {
            let data = crate::writes::component_mut(loader, entity, type_hash)?;
            crate::conflicts::note_write(type_hash, index);
            Ok(data)
        }) {
            // SAFETY: `HostApi::component_write` has the mod pass `out` null or
            // pointing at a writable, aligned `c_void` that no other thread
            // touches during the call.
            Ok(data) => unsafe { put(out, data) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn emit(
    me: ModHandle,
    type_hash: u32,
    data: *const u8,
    size: u32,
) -> Status {
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

pub(crate) extern "system" fn teleport(
    me: ModHandle,
    request: *const enshwick::abi::TeleportRequest,
) -> Status {
    entry("teleport", || {
        // SAFETY: the caller passes null or a request with its header; an
        // all-zero request is valid.
        let Some(request) = (unsafe {
            enshwick::abi::read_headed(request, size_of::<enshwick::abi::TeleportRequest>())
        }) else {
            return Status::BAD_ARGS;
        };
        match caller(me).and_then(|loader| crate::emits::teleport(loader, &request)) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn spawn(
    me: ModHandle,
    request: *const enshwick::abi::SpawnRequest,
    out: *mut u32,
) -> Status {
    entry("spawn", || {
        // SAFETY: the caller passes null or a request with its header; an
        // all-zero request is valid.
        let Some(request) = (unsafe {
            enshwick::abi::read_headed(request, size_of::<enshwick::abi::SpawnRequest>())
        }) else {
            return Status::BAD_ARGS;
        };
        match caller(me).and_then(|loader| crate::spawns::spawn(loader, &request)) {
            // SAFETY: `HostApi::spawn` has the mod pass `out` null or pointing
            // at a writable, aligned `u32` that no other thread touches during
            // the call.
            Ok(entity) => unsafe { put(out, entity) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn notify(me: ModHandle, key: u64, text: Str) -> Status {
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

pub(crate) extern "system" fn time_of_day(
    me: ModHandle,
    out: *mut enshwick::abi::DaytimeInfo,
) -> Status {
    entry("time_of_day", || {
        match caller(me).and_then(crate::daytime::info) {
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
        match caller(me).and_then(|loader| crate::daytime::set_time(loader, time)) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn time_speed_control(me: ModHandle) -> Status {
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

pub(crate) extern "system" fn set_time_speed(me: ModHandle, factor: f32) -> Status {
    entry("set_time_speed", || {
        match caller_index(me)
            .and_then(|(loader, index)| crate::daytime::set_speed(loader, index, factor))
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
        match caller(me).and_then(|loader| crate::game_settings::read(loader, out)) {
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
        match caller(me).and_then(|loader| crate::game_settings::write(loader, bytes)) {
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
        match caller(me).and_then(|loader| crate::game_settings::keep(loader, bytes, mask)) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn forget_game_settings(me: ModHandle) -> Status {
    entry("forget_game_settings", || {
        match caller(me).and_then(crate::game_settings::forget) {
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
        match caller(me).and_then(|loader| crate::weather::sample(loader, position)) {
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
    out: *mut enshwick::abi::WeatherFront,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("weather_fronts", || {
        match caller(me).and_then(crate::weather::fronts) {
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
        match caller(me).and_then(|loader| crate::weather::set(loader, effect, duration)) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn terrain_materials(
    me: ModHandle,
    out: *mut enshwick::abi::TerrainMaterialInfo,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("terrain_materials", || {
        match caller(me).and_then(crate::terrain::materials) {
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
    edit: *const enshwick::abi::TerrainEdit,
    amount: *mut f32,
) -> Status {
    entry("terrain_edit", || {
        // SAFETY: the caller passes null or an edit with its header; an
        // all-zero edit is valid.
        let Some(edit) =
            (unsafe { enshwick::abi::read_headed(edit, size_of::<enshwick::abi::TerrainEdit>()) })
        else {
            return Status::BAD_ARGS;
        };
        match caller(me).and_then(|loader| crate::terrain::edit(loader, &edit)) {
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
    out: *mut enshwick::abi::Voxel,
    capacity: u64,
) -> Status {
    entry("voxel_read", || {
        if min.is_null() || size.is_null() || out.is_null() {
            return Status::BAD_ARGS;
        }
        // SAFETY: `min` is non-null, checked above; `HostApi::voxel_read` has
        // the mod pass it pointing at three `u32`s borrowed for the call, and
        // an unaligned read needs no alignment.
        let min = unsafe { min.read_unaligned() };
        // SAFETY: `size` is non-null, checked above; `HostApi::voxel_read`
        // has the mod pass it pointing at three `u32`s borrowed for the call,
        // and an unaligned read needs no alignment.
        let size = unsafe { size.read_unaligned() };
        let request = match crate::voxels::ReadBox::new(min, size, capacity) {
            Ok(request) => request,
            Err(status) => return status,
        };
        let words = match caller(me).and_then(|loader| crate::voxels::read(loader, &request)) {
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
            unsafe { slot.write(crate::voxels::decode(word)) };
        }
        Status::OK
    })
}

pub(crate) extern "system" fn build_rights(
    me: ModHandle,
    check: *mut enshwick::abi::BuildRights,
) -> Status {
    use enshwick::abi::BuildRights;
    entry("build_rights", || {
        // SAFETY: `HostApi::build_rights` has the mod pass `check` null or
        // pointing at a `BuildRights` readable for the size its header
        // states, borrowed for the call.
        let Some(mut request) =
            (unsafe { enshwick::abi::read_headed(check, size_of::<BuildRights>()) })
        else {
            return Status::BAD_ARGS;
        };
        request.refusal = match caller(me).and_then(|loader| crate::rights::check(loader, &request))
        {
            Ok(refusal) => refusal,
            Err(status) => return status,
        };
        // SAFETY: `check` is non-null (read above) and `HostApi::build_rights`
        // has the mod pass it writable for the size its header states,
        // untouched by other threads during the call.
        if unsafe { enshwick::abi::write_headed(check, request) } {
            Status::OK
        } else {
            Status::BAD_ARGS
        }
    })
}

pub(crate) extern "system" fn world_persists(
    me: ModHandle,
    min: *const [f64; 3],
    max: *const [f64; 3],
    kept: *mut u8,
) -> Status {
    entry("world_persists", || {
        if min.is_null() || max.is_null() || kept.is_null() {
            return Status::BAD_ARGS;
        }
        // SAFETY: `HostApi::world_persists` has the mod pass `min` readable
        // for three `f64`, borrowed for the call; non-null by the check
        // before it.
        let low = unsafe { min.read_unaligned() };
        // SAFETY: as for `min`, `max` readable for three `f64`.
        let high = unsafe { max.read_unaligned() };
        let edit = match crate::rights::EditBox::between(low, high) {
            Ok(edit) => edit,
            Err(status) => return status,
        };
        match caller(me).and_then(|loader| crate::rights::kept_by_save(loader, edit)) {
            Ok(saved) => {
                // SAFETY: `HostApi::world_persists` has the mod pass `kept`
                // writable for one byte, untouched by other threads during the
                // call; non-null by the check before it.
                unsafe { kept.write(u8::from(saved)) };
                Status::OK
            }
            Err(status) => status,
        }
    })
}
