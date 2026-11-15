//! The mod's interface: frames, widgets, markers and dialogs.

use core::ffi::c_void;

use crate::abi::Metres;
use crate::abi::{Entity, Header, RecipeId, Status, Str};
use crate::content::Guid;

/// A frame of the client's interface, handed to the callbacks registered with
/// [`HostApi::ui_frame`](super::HostApi::ui_frame); widgets drawn with [`HostApi::ui_widget`](super::HostApi::ui_widget) name its
/// `token`.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, Str, UiFrame};
///
/// let frame = UiFrame {
///     header: Header::of::<UiFrame>(),
///     token: 7,
///     screen: UiFrame::IN_GAME,
///     reserved: 0,
///     hud_screen: Str::new("StorageChest"),
///     prompt_title: Str::EMPTY,
///     prompt_verb: 0,
///     screen_host: enshwick::Entity(512),
///     screen_host_template: enshwick::abi::Guid::NIL,
///     screen_host_position: [0.0; 3],
/// };
/// assert_eq!(frame.screen, UiFrame::IN_GAME);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct UiFrame {
    /// Size and version of this struct.
    pub header: Header,
    /// Names this frame in [`HostApi::ui_widget`](super::HostApi::ui_widget); valid until the callback
    /// returns.
    pub token: u64,
    /// Where the player is: [`UiFrame::MENU`], [`UiFrame::LOADING`] or
    /// [`UiFrame::IN_GAME`].
    pub screen: u32,
    /// Zero.
    pub reserved: u32,
    /// The game's name of its screen the player has open, such as
    /// `StorageChest`, a member of its `HudScreenType`; empty outside a game
    /// and on a build where the loader does not find it. The loader keeps
    /// the text for the life of the process.
    pub hud_screen: Str,
    /// The title of what the player's interaction prompt offers now, such
    /// as a chest's name, as the game shows it; empty while it offers
    /// nothing, outside a game and on a build where the loader does not
    /// find it. The game's own text, borrowed for the callback.
    pub prompt_title: Str,
    /// The game's text id (`LocaTagId`) of the prompt's verb, such as the
    /// one for "Open"; 0 while the prompt offers nothing.
    pub prompt_verb: u32,
    /// The entity of the client's world the open screen belongs to, such
    /// as the chest whose inventory shows; entity 0 for a screen of no
    /// object, outside a game and on a build where the
    /// loader does not find it.
    pub screen_host: Entity,
    /// The template GUID of `screen_host` in the client's world, as the
    /// client's last step read it; all zero for no host, before a step read
    /// this host, and on a build where the loader does not find it.
    pub screen_host_template: Guid,
    /// Where `screen_host` stands in the client's world, in
    /// metres, from the same step; meaningful only while
    /// `screen_host_template` is not all zero.
    pub screen_host_position: Metres,
}

impl_headed!(UiFrame);

impl UiFrame {
    /// The title screen and the menus before a game.
    pub const MENU: u32 = 0;
    /// A game loading, or the intro and outro.
    pub const LOADING: u32 = 1;
    /// In a game.
    pub const IN_GAME: u32 = 2;
}

/// A mod's callback for each frame of the client's interface: the frame and
/// the pointer given at registration. It must not unwind.
///
/// Null is never a callback: the loader refuses a null one with
/// [`Status::BAD_ARGS`](crate::abi::Status::BAD_ARGS) and keeps nothing.
/// Runs on the client's main thread, which draws the interface.
///
/// # Examples
///
/// The safe layer wraps it in [`Load::on_frame`](../struct.Load.html#method.on_frame) and
/// [`Load::on_hud`](../struct.Load.html#method.on_hud).
pub type UiFrameFn = unsafe extern "system" fn(frame: *const UiFrame, user: *mut c_void);

/// An element a mod adds to one of the game's screens, registered with
/// [`HostApi::ui_extension`](super::HostApi::ui_extension): a panel at an edge of
/// the screen that shows and hides with it.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, Str, UiExtension};
///
/// let spec = UiExtension {
///     header: Header::of::<UiExtension>(),
///     screen: Str::new("StorageChest"),
///     edge: UiExtension::RIGHT,
///     reserved: 0,
///     hosts: core::ptr::null(),
///     host_count: 0,
///     reserved_hosts: 0,
/// };
/// assert_eq!(spec.edge, UiExtension::RIGHT);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct UiExtension {
    /// Size and version of this struct.
    pub header: Header,
    /// The game's screen, by the name of its member of the game's screen
    /// kinds, such as `StorageChest`. Valid for the call.
    pub screen: Str,
    /// The screen's edge the panel sits at: [`UiExtension::LEFT`] to
    /// [`UiExtension::BOTTOM`].
    pub edge: u32,
    /// Zero.
    pub reserved: u32,
    /// `host_count` template GUIDs: the panel shows only while the open
    /// screen's host was made from one of them; null with a count of 0 for
    /// a panel of every host. Borrowed for the call: the loader copies
    /// them.
    pub hosts: *const Guid,
    /// How many GUIDs `hosts` holds, at most [`UiExtension::MAX_HOSTS`].
    pub host_count: u32,
    /// Zero.
    pub reserved_hosts: u32,
}

