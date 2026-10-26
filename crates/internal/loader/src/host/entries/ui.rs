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
        let Some(mut copy) = (unsafe { get_headed(widget, size_of::<api_frame::UiWidget>()) })
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
        let Some(given) =
            (unsafe { get_headed_list(markers, count, size_of::<api_frame::UiMarker>()) })
        else {
            return Status::BAD_ARGS;
        };
        crate::ui::set_markers(index, &given)
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
        let Some(dialog) = (unsafe { get_headed(dialog, size_of::<api_frame::UiDialog>()) }) else {
            return Status::BAD_ARGS;
        };
        crate::ui::mod_dialog(loader, index, &dialog, callback, user)
    })
}
