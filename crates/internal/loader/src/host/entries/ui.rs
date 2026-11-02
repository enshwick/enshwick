//! Host functions for the mod's interface.

use enshwick::abi as api_frame;
use enshwick::abi::{ModHandle, Side, Status};

use crate::host::memory::{get_headed, get_headed_list, put_headed};
use crate::host::{caller_index, entry};

pub(crate) extern "system" fn ui_frame(
    me: ModHandle,
    callback: api_frame::UiFrameFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("ui_frame", || match caller_index(me) {
        Ok((loader, index)) => crate::ui::register_frame(loader, index, callback, user),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn ui_widget(
    me: ModHandle,
    frame: u64,
    widget: *mut api_frame::UiWidget,
) -> Status {
    entry("ui_widget", || {
        let (loader, index) = match caller_index(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        // SAFETY: a mod passes null or its version of the widget, and all of
        // it is required.
        let Some(mut copy) =
            (unsafe { get_headed(widget, <api_frame::UiWidget as enshwick::abi::Headed>::BORN) })
        else {
            return Status::BAD_ARGS;
        };
        let status = crate::ui::widget(loader, index, frame, &mut copy);
        // SAFETY: `HostApi::ui_widget` has the mod pass `widget` null or
        // pointing at its version of `UiWidget`, writable for the size its
        // header states, that no other thread touches during the call.
        match unsafe { put_headed(widget, copy) } {
            Status::OK => status,
            refused => refused,
        }
    })
}

pub(crate) extern "system" fn ui_hud(
    me: ModHandle,
    callback: api_frame::UiFrameFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("ui_hud", || match caller_index(me) {
        Ok((loader, index)) => crate::ui::register_hud(loader, index, callback, user),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn ui_markers(
    me: ModHandle,
    markers: *const api_frame::UiMarker,
    count: u32,
) -> Status {
    entry("ui_markers", || {
        let (loader, index) = match caller_index(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if loader.side != Side::CLIENT {
            return Status::MISSING;
        }
        // SAFETY: a mod passes `count` markers of its version, valid for the
        // call, and all of a marker is required.
        let Some(given) = (unsafe {
            get_headed_list(
                markers,
                count,
                <api_frame::UiMarker as enshwick::abi::Headed>::BORN,
            )
        }) else {
            return Status::BAD_ARGS;
        };
        if given.iter().any(|m| m.marker_type != 0) {
            let types = match crate::scene::marker_types(loader) {
                Ok(types) => types,
                Err(status) => return status,
            };
            let listed = |id: u32| types.iter().any(|t| t.marker_id == id);
            if given
                .iter()
                .any(|m| m.marker_type != 0 && !listed(m.marker_type))
            {
                return Status::BAD_ARGS;
            }
        }
        crate::ui::set_markers(index, &given)
    })
}

pub(crate) extern "system" fn map_marker_types(
    me: ModHandle,
    out: *mut api_frame::MapMarkerTypeInfo,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("map_marker_types", || {
        let types =
            match caller_index(me).and_then(|(loader, _)| crate::scene::marker_types(loader)) {
                Ok(types) => types,
                Err(status) => return status,
            };
        // SAFETY: the ABI has the mod pass `out` null or writable for
        // `capacity` entries of `entry_size` bytes and `count` null or a
        // writable `u32`, untouched by the mod during the call.
        unsafe { crate::host::entries::mods::list(&types, out, entry_size, capacity, count) }
    })
}

pub(crate) extern "system" fn ui_dialog(
    me: ModHandle,
    dialog: *const api_frame::UiDialog,
    callback: api_frame::UiAnswerFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("ui_dialog", || {
        let (loader, index) = match caller_index(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        // SAFETY: a mod passes null or its version of the dialog, and all of
        // it is required.
        let Some(dialog) =
            (unsafe { get_headed(dialog, <api_frame::UiDialog as enshwick::abi::Headed>::BORN) })
        else {
            return Status::BAD_ARGS;
        };
        crate::ui::mod_dialog(loader, index, &dialog, callback, user)
    })
}

pub(crate) extern "system" fn fog_open(me: ModHandle) -> Status {
    entry("fog_open", || match caller_index(me) {
        Ok((loader, index)) => crate::fog::open(loader, index),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn fog_read(
    me: ModHandle,
    info: *mut api_frame::FogInfo,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    out: *mut u8,
    capacity: u32,
) -> Status {
    entry("fog_read", || {
        let index = match caller_index(me) {
            Ok((_, index)) => index,
            Err(status) => return status,
        };
        if info.is_null() {
            return Status::BAD_ARGS;
        }
        let wanted = width != 0 && height != 0;
        let Ok(len) = usize::try_from(capacity) else {
            return Status::BAD_ARGS;
        };
        let out: &mut [u8] = if !wanted {
            &mut []
        } else if out.is_null() {
            return Status::BAD_ARGS;
        } else {
            // SAFETY: `HostApi::fog_read` has the mod pass `capacity`
            // writable bytes at a non-null (checked earlier) `out`, untouched
            // by the mod during the call.
            unsafe { core::slice::from_raw_parts_mut(out, len) }
        };
        let mut found = api_frame::FogInfo::EMPTY;
        let status = crate::fog::read(index, &mut found, [x, y], [width, height], out);
        if status == Status::REFUSED || status == Status::MISSING {
            return status;
        }
        // SAFETY: `HostApi::fog_read` has the mod pass `info` pointing at
        // its version of `FogInfo` (non-null, checked earlier), writable for
        // the size its header states and untouched by the mod meanwhile.
        let written = unsafe { put_headed(info, found) };
        if written.is_ok() { status } else { written }
    })
}