impl_headed!(UiExtension);

impl UiExtension {
    /// The left edge, centred from top to bottom.
    pub const LEFT: u32 = 0;
    /// The right edge, centred from top to bottom.
    pub const RIGHT: u32 = 1;
    /// The top edge, centred from left to right.
    pub const TOP: u32 = 2;
    /// The bottom edge, centred from left to right.
    pub const BOTTOM: u32 = 3;
    /// The most host templates one panel names.
    pub const MAX_HOSTS: u32 = 64;

    /// The host templates the panel names, empty for every host, as the
    /// loader reads them; `None` for a null list with a count or more than
    /// [`UiExtension::MAX_HOSTS`]. A spec of an older minor reads zero
    /// there, so it names no host.
    ///
    /// # Safety
    ///
    /// - `hosts` is null, or readable for `host_count` GUIDs while the
    ///   returned slice lives.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{Header, Str, UiExtension};
    /// use enshwick::content::Guid;
    ///
    /// let ours = [Guid([7; 16])];
    /// let mut spec = UiExtension {
    ///     header: Header::of::<UiExtension>(),
    ///     screen: Str::new("StorageChest"),
    ///     edge: UiExtension::RIGHT,
    ///     reserved: 0,
    ///     hosts: ours.as_ptr(),
    ///     host_count: 1,
    ///     reserved_hosts: 0,
    /// };
    /// // SAFETY: `hosts` points at `ours`, one GUID, alive below.
    /// assert_eq!(unsafe { spec.host_list() }, Some(&ours[..]));
    /// spec.hosts = core::ptr::null();
    /// // SAFETY: a null list.
    /// assert_eq!(unsafe { spec.host_list() }, None);
    /// ```
    #[must_use]
    pub unsafe fn host_list(&self) -> Option<&[Guid]> {
        match (self.host_count, self.hosts.is_null()) {
            (0, _) => Some(&[]),
            (count, false) if count <= Self::MAX_HOSTS => {
                // SAFETY: the caller's contract, `hosts` non-null (checked by
                // this arm).
                Some(unsafe { core::slice::from_raw_parts(self.hosts, count as usize) })
            }
            _ => None,
        }
    }
}

/// One widget, drawn with [`HostApi::ui_widget`](super::HostApi::ui_widget): `kind` says which, and
/// which fields it reads; the loader writes `index`, `value` and `result`
/// back.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, Str, UiWidget};
///
/// let button = UiWidget {
///     kind: UiWidget::BUTTON,
///     text: Str::new("Close"),
///     ..UiWidget::EMPTY
/// };
/// assert_eq!(button.header, Header::of::<UiWidget>());
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct UiWidget {
    /// Size and version of this struct.
    pub header: Header,
    /// What to draw: one of the kind constants, [`UiWidget::WINDOW`] to
    /// [`UiWidget::INPUT_HINT`].
    pub kind: u32,
    /// [`UiWidget::READ_ONLY`], [`UiWidget::WRAP`], [`UiWidget::CENTER`].
    pub flags: u32,
    /// A label's style ([`UiWidget::STYLE_BODY`] and on) or a sound
    /// ([`UiWidget::SOUND_CLICK`] and on).
    pub style: u32,
    /// A slider's stops between `min` and `max`, both included; 0 for a
    /// continuous slider.
    pub steps: u32,
    /// The text: a label's, a button's, a row's label, a window's id, an
    /// input hint's action.
    pub text: Str,
    /// A window's title, an input hint's label.
    pub detail: Str,
    /// A dropdown's options, `option_count` of them, valid for the call.
    pub options: *const Str,
    /// How many options `options` holds.
    pub option_count: u32,
    /// In and out: a toggle's state (0 or 1), a dropdown's option.
    pub index: u32,
    /// In and out: a slider's value; a progress bar's fill.
    pub value: f32,
    /// A slider's least value, a progress bar's empty value.
    pub min: f32,
    /// A slider's greatest value, a progress bar's full value.
    pub max: f32,
    /// Out: 1 when a button was clicked or a value changed this frame.
    pub result: u32,
    /// Where a HUD widget shows: [`UiWidget::TOP_LEFT`] to
    /// [`UiWidget::BOTTOM_RIGHT`]; read in a HUD callback only.
    pub anchor: u32,
    /// Zero.
    pub reserved: u32,
    /// The sound container a [`UiWidget::SOUND`] widget plays; nil for the
    /// game's own sound `style`.
    pub sound: Guid,
}

