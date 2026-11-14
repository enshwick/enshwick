//! Host functions for the mod's interface.

use enshwick_core::abi as api_frame;
use enshwick_core::abi::Status;

use crate::host::layer::guards::on_client;
use crate::host::layer::{
    Caller, Headed, HeadedInOut, HeadedList, HeadedOut, MaybeBuffer, Mod, Out, SizedListOut,
    checked, entries,
};

entries! {
    fn local_held(loader as Caller, out as Out<u64>) -> Status {
        on_client(loader)?;
        out.put(crate::actions::local_held::held()?)
    }

    fn ui_frame(me as Mod, callback: api_frame::UiFrameFn, user: *mut core::ffi::c_void) -> Status {
        checked(crate::ui::interface::register_frame(me.loader, me.index, callback, user))
    }

    fn ui_widget(me as Mod, frame: u64, widget as HeadedInOut<api_frame::UiWidget>) -> Status {
        let (mut copy, slot) = widget;
        let status = crate::ui::interface::widget(me.loader, me.index, frame, &mut copy);
        slot.put(copy)?;
        Ok(status)
    }

    fn ui_hud(me as Mod, callback: api_frame::UiFrameFn, user: *mut core::ffi::c_void) -> Status {
        checked(crate::ui::interface::register_hud(me.loader, me.index, callback, user))
    }

    fn ui_extension(
        me as Mod,
        spec as Headed<api_frame::UiExtension>,
        callback: api_frame::UiFrameFn,
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
        checked(crate::ui::interface::register_extension(
            me.loader, me.index, screen, spec.edge, hosts, callback, user,
        ))
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
        checked(crate::ui::interface::set_markers(me.index, &markers))
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
        callback: api_frame::UiAnswerFn,
        user: *mut core::ffi::c_void,
    ) -> Status {
        checked(crate::ui::interface::mod_dialog(me.loader, me.index, &dialog, callback, user))
    }

    fn fog_open(me as Mod) -> Status {
        checked(crate::world::fog::open(me.loader, me.index))
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
        let status = crate::world::fog::read(me.index, &mut found, [x, y], [width, height], out);
        if status == Status::REFUSED || status == Status::MISSING {
            return Err(status.into());
        }
        info.put(found)?;
        Ok(status)
    }
}
