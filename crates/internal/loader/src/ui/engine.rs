//! The game's own widget functions, resolved once and called with the
//! layouts they read.
//!
//! Engine facts from client build 1269579: the interface is immediate mode,
//! redrawn every frame inside `GameUi::update` on the main thread, with the
//! interface context in a global for the frame. Widgets take a config buffer
//! the caller fills; the layouts below are the ones the game's own pages and
//! its error popup fill. Every function is pinned in the build table and its
//! first code bytes checked before use, and every field the loader fills or
//! reads is a constant of the table, read on a build without one from the
//! code that uses it. Each object the game fills or reads has a constant for
//! how far it reaches, checked against the loader's buffer for it.

use crate::host::Loader;
use crate::pinned::{self, Symbol};

/// A buffer the game fills or reads, aligned as the game keeps its own on
/// the stack.
#[repr(C, align(16))]
pub(super) struct Buf<const N: usize>([u8; N]);

impl<const N: usize> Buf<N> {
    pub(super) const fn zeroed() -> Self {
        Self([0; N])
    }

    fn put<const M: usize>(&mut self, at: usize, bytes: [u8; M]) {
        if let Some(slot) = self.0.get_mut(at..at + M) {
            slot.copy_from_slice(&bytes);
        }
    }

    pub(super) fn u8(&mut self, at: usize, value: u8) {
        self.put(at, [value]);
    }

    pub(super) fn u16(&mut self, at: usize, value: u16) {
        self.put(at, value.to_le_bytes());
    }

    pub(super) fn u32(&mut self, at: usize, value: u32) {
        self.put(at, value.to_le_bytes());
    }

    pub(super) fn u64(&mut self, at: usize, value: u64) {
        self.put(at, value.to_le_bytes());
    }

    pub(super) fn f32(&mut self, at: usize, value: f32) {
        self.put(at, value.to_le_bytes());
    }

    pub(super) fn text(&mut self, at: usize, text: &UiText) {
        self.u64(at, text.ptr.expose_provenance() as u64);
        self.u64(at + 8, text.len);
        self.u64(at + 16, text.hash);
    }

    pub(super) fn read_u16(&self, at: usize) -> u16 {
        self.0
            .get(at..at + 2)
            .and_then(|b| b.try_into().ok())
            .map_or(0, u16::from_le_bytes)
    }

    pub(super) fn read_usize(&self, at: usize) -> usize {
        usize::try_from(self.read_u64(at)).unwrap_or(0)
    }

    pub(super) fn read_u64(&self, at: usize) -> u64 {
        self.0
            .get(at..at + 8)
            .and_then(|b| b.try_into().ok())
            .map_or(0, u64::from_le_bytes)
    }

    pub(super) fn ptr(&mut self) -> *mut u8 {
        self.0.as_mut_ptr()
    }
}

/// The game's text: UTF-8 bytes, their length and their hash.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct UiText {
    pub(crate) ptr: *const u8,
    pub(crate) len: u64,
    pub(crate) hash: u64,
}

/// The byte the game points empty texts at.
static NUL: u8 = 0;
const FNV_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0100_0000_01b3;

impl UiText {
    /// The text over `bytes`, which must outlive every use the game makes of
    /// it; hashed as the game hashes, over sign-extended bytes.
    pub(crate) fn new(bytes: &[u8]) -> Self {
        if bytes.is_empty() {
            return Self::EMPTY;
        }
        Self {
            ptr: bytes.as_ptr(),
            len: bytes.len() as u64,
            hash: hash(bytes),
        }
    }

    pub(crate) const EMPTY: Self = Self {
        ptr: &raw const NUL,
        len: 0,
        hash: FNV_BASIS,
    };
}

/// FNV-1a over sign-extended bytes, as the game hashes its texts.
#[expect(
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "the game sign-extends each byte before the xor"
)]
pub(crate) fn hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(FNV_BASIS, |h, &b| {
        (h ^ (i64::from(b as i8) as u64)).wrapping_mul(FNV_PRIME)
    })
}

type Ptr = *mut u8;
pub(super) type PopupFn = unsafe extern "system" fn(state: Ptr);
type WindowBeginFn = unsafe extern "system" fn(
    window: Ptr,
    name: *const [u64; 2],
    fullscreen: u8,
    layer: u32,
    input: u32,
    flags: u32,
) -> Ptr;
type OneFn = unsafe extern "system" fn(object: Ptr);
type FrameFromNodeFn = unsafe extern "system" fn(frame: Ptr, node: usize) -> Ptr;
type FrameOpenFn = unsafe extern "system" fn(frame: Ptr, name: usize) -> Ptr;
type ColorFn = unsafe extern "system" fn(frame: Ptr, color: *const u64);
type NodeRectFn = unsafe extern "system" fn(node: usize, edges: *const [f32; 4]);
type NodeSpacingFn = unsafe extern "system" fn(node: usize, gap: *const [f32; 2], mode: u32);
type NodeAlignFn = unsafe extern "system" fn(node: usize, align: *const u16);
type NodeFloatFn = unsafe extern "system" fn(node: usize, value: f32);
type NodeStretchFn = unsafe extern "system" fn(node: usize, packed: u64);
type VisibleFn = unsafe extern "system" fn(mask: u32) -> u8;
type MainEntryFn = unsafe extern "system" fn(menu: usize, entry: *const u8) -> u8;
type PauseEntryFn =
    unsafe extern "system" fn(text: *const UiText, enabled: u8, attention: u8) -> u8;