impl_headed!(UiWidget);

impl UiWidget {
    /// A window in the game's style, a panel over the screen that takes the
    /// input while it shows: `text` is its id, unique in the mod, `detail`
    /// its title. Widgets that follow go into it until [`UiWidget::END`].
    pub const WINDOW: u32 = 1;
    /// Closes the innermost open window.
    pub const END: u32 = 2;
    /// A line of text in `style`; [`UiWidget::WRAP`] wraps it at the width
    /// of the window, [`UiWidget::CENTER`] centres it.
    pub const LABEL: u32 = 3;
    /// A button labelled `text`; `result` 1 when clicked.
    pub const BUTTON: u32 = 4;
    /// A settings row labelled `text` with an on and off switch, `index`.
    pub const TOGGLE: u32 = 5;
    /// A settings row labelled `text` with a slider over `min..=max` in
    /// `steps` stops, `value`.
    pub const SLIDER: u32 = 6;
    /// A settings row labelled `text` with a choice among `options`, `index`.
    pub const DROPDOWN: u32 = 7;
    /// A bar filled from `min` to `max` at `value`.
    pub const PROGRESS: u32 = 8;
    /// Plays an interface sound, heard only on this client and from no place
    /// in the world: the sound container `sound`, or the game's own sound
    /// `style` when `sound` is nil; [`Status::MISSING`](super::Status::MISSING)
    /// for a container neither the client's archive nor a mod holds. Needs
    /// no window.
    pub const SOUND: u32 = 9;
    /// The key or button the player bound to a game action, as the game
    /// draws it beside its own prompts, so it follows a rebind: `text` is
    /// the action's reflected `GameplayAction` name, such as
    /// `ContextualAction`, and `detail` a label beside the key, empty for
    /// none. `result` 1 when the game drew a key, 0 when the action has no
    /// binding; [`Status::MISSING`](super::Status::MISSING) for an action
    /// this build does not name,
    /// [`Status::UNAVAILABLE`](super::Status::UNAVAILABLE) on a build where
    /// the loader does not find the game's hint. Client only.
    pub const INPUT_HINT: u32 = 10;

    /// The row shows its value and does not take changes; a button does not
    /// take clicks.
    pub const READ_ONLY: u32 = 1;
    /// A label wraps at the width of its window.
    pub const WRAP: u32 = 2;
    /// A label is centred.
    pub const CENTER: u32 = 4;

    /// The game's body text.
    pub const STYLE_BODY: u32 = 0;
    /// The game's dimmer, secondary text.
    pub const STYLE_SECONDARY: u32 = 1;
    /// The game's largest heading.
    pub const STYLE_H1: u32 = 2;
    /// The game's second heading.
    pub const STYLE_H2: u32 = 3;
    /// The game's third heading.
    pub const STYLE_H3: u32 = 4;
    /// The game's warning text.
    pub const STYLE_WARNING: u32 = 5;
    /// The game's text for good news.
    pub const STYLE_POSITIVE: u32 = 6;
    /// The game's text for bad news.
    pub const STYLE_NEGATIVE: u32 = 7;

    /// The game's click.
    pub const SOUND_CLICK: u32 = 0;
    /// The game's confirmation.
    pub const SOUND_CONFIRM: u32 = 1;
    /// The game's step back.
    pub const SOUND_BACK: u32 = 2;
    /// The game's refusal.
    pub const SOUND_ERROR: u32 = 3;

    /// A widget of no kind, every field zero: the base for struct updates.
    pub const EMPTY: Self = Self {
        header: Header::of::<Self>(),
        kind: 0,
        flags: 0,
        style: 0,
        steps: 0,
        text: Str::EMPTY,
        detail: Str::EMPTY,
        options: core::ptr::null(),
        option_count: 0,
        index: 0,
        value: 0.0,
        min: 0.0,
        max: 0.0,
        result: 0,
        anchor: 0,
        reserved: 0,
        sound: Guid::NIL,
    };

