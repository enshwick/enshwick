//! The mod's interface: frames, widgets, markers and dialogs.

use core::ffi::c_void;

use crate::abi::{Header, Status, Str};

/// A frame of the client's interface, handed to the callbacks registered with
/// [`HostApi::ui_frame`](super::HostApi::ui_frame); widgets drawn with [`HostApi::ui_widget`](super::HostApi::ui_widget) name its
/// `token`.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_abi as enshwick;
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
///     screen_host_template: [0; 16],
///     screen_host_position: [0; 3],
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
    /// find it. The game's own text, borrowed for the callback. ABI 11.14.
    pub prompt_title: Str,
    /// The game's text id (`LocaTagId`) of the prompt's verb, such as the
    /// one for "Open"; 0 while the prompt offers nothing. ABI 11.14.
    pub prompt_verb: u32,
    /// The entity of the client's world the open screen belongs to, such
    /// as the chest whose inventory shows; entity 0 for a screen of no
    /// object, outside a game and on a build where the
    /// loader does not find it. ABI 11.14.
    pub screen_host: crate::Entity,
    /// The template GUID of `screen_host` in the client's world, as the
    /// client's last step read it; all zero for no host, before a step read
    /// this host, and on a build where the loader does not find it.
    /// ABI 11.18.
    pub screen_host_template: [u8; 16],
    /// Where `screen_host` stands in the client's world, in 32.32
    /// fixed-point metres, y up, from the same step; meaningful only while
    /// `screen_host_template` is not all zero. ABI 11.18.
    pub screen_host_position: [i64; 3],
}

impl_headed!(UiFrame, born = 24);

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
/// # extern crate enshwick_abi as enshwick;
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
    /// them. ABI 11.18.
    pub hosts: *const [u8; 16],
    /// How many GUIDs `hosts` holds, at most [`UiExtension::MAX_HOSTS`].
    /// ABI 11.18.
    pub host_count: u32,
    /// Zero.
    pub reserved_hosts: u32,
}

impl_headed!(UiExtension, born = 32);

impl UiExtension {
    /// The left edge, centred from top to bottom.
    pub const LEFT: u32 = 0;
    /// The right edge, centred from top to bottom.
    pub const RIGHT: u32 = 1;
    /// The top edge, centred from left to right.
    pub const TOP: u32 = 2;
    /// The bottom edge, centred from left to right.
    pub const BOTTOM: u32 = 3;
    /// The most host templates one panel names. ABI 11.18.
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
    /// # extern crate enshwick_abi as enshwick;
    /// use enshwick::abi::{Header, Str, UiExtension};
    ///
    /// let ours = [[7u8; 16]];
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
    pub unsafe fn host_list(&self) -> Option<&[[u8; 16]]> {
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
/// # extern crate enshwick_abi as enshwick;
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
    /// The sound container a [`UiWidget::PLAY`] widget plays, its GUID in
    /// memory order. ABI 11.14.
    pub sound: [u8; 16],
}

impl_headed!(UiWidget, born = 96);

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
    /// Plays the interface sound `style`.
    pub const SOUND: u32 = 9;
    /// Plays the sound container `sound` as an interface sound, heard only
    /// on this client and from no place in the world;
    /// [`Status::MISSING`](super::Status::MISSING) for a container neither
    /// the client's archive nor a mod holds. Needs no window. ABI 11.14.
    pub const PLAY: u32 = 10;
    /// The key or button the player bound to a game action, as the game
    /// draws it beside its own prompts, so it follows a rebind: `text` is
    /// the action's reflected `GameplayAction` name, such as
    /// `ContextualAction`, and `detail` a label beside the key, empty for
    /// none. `result` 1 when the game drew a key, 0 when the action has no
    /// binding; [`Status::MISSING`](super::Status::MISSING) for an action
    /// this build does not name, or on a build where the loader does not
    /// find the game's hint. Client only. ABI 11.18.
    pub const INPUT_HINT: u32 = 11;

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
        sound: [0; 16],
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
/// # extern crate enshwick_abi as enshwick;
/// use enshwick::abi::{Header, Str, UiMarker};
///
/// let marker = UiMarker {
///     header: Header::of::<UiMarker>(),
///     position: [100 << 32, 40 << 32, -(20 << 32)],
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
    /// Where, in 32.32 fixed-point metres, y up.
    pub position: [i64; 3],
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
/// # extern crate enshwick_abi as enshwick;
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
/// # extern crate enshwick_abi as enshwick;
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
    /// Its name's text tag (`typeName`), a GUID in memory order; zero for
    /// none.
    pub type_name: [u8; 16],
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
        type_name: [0; 16],
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
/// # extern crate enshwick_abi as enshwick;
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
/// # extern crate enshwick_abi as enshwick;
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
    /// new one at most once a second; 0 before the first. ABI 11.14.
    pub uncovered: u64,
    /// The pixels the last window turned from hidden to shown. ABI 11.14.
    pub uncovered_pixels: u32,
    /// Zero.
    pub reserved: u32,
    /// The lowest pixel of the box around the last window's pixels, as
    /// x and row. ABI 11.14.
    pub uncovered_min: [u32; 2],
    /// The box's width and height in pixels. ABI 11.14.
    pub uncovered_size: [u32; 2],
}

impl_headed!(FogInfo, born = 40);

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
