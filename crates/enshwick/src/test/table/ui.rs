//! The fake's interface: frames, widgets, markers and dialogs.

use core::ffi::c_void;

use crate::abi::{
    FogInfo, ModHandle, Side, Status, UiAnswerFn, UiDialog, UiFrameFn, UiMarker, UiWidget,
};
use crate::test::table::{State, text, with};

/// table it sees for the process, so it is one static.
pub(crate) extern "system" fn ui_frame(
    _me: ModHandle,
    callback: UiFrameFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        if s.host.side == Side::SERVER {
            return Status::MISSING;
        }
        s.frames.push((callback, user.expose_provenance()));
        Status::OK
    })
}

/// The input waiting for the widget labelled `text`, taken.
pub(crate) fn input(s: &mut State, text: &str) -> Option<crate::test::Input> {
    let at = s.inputs.iter().position(|(t, _)| t == text)?;
    Some(s.inputs.remove(at).1)
}

/// A progress bar's filled part, as the loader draws it; `None` for a bar
/// the loader refuses.
pub(crate) fn fraction(w: &UiWidget) -> Option<f32> {
    let fine = w.min.is_finite() && w.max.is_finite() && w.value.is_finite() && w.min < w.max;
    fine.then(|| (w.value.clamp(w.min, w.max) - w.min) / (w.max - w.min))
}

/// Draws a slider row as the loader does, taking its waiting input.
pub(crate) fn slider(
    s: &mut State,
    w: &mut UiWidget,
    label: String,
    read_only: bool,
) -> Result<crate::test::Drawn, Status> {
    if !(w.min.is_finite() && w.max.is_finite() && w.value.is_finite()) {
        return Err(Status::BAD_ARGS);
    }
    let (min, max) = (w.min.min(w.max), w.max.max(w.min));
    w.value = w.value.clamp(min, max);
    if let (false, Some(crate::test::Input::Value(value))) = (read_only, input(s, &label))
        && value.is_finite()
    {
        w.value = value.clamp(min, max);
        w.result = 1;
    }
    Ok(crate::test::Drawn::Slider {
        label,
        value: w.value,
        min: w.min,
        max: w.max,
        steps: w.steps,
        read_only,
    })
}

/// Records a HUD callback's widget; `None` outside a HUD callback.
pub(crate) fn hud_widget(s: &mut State, frame: u64, w: &UiWidget, label: &str) -> Option<Status> {
    let (token, list) = s.hud.as_mut()?;
    if *token != frame {
        return None;
    }
    let drawn = match w.kind {
        UiWidget::LABEL => crate::test::Drawn::Label {
            text: label.to_owned(),
            style: w.style,
        },
        UiWidget::PROGRESS => match fraction(w) {
            Some(fraction) => crate::test::Drawn::Progress { fraction },
            None => return Some(Status::BAD_ARGS),
        },
        _ => return Some(Status::REFUSED),
    };
    list.push((w.anchor, drawn));
    Some(Status::OK)
}

pub(crate) extern "system" fn ui_widget(
    _me: ModHandle,
    frame: u64,
    widget: *mut UiWidget,
) -> Status {
    use crate::test::{Drawn, Input};
    // SAFETY: the safe layer passes its own widget, valid for the call.
    let Some(w) = (unsafe { widget.as_mut() }) else {
        return Status::BAD_ARGS;
    };
    let label = text(w.text);
    let options: Vec<String> = if w.options.is_null() {
        Vec::new()
    } else {
        // SAFETY: the safe layer passes `option_count` texts.
        unsafe { core::slice::from_raw_parts(w.options, w.option_count as usize) }
            .iter()
            .map(|o| text(*o))
            .collect()
    };
    with(Status::MISSING, |s| {
        if let Some(status) = hud_widget(s, frame, w, &label) {
            return status;
        }
        let open = match s.frame.as_ref() {
            Some((token, open, _)) if *token == frame => *open,
            _ => return Status::WRONG_THREAD,
        };
        let read_only = w.flags & UiWidget::READ_ONLY != 0;
        w.result = 0;
        let drawn = match w.kind {
            UiWidget::WINDOW if !open => Drawn::Window {
                id: label,
                title: text(w.detail),
            },
            UiWidget::END if open => Drawn::End,
            UiWidget::SOUND => Drawn::Sound(w.style),
            UiWidget::WINDOW | UiWidget::END => return Status::REFUSED,
            _ if !open => return Status::REFUSED,
            UiWidget::LABEL => Drawn::Label {
                text: label,
                style: w.style,
            },
            UiWidget::BUTTON => {
                if !read_only && matches!(input(s, &label), Some(Input::Click)) {
                    w.result = 1;
                }
                Drawn::Button {
                    text: label,
                    enabled: !read_only,
                }
            }
            UiWidget::TOGGLE => {
                if let (false, Some(Input::Toggle(on))) = (read_only, input(s, &label)) {
                    w.index = u32::from(on);
                    w.result = 1;
                }
                Drawn::Toggle {
                    label,
                    on: w.index != 0,
                    read_only,
                }
            }
            UiWidget::SLIDER => match slider(s, w, label, read_only) {
                Ok(drawn) => drawn,
                Err(status) => return status,
            },
            UiWidget::DROPDOWN => {
                if let (false, Some(Input::Pick(index))) = (read_only, input(s, &label))
                    && index < options.len()
                {
                    w.index = u32::try_from(index).unwrap_or(0);
                    w.result = 1;
                }
                Drawn::Dropdown {
                    label,
                    options,
                    index: w.index as usize,
                    read_only,
                }
            }
            UiWidget::PROGRESS => match fraction(w) {
                Some(fraction) => Drawn::Progress { fraction },
                None => return Status::BAD_ARGS,
            },
            _ => return Status::MISSING,
        };
        if let Some((_, open, list)) = s.frame.as_mut() {
            match drawn {
                Drawn::Window { .. } => *open = true,
                Drawn::End => *open = false,
                _ => {}
            }
            list.push(drawn);
        }
        Status::OK
    })
}