    /// The HUD's top left corner.
    pub const TOP_LEFT: u32 = 0;
    /// The HUD's top edge, centred.
    pub const TOP_CENTER: u32 = 1;
    /// The HUD's top right corner.
    pub const TOP_RIGHT: u32 = 2;
    /// The HUD's bottom left corner.
    pub const BOTTOM_LEFT: u32 = 3;
    /// The HUD's bottom edge, centred.
    pub const BOTTOM_CENTER: u32 = 4;
    /// The HUD's bottom right corner.
    pub const BOTTOM_RIGHT: u32 = 5;
}

/// A marker on the client's map, one of the set [`HostApi::ui_markers`](super::HostApi::ui_markers)
/// hands over.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, Str, UiMarker};
///
/// let marker = UiMarker {
///     header: Header::of::<UiMarker>(),
///     position: [100.0, 40.0, -20.0],
///     label: Str::new("Locker of Bob"),
///     color: UiMarker::GREEN,
///     marker_type: 0,
/// };
/// assert_eq!(marker.color, 2);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct UiMarker {
    /// Size and version of this struct.
    pub header: Header,
    /// Where, in metres.
    pub position: Metres,
    /// The label the map shows beside the icon.
    pub label: Str,
    /// The colour, one of the game's custom marker colours,
    /// [`UiMarker::WHITE`] to [`UiMarker::BLACK`].
    pub color: u32,
    /// The registry key of its marker type, one
    /// [`HostApi::map_marker_types`](super::HostApi::map_marker_types)
    /// lists; 0 for the loader's own choice, a custom marker type with a
    /// label.
    pub marker_type: u32,
}

impl_headed!(UiMarker);

impl UiMarker {
    /// White.
    pub const WHITE: u32 = 0;
    /// Red.
    pub const RED: u32 = 1;
    /// Green.
    pub const GREEN: u32 = 2;
    /// Blue.
    pub const BLUE: u32 = 3;
    /// Yellow.
    pub const YELLOW: u32 = 4;
    /// Purple.
    pub const PURPLE: u32 = 5;
    /// Turquoise.
    pub const TURQUOISE: u32 = 6;
    /// Black.
    pub const BLACK: u32 = 7;
}

/// The group the client's map sorts a marker type into, the game's
/// `MarkerTypeSorting`.
///
/// # Examples
///
/// ```
/// use enshwick::abi::MarkerTypeSorting;
///
/// assert_ne!(MarkerTypeSorting::DUNGEONS, MarkerTypeSorting::LOCATIONS);
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, bytemuck::Zeroable)]
pub struct MarkerTypeSorting(pub u8);

impl MarkerTypeSorting {
    /// In no group.
    pub const NONE: Self = Self(0);
    /// The Flame's: altars and shrines.
    pub const FLAME_RELATED: Self = Self(1);
    /// Dungeons.
    pub const DUNGEONS: Self = Self(2);
    /// Places in the world.
    pub const LOCATIONS: Self = Self(3);
    /// Markers players place themselves.
    pub const CUSTOM_MARKER: Self = Self(4);
    /// A group of the game's that this loader does not know.
    pub const UNKNOWN: Self = Self(u8::MAX);
}

/// A marker type of the client's marker type registry, one of the list
/// [`HostApi::map_marker_types`](super::HostApi::map_marker_types) writes.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{MapMarkerTypeInfo, MarkerTypeSorting};
///
/// let info = MapMarkerTypeInfo::EMPTY;
/// assert_eq!(info.sorting_category, MarkerTypeSorting::NONE);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct MapMarkerTypeInfo {
    /// Size and version of this struct.
    pub header: Header,
    /// Its name's text tag (`typeName`); zero for
    /// none.
    pub type_name: Guid,
    /// Its registry key (`markerId`), as
    /// [`UiMarker::marker_type`] takes it.
    pub marker_id: u32,
    /// The group the map sorts it into.
    pub sorting_category: MarkerTypeSorting,
    /// 1 when players can travel to markers of this type, else 0.
    pub is_fast_travel_destination: u8,
    /// 1 for a type of the markers players place themselves, else 0.
    pub is_custom_marker_type: u8,
    /// 1 when its markers show over the fog of war, else 0.
    pub show_above_fog_of_war: u8,
    /// 1 when its markers show their label, else 0.
    pub show_label: u8,
    /// Zero.
    pub reserved: [u8; 3],
}

impl_headed!(MapMarkerTypeInfo);

impl MapMarkerTypeInfo {
    /// A marker type with nothing known, its header set.
    pub const EMPTY: Self = Self {
        header: Header::of::<Self>(),
        type_name: Guid::NIL,
        marker_id: 0,
        sorting_category: MarkerTypeSorting::NONE,
        is_fast_travel_destination: 0,
        is_custom_marker_type: 0,
        show_above_fog_of_war: 0,
        show_label: 0,
        reserved: [0; 3],
    };
}

