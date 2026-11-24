//! Host functions for the mod's interface.

use enshwick_core::abi as api_frame;
use enshwick_core::abi::Status;

use crate::host::layer::guards::on_client;
use crate::host::layer::{
    Callback, Caller, Headed, HeadedInOut, HeadedList, HeadedOut, List, MaybeBuffer, Mod, Out,
    SizedListOut, Text, entries,
};

entries! {
    fn local_input_status(loader as Caller, out as HeadedOut<api_frame::LocalInputInfo>) -> Status {
        on_client(loader)?;
        out.put(crate::actions::local_held::status())
    }

    fn client_snapshot(loader as Caller, out as HeadedOut<api_frame::ClientSnapshot>) -> Status {
        on_client(loader)?;
        let scene = crate::base::saves::world_key(loader).map(crate::base::saves::stem);
        out.put(crate::actions::client_snapshot::snapshot(loader, scene))
    }

    fn client_command(
        me as Mod,
        spec as Headed<api_frame::ClientCommand>,
        id as Out<u64>,
    ) -> Status {
        id.put(crate::actions::client_commands::queue(me.loader, me.index, &spec)?)
    }

    fn client_command_status(
        me as Mod,
        id: u64,
        out as HeadedOut<api_frame::ClientCommandStatus>,
    ) -> Status {
        on_client(me.loader)?;
        let status = crate::actions::client_commands::status(me.index, id).ok_or(Status::UNAVAILABLE)?;
        out.put(status)
    }

    fn sound_play(me as Mod, spec as Headed<api_frame::SoundPlay>, id as Out<u64>) -> Status {
        id.put(crate::ui::sound::play(me.loader, me.index, &spec)?)
    }

    fn sound_steer(me as Mod, id: u64, steer as Headed<api_frame::SoundSteer>) -> Status {
        Ok(crate::ui::sound::steer(me.loader, me.index, id, &steer)?)
    }

    fn sound_state(me as Mod, id: u64, out as HeadedOut<api_frame::SoundInfo>) -> Status {
        out.put(crate::ui::sound::state(me.loader, me.index, id)?)
    }

    fn sound_play_for(
        me as Mod,
        audience as Headed<api_frame::SoundAudience>,
        spec as Headed<api_frame::SoundPlay>,
        id as Out<u64>,
    ) -> Status {
        id.put(crate::ui::sound::play_for(me.loader, me.index, &audience, &spec)?)
    }

    fn sound_steer_for(me as Mod, id: u64, steer as Headed<api_frame::SoundSteer>) -> Status {
        Ok(crate::ui::sound::steer_for(me.loader, me.index, id, &steer)?)
    }

    fn local_held(loader as Caller, out as Out<u64>) -> Status {
        on_client(loader)?;
        out.put(crate::actions::local_held::held()?)
    }

    fn ui_frame(me as Mod, callback as Callback<api_frame::UiFrameFn>, user: *mut core::ffi::c_void) -> Status {
        Ok(crate::ui::interface::register_frame(me.loader, me.index, callback, user)?)
    }

    fn ui_item_tooltip(
        me as Mod,
        callback as Callback<api_frame::UiTooltipFn>,
        user: *mut core::ffi::c_void,
    ) -> Status {
        Ok(crate::ui::interface::tooltips::register(me.loader, me.index, callback, user)?)
    }

    fn ui_widget(me as Mod, frame: u64, widget as HeadedInOut<api_frame::UiWidget>) -> Status {
        let (mut copy, slot) = widget;
        let drawn = crate::ui::interface::widget(me.loader, me.index, frame, &mut copy);
        slot.put(copy)?;
        Ok(drawn?)
    }

    fn ui_hud(me as Mod, callback as Callback<api_frame::UiFrameFn>, user: *mut core::ffi::c_void) -> Status {
        Ok(crate::ui::interface::register_hud(me.loader, me.index, callback, user)?)
    }

    fn ui_extension(
        me as Mod,
        spec as Headed<api_frame::UiExtension>,
        callback as Callback<api_frame::UiFrameFn>,
        user: *mut core::ffi::c_void,
    ) -> Status {
        // SAFETY: the mod's screen name stays alive and unchanged for this
        // call by the ABI's borrow rule; `register_extension` resolves it
        // before it returns.
        let screen = unsafe { spec.screen.as_str() }.ok_or(Status::BAD_ARGS)?;
        // SAFETY: `HostApi::ui_extension` has the mod pass `hosts` null or
        // readable for `host_count` GUIDs for the call, which
        // `register_extension` copies before it returns.
        let hosts = unsafe { spec.host_list() }.ok_or(Status::BAD_ARGS)?;
        let place = crate::ui::place::Place::of(&spec);
        Ok(crate::ui::interface::register_extension(
            me.loader, me.index, screen, place, bytemuck::cast_slice(hosts), callback, user,
        )?)
    }

    fn ui_markers(me as Mod, [markers, count] as HeadedList<api_frame::UiMarker>) -> Status {
        on_client(me.loader)?;
        if markers.iter().any(|m| m.marker_type != 0) {
            let types = crate::world::scene::marker_types(me.loader)?;
            let listed = |id: u32| types.iter().any(|t| t.marker_id == id);
            if markers
                .iter()
                .any(|m| m.marker_type != 0 && !listed(m.marker_type))
            {
                return Err(Status::BAD_ARGS.into());
            }
        }
        Ok(crate::ui::interface::set_markers(me.index, &markers)?)
    }

    fn map_marker_types(
        me as Mod,
        [out, entry_size, capacity, count] as SizedListOut<api_frame::MapMarkerTypeInfo>,
    ) -> Status {
        out.put(&crate::world::scene::marker_types(me.loader)?)
    }

    fn ui_dialog(
        me as Mod,
        dialog as Headed<api_frame::UiDialog>,
        callback as Callback<api_frame::UiAnswerFn>,
        user: *mut core::ffi::c_void,
    ) -> Status {
        Ok(crate::ui::interface::mod_dialog(me.loader, me.index, &dialog, callback, user)?)
    }

    fn request_fog(me as Mod) -> Status {
        Ok(crate::world::fog::open(me.loader, me.index)?)
    }

    fn fog_read(
        me as Mod,
        info as HeadedOut<api_frame::FogInfo>,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        [out, capacity] as MaybeBuffer<u8>,
    ) -> Status {
        let out: &mut [u8] = match out {
            _ if width == 0 || height == 0 => &mut [],
            Some(out) => out,
            None => return Err(Status::BAD_ARGS.into()),
        };
        let mut found = api_frame::FogInfo::EMPTY;
        let read = crate::world::fog::read(me.index, &mut found, [x, y], [width, height], out)?;
        info.put(found)?;
        Ok(read?)
    }

    fn action_window(
        me as Mod,
        id as Text,
        [players, count] as List<enshwick_core::PlayerKey>,
        change: u32,
    ) -> Status {
        if players.len() > crate::ui::windows::MAX_PLAYERS {
            return Err(Status::BAD_ARGS.into());
        }
        Ok(crate::ui::windows::show(me.loader, me.index, id, players, change)?)
    }
}