type ActionNewFn = unsafe extern "system" fn(spec: *mut u8, action: u8, flag: u8) -> *mut u8;
type ActionQueryFn = unsafe extern "system" fn(spec: *const u8, out: *mut u8, modality: u16) -> u8;
type PanelFn = unsafe extern "system" fn(frame: Ptr, config: *const u8);
type NavInitFn = unsafe extern "system" fn(nav: Ptr, config: *const u8) -> Ptr;
type NavInputFn =
    unsafe extern "system" fn(out: *mut [u64; 2], config: *const u8, flag: u8) -> *mut [u64; 2];
type NavSetFn = unsafe extern "system" fn(nav: Ptr, input: *const [u64; 2]);
type NavPushFn = unsafe extern "system" fn(context: usize, nav: Ptr) -> usize;
type NavEndFn = unsafe extern "system" fn(nav: Ptr, flag: u32);
type LabelNewFn =
    unsafe extern "system" fn(label: Ptr, text: *const UiText, style: usize, wrap: u8) -> Ptr;
type ButtonFn = unsafe extern "system" fn(config: Ptr, text: *const UiText) -> u8;
type ToggleFn = unsafe extern "system" fn(config: Ptr, value: *mut u8) -> u8;
type RowInitFn = unsafe extern "system" fn(config: Ptr, label: *const UiText) -> Ptr;
type SliderFn = unsafe extern "system" fn(config: Ptr, value: *mut f32, gamepad: *mut i32) -> u8;
type DropdownFn = unsafe extern "system" fn(config: Ptr, value: *mut i32, open: *mut u8) -> u8;
type ProgressFn = unsafe extern "system" fn(value: f32, max: f32, fill: u64, background: u64);
type SoundFn = unsafe extern "system" fn(context: usize, sound: usize);
type LocaIdFn = unsafe extern "system" fn(reference: usize) -> u32;
type LocaTextFn = unsafe extern "system" fn(out: *mut UiText, id: u32) -> *mut UiText;

/// The size of the loader's own popup states.
pub(super) const POPUP_SIZE: usize = 0x560;
const WINDOW_OBJECT: usize = 0x20;
const FRAME_SIZE: usize = 0x40;
const NAV_SIZE: usize = 0x300;
const NAV_CONFIG: usize = 0x58;
const PANEL_CONFIG: usize = 0x100;
const BUTTON_CONFIG: usize = 0x80;
const TOGGLE_CONFIG: usize = 0xc50;
const SLIDER_CONFIG: usize = 0x120;
const DROPDOWN_CONFIG: usize = 0xca0;
const MENU_ENTRY: usize = 0x20;
const ACTION_SPEC: usize = 0x50;
const LABEL_SIZE: usize = 0x150;
/// The entries of a toggle: off and on.
const TOGGLE_ENTRIES: usize = 2;

/// Where the interface's fields sit on the running build, read from its
/// table: the context's, the widgets' configs and the objects the widget
/// functions fill.
#[derive(Debug, Clone, Copy)]
pub(super) struct Layout {
    context_nav: usize,
    context_host: usize,
    context_bundle: usize,
    /// The context's play state, whose record's byte at `state_screen`
    /// says the screen: 1 to 3 menus, 4 and 5 slideshows, 6 loading, 7 in
    /// game.
    context_state: usize,
    state_screen: usize,
    pub(super) popup_active: usize,
    frame_node: usize,
    window_node: usize,
    node_id: usize,
    panel_kind: usize,
    panel_alpha: usize,
    /// The navigation config's fields from its start, in the order of
    /// [`NAV_IDS`].
    nav: [usize; 7],
    label_align: usize,
    label_shown: usize,
    label_font_size: usize,
    label_flags: usize,
    button_align: usize,
    button_centered: usize,
    button_enabled: usize,
    button_size_mode: usize,
    button_padding: [usize; 2],
    button_scale: usize,
    button_sound: usize,
    entry_size: usize,
    entry_value: usize,
    entry_text: usize,
    toggle_count: usize,
    toggle_label: usize,
    toggle_explainer: usize,
    toggle_explain: usize,
    toggle_enabled: usize,
    slider_min: usize,
    slider_max: usize,
    slider_bounded: usize,
    slider_read_only: usize,
    slider_on_release: usize,
    slider_steps: usize,
    slider_format: usize,
    slider_discrete: usize,
    slider_stop_count: usize,
    slider_stops: usize,
    slider_stop_capacity: usize,
    dropdown_count: usize,
    dropdown_enabled: usize,
    dropdown_capacity: usize,
    dropdown_entry_text: usize,
    menu_entry_text: usize,
    menu_entry_font: usize,
    action_node: usize,
    action_bound: usize,
}