/// A dialog in the game's own style, queued with [`HostApi::ui_dialog`](super::HostApi::ui_dialog).
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, Str, UiDialog};
///
/// let dialog = UiDialog {
///     header: Header::of::<UiDialog>(),
///     title: Str::new("Enshwick"),
///     message: Str::new("Open every locker in the world?"),
///     detail: Str::EMPTY,
///     buttons: 1 << UiDialog::OK | 1 << UiDialog::CANCEL,
///     flags: 0,
/// };
/// assert_eq!(dialog.buttons, 0xc0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct UiDialog {
    /// Size and version of this struct.
    pub header: Header,
    /// The title.
    pub title: Str,
    /// The message under the title.
    pub message: Str,
    /// Smaller text under the message; empty for none.
    pub detail: Str,
    /// Bit `i` adds the button with the game's label for answer `i`, such
    /// as [`UiDialog::OK`]; the buttons show in the order of their bits. At
    /// least one.
    pub buttons: u32,
    /// [`UiDialog::WAITING`] when the caller blocks until the answer.
    pub flags: u32,
}

impl_headed!(UiDialog);

impl UiDialog {
    /// "Retry".
    pub const RETRY: u32 = 0;
    /// "Disable".
    pub const DISABLE: u32 = 1;
    /// "Erase".
    pub const ERASE: u32 = 2;
    /// "Reset".
    pub const RESET: u32 = 3;
    /// "Overwrite".
    pub const OVERWRITE: u32 = 5;
    /// "Cancel".
    pub const CANCEL: u32 = 6;
    /// "OK".
    pub const OK: u32 = 7;
    /// "Ignore".
    pub const IGNORE: u32 = 8;
    /// The buttons a dialog may carry.
    pub const BUTTONS: u32 = 0x1ef;
    /// The answer when the dialog closed without one, as when the game
    /// quits.
    pub const CLOSED: u32 = u32::MAX;
    /// The caller blocks until the answer: refused on the thread that
    /// draws the interface, which would never draw it.
    pub const WAITING: u32 = 1;
}

/// A mod's callback for the answer to its dialog: the answer, such as
/// [`UiDialog::OK`] or [`UiDialog::CLOSED`], and the pointer given with the
/// dialog.
///
/// Called once, on the thread that draws the interface, or on a thread of the
/// loader's when the dialog shows as a system window. It must not unwind.
///
/// Null is never a callback: the loader refuses a null one with
/// [`Status::BAD_ARGS`](crate::abi::Status::BAD_ARGS) and keeps nothing.
///
/// # Examples
///
/// The safe layer wraps it in [`Ui::dialog`](../ui/struct.Ui.html#method.dialog).
pub type UiAnswerFn = unsafe extern "system" fn(answer: u32, user: *mut c_void);

/// A mod's check of a new text of its settings file, before the loader writes
/// it: [`Status::OK`] to accept it; else the reason, written to `why` and valid
/// until the mod's next call, and [`Status::REFUSED`].
///
/// A reason that starts with a key of the mod's texts, followed by
/// tab-separated `name=value` pairs (`"config.min\tmin=2"`), shows as that
/// text in the reader's language with each `{name}` filled; any other
/// reason shows as written. A name holds no `=`; neither holds a tab.
///
/// It must not unwind.
///
/// Null is never a callback: the loader refuses a null one with
/// [`Status::BAD_ARGS`](crate::abi::Status::BAD_ARGS) and keeps nothing.
/// Its thread is not fixed, so it must be thread-safe.
///
/// # Examples
///
/// The safe layer wraps it in [`Load::on_settings`](../struct.Load.html#method.on_settings).
pub type ConfigCheckFn =
    unsafe extern "system" fn(text: Str, why: *mut Str, user: *mut c_void) -> Status;

