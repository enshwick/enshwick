//! The fake's interface: frames, widgets, markers and dialogs.

use crate::test::table::base::args;
use crate::test::table::entries;
use core::ffi::c_void;

mod sound;

use crate::test::table::{Registered, State, text, with};
use enshwick_core::__fake::SCREENS;
use enshwick_core::abi::{
    FogInfo, MapMarkerTypeInfo, ModHandle, Side, Status, UiAnswerFn, UiDialog, UiExtension,
    UiFrameFn, UiMarker, UiTooltipFn, UiTooltipItem, UiWidget,
};
#[cfg(feature = "unstable")]
pub(crate) use sound::hear;
pub(crate) use sound::{sound_play, sound_play_for, sound_state, sound_steer, sound_steer_for};

/// Where an extension's panel sits, as its spec asks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Place {
    pub(crate) edge: u32,
    pub(crate) align: u32,
    pub(crate) shift: f32,
    pub(crate) inset: f32,
}

/// An extension of a game screen: screen name, place, callback and the host
/// templates it shows for (empty for every host).
pub(crate) type Extension = (
    String,
    Place,
    Registered<UiFrameFn>,
    Vec<enshwick_core::content::Guid>,
);

/// The local player's held controls as the test set them, `None` as outside
/// a game; whether a mod asked yet, as the loader starts reading them at the
/// first ask; and whether an interface step ran since, the first that reads
/// them.
#[derive(Default)]
pub(crate) struct Held {
    pub(crate) bits: Option<u64>,
    pub(crate) asked: bool,
    pub(crate) stepped: bool,
}

/// What the installed fake's interface answers and has recorded.
pub(crate) struct UiState {
    /// Whether the mod opened the fog of war.
    pub(crate) fog_opened: bool,
    /// Frame callbacks.
    pub(crate) frames: Vec<Registered<UiFrameFn>>,
    /// HUD callbacks.
    pub(crate) huds: Vec<Registered<UiFrameFn>>,
    /// Extensions of game screens: screen name, place, callback and the host
    /// templates it shows for (empty for every host).
    pub(crate) extensions: Vec<Extension>,
    /// The handle whose callback draws now: the loader refuses a widget
    /// from any other.
    pub(crate) drawing: Option<ModHandle>,
    /// The frame token of the extension drawing now, whose window the
    /// loader opened.
    pub(crate) extending: Option<u64>,
    /// The game screen the player has open, as frames and HUDs hear it;
    /// empty for none.
    pub(crate) open_screen: &'static str,
    /// The interaction prompt the frames hear: its title and verb.
    pub(crate) prompt: Option<(String, u32)>,
    /// The entity the open screen belongs to, 0 for none.
    pub(crate) screen_host: u32,
    /// The host's template and fixed-point position as the client's last
    /// step read them, `None` before one did.
    pub(crate) screen_host_read: Option<(enshwick_core::content::Guid, [i64; 3])>,
    /// The local player's held controls.
    pub(crate) local_held: Held,
    pub(crate) local_input_info: enshwick_core::abi::LocalInputInfo,
    /// The client snapshot the loader would write, as the test set it.
    pub(crate) client_snapshot: enshwick_core::abi::ClientSnapshot,
    /// Whether a snapshot ask started the frame's reads, and whether an
    /// interface step read them since.
    pub(crate) snapshot_reads: (bool, bool),
    /// The client commands mods queued, each with its kind, target and
    /// status, as the test drives them.
    pub(crate) client_commands: Vec<(u64, u32, String, enshwick_core::abi::ClientCommandStatus)>,
    /// The slot of an inventory screen under the cursor, as the frames
    /// hear it: the packed slot, item, count and item entity.
    pub(crate) hovered: Option<(u64, enshwick_core::ItemId, u32, enshwick_core::Entity)>,
    /// Item tooltip callbacks.
    pub(crate) tooltips: Vec<Registered<UiTooltipFn>>,
    /// The lines the tooltip callback running now added, with its token.
    pub(crate) tooltip: Option<(u64, Vec<crate::test::Drawn>)>,
    /// What the HUD drawing now drew, with each item's anchor, while one
    /// draws.
    pub(crate) hud: Option<(u64, Vec<(u32, crate::test::Drawn)>)>,
    /// The mod's markers on the map.
    pub(crate) markers: Vec<crate::test::FakeMarker>,
    /// The frame drawing now: its token, whether a window is open and what
    /// was drawn.
    pub(crate) frame: Option<(u64, bool, Vec<crate::test::Drawn>)>,
    /// Inputs waiting for the widget with their text.
    pub(crate) inputs: Vec<(String, crate::test::Input)>,
    /// Dialogs waiting for an answer, with where it goes.
    pub(crate) dialogs: Vec<(crate::test::ShownDialog, Registered<UiAnswerFn>)>,
    /// Whether the player pressed Back over a window under the dialog this
    /// frame, which closes the dialog once the frame's windows drew.
    pub(crate) dialog_backed: bool,
    /// The sounds the mod started and the starts waiting for a frame.
    pub(crate) sound: sound::SoundState,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            fog_opened: false,
            frames: Vec::new(),
            huds: Vec::new(),
            extensions: Vec::new(),
            drawing: None,
            extending: None,
            open_screen: "",
            prompt: None,
            screen_host: 0,
            screen_host_read: None,
            local_held: Held::default(),
            local_input_info: enshwick_core::abi::LocalInputInfo::EMPTY,
            client_snapshot: enshwick_core::abi::ClientSnapshot::EMPTY,
            snapshot_reads: (false, false),
            client_commands: Vec::new(),
            hovered: None,
            tooltips: Vec::new(),
            tooltip: None,
            hud: None,
            markers: Vec::new(),
            frame: None,
            inputs: Vec::new(),
            dialogs: Vec::new(),
            dialog_backed: false,
            sound: sound::SoundState::default(),
        }
    }
}

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