/// The navigation group's fields the loader's config sets, as offsets in
/// the group; its config sits at `ui.nav_config` in it.
const NAV_IDS: [&str; 7] = [
    pinned::ui::NAV_KIND.id(),
    pinned::ui::NAV_WRAP.id(),
    pinned::ui::NAV_AXIS.id(),
    pinned::ui::NAV_FLAGS.id(),
    pinned::ui::NAV_INPUT_KIND.id(),
    pinned::ui::NAV_LAYER.id(),
    pinned::ui::NAV_SOUND.id(),
];

/// The popup state's fields and the host's answer, which the loader's
/// dialogs fill and read.
#[derive(Debug, Clone, Copy)]
pub(super) struct DialogLayout {
    pub(super) title: usize,
    pub(super) message: usize,
    pub(super) buttons: usize,
    pub(super) detail: usize,
    pub(super) detail_len: usize,
    pub(super) answer: usize,
    pub(super) answered: usize,
}

/// The world settings view's fields at the loader's hook in it: the
/// config's byte that makes every row read-only, and the shown tab.
#[derive(Debug, Clone, Copy)]
pub(super) struct ViewLayout {
    pub(super) read_only: usize,
    pub(super) tab: usize,
}

/// The build's constants by id.
type Lookup<'a> = &'a dyn Fn(&str) -> Option<u64>;

/// A layout constant of the build.
fn field(lookup: Lookup<'_>, id: &'static str) -> Result<usize, Missing> {
    lookup(id)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or(Missing::Entry(id))
}

/// Checks a layout constant that gives how far the game reads or writes an
/// object the loader keeps in `room` bytes.
fn fits(lookup: Lookup<'_>, id: &'static str, room: usize) -> Result<(), Missing> {
    if field(lookup, id)? > room {
        return Err(Missing::Room(id));
    }
    Ok(())
}

impl Layout {
    fn read(lookup: Lookup<'_>) -> Result<Self, Missing> {
        let f = |id| field(lookup, id);
        for (id, room) in [
            (pinned::ui::WINDOW_EXTENT, WINDOW_OBJECT),
            (pinned::ui::FRAME_EXTENT, FRAME_SIZE),
            (pinned::ui::NAV_EXTENT, NAV_SIZE),
            (pinned::ui::NAV_CONFIG_SIZE, NAV_CONFIG),
            (pinned::ui::PANEL_EXTENT, PANEL_CONFIG),
            (pinned::ui::LABEL_EXTENT, LABEL_SIZE),
            (pinned::ui::BUTTON_EXTENT, BUTTON_CONFIG),
            (pinned::ui::TOGGLE_EXTENT, TOGGLE_CONFIG),
            (pinned::ui::DROPDOWN_EXTENT, DROPDOWN_CONFIG),
            (pinned::ui::MENU_ENTRY_EXTENT, MENU_ENTRY),
            (pinned::ui::ACTION_EXTENT, ACTION_SPEC),
        ] {
            fits(lookup, id.id(), room)?;
        }
        let config = f(pinned::ui::NAV_CONFIG.id())?;
        let mut nav = [0; 7];
        for (slot, id) in nav.iter_mut().zip(NAV_IDS) {
            *slot = f(id)?.checked_sub(config).ok_or(Missing::Room(id))?;
        }
        Self {
            context_nav: f(pinned::ui::CONTEXT_NAV.id())?,
            context_host: f(pinned::ui::CONTEXT_HOST.id())?,
            context_bundle: f(pinned::ui::CONTEXT_BUNDLE.id())?,
            context_state: f(pinned::ui::CONTEXT_STATE.id())?,
            state_screen: f(pinned::ui::STATE_SCREEN.id())?,
            popup_active: f(pinned::ui::POPUP_ACTIVE.id())?,
            frame_node: f(pinned::ui::FRAME_NODE.id())?,
            window_node: f(pinned::ui::WINDOW_NODE.id())?,
            node_id: f(pinned::ui::NODE_ID.id())?,
            panel_kind: f(pinned::ui::PANEL_KIND.id())?,
            panel_alpha: f(pinned::ui::PANEL_ALPHA.id())?,
            nav,
            label_align: f(pinned::ui::LABEL_ALIGN.id())?,
            label_shown: f(pinned::ui::LABEL_SHOWN.id())?,
            label_font_size: f(pinned::ui::LABEL_FONT_SIZE.id())?,
            label_flags: f(pinned::ui::LABEL_FLAGS.id())?,
            button_align: f(pinned::ui::BUTTON_ALIGN.id())?,
            button_centered: f(pinned::ui::BUTTON_CENTERED.id())?,
            button_enabled: f(pinned::ui::BUTTON_ENABLED.id())?,
            button_size_mode: f(pinned::ui::BUTTON_SIZE_MODE.id())?,
            button_padding: [
                f(pinned::ui::BUTTON_PADDING_X.id())?,
                f(pinned::ui::BUTTON_PADDING_Y.id())?,
            ],
            button_scale: f(pinned::ui::BUTTON_SCALE.id())?,
            button_sound: f(pinned::ui::BUTTON_SOUND.id())?,
            entry_size: f(pinned::ui::ENTRY_SIZE.id())?,
            entry_value: f(pinned::ui::ENTRY_VALUE.id())?,
            entry_text: f(pinned::ui::ENTRY_TEXT.id())?,
            toggle_count: f(pinned::ui::TOGGLE_COUNT.id())?,
            toggle_label: f(pinned::ui::TOGGLE_LABEL.id())?,
            toggle_explainer: f(pinned::ui::TOGGLE_EXPLAINER.id())?,
            toggle_explain: f(pinned::ui::TOGGLE_EXPLAIN.id())?,
            toggle_enabled: f(pinned::ui::TOGGLE_ENABLED.id())?,
            slider_min: f(pinned::ui::SLIDER_MIN.id())?,
            slider_max: f(pinned::ui::SLIDER_MAX.id())?,
            slider_bounded: f(pinned::ui::SLIDER_BOUNDED.id())?,
            slider_read_only: f(pinned::ui::SLIDER_READ_ONLY.id())?,
            slider_on_release: f(pinned::ui::SLIDER_ON_RELEASE.id())?,
            slider_steps: f(pinned::ui::SLIDER_STEPS.id())?,
            slider_format: f(pinned::ui::SLIDER_FORMAT.id())?,
            slider_discrete: f(pinned::ui::SLIDER_DISCRETE.id())?,
            slider_stop_count: f(pinned::ui::SLIDER_STOP_COUNT.id())?,
            slider_stops: f(pinned::ui::SLIDER_STOPS.id())?,
            slider_stop_capacity: f(pinned::ui::SLIDER_STOP_CAPACITY.id())?,
            dropdown_count: f(pinned::ui::DROPDOWN_COUNT.id())?,
            dropdown_enabled: f(pinned::ui::DROPDOWN_ENABLED.id())?,
            dropdown_capacity: f(pinned::ui::DROPDOWN_CAPACITY.id())?,
            dropdown_entry_text: f(pinned::ui::DROPDOWN_ENTRY_TEXT.id())?,
            menu_entry_text: f(pinned::ui::MENU_ENTRY_TEXT.id())?,
            menu_entry_font: f(pinned::ui::MENU_ENTRY_FONT.id())?,
            action_node: f(pinned::ui::ACTION_NODE.id())?,
            action_bound: f(pinned::ui::ACTION_BOUND.id())?,
        }
        .checked()
    }