/// The size, scale and version of the client's fog of war, which
/// [`HostApi::fog_read`](super::HostApi::fog_read) writes.
///
/// The fog is one image of a byte per pixel, the coverage from 0 (hidden)
/// to 255 (uncovered), row 0 at the world's high z edge and x growing with
/// world x.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{FogInfo, Header};
///
/// let info = FogInfo { header: Header::of::<FogInfo>(), ..FogInfo::EMPTY };
/// assert_eq!(info.width, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct FogInfo {
    /// Size and version of this struct.
    pub header: Header,
    /// The image's width in pixels.
    pub width: u32,
    /// The image's height in pixels.
    pub height: u32,
    /// The metres one pixel covers along each axis.
    pub metres_per_pixel: f32,
    /// The world's z size the fog covers, in metres: pixel row
    /// `(size_z - z) / metres_per_pixel`.
    pub size_z: f32,
    /// Grows whenever pixels of the copy changed.
    pub version: u64,
    /// Counts the fogs the loader has seen, one per world the client
    /// enters; a new one starts from the save of that world.
    pub session: u64,
    /// Counts the closed windows of uncovering in this session: the
    /// loader closes the pixels uncovered since the last window into a
    /// new one at most once a second; 0 before the first.
    pub uncovered: u64,
    /// The pixels the last window turned from hidden to shown.
    pub uncovered_pixels: u32,
    /// Zero.
    pub reserved: u32,
    /// The lowest pixel of the box around the last window's pixels, as
    /// x and row.
    pub uncovered_min: [u32; 2],
    /// The box's width and height in pixels.
    pub uncovered_size: [u32; 2],
}

impl_headed!(FogInfo);

impl FogInfo {
    /// No fog, its header set.
    pub const EMPTY: Self = Self {
        header: Header::of::<Self>(),
        width: 0,
        height: 0,
        metres_per_pixel: 0.0,
        size_z: 0.0,
        version: 0,
        session: 0,
        uncovered: 0,
        uncovered_pixels: 0,
        reserved: 0,
        uncovered_min: [0; 2],
        uncovered_size: [0; 2],
    };
}

/// Cached diagnostics of the client's local gameplay-input sampler.
/// Flags and age are weakly consistent observations, not world or session identity.
/// Reading this does not request sampling. ABI 12.1.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct LocalInputInfo {
    /// Size and version of this struct.
    pub header: Header,
    /// A combination of this type's flag constants; unknown flags are ignored.
    pub flags: u32,
    /// Reserved; 0.
    pub reserved: u32,
    /// Age of the last cached sample in milliseconds; `u64::MAX` means never sampled.
    pub age_ms: u64,
}
impl_headed!(LocalInputInfo);
impl LocalInputInfo {
    /// A mod requested sampling through `local_held`.
    pub const REQUESTED: u32 = 1;
    /// The sampler's existing layout resolved.
    pub const LAYOUT_AVAILABLE: u32 = 2;
    /// The existing sampler link succeeded and its fuse remains on.
    pub const LINK_AVAILABLE: u32 = 4;
    /// The sampler's read fuse remains on; this alone does not mean reads can run.
    pub const READ_ENABLED: u32 = 8;
    /// At least one sample was cached, including a zero mask.
    pub const SAMPLE_PRESENT: u32 = 16;
    /// Requested, available, enabled and sampled within the existing 1000 ms window.
    pub const FRESH: u32 = 32;
}

/// The client's state in one copy, which
/// [`HostApi::client_snapshot`](super::HostApi::client_snapshot) writes.
///
/// It holds the
/// session and world, the open screens and the in-game menu's tab, read in
/// the interface's last frame, and the local player's held controls, read
/// in its last step.
///
/// Each group of fields counts only while its bit of `available` is set; a
/// build that lacks what a group reads leaves its bit clear and the rest of
/// the copy whole. Names are the game's own: a screen or tab is a member of
/// its reflected `HudScreenType`, and empty while none is. The names are
/// static text, valid for the process.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{ClientSnapshot, Header};
///
/// let snapshot = ClientSnapshot { header: Header::of::<ClientSnapshot>(), ..ClientSnapshot::EMPTY };
/// assert_eq!(snapshot.state, ClientSnapshot::NEVER);
/// assert_eq!(snapshot.available & ClientSnapshot::SCREENS, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct ClientSnapshot {
    /// Size and version of this struct.
    pub header: Header,
    /// How old the interface's reads are: [`Self::NEVER`] to [`Self::FRESH`].
    pub state: u32,
    /// Which field groups count: [`Self::SCREENS`] and the other group bits.
    pub available: u32,
    /// The loader's count of client sessions, from 1: it grows when the
    /// client's game object, the local player's entity or the world
    /// changes, so a value read twice names one session. 0 before the
    /// interface read a game.
    pub session: u64,
    /// Milliseconds since the interface's last read; `u64::MAX` before
    /// the first.
    pub age_ms: u64,
    /// The world's name, as [`HostApi::world_key`](super::HostApi::world_key)
    /// gives it; empty while none is known, as on a client joined to a
    /// server.
    pub scene: Str,
    /// The open HUD screen; group [`Self::SCREENS`].
    pub hud_screen: Str,
    /// The screen the open one returns to when it closes, empty when it
    /// returns to the world; group [`Self::SCREENS`].
    pub return_screen: Str,
    /// The tab the in-game menu shows, as the screen it stands for
    /// (`Social`, `Map`, `Journal`, `PlayerCrafting`, `Backpack`,
    /// `Character` or `SkillTree`), empty while no menu shows; group
    /// [`Self::MENU_TAB`].
    pub menu_tab: Str,
    /// The local player's held controls, as
    /// [`HostApi::local_held`](super::HostApi::local_held) gives them; group
    /// [`Self::HELD`].
    pub held: u64,
    /// The local player's entity; group [`Self::PLAYER`].
    pub player: Entity,
    /// The open screen's host, such as the chest whose screen shows, or 0;
    /// group [`Self::SCREENS`].
    pub host: Entity,
    /// The recipe the crafting menu highlights, 0 for none; group
    /// [`Self::RECIPE`].
    pub highlighted_recipe: RecipeId,
    /// Zero.
    pub reserved: u32,
}

