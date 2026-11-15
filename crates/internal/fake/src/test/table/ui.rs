//! The fake's interface: frames, widgets, markers and dialogs.

use core::ffi::c_void;

use crate::test::table::{Registered, State, text, with};
use enshwick_core::__fake::SCREENS;
use enshwick_core::abi::{
    FogInfo, MapMarkerTypeInfo, ModHandle, Side, Status, UiAnswerFn, UiDialog, UiExtension,
    UiFrameFn, UiMarker, UiWidget,
};

/// The screens on which the player is in the world with no window open,
/// as the loader's controls gate counts them; a panel there is refused.
const OPEN: [&str; 7] = [
    "InGame",
    "BlueprintMultiTool",
    "OvergrowthMultiTool",
    "DecayMultiTool",
    "ScrewdriverMultiToolBuildingMode",
    "TakeScreenshot",
    "NoUi",
];

/// table it sees for the process, so it is one static.
pub(crate) extern "system" fn ui_frame(
    me: ModHandle,
    callback: Option<UiFrameFn>,
    user: *mut c_void,
) -> Status {
    let Some(callback) = callback else {
        return Status::BAD_ARGS;
    };
    with(Status::MISSING, |s| {
        if s.registration == crate::test::table::Registration::Closed {
            return Status::TOO_LATE;
        }
        if s.host.side == Side::SERVER {
            return Status::UNAVAILABLE;
        }
        s.frames.push(Registered::new(callback, user, me));
        Status::OK
    })
}