entries! {
    /// table it sees for the process, so it is one static.
    pub(crate) fn ui_frame(
        me as Mod,
        callback: Option<UiFrameFn> as args::Callback,
        user: *mut c_void,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        crate::test::table::with_open(|s| {
            if s.host.base.side == Side::SERVER {
                return Status::UNAVAILABLE;
            }
            s.ui.frames.push(Registered::new(callback, user, me));
            Status::OK
        })
    }

    /// The local player's held controls, as the test set them; the loader's
    /// answers: [`Status::UNAVAILABLE`] on a server, outside a game, and from
    /// the first ask, which starts the reads, until an interface step reads
    /// them.
    pub(crate) fn local_held(me as Caller, out: *mut u64 as args::Out) -> Status {
        if out.is_null() {
            return Status::BAD_ARGS;
        }
        with(Status::MISSING, |s| {
            s.ui.local_held.asked = true;
            match s
                .ui
                .local_held
                .bits
                .filter(|_| s.ui.local_held.stepped && s.host.base.side != Side::SERVER)
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
}

/// The input waiting for the widget labelled `text`, taken.
pub(crate) fn input(s: &mut State, text: &str) -> Option<crate::test::Input> {
    let at = s.ui.inputs.iter().position(|(t, _)| t == text)?;
    Some(s.ui.inputs.remove(at).1)
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
    // The glyph draws from the game's runtime bindings, rebinds included.
    let bound = s.actions.bindings.table(&action)? != [0, 0];
    w.result = u32::from(bound);
    Some(crate::test::Drawn::InputHint {
        action,
        label: text(w.detail),
        bound,
    })
}

/// Records a HUD callback's widget; `None` outside a HUD callback.
pub(crate) fn hud_widget(s: &mut State, frame: u64, w: &UiWidget, label: &str) -> Option<Status> {
    let (token, list) = s.ui.hud.as_mut()?;
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

/// Records a tooltip callback's line, as the loader keeps it: labels only,
/// at most [`UiTooltipItem::MAX_LINES`]; `None` outside a tooltip callback.
fn tooltip_widget(s: &mut State, frame: u64, w: &UiWidget, label: &str) -> Option<Status> {
    let (token, lines) = s.ui.tooltip.as_mut()?;
    if *token != frame {
        return None;
    }
    if w.kind != UiWidget::LABEL {
        return Some(Status::REFUSED);
    }
    if lines.len() >= UiTooltipItem::MAX_LINES as usize {
        return Some(Status::BUSY);
    }
    lines.push(crate::test::Drawn::Label {
        text: label.to_owned(),
        style: w.style,
    });
    Some(Status::OK)
}

/// Draws an item slot as the loader does, taking its waiting click or
/// hover; `Err` with the loader's answer for a count of 0 or an item the
/// fake build does not know.
fn item_slot(s: &mut State, w: &mut UiWidget) -> Result<crate::test::Drawn, Status> {
    if w.count == 0 {
        return Err(Status::BAD_ARGS);
    }
    if !s.host.content.items.contains_key(&w.item.get()) {
        return Err(Status::MISSING);
    }
    match input(s, &crate::test::slot_key(w.item)) {
        Some(crate::test::Input::Click) => {
            w.result = UiWidget::SLOT_CLICKED | UiWidget::SLOT_HOVERED;
        }
        Some(crate::test::Input::Hover) => w.result = UiWidget::SLOT_HOVERED,
        _ => {}
    }
    Ok(crate::test::Drawn::ItemSlot {
        item: w.item,
        count: w.count,
    })
}

entries! {
    /// Registers an item tooltip callback, refusing what the loader refuses.
    pub(crate) fn ui_item_tooltip(
        me as Mod,
        callback: Option<UiTooltipFn> as args::Callback,
        user: *mut c_void,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        crate::test::table::with_open(|s| {
            if s.host.base.side == Side::SERVER {
                return Status::UNAVAILABLE;
            }
            s.ui.tooltips.push(Registered::new(callback, user, me));
            Status::OK
        })
    }

    pub(crate) fn ui_widget(
        me as Mod,
        frame: u64,
        widget: *mut UiWidget as args::HeadedInOut,
    ) -> Status {
        let Some(mut copy) = super::get_headed(widget.cast_const()) else {
            return Status::BAD_ARGS;
        };
        // The loader refuses a widget outside a callback or from a handle not
        // the drawing callback's.
        if with(false, |s| s.ui.drawing != Some(me)) {
            return Status::WRONG_THREAD;
        }
        let status = draw(frame, &mut copy);
        match super::put_headed(widget, copy) {
            Status::OK => status,
            refused => refused,
        }
    }
}

/// Whether the archive or the mod holds a sound container with GUID `guid`,
/// as the loader checks before it plays one.
fn holds_sound(s: &State, guid: enshwick_core::content::Guid) -> bool {
    s.host
        .base
        .type_hashes
        .get(enshwick_core::__fake::ids::SOUND_CONTAINER)
        .is_some_and(|&hash| {
            s.host.content.resources.contains_key(&(guid, hash, 0))
                || s.content
                    .served
                    .iter()
                    .any(|served| served.0.key.guid == guid && served.0.key.type_hash == hash)
        })
}

/// Draws window `w` with id `id`, answering 1 when the test pressed Back
/// over it, as the loader does: only Back is the window's, and while a
/// dialog shows the dialog takes the press and closes after the frame.
fn window(s: &mut State, w: &mut UiWidget, id: String) -> crate::test::Drawn {
    let back = |(t, i): &(String, crate::test::Input)| *t == id && *i == crate::test::Input::Back;
    if let Some(at) = s.ui.inputs.iter().position(back) {
        s.ui.inputs.remove(at);
        w.result = u32::from(s.ui.dialogs.is_empty());
        s.ui.dialog_backed |= !s.ui.dialogs.is_empty();
    }
    crate::test::Drawn::Window {
        id,
        title: text(w.detail),
    }
}

/// A dropdown's options, copied; none for a null list.
fn options(w: &UiWidget) -> Vec<String> {
    if w.options.is_null() {
        return Vec::new();
    }
    // SAFETY: the safe layer passes `option_count` texts.
    unsafe { core::slice::from_raw_parts(w.options, w.option_count as usize) }
        .iter()
        .map(|o| text(*o))
        .collect()
}

/// Draws the widget `w` in `frame` and writes its answer into it.
fn draw(frame: u64, w: &mut UiWidget) -> Status {
    use crate::test::{Drawn, Input};
    let label = text(w.text);
    let options = options(w);
    with(Status::MISSING, |s| {
        let gathered = hud_widget(s, frame, w, &label);
        if let Some(status) = gathered.or_else(|| tooltip_widget(s, frame, w, &label)) {
            return status;
        }
        let open = match s.ui.frame.as_ref() {
            Some((token, open, _)) if *token == frame => *open,
            _ => return Status::WRONG_THREAD,
        };
        let read_only = w.flags & UiWidget::READ_ONLY != 0;
        w.result = 0;
        let drawn = match w.kind {
            UiWidget::WINDOW if !open => window(s, w, label),
            UiWidget::END if open && s.ui.extending != Some(frame) => Drawn::End,
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
            UiWidget::ITEM_SLOT => match item_slot(s, w) {
                Ok(drawn) => drawn,
                Err(status) => return status,
            },
            _ => return Status::MISSING,
        };
        if let Some((_, open, list)) = s.ui.frame.as_mut() {
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

entries! {
    pub(crate) fn ui_hud(
        me as Mod,
        callback: Option<UiFrameFn> as args::Callback,
        user: *mut c_void,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        crate::test::table::with_open(|s| {
            if s.host.base.side == Side::SERVER {
                return Status::UNAVAILABLE;
            }
            s.ui.huds.push(Registered::new(callback, user, me));
            Status::OK
        })
    }

    /// Records an extension of a game screen, refusing what the loader
    /// refuses, in its order: a server, a place it does not take or a screen where the
    /// player is in the world with no window open, a screen the game lacks.
    pub(crate) fn ui_extension(
        me as Mod,
        spec: *const UiExtension as args::Headed,
        callback: Option<UiFrameFn> as args::Callback,
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
        crate::test::table::with_open(|s| {
            if s.host.base.side == Side::SERVER {
                return Status::UNAVAILABLE;
            }
            if !spec.placement_holds() || OPEN.contains(&screen.as_str()) {
                return Status::BAD_ARGS;
            }
            if !SCREENS.contains(&screen.as_str()) {
                return Status::MISSING;
            }
            let place = Place {
                edge: spec.edge,
                align: spec.align,
                shift: spec.shift,
                inset: spec.inset,
            };
            s.ui.extensions
                .push((screen, place, Registered::new(callback, user, me), hosts));
            Status::OK
        })
    }

    pub(crate) fn ui_markers(
        me as Mod,
        [markers: *const UiMarker, count: u32] as args::HeadedList,
    ) -> Status {
        if with(true, |s| s.host.base.side == Side::SERVER) {
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
            if s.host.base.side == Side::SERVER {
                return Status::UNAVAILABLE;
            }
            for marker in markers.iter().filter(|m| m.marker_type != 0) {
                let Some(types) = &s.host.ui.map_marker_types else {
                    return Status::UNAVAILABLE;
                };
                if !types.iter().any(|t| t.marker_id == marker.marker_type) {
                    return Status::BAD_ARGS;
                }
            }
            s.ui.markers = markers;
            Status::OK
        })
    }

    pub(crate) fn map_marker_types(
        me as Mod,
        [out: *mut MapMarkerTypeInfo, entry_size: u32, capacity: u32, count: *mut u32] as args::SizedListOut,
    ) -> Status {
        let Some(types) = with(None, |s| {
            (s.host.base.side != Side::SERVER)
                .then(|| s.host.ui.map_marker_types.clone())
                .flatten()
        }) else {
            return Status::UNAVAILABLE;
        };
        crate::test::table::base::io::list(&types, out, entry_size, capacity, count)
    }

    pub(crate) fn ui_dialog(
        me as Mod,
        dialog: *const UiDialog as args::Headed,
        callback: Option<UiAnswerFn> as args::Callback,
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
            if s.host.base.side == Side::SERVER {
                return Status::UNAVAILABLE;
            }
            if s.ui.dialogs.len() >= 16 {
                return Status::REFUSED;
            }
            s.ui.dialogs
                .push((shown, Registered::new(callback, user, me)));
            Status::OK
        })
    }

    pub(crate) fn request_fog(me as Mod) -> Status {
        crate::test::table::with_open(|s| {
            if s.host.base.side == Side::SERVER {
                return Status::UNAVAILABLE;
            }
            s.ui.fog_opened = true;
            Status::OK
        })
    }

    pub(crate) fn fog_read(
        me as Mod,
        info: *mut FogInfo as args::HeadedOut,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        [out: *mut u8, capacity: u32] as args::MaybeBuffer,
    ) -> Status {
        let wanted = width != 0 && height != 0;
        if info.is_null() || (wanted && out.is_null()) {
            return Status::BAD_ARGS;
        }
        with(Status::MISSING, |s| {
            if !s.ui.fog_opened {
                return Status::REFUSED;
            }
            let Some((found, pixels)) = s.host.ui.fog.as_ref() else {
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

    /// A cached client sampler status supplied by the test, requested once a
    /// `local_held` ask started the reads; this getter requests nothing.
    pub(crate) fn local_input_status(
        me as Caller,
        out: *mut enshwick_core::abi::LocalInputInfo as args::HeadedOut,
    ) -> Status {
        if out.is_null() {
            return Status::BAD_ARGS;
        }
        with(Status::MISSING, |s| {
            if s.host.base.side == Side::SERVER {
                return Status::UNAVAILABLE;
            }
            // As the loader: the first `local_held` ask started the reads.
            let mut info = s.ui.local_input_info;
            if s.ui.local_held.asked {
                info.flags |= enshwick_core::abi::LocalInputInfo::REQUESTED;
            }
            super::put_headed(out, info)
        })
    }

    /// Queues a client command as the loader does: unavailable on a server,
    /// a bad argument for an unknown kind, refused while one of its kind waits;
    /// the test drives its status.
    pub(crate) fn client_command(
        me as Mod,
        spec: *const enshwick_core::abi::ClientCommand as args::Headed,
        id: *mut u64 as args::Out,
    ) -> Status {
        use enshwick_core::abi::{ClientCommand as Spec, ClientCommandStatus as Raw};
        let Some(spec) = super::get_headed(spec) else {
            return Status::BAD_ARGS;
        };
        if id.is_null() {
            return Status::BAD_ARGS;
        }
        with(Status::MISSING, |s| {
            if s.host.base.side == Side::SERVER {
                return Status::UNAVAILABLE;
            }
            let open = |status: &Raw| status.state == Raw::PENDING || status.state == Raw::APPLIED;
            if spec.kind == Spec::CANCEL {
                if let Some((.., status)) =
                    s.ui.client_commands
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
            if !(Spec::OPEN_SCREEN..=Spec::LEAVE).contains(&spec.kind)
                || !spec.in_bounds()
                || !named_as_the_loader_takes(&spec)
            {
                return Status::BAD_ARGS;
            }
            if s.ui
                .client_commands
                .iter()
                .any(|(_, kind, _, status)| *kind == spec.kind && open(status))
            {
                return Status::REFUSED;
            }
            // One join or leave at a time; a join from the main menu, outside a
            // world, which the snapshot's session names, and a leave inside one.
            let plays = |kind: u32| kind == Spec::JOIN || kind == Spec::LEAVE;
            if plays(spec.kind) {
                let in_world = s.ui.client_snapshot.session != 0;
                let running =
                    s.ui.client_commands
                        .iter()
                        .any(|(_, kind, _, status)| plays(*kind) && open(status));
                if running || in_world == (spec.kind == Spec::JOIN) {
                    return Status::REFUSED;
                }
            }
            let next = s.ui.client_commands.len() as u64 + 1;
            // SAFETY: the mod's spec, its target borrowed for the call; the
            // password is never read, kept or returned.
            let target = unsafe { spec.target.as_str() }
                .unwrap_or_default()
                .to_owned();
            s.ui.client_commands.push((
                next,
                spec.kind,
                target,
                Raw {
                    kind: spec.kind,
                    // As the loader queues them: a join waits for its server's
                    // answer, a leave starts in the world.
                    phase: match spec.kind {
                        Spec::JOIN => Raw::PHASE_QUEUED,
                        Spec::LEAVE => Raw::PHASE_IN_WORLD,
                        _ => 0,
                    },
                    ..Raw::EMPTY
                },
            ));
            // SAFETY: the safe layer passes a writable `u64`, non-null above.
            unsafe { id.write(next) };
            Status::OK
        })
    }
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

entries! {
    /// Where a queued client command stands, as the test set it.
    pub(crate) fn client_command_status(
        me as Mod,
        id: u64,
        out: *mut enshwick_core::abi::ClientCommandStatus as args::HeadedOut,
    ) -> Status {
        if out.is_null() {
            return Status::BAD_ARGS;
        }
        with(Status::MISSING, |s| {
            match s
                .ui
                .client_commands
                .iter()
                .find(|(queued, ..)| *queued == id)
            {
                Some((.., status)) => super::put_headed(out, *status),
                None => Status::UNAVAILABLE,
            }
        })
    }

    /// The client snapshot the test set; the loader's answers: unavailable on
    /// a server, and from the first ask, which starts the reads, until an
    /// interface step reads them, the empty snapshot, never read.
    pub(crate) fn client_snapshot(
        me as Caller,
        out: *mut enshwick_core::abi::ClientSnapshot as args::HeadedOut,
    ) -> Status {
        if out.is_null() {
            return Status::BAD_ARGS;
        }
        with(Status::MISSING, |s| {
            if s.host.base.side == Side::SERVER {
                return Status::UNAVAILABLE;
            }
            s.ui.snapshot_reads.0 = true;
            let snapshot = if s.ui.snapshot_reads.1 {
                s.ui.client_snapshot
            } else {
                enshwick_core::abi::ClientSnapshot::EMPTY
            };
            super::put_headed(out, snapshot)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_screen_without_a_window_is_one_the_game_names() {
        assert!(OPEN.iter().all(|open| SCREENS.contains(open)));
    }
}
