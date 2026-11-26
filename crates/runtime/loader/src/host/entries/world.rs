//! Host functions for the world domain: players, entities, labels,
//! notices, the world's key, map markers and build rights.

use enshwick_core::abi::{Status, Str};
use enshwick_core::{Entity, abi as api_frame};

use crate::engine::conflicts::Mode;
use crate::host::layer::{
    Callback, Caller, Headed, HeadedInOut, HeadedOut, In, ListOut, Mod, Out, SizedListOut, Text,
    entries,
};

pub(crate) mod altars;
pub(crate) mod buffs;
pub(crate) mod environment;
pub(crate) mod saves;
pub(crate) mod transactions;

/// [`Status::BAD_ARGS`] for the account key no player has.
pub(crate) fn account(key: enshwick_core::PlayerKey) -> Result<u64, Status> {
    if key == enshwick_core::PlayerKey::new(0) {
        Err(Status::BAD_ARGS)
    } else {
        Ok(key.get())
    }
}

entries! {
    fn respawn_point(
        loader as Caller,
        character: Entity,
        out as HeadedOut<api_frame::RespawnPointInfo>,
    ) -> Status {
        out.put(crate::world::respawn::respawn_point(loader, character)?)
    }

    fn set_respawn_point(loader as Caller, character: Entity, spawn_point: Entity) -> Status {
        crate::world::respawn::set_respawn_point(loader, character, spawn_point)?;
        Ok(())
    }

    fn prop_placement_info(
        loader as Caller,
        item: enshwick_core::ItemId,
        out as HeadedOut<api_frame::PropPlacementInfo>,
    ) -> Status {
        out.put(crate::world::transactions::prop_placement_info(loader, item.get())?)
    }

    fn language_of(_me as Mod, key: enshwick_core::PlayerKey, out as Out<u32>) -> Status {
        out.put(crate::base::net::language_number(key.get()))
    }

    fn player(loader as Caller, entity: Entity, out as HeadedOut<api_frame::PlayerInfo>) -> Status {
        out.put(crate::engine::world::player(loader, entity)?)
    }

    fn players(
        loader as Caller,
        [out, entry_size, capacity, count] as SizedListOut<api_frame::PlayerInfo>,
    ) -> Status {
        out.put(&crate::engine::world::players(loader)?)
    }

    fn set_label(loader as Caller, entity: Entity, label as Text) -> Status {
        Ok(crate::world::label::set(loader, entity, label)?)
    }

    fn rename_filter(
        me as Mod,
        callback as Callback<api_frame::RenameFilterFn>,
        user: *mut core::ffi::c_void,
    ) -> Status {
        me.claim(
            "rename filter",
            Mode::Chained,
            crate::world::label::register(me.index, callback, user),
        )
    }

    fn join_rule(
        me as Mod,
        callback as Callback<api_frame::JoinRuleFn>,
        user: *mut core::ffi::c_void,
    ) -> Status {
        me.claim(
            "join rule",
            Mode::Chained,
            crate::world::join_gate::register(me.loader, me.index, callback, user),
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
        let key = crate::base::saves::save_key(loader).ok_or(Status::UNAVAILABLE)?;
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

    fn waypoint(
        loader as Caller,
        entity: Entity,
        out as HeadedOut<api_frame::WaypointData>,
    ) -> Status {
        out.put(crate::world::waypoints::read_now(loader, entity)?)
    }

    fn fog_uncover(
        loader as Caller,
        player: enshwick_core::PlayerKey,
        x: f32,
        z: f32,
        radius: f32,
    ) -> Status {
        Ok(crate::world::fog::uncover_for(loader, player.get(), [x, z], radius)?)
    }

    fn component_write(
        me as Mod,
        entity: Entity,
        type_hash: u32,
        out as Out<*mut core::ffi::c_void>,
    ) -> Status {
        let data = crate::world::writes::component_mut(me.loader, entity.get(), type_hash)?;
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

    /// `HostApi::change_permissions`: replaces the mod's change to a
    /// player's permissions.
    fn change_permissions(
        me as Mod,
        key: enshwick_core::PlayerKey,
        deny: u32,
        grant: u32,
    ) -> Status {
        let account = account(key)?;
        crate::world::permissions::change(me.loader, me.index, account, deny, grant)?;
        Ok(())
    }

    fn spawn(
        loader as Caller,
        request as Headed<enshwick_core::abi::SpawnRequest>,
        out as Out<Entity>,
    ) -> Status {
        out.put(Entity::from_engine(crate::world::spawns::spawn(loader, &request)?))
    }
}
