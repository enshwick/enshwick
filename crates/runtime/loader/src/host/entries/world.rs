//! Host functions for the world domain: players, entities, chat, labels,
//! notices, the world's key, map markers and build rights.

use enshwick_core::abi::{AccountActionType, ModHandle, Status, Str};
use enshwick_core::{Entity, abi as api_frame};

use crate::actions::console::accounts::Act;
use crate::host::memory::{put, put_headed, put_headed_list, text};
use crate::host::{caller, caller_index, calling, entry};

pub(crate) mod altars;
pub(crate) mod buffs;
pub(crate) mod environment;
pub(crate) mod saves;
pub(crate) mod transactions;

pub(crate) extern "system" fn progress_write(
    me: ModHandle,
    entity: Entity,
    what: u32,
    value: u32,
) -> Status {
    entry("progress_write", || {
        match caller_index(me).and_then(|(loader, index)| {
            crate::knowledge::progress::write(loader, index, entity.0, what, value)
        }) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn item_info(
    me: ModHandle,
    item: enshwick_core::ItemId,
    out: *mut api_frame::ItemData,
) -> Status {
    entry("item_info", || {
        match caller(me)
            .and_then(|loader| crate::inventory::transactions::item_info(loader, item.get()))
        {
            // SAFETY: `HostApi::item_info` has the mod pass `out` null or
            // pointing at its version of `ItemData`, writable for the size its
            // header states, that no other thread touches during the call.
            Ok(data) => unsafe { put_headed(out, data) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn player(
    me: ModHandle,
    entity: Entity,
    out: *mut api_frame::PlayerInfo,
) -> Status {
    entry("player", || {
        let found = caller(me)
            .and_then(crate::engine::world::players)
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
        let players = match caller(me).and_then(crate::engine::world::players) {
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

pub(crate) extern "system" fn chat(
    me: ModHandle,
    to: enshwick_core::PlayerKey,
    message: Str,
) -> Status {
    entry("chat", || {
        let Some(message) = text(message) else {
            return Status::BAD_ARGS;
        };
        match caller(me) {
            Ok(_) => crate::actions::chat::send(to.get(), message),
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn set_label(me: ModHandle, entity: Entity, label: Str) -> Status {
    entry("set_label", || {
        let Some(label) = text(label) else {
            return Status::BAD_ARGS;
        };
        match caller(me) {
            Ok(loader) => crate::world::label::set(loader, entity, label),
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
        crate::engine::conflicts::note(
            "rename filter",
            index,
            crate::engine::conflicts::Mode::Chained,
            crate::world::label::register(index, callback, user),
        )
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
            .and_then(|loader| crate::world::entities::with_component(loader, type_hash))
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

pub(crate) extern "system" fn destroy(me: ModHandle, entity: Entity) -> Status {
    entry("destroy", || {
        match caller(me).and_then(|loader| crate::world::entities::destroy(loader, entity)) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn save_world(me: ModHandle) -> Status {
    entry("save_world", || {
        match caller(me).and_then(crate::base::saves::request) {
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
            crate::base::saves::world_key(loader).map_or(Status::MISSING, |key| unsafe {
                put(out, Str::new(crate::base::saves::stem(key)))
            })
        }
        Err(status) => status,
    })
}

pub(crate) extern "system" fn world_quiet(me: ModHandle) -> Status {
    entry("world_quiet", || {
        match caller(me).and_then(crate::engine::world::quiet_world) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn map_markers(
    me: ModHandle,
    out: *mut enshwick_core::abi::MapMarkerInfo,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("map_markers", || {
        let markers = match caller(me).and_then(crate::world::scene::markers) {
            Ok(markers) => markers,
            Err(status) => return status,
        };
        // SAFETY: the ABI has the mod pass `out` null or writable for
        // `capacity` entries of `entry_size` bytes and `count` null or a
        // writable `u32`, untouched by the mod during the call.
        unsafe { crate::host::memory::list(&markers, out, entry_size, capacity, count) }
    })
}

pub(crate) extern "system" fn map_frame(
    me: ModHandle,
    out: *mut enshwick_core::abi::MapFrameInfo,
) -> Status {
    entry("map_frame", || {
        match caller(me).and_then(crate::world::scene::frame_info) {
            // SAFETY: `HostApi::map_frame` has the mod pass `out` null or
            // pointing at its version of `MapFrameInfo`, writable for the
            // size its header states, that no other thread touches during
            // the call.
            Ok(info) => unsafe { put_headed(out, info) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn component_write(
    me: ModHandle,
    entity: Entity,
    type_hash: u32,
    out: *mut *mut core::ffi::c_void,
) -> Status {
    entry("component_write", || {
        match caller_index(me).and_then(|(loader, index)| {
            let data = crate::world::writes::component_mut(loader, entity.0, type_hash)?;
            crate::engine::conflicts::note_write(type_hash, index);
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

pub(crate) extern "system" fn notify(
    me: ModHandle,
    key: enshwick_core::PlayerKey,
    text: Str,
) -> Status {
    entry("notify", || {
        // SAFETY: the text is borrowed for the call.
        let Some(text) = (unsafe { text.as_str() }) else {
            return Status::BAD_ARGS;
        };
        match caller(me).and_then(|loader| crate::ui::notices::send(loader, key.get(), text)) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn build_rights(
    me: ModHandle,
    check: *mut enshwick_core::abi::BuildRights,
) -> Status {
    use enshwick_core::abi::BuildRights;
    entry("build_rights", || {
        // SAFETY: `HostApi::build_rights` has the mod pass `check` null or
        // pointing at a `BuildRights` readable for the size its header
        // states, borrowed for the call.
        let Some(mut request) = (unsafe {
            enshwick_core::abi::read_headed(
                check,
                <BuildRights as enshwick_core::abi::Headed>::BORN,
            )
        }) else {
            return Status::BAD_ARGS;
        };
        request.refusal =
            match caller(me).and_then(|loader| crate::world::rights::check(loader, &request)) {
                Ok(refusal) => refusal,
                Err(status) => return status,
            };
        // SAFETY: `check` is non-null (read above) and `HostApi::build_rights`
        // has the mod pass it writable for the size its header states,
        // untouched by other threads during the call.
        if unsafe { enshwick_core::abi::write_headed(check, request) } {
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
        // The mod's buffer, not game memory.
        // ast-grep-ignore: game-memory-through-raw
        let low = unsafe { min.read_unaligned() };
        // SAFETY: `HostApi::world_persists` has the mod pass `max` readable
        // for three `f64`, borrowed for the call; non-null by the check
        // before it.
        // The mod's buffer, not game memory.
        // ast-grep-ignore: game-memory-through-raw
        let high = unsafe { max.read_unaligned() };
        let edit = match crate::world::rights::EditBox::between(low, high) {
            Ok(edit) => edit,
            Err(status) => return status,
        };
        match caller(me).and_then(|loader| crate::world::rights::kept_by_save(loader, edit)) {
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

pub(crate) extern "system" fn account_action(
    me: ModHandle,
    action: AccountActionType,
    account: enshwick_core::PlayerKey,
) -> Status {
    entry("account_action", || {
        let act = match action {
            AccountActionType::KICK => Act::Kick,
            AccountActionType::BAN => Act::Ban,
            AccountActionType::UN_BAN => Act::Unban,
            _ => return Status::BAD_ARGS,
        };
        match caller(me)
            .and_then(|loader| crate::actions::console::account_action(loader, act, account.get()))
        {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

/// `HostApi::chat_mute_set`: mutes `account` in chat or lifts its mute.
pub(crate) extern "system" fn chat_mute_set(
    me: ModHandle,
    account: enshwick_core::PlayerKey,
    muted: u8,
) -> Status {
    entry("chat_mute_set", || {
        if account == enshwick_core::PlayerKey::new(0) || muted > 1 {
            return Status::BAD_ARGS;
        }
        let (loader, record) = match calling(me) {
            Ok((loader, _, record)) if crate::actions::chat_mute::serving(loader) => {
                (loader, record)
            }
            Ok(_) => return Status::MISSING,
            Err(status) => return status,
        };
        let who = format!("the mod {}", record.id);
        match crate::actions::chat_mute::set(loader, account.get(), muted == 1, &who) {
            Ok(()) => Status::OK,
            Err(crate::actions::chat_mute::SetMuteError::Protected) => Status::REFUSED,
            Err(crate::actions::chat_mute::SetMuteError::NoWorld) => Status::MISSING,
            Err(e) => {
                crate::log!(WARN, "chat mutes: {who} changed nothing: {e}");
                Status::REFUSED
            }
        }
    })
}

/// `HostApi::chat_mute_has`: whether `account` is muted in chat.
pub(crate) extern "system" fn chat_mute_has(
    me: ModHandle,
    account: enshwick_core::PlayerKey,
    muted: *mut u8,
) -> Status {
    entry("chat_mute_has", || {
        if account == enshwick_core::PlayerKey::new(0) || muted.is_null() {
            return Status::BAD_ARGS;
        }
        let loader = match caller(me) {
            Ok(loader) if crate::actions::chat_mute::serving(loader) => loader,
            Ok(_) => return Status::MISSING,
            Err(status) => return status,
        };
        let has = crate::actions::chat_mute::has(loader, account.get());
        // SAFETY: the caller passes `muted` writable for one byte for the
        // call, as `HostApi::chat_mute_has` states; `put` refuses null.
        unsafe { put(muted, u8::from(has)) }
    })
}