    /// Refuses a layout whose lists would run past the loader's buffers.
    fn checked(self) -> Result<Self, Missing> {
        let stops = self
            .slider_stop_capacity
            .checked_mul(4)
            .and_then(|size| size.checked_add(self.slider_stops));
        if stops.is_none_or(|end| end > SLIDER_CONFIG) {
            return Err(Missing::Room(pinned::ui::SLIDER_STOP_CAPACITY.id()));
        }
        let entries = |count: usize| count.checked_mul(self.entry_size);
        if entries(TOGGLE_ENTRIES).is_none_or(|end| end > self.toggle_count) {
            return Err(Missing::Room(pinned::ui::TOGGLE_COUNT.id()));
        }
        if entries(self.dropdown_capacity).is_none_or(|end| end > self.dropdown_count) {
            return Err(Missing::Room(pinned::ui::DROPDOWN_CAPACITY.id()));
        }
        Ok(self)
    }
}

impl DialogLayout {
    fn read(lookup: Lookup<'_>) -> Result<Self, Missing> {
        let f = |id| field(lookup, id);
        fits(lookup, pinned::ui::POPUP_EXTENT.id(), POPUP_SIZE)?;
        Ok(Self {
            title: f(pinned::ui::POPUP_TITLE.id())?,
            message: f(pinned::ui::POPUP_MESSAGE.id())?,
            buttons: f(pinned::ui::POPUP_BUTTONS.id())?,
            detail: f(pinned::ui::POPUP_DETAIL.id())?,
            detail_len: f(pinned::ui::POPUP_DETAIL_LEN.id())?,
            answer: f(pinned::ui::HOST_ANSWER.id())?,
            answered: f(pinned::ui::HOST_ANSWERED.id())?,
        })
    }
}

impl ViewLayout {
    fn read(lookup: Lookup<'_>) -> Result<Self, Missing> {
        Ok(Self {
            read_only: field(lookup, pinned::ui::VIEW_READ_ONLY.id())?,
            tab: field(lookup, pinned::ui::VIEW_TAB.id())?,
        })
    }
}

/// A window's own objects, kept in place from its start to its end.
pub(super) struct Window {
    object: Buf<WINDOW_OBJECT>,
    backdrop: Buf<FRAME_SIZE>,
    panel: Buf<FRAME_SIZE>,
    nav: Buf<NAV_SIZE>,
    saved_nav: usize,
    layer: u32,
    gap: f32,
}

impl Window {
    pub(super) const fn new() -> Self {
        Self {
            object: Buf::zeroed(),
            backdrop: Buf::zeroed(),
            panel: Buf::zeroed(),
            nav: Buf::zeroed(),
            saved_nav: 0,
            layer: 0x1f,
            gap: 30.0,
        }
    }

    /// A full screen of the loader's own on `layer`, its rows `gap` apart.
    pub(super) const fn screen(layer: u32, gap: f32) -> Self {
        let mut window = Self::new();
        window.layer = layer;
        window.gap = gap;
        window
    }
}