/// The local player's held controls, as the test set them; the loader's
/// answers: [`Status::MISSING`] on a server, outside a game and at the
/// first ask, which starts the reads.
pub(crate) extern "system" fn local_held(_me: ModHandle, out: *mut u64) -> Status {
    if out.is_null() {
        return Status::BAD_ARGS;
    }
    with(Status::MISSING, |s| {
        let asked = core::mem::replace(&mut s.local_held.asked, true);
        match s
            .local_held
            .bits
            .filter(|_| asked && s.host.side != Side::SERVER)
        {
            Some(bits) => {
                // SAFETY: the safe layer passes a writable `u64` for the call,
                // non-null by the check above.
                unsafe { out.write(bits) };
                Status::OK
            }
            None => Status::UNAVAILABLE,
        }
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

/// Draws the key hint of the game action `action` as the loader does;
/// `None` for an action the fake build does not name.
fn input_hint(s: &State, w: &mut UiWidget, action: String) -> Option<crate::test::Drawn> {
    let &(_, bound) = s.game_actions.iter().find(|(name, _)| *name == action)?;
    w.result = u32::from(bound);
    Some(crate::test::Drawn::InputHint {
        action,
        label: text(w.detail),
        bound,
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
    me: ModHandle,
    frame: u64,
    widget: *mut UiWidget,
) -> Status {
    let Some(mut copy) = super::get_headed(widget.cast_const()) else {
        return Status::BAD_ARGS;
    };
    // The loader refuses a widget outside a callback or from a handle not
    // the drawing callback's.
    if with(false, |s| s.drawing != Some(me)) {
        return Status::WRONG_THREAD;
    }
    let status = draw(frame, &mut copy);
    match super::put_headed(widget, copy) {
        Status::OK => status,
        refused => refused,
    }
}

/// Whether the archive or the mod holds a sound container with GUID `guid`,
/// as the loader checks before it plays one.
fn holds_sound(s: &State, guid: enshwick_core::content::Guid) -> bool {
    s.host
        .type_hashes
        .get(enshwick_core::__fake::ids::SOUND_CONTAINER)
        .is_some_and(|&hash| {
            s.host.resources.contains_key(&(guid, hash, 0))
                || s.served
                    .iter()
                    .any(|served| served.0.key.guid == guid && served.0.key.type_hash == hash)
        })
}

/// Draws the widget `w` in `frame` and writes its answer into it.
fn draw(frame: u64, w: &mut UiWidget) -> Status {
    use crate::test::{Drawn, Input};
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
            UiWidget::END if open && s.extending != Some(frame) => Drawn::End,
            UiWidget::SOUND if w.sound == enshwick_core::content::Guid::NIL => {
                Drawn::Sound(w.style)
            }
            UiWidget::SOUND => {
                if !holds_sound(s, w.sound) {
                    return Status::MISSING;
                }
                Drawn::Play(w.sound)
            }
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
            UiWidget::INPUT_HINT => match input_hint(s, w, label) {
                Some(drawn) => drawn,
                None => return Status::MISSING,
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
    me: ModHandle,
    callback: Option<UiFrameFn>,
    user: *mut c_void,
) -> Status {
    let Some(callback) = callback else {
        return Status::BAD_ARGS;
    };
    with(Status::MISSING, |s| {
        if s.registration == crate::test::table::Registration::Closed {
            return Status::TOO_LATE;
        }
        if s.host.side == Side::SERVER {
            return Status::UNAVAILABLE;
        }
        s.huds.push(Registered::new(callback, user, me));
        Status::OK
    })
}

/// Records an extension of a game screen, refusing what the loader
/// refuses, in its order: a server, an unknown edge or a screen where the
/// player is in the world with no window open, a screen the game lacks.
pub(crate) extern "system" fn ui_extension(
    me: ModHandle,
    spec: *const UiExtension,
    callback: Option<UiFrameFn>,
    user: *mut c_void,
) -> Status {
    let Some(callback) = callback else {
        return Status::BAD_ARGS;
    };
    let Some(spec) = super::get_headed(spec) else {
        return Status::BAD_ARGS;
    };
    let screen = text(spec.screen);
    // SAFETY: the safe layer passes `hosts` null or with `host_count` GUIDs
    // for the call; they are copied here.
    let Some(hosts) = (unsafe { spec.host_list() }).map(<[enshwick_core::content::Guid]>::to_vec)
    else {
        return Status::BAD_ARGS;
    };
    with(Status::MISSING, |s| {
        if s.registration == crate::test::table::Registration::Closed {
            return Status::TOO_LATE;
        }
        if s.host.side == Side::SERVER {
            return Status::UNAVAILABLE;
        }
        if spec.edge > UiExtension::BOTTOM || OPEN.contains(&screen.as_str()) {
            return Status::BAD_ARGS;
        }
        if !SCREENS.contains(&screen.as_str()) {
            return Status::MISSING;
        }
        s.extensions.push((
            screen,
            spec.edge,
            Registered::new(callback, user, me),
            hosts,
        ));
        Status::OK
    })
}

pub(crate) extern "system" fn ui_markers(
    _me: ModHandle,
    markers: *const UiMarker,
    count: u32,
) -> Status {
    if with(true, |s| s.host.side == Side::SERVER) {
        return Status::UNAVAILABLE;
    }
    if count as usize > enshwick_core::__fake::MARKER_LIMIT {
        return Status::BAD_ARGS;
    }
    let Some(given) = super::get_headed_list(markers, count) else {
        return Status::BAD_ARGS;
    };
    let markers: Vec<crate::test::FakeMarker> = given
        .iter()
        .map(|m| crate::test::FakeMarker {
            position: m.position,
            label: text(m.label),
            color: m.color,
            marker_type: m.marker_type,
        })
        .collect();
    with(Status::MISSING, |s| {
        if s.host.side == Side::SERVER {
            return Status::UNAVAILABLE;
        }
        for marker in markers.iter().filter(|m| m.marker_type != 0) {
            let Some(types) = &s.host.map_marker_types else {
                return Status::UNAVAILABLE;
            };
            if !types.iter().any(|t| t.marker_id == marker.marker_type) {
                return Status::BAD_ARGS;
            }
        }
        s.markers = markers;
        Status::OK
    })
}

pub(crate) extern "system" fn map_marker_types(
    _me: ModHandle,
    out: *mut MapMarkerTypeInfo,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let Some(types) = with(None, |s| {
        (s.host.side != Side::SERVER)
            .then(|| s.host.map_marker_types.clone())
            .flatten()
    }) else {
        return Status::UNAVAILABLE;
    };
    crate::test::table::base::io::list(&types, out, entry_size, capacity, count)
}

pub(crate) extern "system" fn ui_dialog(
    me: ModHandle,
    dialog: *const UiDialog,
    callback: Option<UiAnswerFn>,
    user: *mut c_void,
) -> Status {
    let Some(callback) = callback else {
        return Status::BAD_ARGS;
    };
    let Some(d) = super::get_headed(dialog) else {
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
            return Status::UNAVAILABLE;
        }
        if s.dialogs.len() >= 16 {
            return Status::REFUSED;
        }
        s.dialogs.push((shown, Registered::new(callback, user, me)));
        Status::OK
    })
}

pub(crate) extern "system" fn request_fog(_me: ModHandle) -> Status {
    with(Status::MISSING, |s| {
        if s.registration == crate::test::table::Registration::Closed {
            return Status::TOO_LATE;
        }
        if s.host.side == Side::SERVER {
            return Status::UNAVAILABLE;
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
            return Status::UNAVAILABLE;
        };
        let written = super::put_headed(info, *found);
        if written != Status::OK {
            return written;
        }
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

/// A cached client sampler status supplied by the test; no sampling is requested.
pub(crate) extern "system" fn local_input_status(
    _me: ModHandle,
    out: *mut enshwick_core::abi::LocalInputInfo,
) -> Status {
    if out.is_null() {
        return Status::BAD_ARGS;
    }
    with(Status::MISSING, |s| {
        if s.host.side == Side::SERVER {
            return Status::UNAVAILABLE;
        }
        super::put_headed(out, s.local_input_info)
    })
}

/// Queues a client command as the loader does: unavailable on a server,
/// a bad argument for an unknown kind, refused while one of its kind waits;
/// the test drives its status.
pub(crate) extern "system" fn client_command(
    _me: ModHandle,
    spec: *const enshwick_core::abi::ClientCommand,
    id: *mut u64,
) -> Status {
    use enshwick_core::abi::{ClientCommand as Spec, ClientCommandStatus as Raw};
    let Some(spec) = super::get_headed(spec) else {
        return Status::BAD_ARGS;
    };
    if id.is_null() {
        return Status::BAD_ARGS;
    }
    with(Status::MISSING, |s| {
        if s.host.side == Side::SERVER {
            return Status::UNAVAILABLE;
        }
        let open = |status: &Raw| status.state == Raw::PENDING || status.state == Raw::APPLIED;
        if spec.kind == Spec::CANCEL {
            if let Some((.., status)) = s
                .client_commands
                .iter_mut()
                .find(|(queued, .., status)| *queued == spec.cancels && open(status))
            {
                status.state = Raw::CANCELLED;
                status.reason = Raw::BY_MOD;
            }
            // SAFETY: the safe layer passes a writable `u64`, non-null above.
            unsafe { id.write(spec.cancels) };
            return Status::OK;
        }
        if !(Spec::OPEN_SCREEN..=Spec::HIGHLIGHT_RECIPE).contains(&spec.kind)
            || !spec.in_bounds()
            || !named_as_the_loader_takes(&spec)
        {
            return Status::BAD_ARGS;
        }
        if s.client_commands
            .iter()
            .any(|(_, kind, _, status)| *kind == spec.kind && open(status))
        {
            return Status::REFUSED;
        }
        let next = s.client_commands.len() as u64 + 1;
        // SAFETY: the mod's spec, its target borrowed for the call.
        let target = unsafe { spec.target.as_str() }
            .unwrap_or_default()
            .to_owned();
        s.client_commands.push((
            next,
            spec.kind,
            target,
            Raw {
                kind: spec.kind,
                ..Raw::EMPTY
            },
        ));
        // SAFETY: the safe layer passes a writable `u64`, non-null above.
        unsafe { id.write(next) };
        Status::OK
    })
}

/// Whether `spec`'s target and expected screen are ones the loader takes:
/// an open of a screen the game opens from the world, a tab the menu has,
/// and a look or move that expects only the world's screen.
fn named_as_the_loader_takes(spec: &enshwick_core::abi::ClientCommand) -> bool {
    // SAFETY: the mod's spec, its target borrowed for the call.
    let target = unsafe { spec.target.as_str() };
    // SAFETY: the mod's spec, its expected screen borrowed for the call.
    let expected = unsafe { spec.expected_screen.as_str() };
    target.zip(expected).is_some_and(|(target, expected)| {
        enshwick_core::__fake::rules::client_command_names(spec.kind, target, expected)
    })
}

/// Where a queued client command stands, as the test set it.
pub(crate) extern "system" fn client_command_status(
    _me: ModHandle,
    id: u64,
    out: *mut enshwick_core::abi::ClientCommandStatus,
) -> Status {
    if out.is_null() {
        return Status::BAD_ARGS;
    }
    with(Status::MISSING, |s| {
        match s.client_commands.iter().find(|(queued, ..)| *queued == id) {
            Some((.., status)) => super::put_headed(out, *status),
            None => Status::UNAVAILABLE,
        }
    })
}

/// The client snapshot the test set; the loader's answers: unavailable on
/// a server.
pub(crate) extern "system" fn client_snapshot(
    _me: ModHandle,
    out: *mut enshwick_core::abi::ClientSnapshot,
) -> Status {
    if out.is_null() {
        return Status::BAD_ARGS;
    }
    with(Status::MISSING, |s| {
        if s.host.side == Side::SERVER {
            return Status::UNAVAILABLE;
        }
        super::put_headed(out, s.client_snapshot)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_screen_without_a_window_is_one_the_game_names() {
        assert!(OPEN.iter().all(|open| SCREENS.contains(open)));
    }
}