pub(crate) extern "system" fn ui_hud(
    _me: ModHandle,
    callback: UiFrameFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        if s.host.side == Side::SERVER {
            return Status::MISSING;
        }
        s.huds.push((callback, user.expose_provenance()));
        Status::OK
    })
}

pub(crate) extern "system" fn ui_markers(
    _me: ModHandle,
    markers: *const UiMarker,
    count: u32,
) -> Status {
    if count as usize > crate::ui::MAX_MARKERS || (markers.is_null() && count > 0) {
        return Status::BAD_ARGS;
    }
    let given = if count == 0 {
        &[][..]
    } else {
        // SAFETY: the safe layer passes `count` markers, valid for the call.
        unsafe { core::slice::from_raw_parts(markers, count as usize) }
    };
    let markers: Vec<crate::test::FakeMarker> = given
        .iter()
        .map(|m| crate::test::FakeMarker {
            position: m.position,
            label: text(m.label),
            color: m.color,
        })
        .collect();
    with(Status::MISSING, |s| {
        if s.host.side == Side::SERVER {
            return Status::MISSING;
        }
        s.markers = markers;
        Status::OK
    })
}

pub(crate) extern "system" fn ui_dialog(
    _me: ModHandle,
    dialog: *const UiDialog,
    callback: UiAnswerFn,
    user: *mut c_void,
) -> Status {
    // SAFETY: the safe layer passes its own dialog, valid for the call.
    let Some(d) = (unsafe { dialog.as_ref() }) else {
        return Status::BAD_ARGS;
    };
    let shown = crate::test::ShownDialog {
        title: text(d.title),
        message: text(d.message),
        detail: text(d.detail),
        buttons: d.buttons,
    };
    with(Status::MISSING, |s| {
        if s.host.side == Side::SERVER {
            return Status::MISSING;
        }
        if s.dialogs.len() >= 16 {
            return Status::REFUSED;
        }
        s.dialogs.push((shown, callback, user.expose_provenance()));
        Status::OK
    })
}

pub(crate) extern "system" fn fog_open(_me: ModHandle) -> Status {
    with(Status::MISSING, |s| {
        if s.host.side == Side::SERVER {
            return Status::MISSING;
        }
        s.fog_opened = true;
        Status::OK
    })
}

pub(crate) extern "system" fn fog_read(
    _me: ModHandle,
    info: *mut FogInfo,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    out: *mut u8,
    capacity: u32,
) -> Status {
    let wanted = width != 0 && height != 0;
    if info.is_null() || (wanted && out.is_null()) {
        return Status::BAD_ARGS;
    }
    with(Status::MISSING, |s| {
        if !s.fog_opened {
            return Status::REFUSED;
        }
        let Some((found, pixels)) = s.host.fog.as_ref() else {
            return Status::MISSING;
        };
        // SAFETY: the safe layer passes a writable `FogInfo` for the call.
        unsafe { info.write(*found) };
        if !wanted {
            return Status::OK;
        }
        let inside = |start: u32, size: u32, limit: u32| {
            start.checked_add(size).is_some_and(|end| end <= limit)
        };
        if !inside(x, width, found.width) || !inside(y, height, found.height) {
            return Status::OUTSIDE_WORLD;
        }
        let (row, rows) = (width as usize, height as usize);
        if (capacity as usize) < row * rows {
            return Status::BAD_ARGS;
        }
        for r in 0..rows {
            let start = (y as usize + r) * found.width as usize + x as usize;
            let to = out.wrapping_add(r * row);
            // SAFETY: the safe layer passes `capacity` writable bytes at
            // `out`, at least `row * rows` (checked above), so `row` bytes
            // at `to` lie inside them.
            unsafe { core::ptr::copy_nonoverlapping(pixels[start..start + row].as_ptr(), to, row) };
        }
        Status::OK
    })
}