impl_headed!(ClientSnapshot);

impl ClientSnapshot {
    /// No interface read yet.
    pub const NEVER: u32 = 0;
    /// The interface's reads are off on this build or after a fault.
    pub const OFF: u32 = 1;
    /// The last read is older than a second: no frame drew a game since,
    /// as while loading.
    pub const STALE: u32 = 2;
    /// The last read is at most a second old.
    pub const FRESH: u32 = 3;
    /// [`Self::hud_screen`], [`Self::return_screen`] and [`Self::host`].
    pub const SCREENS: u32 = 1;
    /// [`Self::menu_tab`].
    pub const MENU_TAB: u32 = 2;
    /// [`Self::held`], a read at most a second old.
    pub const HELD: u32 = 4;
    /// [`Self::player`].
    pub const PLAYER: u32 = 8;
    /// [`Self::highlighted_recipe`].
    pub const RECIPE: u32 = 16;
    /// Nothing read, its header set.
    pub const EMPTY: Self = Self {
        header: Header::of::<Self>(),
        state: Self::NEVER,
        available: 0,
        session: 0,
        age_ms: u64::MAX,
        scene: Str::EMPTY,
        hud_screen: Str::EMPTY,
        return_screen: Str::EMPTY,
        menu_tab: Str::EMPTY,
        held: 0,
        player: Entity(0),
        host: Entity(0),
        highlighted_recipe: RecipeId(0),
        reserved: 0,
    };
}

/// A request to the client's own interface or input, which
/// [`HostApi::client_command`](super::HostApi::client_command) queues.
///
/// The loader runs it in the interface's frame or the input builder's step,
/// never inside a mod's callback, and only while the client is still in
/// `session` on `expected_screen`. Its outcome is what
/// [`HostApi::client_command_status`](super::HostApi::client_command_status)
/// reports; the call itself answers only that it is queued.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{ClientCommand, Str};
///
/// let open = ClientCommand {
///     kind: ClientCommand::OPEN_SCREEN,
///     session: 1,
///     expected_screen: Str::new("InGame"),
///     target: Str::new("Backpack"),
///     ..ClientCommand::EMPTY
/// };
/// assert_eq!(open.hold_ms, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct ClientCommand {
    /// Size and version of this struct.
    pub header: Header,
    /// What to do: [`Self::OPEN_SCREEN`] to [`Self::HIGHLIGHT_RECIPE`].
    pub kind: u32,
    /// A move's hold in milliseconds, 1 to [`Self::MAX_HOLD_MS`].
    pub hold_ms: u32,
    /// The session the command belongs to, as
    /// [`ClientSnapshot::session`] names it; a command whose session ended
    /// is cancelled.
    pub session: u64,
    /// The HUD screen the client must show, a `HudScreenType` member; empty
    /// for any screen.
    pub expected_screen: Str,
    /// What the command opens or chooses: a screen for
    /// [`Self::OPEN_SCREEN`], a tab for [`Self::CHOOSE_TAB`], by the
    /// `HudScreenType` member it stands for.
    pub target: Str,
    /// A look's turn in radians of yaw and pitch, before the player's
    /// camera sensitivity; each at most [`Self::MAX_LOOK`].
    pub look: [f32; 2],
    /// A move's direction, x right and y forward, of length at most 1.
    pub direction: [f32; 2],
    /// The recipe to highlight.
    pub recipe: RecipeId,
    /// Zero.
    pub reserved: u32,
    /// The command [`Self::CANCEL`] cancels.
    pub cancels: u64,
}