/// The HUD window's own objects, kept in place from its start to its end.
pub(super) struct HudWindow {
    object: Buf<WINDOW_OBJECT>,
    root: Buf<FRAME_SIZE>,
    safe: Buf<FRAME_SIZE>,
}

impl HudWindow {
    pub(super) const fn new() -> Self {
        Self {
            object: Buf::zeroed(),
            root: Buf::zeroed(),
            safe: Buf::zeroed(),
        }
    }
}

/// Where the looks the loader uses sit in the look bundle, from reflection.
#[derive(Debug, Clone, Copy)]
struct Looks {
    /// Label styles, by `UiWidget::STYLE_*`.
    styles: [usize; 8],
    title: usize,
    title_size: usize,
    outline: usize,
    /// Sounds, by `UiWidget::SOUND_*`.
    sounds: [usize; 4],
    setting_off: usize,
    setting_on: usize,
}

/// The engine functions and looks, resolved for the running build.
#[derive(Debug)]
pub(crate) struct Engine {
    context: usize,
    looks: Looks,
    pub(super) layout: Layout,
    /// The dialogs' fields; `None` keeps dialogs in system windows.
    pub(super) dialog: Option<DialogLayout>,
    /// The world settings view's fields; `None` keeps the mods' rows out.
    pub(super) view: Option<ViewLayout>,
    /// The scroll area's functions and looks; `None` pages long lists.
    scroll: Option<scroll::Scrolling>,
    /// The text field's functions; `None` shows texts read-only.
    text_fields: Option<text_field::TextFields>,
    window_begin: WindowBeginFn,
    window_end: OneFn,
    frame_from_node: FrameFromNodeFn,
    frame_open: FrameOpenFn,
    frame_close: OneFn,
    frame_stretch: OneFn,
    frame_color: ColorFn,
    node_padding: NodeRectFn,
    node_spacing: NodeSpacingFn,
    node_align: NodeAlignFn,
    node_width: NodeFloatFn,
    node_height: NodeFloatFn,
    node_stretch: NodeStretchFn,
    panel: PanelFn,
    nav_init: NavInitFn,
    nav_input: NavInputFn,
    nav_set: NavSetFn,
    nav_push: NavPushFn,
    nav_end: NavEndFn,
    nav_drop: OneFn,
    label_new: LabelNewFn,
    label_draw: OneFn,
    button: ButtonFn,
    toggle: ToggleFn,
    slider_init: RowInitFn,
    slider: SliderFn,
    dropdown_init: RowInitFn,
    dropdown: DropdownFn,
    progress: ProgressFn,
    sound: SoundFn,
    loca_id: LocaIdFn,
    loca_text: LocaTextFn,
    hud_visible: VisibleFn,
    main_entry: MainEntryFn,
    pause_entry: PauseEntryFn,
    action_new: ActionNewFn,
    action_query: ActionQueryFn,
    pub(super) main_menu_site: u64,
    pub(super) pause_menu_site: u64,
    pub(super) world_settings_site: u64,
    /// The main menu's site between its version line and its legal line,
    /// when its code holds on this build; 0 keeps the loader's line off.
    pub(super) legal_line_site: u64,
    /// The look of the main menu's version line; `None` keeps the loader's
    /// line off.
    legal: Option<Legal>,
}

/// Where the look of the main menu's version line sits in the look bundle,
/// from reflection.
#[derive(Debug, Clone, Copy)]
struct Legal {
    style: usize,
    size: usize,
}

impl Legal {
    fn find(loader: &Loader) -> Option<Self> {
        Some(Self {
            style: style(loader, "legalText")?,
            size: path(loader, &[(BUNDLE, "fontSizes"), (FONT_SIZES, "xsmall")])?,
        })
    }
}

/// Every function the loader calls or hooks, by symbol; each must read its
/// `<symbol>.bytes` constant.
const FUNCTIONS: [Symbol; 43] = [
    pinned::ui::WORLD_SETTINGS_SITE,
    pinned::ui::MAIN_ENTRY,
    pinned::ui::PAUSE_ENTRY,
    pinned::ui::ACTION_NEW,
    pinned::ui::ACTION_QUERY,
    pinned::ui::MAIN_MENU_SITE,
    pinned::ui::PAUSE_MENU_SITE,
    pinned::ui::HUD,
    pinned::ui::HUD_VISIBLE,
    pinned::ui::POPUP,
    pinned::ui::WINDOW_BEGIN,
    pinned::ui::WINDOW_END,
    pinned::ui::FRAME_FROM_NODE,
    pinned::ui::FRAME_OPEN,
    pinned::ui::FRAME_CLOSE,
    pinned::ui::FRAME_STRETCH,
    pinned::ui::FRAME_COLOR,
    pinned::ui::NODE_PADDING,
    pinned::ui::NODE_SPACING,
    pinned::ui::NODE_ALIGN,
    pinned::ui::NODE_WIDTH,
    pinned::ui::NODE_HEIGHT,
    pinned::ui::NODE_STRETCH,
    pinned::ui::PANEL,
    pinned::ui::NAV_INIT,
    pinned::ui::NAV_INPUT,
    pinned::ui::NAV_SET,
    pinned::ui::NAV_PUSH,
    pinned::ui::NAV_END,
    pinned::ui::NAV_DROP,
    pinned::ui::LABEL_NEW,
    pinned::ui::LABEL_DRAW,
    pinned::ui::BUTTON,
    pinned::ui::TOGGLE,
    pinned::ui::SLIDER_INIT,
    pinned::ui::SLIDER,
    pinned::ui::DROPDOWN_INIT,
    pinned::ui::DROPDOWN,
    pinned::ui::PROGRESS,
    pinned::ui::SOUND,
    pinned::ui::LOCA_ID,
    pinned::ui::LOCA_TEXT,
    pinned::ui::CONTEXT,
];

