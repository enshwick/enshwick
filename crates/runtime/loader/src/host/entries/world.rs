//! Host functions for the world domain: players, entities, chat, labels,
//! notices, the world's key, map markers and build rights.

use enshwick_core::abi::{AccountActionType, Status, Str};
use enshwick_core::{Entity, abi as api_frame};

use crate::actions::console::accounts::Act;
use crate::engine::conflicts::Mode;
use crate::host::layer::{
    Caller, HeadedInOut, HeadedListOut, HeadedOut, In, ListOut, Mod, Out, SizedListOut, Text,
    checked, entries,
};

pub(crate) mod altars;
pub(crate) mod buffs;
pub(crate) mod environment;
pub(crate) mod saves;
pub(crate) mod transactions;

/// [`Status::MISSING`] when chat mutes are not served, as on a client.
fn mutes_served(loader: &crate::engine::loader::Loader) -> Result<(), Status> {
    if crate::actions::chat_mute::serving(loader) {
        Ok(())
    } else {
        Err(Status::MISSING)
    }
}

/// [`Status::BAD_ARGS`] for the account key no player has.
fn account(key: enshwick_core::PlayerKey) -> Result<u64, Status> {
    if key == enshwick_core::PlayerKey::new(0) {
        Err(Status::BAD_ARGS)
    } else {
        Ok(key.get())
    }
}

entries! {
    fn language_of(_me as Mod, key: enshwick_core::PlayerKey) -> u32 {
        Ok(crate::base::net::language_number(key.get()))
    }

    fn progress_write(me as Mod, entity: Entity, what: u32, value: u32) -> Status {
        crate::knowledge::progress::write(me.loader, me.index, entity.0, what, value)?;
        Ok(())
    }

    fn item_info(
        loader as Caller,
        item: enshwick_core::ItemId,
        out as HeadedOut<api_frame::ItemData>,
    ) -> Status {
        out.put(crate::inventory::transactions::item_info(loader, item.get())?)
    }

    fn player(loader as Caller, entity: Entity, out as HeadedOut<api_frame::PlayerInfo>) -> Status {
        out.put(crate::engine::world::player(loader, entity)?)
    }

    fn players(
        loader as Caller,
        [out, capacity, count] as HeadedListOut<api_frame::PlayerInfo>,
    ) -> Status {
        out.put(&crate::engine::world::players(loader)?)
    }

    fn chat(_loader as Caller, to: enshwick_core::PlayerKey, message as Text) -> Status {
        checked(crate::actions::chat::send(to.get(), message))
    }

    fn set_label(loader as Caller, entity: Entity, label as Text) -> Status {
        checked(crate::world::label::set(loader, entity, label))
    }

    fn rename_filter(
        me as Mod,
        callback: api_frame::RenameCallFn,
        user: *mut core::ffi::c_void,
    ) -> Status {
        me.claim(
            "rename filter",
            Mode::Chained,
            checked(crate::world::label::register(me.index, callback, user)),
        )
    }

    fn join_rule(
        me as Mod,
        callback: api_frame::JoinRuleFn,
        user: *mut core::ffi::c_void,
    ) -> Status {
        me.claim(
            "join rule",
            Mode::Chained,
            checked(crate::world::join_gate::register(me.index, callback, user)),
        )
    }

    fn entities(
        loader as Caller,
        type_hash: u32,
        [out, capacity, count] as ListOut<api_frame::EntityPlace>,
    ) -> Status {
        out.put(&crate::world::entities::with_component(loader, type_hash)?)
    }

    fn destroy(loader as Caller, entity: Entity) -> Status {
        crate::world::entities::destroy(loader, entity)?;
        Ok(())
    }

    fn save_world(loader as Caller) -> Status {
        crate::base::saves::request(loader)?;
        Ok(())
    }

    fn world_key(loader as Caller, out as Out<Str>) -> Status {
        let key = crate::base::saves::world_key(loader).ok_or(Status::MISSING)?;
        out.put(Str::new(crate::base::saves::stem(key)))
    }

    fn world_quiet(loader as Caller) -> Status {
        crate::engine::world::quiet_world(loader)?;
        Ok(())
    }

    fn map_markers(
        loader as Caller,
        [out, entry_size, capacity, count] as SizedListOut<api_frame::MapMarkerInfo>,
    ) -> Status {
        out.put(&crate::world::scene::markers(loader)?)
    }

    fn map_frame(loader as Caller, out as HeadedOut<api_frame::MapFrameInfo>) -> Status {
        out.put(crate::world::scene::frame_info(loader)?)
    }

    fn fog_uncover(
        loader as Caller,
        player: enshwick_core::PlayerKey,
        x: f32,
        z: f32,
        radius: f32,
    ) -> Status {
        checked(crate::world::fog::uncover_for(loader, player.get(), [x, z], radius))
    }

    fn component_write(
        me as Mod,
        entity: Entity,
        type_hash: u32,
        out as Out<*mut core::ffi::c_void>,
    ) -> Status {
        let data = crate::world::writes::component_mut(me.loader, entity.0, type_hash)?;
        crate::engine::conflicts::note_write(type_hash, me.index);
        out.put(data)
    }

    fn notify(loader as Caller, key: enshwick_core::PlayerKey, text as Text) -> Status {
        crate::base::notices::send(loader, key.get(), text)?;
        Ok(())
    }

    fn build_rights(loader as Caller, check as HeadedInOut<api_frame::BuildRights>) -> Status {
        let (mut request, slot) = check;
        request.refusal = crate::world::rights::check(loader, &request)?;
        slot.put(request)
    }

    fn world_persists(
        loader as Caller,
        min as In<[f64; 3]>,
        max as In<[f64; 3]>,
        kept as Out<u8>,
    ) -> Status {
        let edit = crate::world::rights::EditBox::between(min, max)?;
        kept.put(u8::from(crate::world::rights::kept_by_save(loader, edit)?))
    }

    fn account_action(
        loader as Caller,
        action: AccountActionType,
        account: enshwick_core::PlayerKey,
    ) -> Status {
        let act = match action {
            AccountActionType::KICK => Act::Kick,
            AccountActionType::BAN => Act::Ban,
            AccountActionType::UN_BAN => Act::Unban,
            _ => return Err(Status::BAD_ARGS.into()),
        };
        crate::actions::console::account_action(loader, act, account.get())?;
        Ok(())
    }

    /// `HostApi::chat_mute_set`: mutes `account` in chat or lifts its mute.
    fn chat_mute_set(me as Mod, key: enshwick_core::PlayerKey, muted: u8) -> Status {
        let account = account(key)?;
        if muted > 1 {
            return Err(Status::BAD_ARGS.into());
        }
        mutes_served(me.loader)?;
        let who = format!("the mod {}", me.record.id);
        crate::actions::chat_mute::set(me.loader, account, muted == 1, &who)?;
        Ok(())
    }

    /// `HostApi::chat_mute_has`: whether `account` is muted in chat.
    fn chat_mute_has(loader as Caller, key: enshwick_core::PlayerKey, muted as Out<u8>) -> Status {
        let account = account(key)?;
        mutes_served(loader)?;
        muted.put(u8::from(crate::actions::chat_mute::has(loader, account)))
    }
}