impl_headed!(ClientCommand);

impl ClientCommand {
    /// Opens `target`, one of the screens the game opens from a key in the
    /// world.
    pub const OPEN_SCREEN: u32 = 1;
    /// Closes the open screen, back to the world.
    pub const CLOSE_SCREEN: u32 = 2;
    /// Shows the in-game menu's tab `target` while the menu shows.
    pub const CHOOSE_TAB: u32 = 3;
    /// Cancels the mod's command `cancels`.
    pub const CANCEL: u32 = 4;
    /// Turns the camera by `look` once.
    pub const LOOK: u32 = 5;
    /// Holds `direction` for `hold_ms`.
    pub const MOVE: u32 = 6;
    /// Highlights `recipe` in the open crafting menu, crafting nothing.
    pub const HIGHLIGHT_RECIPE: u32 = 7;
    /// The longest move.
    pub const MAX_HOLD_MS: u32 = 250;
    /// The largest turn of one look, in radians, a quarter turn.
    #[expect(
        clippy::approx_constant,
        reason = "the golden layout file reads a constant's literal, not a path"
    )]
    pub const MAX_LOOK: f32 = 1.570_796_4;
    /// No command, its header set.
    pub const EMPTY: Self = Self {
        header: Header::of::<Self>(),
        kind: 0,
        hold_ms: 0,
        session: 0,
        expected_screen: Str::EMPTY,
        target: Str::EMPTY,
        look: [0.0; 2],
        direction: [0.0; 2],
        recipe: RecipeId(0),
        reserved: 0,
        cancels: 0,
    };
}

/// Where a [`ClientCommand`] stands, which
/// [`HostApi::client_command_status`](super::HostApi::client_command_status)
/// writes.
///
/// A command is [`Self::PENDING`] until the game took it
/// ([`Self::APPLIED`]), then [`Self::OBSERVED`] once the client's own state
/// shows its effect. It may end [`Self::CANCELLED`] for `reason` or
/// [`Self::UNAVAILABLE`] on a build that lacks what it needs. Nothing
/// answers a guessed success.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{ClientCommandStatus, Header};
///
/// let status = ClientCommandStatus { header: Header::of::<ClientCommandStatus>(), ..ClientCommandStatus::EMPTY };
/// assert_eq!(status.state, ClientCommandStatus::PENDING);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct ClientCommandStatus {
    /// Size and version of this struct.
    pub header: Header,
    /// [`Self::PENDING`] to [`Self::UNAVAILABLE`].
    pub state: u32,
    /// Why a cancelled command ended: [`Self::SESSION_CHANGED`] to
    /// [`Self::MOD_OFF`]; 0 otherwise.
    pub reason: u32,
    /// The command's kind.
    pub kind: u32,
    /// Zero.
    pub reserved: u32,
    /// Milliseconds since the command was queued.
    pub age_ms: u64,
}

impl_headed!(ClientCommandStatus);

impl ClientCommandStatus {
    /// Queued; the game has not taken it yet.
    pub const PENDING: u32 = 0;
    /// The game took it: its queue holds the screen change, the menu its
    /// tab, or the input builder the look or move.
    pub const APPLIED: u32 = 1;
    /// The client's own state shows the effect: the screen or tab shows,
    /// the look was written whole, the move's hold ran out, the recipe is
    /// highlighted.
    pub const OBSERVED: u32 = 2;
    /// It ended without its effect, for `reason`.
    pub const CANCELLED: u32 = 3;
    /// The running build lacks what it needs, or the target is not there,
    /// such as a recipe the open menu does not list.
    pub const UNAVAILABLE: u32 = 4;
    /// The client's session changed.
    pub const SESSION_CHANGED: u32 = 1;
    /// Another screen than the expected one shows.
    pub const OTHER_SCREEN: u32 = 2;
    /// The player pressed a control or moved the stick or mouse.
    pub const PLAYER_INPUT: u32 = 3;
    /// The mod cancelled it.
    pub const BY_MOD: u32 = 4;
    /// Two seconds passed without its effect.
    pub const TIMED_OUT: u32 = 5;
    /// The game blocks the player's input now.
    pub const INPUT_BLOCKED: u32 = 6;
    /// The mod or the feature turned off.
    pub const MOD_OFF: u32 = 7;
    /// A command queued now, its header set.
    pub const EMPTY: Self = Self {
        header: Header::of::<Self>(),
        state: Self::PENDING,
        reason: 0,
        kind: 0,
        reserved: 0,
        age_ms: 0,
    };
}