/// The bundle offset of a reflected path, `(type, field)` pairs summed.
fn path(loader: &Loader, steps: &[(&str, &str)]) -> Option<usize> {
    steps.iter().try_fold(0usize, |sum, (ty, field)| {
        let offset = loader.reflection.field_offset(ty, field).ok()?;
        Some(sum + usize::try_from(offset).ok()?)
    })
}

const BUNDLE: &str = "keen::FbUiBundle";
const STYLES: &str = "keen::FbUiCharacterStyles";
const SOUNDS: &str = "keen::FbUiSounds";
const FONT_SIZES: &str = "keen::FbUiFontSizes";
const MENU_SOUNDS: &str = "keen::FbUiSoundsMenuGeneric";

fn style(loader: &Loader, name: &str) -> Option<usize> {
    path(loader, &[(BUNDLE, "namedStyles"), (STYLES, name)])
}

fn sound(loader: &Loader, name: &str) -> Option<usize> {
    path(
        loader,
        &[(BUNDLE, "sounds"), (SOUNDS, "menu"), (MENU_SOUNDS, name)],
    )
}

fn setting_text(loader: &Loader, name: &str) -> Option<usize> {
    path(
        loader,
        &[
            (BUNDLE, "locatags"),
            ("keen::FbUiLocaLinks", "menuStrings"),
            ("keen::FbUiLocaMenus", "settings"),
            ("keen::FbUiLocaMenuSettings", name),
        ],
    )
}

fn looks(loader: &Loader) -> Option<Looks> {
    Some(Looks {
        styles: [
            style(loader, "defaultStyle")?,
            style(loader, "secondaryText")?,
            style(loader, "h1")?,
            style(loader, "h2")?,
            style(loader, "h3")?,
            style(loader, "warning")?,
            style(loader, "greenText")?,
            style(loader, "redText")?,
        ],
        title: path(
            loader,
            &[
                (BUNDLE, "namedStyles"),
                (STYLES, "menu"),
                ("keen::FbUiMenuCharacterStyles", "header1"),
            ],
        )?,
        outline: style(loader, "defaultStyleOutline")?,
        title_size: path(loader, &[(BUNDLE, "fontSizes"), (FONT_SIZES, "caps_large")])?,
        sounds: [
            sound(loader, "mouseClickGeneric")?,
            sound(loader, "entryTrigger")?,
            sound(loader, "back")?,
            sound(loader, "entryTriggerFailed")?,
        ],
        setting_off: setting_text(loader, "settingOff")?,
        setting_on: setting_text(loader, "settingOn")?,
    })
}

/// The address of game function `name`, once its first eight bytes read as
/// its `<name>.bytes` constant says.
fn verify(loader: &Loader, name: Symbol) -> Result<usize, Missing> {
    let address = loader.address(name).ok_or(Missing::Entry(name.id()))?;
    let expected = loader
        .build
        .constant(&format!("{name}.bytes"))
        .ok_or(Missing::Bytes(name.id()))?;
    let address = usize::try_from(address).map_err(|_| Missing::Entry(name.id()))?;
    // SAFETY: `loader.address` resolves `name` to a function in the game's
    // executable, mapped for the process, so eight bytes at its entry are
    // readable; code is not written while it runs.
    let found = unsafe { crate::memory::read::<u64>(address, 0) };
    if found != expected {
        return Err(Missing::Bytes(name.id()));
    }
    Ok(address)
}

/// Game function `name`, checked by [`verify`], as a function pointer of
/// type `F`.
///
/// # Safety
///
/// - `F` is the `extern "system"` function pointer type of the game
///   function `name` names on every build whose table pins it.
unsafe fn game_fn<F: Copy>(loader: &Loader, name: Symbol) -> Result<F, Missing> {
    const { assert!(size_of::<F>() == size_of::<usize>()) };
    let address = verify(loader, name)?;
    // SAFETY: `address` is non-zero code of the game whose entry bytes
    // `verify` just matched against the table, and `F` is a function
    // pointer of its type and of `usize`'s size by this function's
    // `# Safety` and the assertion above.
    Ok(unsafe { core::mem::transmute_copy::<usize, F>(&address) })
}

/// Why the interface stays off on this build.
#[derive(Debug)]
pub(crate) enum Missing {
    Entry(&'static str),
    Bytes(&'static str),
    Room(&'static str),
    Looks,
}

impl core::fmt::Display for Missing {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Entry(name) => write!(f, "{name} is not in this build"),
            Self::Bytes(name) => write!(f, "{name} does not read as the build table says"),
            Self::Room(name) => write!(f, "{name} runs past the loader's buffer"),
            Self::Looks => f.write_str("the look bundle lacks a reflected field"),
        }
    }
}

impl Engine {
    /// Resolves every function and look; the first that is missing or does
    /// not read as expected keeps the interface off. The scroll area and
    /// the text field resolve on their own: without them the interface
    /// stays on and pages its lists and shows texts read-only.
    pub(crate) fn resolve(loader: &Loader) -> Result<Self, Missing> {
        for name in FUNCTIONS {
            if name == pinned::ui::CONTEXT {
                loader.address(name).ok_or(Missing::Entry(name.id()))?;
                continue;
            }
            verify(loader, name)?;
        }
        let looks = looks(loader).ok_or(Missing::Looks)?;
        let lookup = |id: &str| loader.build.constant(id);
        let layout = Layout::read(&lookup)?;
        let dialog = DialogLayout::read(&lookup)
            .inspect_err(|why| crate::log!(WARN, "interface dialogs off: {why}"))
            .ok();
        let view = ViewLayout::read(&lookup)
            .inspect_err(|why| crate::log!(WARN, "the world settings' Mods rows off: {why}"))
            .ok();
        let scroll = scroll::Scrolling::resolve(loader)
            .inspect_err(|why| {
                crate::log!(WARN, "the Mods screen pages instead of scrolling: {why}");
            })
            .ok();
        let text_fields = text_field::TextFields::resolve(loader)
            .inspect_err(|why| {
                crate::log!(
                    WARN,
                    "text settings are read-only on the Mods screen: {why}"
                );
            })
            .ok();
        let at = |name: Symbol| {
            loader
                .address(name)
                .and_then(|a| usize::try_from(a).ok())
                .unwrap_or(0)
        };
        macro_rules! function {
            ($name:expr, $ty:ty) => {{
                let address = at($name);
                // SAFETY: the loop over `FUNCTIONS` returned unless every
                // function's first eight bytes match its `<name>.bytes`
                // constant, so the address is the game function the build table
                // types as `$ty`.
                unsafe { core::mem::transmute::<usize, $ty>(address) }
            }};
        }
        Ok(Self {
            context: at(pinned::ui::CONTEXT),
            looks,
            layout,
            dialog,
            view,
            scroll,
            text_fields,
            window_begin: function!(pinned::ui::WINDOW_BEGIN, WindowBeginFn),
            window_end: function!(pinned::ui::WINDOW_END, OneFn),
            frame_from_node: function!(pinned::ui::FRAME_FROM_NODE, FrameFromNodeFn),
            frame_open: function!(pinned::ui::FRAME_OPEN, FrameOpenFn),
            frame_close: function!(pinned::ui::FRAME_CLOSE, OneFn),
            frame_stretch: function!(pinned::ui::FRAME_STRETCH, OneFn),
            frame_color: function!(pinned::ui::FRAME_COLOR, ColorFn),
            node_padding: function!(pinned::ui::NODE_PADDING, NodeRectFn),
            node_spacing: function!(pinned::ui::NODE_SPACING, NodeSpacingFn),
            node_align: function!(pinned::ui::NODE_ALIGN, NodeAlignFn),
            node_width: function!(pinned::ui::NODE_WIDTH, NodeFloatFn),
            node_height: function!(pinned::ui::NODE_HEIGHT, NodeFloatFn),
            node_stretch: function!(pinned::ui::NODE_STRETCH, NodeStretchFn),
            panel: function!(pinned::ui::PANEL, PanelFn),
            nav_init: function!(pinned::ui::NAV_INIT, NavInitFn),
            nav_input: function!(pinned::ui::NAV_INPUT, NavInputFn),
            nav_set: function!(pinned::ui::NAV_SET, NavSetFn),
            nav_push: function!(pinned::ui::NAV_PUSH, NavPushFn),
            nav_end: function!(pinned::ui::NAV_END, NavEndFn),
            nav_drop: function!(pinned::ui::NAV_DROP, OneFn),
            label_new: function!(pinned::ui::LABEL_NEW, LabelNewFn),
            label_draw: function!(pinned::ui::LABEL_DRAW, OneFn),
            button: function!(pinned::ui::BUTTON, ButtonFn),
            toggle: function!(pinned::ui::TOGGLE, ToggleFn),
            slider_init: function!(pinned::ui::SLIDER_INIT, RowInitFn),
            slider: function!(pinned::ui::SLIDER, SliderFn),
            dropdown_init: function!(pinned::ui::DROPDOWN_INIT, RowInitFn),
            dropdown: function!(pinned::ui::DROPDOWN, DropdownFn),
            progress: function!(pinned::ui::PROGRESS, ProgressFn),
            sound: function!(pinned::ui::SOUND, SoundFn),
            loca_id: function!(pinned::ui::LOCA_ID, LocaIdFn),
            loca_text: function!(pinned::ui::LOCA_TEXT, LocaTextFn),
            hud_visible: function!(pinned::ui::HUD_VISIBLE, VisibleFn),
            main_entry: function!(pinned::ui::MAIN_ENTRY, MainEntryFn),
            pause_entry: function!(pinned::ui::PAUSE_ENTRY, PauseEntryFn),
            action_new: function!(pinned::ui::ACTION_NEW, ActionNewFn),
            action_query: function!(pinned::ui::ACTION_QUERY, ActionQueryFn),
            main_menu_site: loader.address(pinned::ui::MAIN_MENU_SITE).unwrap_or(0),
            pause_menu_site: loader.address(pinned::ui::PAUSE_MENU_SITE).unwrap_or(0),
            world_settings_site: loader.address(pinned::ui::WORLD_SETTINGS_SITE).unwrap_or(0),
            legal_line_site: if loader.code_holds(pinned::ui::LEGAL_LINE_SITE) {
                loader.address(pinned::ui::LEGAL_LINE_SITE).unwrap_or(0)
            } else {
                0
            },
            legal: Legal::find(loader),
        })
    }

    /// The interface context of the frame being drawn; 0 outside one.
    pub(super) fn context(&self) -> usize {
        // SAFETY: `self.context` is the address of the game's `ui.context`
        // global from the build table, eight bytes of the executable's data
        // mapped for the process; it holds the context or 0 outside a frame.
        unsafe { crate::memory::read::<usize>(self.context, 0) }
    }

    fn bundle(&self, context: usize) -> usize {
        // SAFETY: every caller passes the non-zero context `Engine::context`
        // read in this frame, which lives until the frame ends; its bundle
        // pointer sits at the table's `ui.context_bundle`.
        unsafe { crate::memory::read::<usize>(context, self.layout.context_bundle) }
    }

    /// The interface host of `context`.
    pub(super) fn host(&self, context: usize) -> usize {
        // SAFETY: every caller passes the non-zero context `Engine::context`
        // read in this frame, which lives until the frame ends; its host
        // pointer sits at the table's `ui.context_host`.
        unsafe { crate::memory::read::<usize>(context, self.layout.context_host) }
    }

    /// The screen byte of the context's play state: 1 to 3 menus, 4 and 5
    /// slideshows, 6 loading, 7 in game; 0 when it cannot be read.
    pub(super) fn app_state(&self, context: usize) -> u8 {
        // SAFETY: every caller passes the non-zero context `Engine::context`
        // read in this frame, which lives until the frame ends; its play
        // state pointer sits at the table's `ui.context_state`.
        let state = unsafe { crate::memory::read::<usize>(context, self.layout.context_state) };
        if state == 0 {
            return 0;
        }
        // SAFETY: `state` is non-zero, checked by the guard before it, and
        // points at the play state, whose first eight bytes hold its record
        // pointer.
        let record = unsafe { crate::memory::read::<usize>(state, 0) };
        if record == 0 {
            return 0;
        }
        // SAFETY: `record` is non-zero, checked by the guard before it, and
        // points at the play state's record, whose screen byte sits at the
        // table's `ui.state_screen`.
        unsafe { crate::memory::read::<u8>(record, self.layout.state_screen) }
    }

    /// The navigation group the context points at.
    pub(super) fn nav(&self, context: usize) -> usize {
        // SAFETY: every caller passes the non-zero context `Engine::context`
        // read in this frame, which lives until the frame ends; its
        // navigation group pointer sits at the table's `ui.context_nav`.
        unsafe { crate::memory::read::<usize>(context, self.layout.context_nav) }
    }

    /// Points the context back at `nav`, after a draw that left it at an
    /// object of its own stack.
    pub(super) fn restore_nav(&self, context: usize, nav: usize) {
        // SAFETY: every caller passes the non-zero context `Engine::context`
        // read in this frame, on the thread that draws it, after the game's
        // own draw returned, so nothing else touches the navigation pointer
        // at the table's `ui.context_nav`.
        unsafe { crate::memory::write(context, self.layout.context_nav, nav) };
    }
}

const WINDOW_WIDTH: f32 = 900.0;
/// HUD elements the HUD hides from: full-screen menus, screenshot mode
/// (0x400) and character dialogs (0x200), as the game's status bar.
const HUD_MASK: u32 = 0x600;
const HUD_MARGIN: f32 = 48.0;
const HUD_WIDTH: f32 = 420.0;
const HUD_GAP: f32 = 6.0;
const BAR_HEIGHT: f32 = 20.0;

fn stretch(x: f32, y: f32) -> u64 {
    u64::from(x.to_bits()) | u64::from(y.to_bits()) << 32
}

/// The slider's value text: whole numbers when every stop is one, else two
/// decimals.
fn slider_format(min: f32, max: f32, steps: u32) -> u32 {
    let whole = |v: f32| v.fract() == 0.0;
    #[expect(clippy::cast_precision_loss, reason = "a count of stops")]
    let unit = steps >= 2 && whole(min) && whole(max) && whole((max - min) / (steps - 1) as f32);
    if unit { 2 } else { 4 }
}

mod hud;
mod menus;
mod scroll;
#[cfg(test)]
mod tests;
mod text_field;
mod widgets;
mod windows;

pub(super) use scroll::Scroll;
pub(super) use text_field::{TextBuffer, Typed};
