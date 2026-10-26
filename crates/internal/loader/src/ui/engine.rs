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
    pub(super) popup: u64,
    looks: Looks,
    pub(super) layout: Layout,
    /// The dialogs' fields; `None` keeps dialogs in system windows.
    pub(super) dialog: Option<DialogLayout>,
    /// The world settings view's fields; `None` keeps the mods' rows out.
    pub(super) view: Option<ViewLayout>,
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
    pub(super) hud: u64,
    main_entry: MainEntryFn,
    pause_entry: PauseEntryFn,
    action_new: ActionNewFn,
    action_query: ActionQueryFn,
    pub(super) main_menu_site: u64,
    pub(super) pause_menu_site: u64,
    pub(super) world_settings_site: u64,
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
        title_size: path(
            loader,
            &[(BUNDLE, "fontSizes"), ("keen::FbUiFontSizes", "caps_large")],
        )?,
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
    /// not read as expected keeps the interface off.
    pub(crate) fn resolve(loader: &Loader) -> Result<Self, Missing> {
        for name in FUNCTIONS {
            let address = loader.address(name).ok_or(Missing::Entry(name.id()))?;
            if name == pinned::ui::CONTEXT {
                continue;
            }
            let expected = loader
                .build
                .constant(&format!("{name}.bytes"))
                .ok_or(Missing::Bytes(name.id()))?;
            let address = usize::try_from(address).map_err(|_| Missing::Entry(name.id()))?;
            // SAFETY: `loader.address` resolves `name` to a function in the
            // game's executable, mapped for the process, so eight bytes at
            // its entry are readable; code is not written while it runs.
            let found = unsafe { crate::memory::read::<u64>(address, 0) };
            if found != expected {
                return Err(Missing::Bytes(name.id()));
            }
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
            popup: loader.address(pinned::ui::POPUP).unwrap_or(0),
            looks,
            layout,
            dialog,
            view,
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
            hud: loader.address(pinned::ui::HUD).unwrap_or(0),
            main_entry: function!(pinned::ui::MAIN_ENTRY, MainEntryFn),
            pause_entry: function!(pinned::ui::PAUSE_ENTRY, PauseEntryFn),
            action_new: function!(pinned::ui::ACTION_NEW, ActionNewFn),
            action_query: function!(pinned::ui::ACTION_QUERY, ActionQueryFn),
            main_menu_site: loader.address(pinned::ui::MAIN_MENU_SITE).unwrap_or(0),
            pause_menu_site: loader.address(pinned::ui::PAUSE_MENU_SITE).unwrap_or(0),
            world_settings_site: loader.address(pinned::ui::WORLD_SETTINGS_SITE).unwrap_or(0),
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

    /// The node of a frame the game opened in `frame`.
    fn frame_node<const N: usize>(&self, frame: &Buf<N>) -> usize {
        frame.read_usize(self.layout.frame_node)
    }

    /// The id of `window`'s panel node, for actions bound to the window.
    pub(super) fn node_id(&self, window: &Window) -> u64 {
        let node = self.frame_node(&window.panel);
        if node == 0 {
            return 0;
        }
        // SAFETY: `node` is non-zero, checked by the guard before it: the game
        // wrote it into the panel frame when the window was drawn, and its id
        // sits at the table's `ui.node_id`.
        unsafe { crate::memory::read::<u64>(node, self.layout.node_id) }
    }

    /// Starts a window: the game's popup panel over a dimmed screen, `width`
    /// wide, with a title when `title` is not empty.
    ///
    /// # Safety
    ///
    /// - Called inside a frame of the game's interface update, on its
    ///   thread.
    /// - `context` is the non-zero context [`Engine::context`] read in this
    ///   frame.
    /// - `window` stays in place until [`Engine::window_end`] ends it in the
    ///   same frame.
    /// - The bytes `name` and `title` point at outlive the frame.
    pub(super) unsafe fn window_begin(
        &self,
        context: usize,
        window: &mut Window,
        name: &UiText,
        title: &UiText,
    ) {
        window.saved_nav = self.nav(context);
        let name = [name.ptr.expose_provenance() as u64, name.len];
        // SAFETY: `self.window_begin` is the game's `ui.window_begin`, its
        // entry bytes checked in `Engine::resolve`, called in a frame on its
        // thread by this function's `# Safety`; `window.object` holds the
        // table's `ui.window_extent` bytes (`Layout::read`), and `name` is a
        // live local over bytes that outlive the frame. The arguments are
        // the game popup's: full screen, its layer, all input, flags 1.
        unsafe {
            (self.window_begin)(
                window.object.ptr(),
                &raw const name,
                1,
                window.layer,
                u32::MAX,
                1,
            );
        }
        let node = window.object.read_usize(self.layout.window_node);
        // SAFETY: `self.frame_from_node` is the game's `ui.frame_from_node`,
        // its entry bytes checked in `Engine::resolve`; `node` is the one the
        // game just wrote into the window object, and `window.backdrop` holds
        // the table's `ui.frame_extent` bytes (`Layout::read`).
        unsafe { (self.frame_from_node)(window.backdrop.ptr(), node) };
        // SAFETY: `self.frame_stretch` is the game's `ui.frame_stretch`, its
        // entry bytes checked in `Engine::resolve`, on the backdrop frame the
        // game filled one call earlier.
        unsafe { (self.frame_stretch)(window.backdrop.ptr()) };
        let dim: u64 = 0x8000_0000 | u64::from(1.0f32.to_bits()) << 32;
        // SAFETY: `self.frame_color` is the game's `ui.frame_color`, its
        // entry bytes checked in `Engine::resolve`, on the filled backdrop
        // frame, with `dim` a live local the game reads eight bytes of.
        unsafe { (self.frame_color)(window.backdrop.ptr(), &raw const dim) };
        // SAFETY: `self.frame_open` is the game's `ui.frame_open`, its entry
        // bytes checked in `Engine::resolve`; `window.panel` holds the
        // table's `ui.frame_extent` bytes (`Layout::read`), name 0 for none.
        unsafe { (self.frame_open)(window.panel.ptr(), 0) };
        let panel = self.frame_node(&window.panel);
        let padding = [90.0f32, 60.0, 90.0, 60.0];
        // SAFETY: `self.node_padding` is the game's `ui.node_padding`, its
        // entry bytes checked in `Engine::resolve`, on the node the game just
        // wrote into the panel frame, with four live floats.
        unsafe { (self.node_padding)(panel, &raw const padding) };
        let gap = [0.0f32, window.gap];
        // SAFETY: `self.node_spacing` is the game's `ui.node_spacing`, its
        // entry bytes checked in `Engine::resolve`, on the panel's node, with
        // two live floats.
        unsafe { (self.node_spacing)(panel, &raw const gap, 0) };
        let centre: u16 = 0x202;
        // SAFETY: `self.node_align` is the game's `ui.node_align`, its entry
        // bytes checked in `Engine::resolve`, on the panel's node, with a
        // live alignment word.
        unsafe { (self.node_align)(panel, &raw const centre) };
        // SAFETY: `self.node_width` is the game's `ui.node_width`, its entry
        // bytes checked in `Engine::resolve`, on the panel's node.
        unsafe { (self.node_width)(panel, WINDOW_WIDTH) };
        // SAFETY: `self.node_stretch` is the game's `ui.node_stretch`, its
        // entry bytes checked in `Engine::resolve`, on the panel's node.
        unsafe { (self.node_stretch)(panel, 0) };
        let mut look = Buf::<PANEL_CONFIG>::zeroed();
        look.u32(self.layout.panel_kind, 9);
        look.f32(self.layout.panel_alpha, 1.0);
        // SAFETY: `self.panel` is the game's `ui.panel`, its entry bytes
        // checked in `Engine::resolve`, on the opened panel frame; `look`
        // holds the table's `ui.panel_extent` bytes (`Layout::read`).
        unsafe { (self.panel)(window.panel.ptr(), look.ptr()) };
        if title.len > 0 {
            // SAFETY: this function's `# Safety` gives a frame on its thread,
            // the frame's `context` and a `title` that outlives the frame, and
            // the panel frame opened above is the window `title` asks for.
            unsafe { self.title(context, title) };
        }
        let mut nav = Buf::<NAV_CONFIG>::zeroed();
        let [kind, wrap, axis, flags, input, layer, sound] = self.layout.nav;
        nav.u32(kind, 2);
        nav.u8(wrap, 1);
        nav.u32(axis, 3);
        nav.u16(flags, 0x101);
        nav.u32(input, 2);
        nav.u16(layer, 0x1b);
        nav.u16(sound, 1);
        // SAFETY: `self.nav_init` is the game's `ui.nav_init`, its entry bytes
        // checked in `Engine::resolve`; `window.nav` holds the table's
        // `ui.nav_extent` bytes and `nav` its `ui.nav_config_size`
        // (`Layout::read`).
        unsafe { (self.nav_init)(window.nav.ptr(), nav.ptr()) };
        let mut input = [0u64; 2];
        // SAFETY: `self.nav_input` is the game's `ui.nav_input`, its entry
        // bytes checked in `Engine::resolve`; it writes sixteen bytes to the
        // live `input` and reads the `ui.nav_config_size` bytes of `nav`.
        unsafe { (self.nav_input)(&raw mut input, nav.ptr(), 1) };
        // SAFETY: `self.nav_set` is the game's `ui.nav_set`, its entry bytes
        // checked in `Engine::resolve`, on the group `nav_init` filled, with
        // the live `input`.
        unsafe { (self.nav_set)(window.nav.ptr(), &raw const input) };
        // SAFETY: `self.nav_push` is the game's `ui.nav_push`, its entry bytes
        // checked in `Engine::resolve`, on the frame's `context` by this
        // function's `# Safety`; the group stays in place in `window` until
        // `Engine::window_end` drops it, by the same contract.
        unsafe { (self.nav_push)(context, window.nav.ptr()) };
    }

    /// Ends a window started with [`Engine::window_begin`].
    ///
    /// # Safety
    ///
    /// - Called on the frame's thread in the frame whose
    ///   [`Engine::window_begin`] started `window`, with the same `context`.
    /// - `window` did not move since it started.
    /// - Every frame opened inside `window` is closed.
    pub(super) unsafe fn window_end(&self, context: usize, window: &mut Window) {
        // SAFETY: `self.nav_end` is the game's `ui.nav_end`, its entry bytes
        // checked in `Engine::resolve`, on the group `window_begin` pushed in
        // this frame, unmoved by this function's `# Safety`.
        unsafe { (self.nav_end)(window.nav.ptr(), 0) };
        // SAFETY: `self.nav_drop` is the game's `ui.nav_drop`, its entry bytes
        // checked in `Engine::resolve`, on the group `window_begin` pushed in
        // this frame and `nav_end` just ended.
        unsafe { (self.nav_drop)(window.nav.ptr()) };
        self.restore_nav(context, window.saved_nav);
        // SAFETY: `self.frame_close` is the game's `ui.frame_close`, its entry
        // bytes checked in `Engine::resolve`, on the panel frame
        // `window_begin` opened; every frame inside it is closed by this
        // function's `# Safety`.
        unsafe { (self.frame_close)(window.panel.ptr()) };
        // SAFETY: `self.frame_close` is the game's `ui.frame_close`, its entry
        // bytes checked in `Engine::resolve`, on the backdrop frame
        // `window_begin` made, whose inner panel frame closed one call
        // earlier.
        unsafe { (self.frame_close)(window.backdrop.ptr()) };
        // SAFETY: `self.window_end` is the game's `ui.window_end`, its entry
        // bytes checked in `Engine::resolve`, on the window object
        // `window_begin` started, whose frames are now closed.
        unsafe { (self.window_end)(window.object.ptr()) };
    }

    /// Draws the window's title in the popup's title style.
    ///
    /// # Safety
    ///
    /// - Called inside a window, on the frame's thread.
    /// - `context` is the non-zero context [`Engine::context`] read in this
    ///   frame.
    /// - The bytes `text` points at outlive the frame.
    unsafe fn title(&self, context: usize, text: &UiText) {
        let layout = &self.layout;
        let bundle = self.bundle(context);
        let mut label = Buf::<LABEL_SIZE>::zeroed();
        // SAFETY: `self.label_new` is the game's `ui.label_new`, its entry
        // bytes checked in `Engine::resolve`; `label` holds the table's
        // `ui.label_extent` bytes (`Layout::read`), `text` outlives the frame
        // by this function's `# Safety`, and the style is the reflected
        // `header1` offset inside the frame's look bundle.
        unsafe { (self.label_new)(label.ptr(), text, bundle + self.looks.title, 0) };
        label.u8(layout.label_shown, 1);
        let node = self.frame_node(&label);
        let centre: u16 = 0x202;
        // SAFETY: `self.node_align` is the game's `ui.node_align`, its entry
        // bytes checked in `Engine::resolve`, on the node the game just wrote
        // into `label`, with a live alignment word.
        unsafe { (self.node_align)(node, &raw const centre) };
        label.u16(layout.label_align, centre);
        // SAFETY: `bundle` is the frame's look bundle, read from the live
        // context, and `title_size` the reflected offset of its
        // `caps_large` font size, a float.
        let size = unsafe { crate::memory::read::<f32>(bundle, self.looks.title_size) };
        label.f32(layout.label_font_size, size);
        label.u16(layout.label_flags, label.read_u16(layout.label_flags) | 8);
        // SAFETY: `self.label_draw` is the game's `ui.label_draw`, its entry
        // bytes checked in `Engine::resolve`, on the label `label_new` filled.
        unsafe { (self.label_draw)(label.ptr()) };
    }

    /// Draws a label in style `style`, wrapped and centred as asked.
    ///
    /// # Safety
    ///
    /// - Called inside a window, on the frame's thread.
    /// - `context` is the non-zero context [`Engine::context`] read in this
    ///   frame.
    /// - The bytes `text` points at outlive the frame.
    pub(super) unsafe fn label(
        &self,
        context: usize,
        text: &UiText,
        style: u32,
        wrap: bool,
        center: bool,
    ) {
        let bundle = self.bundle(context);
        let style = self
            .looks
            .styles
            .get(style as usize)
            .copied()
            .unwrap_or(self.looks.styles[0]);
        let mut label = Buf::<LABEL_SIZE>::zeroed();
        // SAFETY: `self.label_new` is the game's `ui.label_new`, checked in
        // `Engine::resolve`; `label` holds the table's `ui.label_extent` bytes
        // (`Layout::read`), `text` outlives the frame by this function's
        // `# Safety`, and `style` is a reflected style offset inside the
        // frame's look bundle.
        unsafe { (self.label_new)(label.ptr(), text, bundle + style, u8::from(wrap)) };
        label.u8(self.layout.label_shown, 1);
        let node = self.frame_node(&label);
        let align: u16 = if center { 0x202 } else { 0x201 };
        // SAFETY: `self.node_align` is the game's `ui.node_align`, checked in
        // `Engine::resolve`, on the node the game just wrote into `label`,
        // with a live alignment word.
        unsafe { (self.node_align)(node, &raw const align) };
        label.u16(self.layout.label_align, align);
        if wrap {
            // SAFETY: `self.node_stretch` is the game's `ui.node_stretch`,
            // checked in `Engine::resolve`, on the node the game just wrote
            // into `label`.
            unsafe { (self.node_stretch)(node, stretch(1.0, 0.0)) };
        }
        // SAFETY: `self.label_draw` is the game's `ui.label_draw`, checked in
        // `Engine::resolve`, on the label `label_new` filled, inside the
        // window this function's `# Safety` gives.
        unsafe { (self.label_draw)(label.ptr()) };
    }

    /// Draws a button; true when clicked this frame.
    ///
    /// # Safety
    ///
    /// - Called inside a window, on the frame's thread.
    /// - The bytes `text` points at outlive the frame.
    pub(super) unsafe fn button(&self, text: &UiText, enabled: bool) -> bool {
        let layout = &self.layout;
        let mut config = Buf::<BUTTON_CONFIG>::zeroed();
        config.u16(layout.button_align, 0x202);
        config.u8(layout.button_centered, 1);
        config.u8(layout.button_enabled, u8::from(enabled));
        config.u64(layout.button_size_mode, 3);
        config.f32(layout.button_padding[0], 10.0);
        config.f32(layout.button_padding[1], 5.0);
        config.f32(layout.button_scale, 1.0);
        config.u8(layout.button_sound, 1);
        // SAFETY: `self.button` is the game's `ui.button`, checked in
        // `Engine::resolve`, inside the window this function's `# Safety`
        // gives; `config` holds the table's `ui.button_extent` bytes
        // (`Layout::read`) and `text` outlives the frame by the same contract.
        let clicked = unsafe { (self.button)(config.ptr(), text) } != 0;
        clicked && enabled
    }

    fn setting_texts(&self, context: usize) -> [UiText; TOGGLE_ENTRIES] {
        let bundle = self.bundle(context);
        [self.looks.setting_off, self.looks.setting_on].map(|offset| {
            // SAFETY: `self.loca_id` is the game's `ui.loca_id`, checked in
            // `Engine::resolve`; `offset` is the reflected offset of a loca
            // reference inside the frame's look bundle.
            let id = unsafe { (self.loca_id)(bundle + offset) };
            let mut text = UiText::EMPTY;
            // SAFETY: `self.loca_text` is the game's `ui.loca_text`, checked
            // in `Engine::resolve`; it writes a `UiText` to the live `text`,
            // pointing at the game's loca storage.
            unsafe { (self.loca_text)(&raw mut text, id) };
            text
        })
    }

    /// Draws a toggle row; the new state when the player changed it.
    ///
    /// # Safety
    ///
    /// - Called inside a window, on the frame's thread.
    /// - `context` is the non-zero context [`Engine::context`] read in this
    ///   frame.
    /// - The bytes `label` points at outlive the frame.
    pub(super) unsafe fn toggle(
        &self,
        context: usize,
        label: &UiText,
        on: bool,
        read_only: bool,
    ) -> Option<bool> {
        let layout = &self.layout;
        let mut config = Buf::<TOGGLE_CONFIG>::zeroed();
        let texts = self.setting_texts(context);
        let entries = (0..).step_by(layout.entry_size).zip(&texts);
        for (value, (at, text)) in (0u32..).zip(entries) {
            config.u32(at + layout.entry_value, value);
            config.text(at + layout.entry_text, text);
        }
        config.u8(layout.toggle_count, 2);
        config.text(layout.toggle_label, label);
        config.text(layout.toggle_explainer, &UiText::EMPTY);
        config.u8(layout.toggle_explain, 1);
        config.u8(layout.toggle_enabled, u8::from(!read_only));
        let mut value = u8::from(on);
        // SAFETY: `self.toggle` is the game's `ui.toggle`, checked in
        // `Engine::resolve`, inside the window this function's `# Safety`
        // gives; `config` holds the table's `ui.toggle_extent` bytes and its
        // two entries fit before `ui.toggle_count` (`Layout::read`), and
        // `value` is a live byte the game writes.
        let changed = unsafe { (self.toggle)(config.ptr(), &raw mut value) } != 0;
        (changed && !read_only).then_some(value != 0)
    }

    /// Draws a slider row; the new value when the player changed it.
    ///
    /// # Safety
    ///
    /// - Called inside a window, on the frame's thread.
    /// - The bytes `label` points at outlive the frame.
    pub(super) unsafe fn slider(
        &self,
        label: &UiText,
        value: f32,
        (min, max): (f32, f32),
        steps: u32,
        read_only: bool,
        on_release: bool,
    ) -> Option<f32> {
        let layout = &self.layout;
        let mut config = Buf::<SLIDER_CONFIG>::zeroed();
        // SAFETY: `self.slider_init` is the game's `ui.slider_init`, checked
        // in `Engine::resolve`; it fills the `SLIDER_CONFIG` bytes of
        // `config`, a size from the game's own slider pages (the table has no
        // extent for it; `Layout::checked` keeps the stops inside it), and
        // `label` outlives the frame by this function's `# Safety`.
        unsafe { (self.slider_init)(config.ptr(), label) };
        config.f32(layout.slider_min, min);
        config.f32(layout.slider_max, max);
        config.u8(layout.slider_bounded, 1);
        config.u8(layout.slider_read_only, u8::from(read_only));
        config.u8(layout.slider_on_release, u8::from(on_release));
        config.u32(layout.slider_format, slider_format(min, max, steps));
        let most = u32::try_from(layout.slider_stop_capacity).unwrap_or(0);
        if (2..=most).contains(&steps) {
            for (i, slot) in (0..steps).zip((layout.slider_stops..).step_by(4)) {
                #[expect(clippy::cast_precision_loss, reason = "a few dozen stops")]
                let at = min + (max - min) * i as f32 / (steps - 1) as f32;
                config.f32(slot, at);
            }
            config.u64(layout.slider_stop_count, u64::from(steps));
            config.u32(layout.slider_discrete, 1);
        } else if steps > most {
            config.u32(layout.slider_steps, steps - 1);
        }
        let mut current = value;
        // SAFETY: `self.slider` is the game's `ui.slider`, checked in
        // `Engine::resolve`, inside the window this function's `# Safety`
        // gives, on the config `slider_init` filled; `current` is a live float
        // the game writes, and the gamepad state may be null.
        let changed =
            unsafe { (self.slider)(config.ptr(), &raw mut current, core::ptr::null_mut()) } != 0;
        (changed && !read_only).then_some(current)
    }

    /// Draws a dropdown row; the new index when the player picked another.
    ///
    /// # Safety
    ///
    /// - Called inside a window, on the frame's thread.
    /// - The bytes `label` and every text of `options` point at outlive the
    ///   frame.
    pub(super) unsafe fn dropdown(
        &self,
        label: &UiText,
        options: &[UiText],
        index: u32,
        read_only: bool,
    ) -> Option<u32> {
        if options.is_empty() {
            return None;
        }
        let layout = &self.layout;
        let mut config = Buf::<DROPDOWN_CONFIG>::zeroed();
        // SAFETY: `self.dropdown_init` is the game's `ui.dropdown_init`,
        // checked in `Engine::resolve`; it fills `config`, which holds the
        // table's `ui.dropdown_extent` bytes (`Layout::read`), and `label`
        // outlives the frame by this function's `# Safety`.
        unsafe { (self.dropdown_init)(config.ptr(), label) };
        let capacity = layout.dropdown_capacity;
        let count = options.len().min(capacity);
        let entries = (0..).step_by(layout.entry_size).zip(&options[..count]);
        for (value, (at, text)) in (0u32..).zip(entries) {
            config.u32(at + layout.entry_value, value);
            config.text(at + layout.dropdown_entry_text, text);
        }
        config.u8(layout.dropdown_count, u8::try_from(count).unwrap_or(0));
        config.u8(layout.dropdown_enabled, u8::from(!read_only));
        // An index past the entries, "none", stays inside the config the
        // game reads the shown entry from.
        let last = count.min(capacity.saturating_sub(1));
        let shown = index.min(u32::try_from(last).unwrap_or(0));
        let mut value = i32::try_from(shown).unwrap_or(0);
        // SAFETY: `self.dropdown` is the game's `ui.dropdown`, checked in
        // `Engine::resolve`, inside the window this function's `# Safety`
        // gives; at most `ui.dropdown_capacity` entries fill the config before
        // `ui.dropdown_count` (`Layout::checked`), `value` is a live integer
        // the game writes, and the open state may be null.
        let changed =
            unsafe { (self.dropdown)(config.ptr(), &raw mut value, core::ptr::null_mut()) } != 0;
        (changed && !read_only)
            .then(|| u32::try_from(value).ok())
            .flatten()
    }

    /// Draws a bar filled to `value` between `min` and `max`, the width of
    /// the window.
    ///
    /// # Safety
    ///
    /// - Called inside a window or a HUD column, on the frame's thread.
    pub(super) unsafe fn progress(&self, value: f32, min: f32, max: f32) {
        let mut frame = Buf::<FRAME_SIZE>::zeroed();
        // SAFETY: `self.frame_open` is the game's `ui.frame_open`, checked in
        // `Engine::resolve`, inside the window this function's `# Safety`
        // gives; `frame` holds the table's `ui.frame_extent` bytes
        // (`Layout::read`), name 0 for none.
        unsafe { (self.frame_open)(frame.ptr(), 0) };
        let node = self.frame_node(&frame);
        // SAFETY: `self.node_height` is the game's `ui.node_height`, checked
        // in `Engine::resolve`, on the node the game just wrote into `frame`.
        unsafe { (self.node_height)(node, BAR_HEIGHT) };
        // SAFETY: `self.node_stretch` is the game's `ui.node_stretch`, checked
        // in `Engine::resolve`, on the node the game just wrote into `frame`.
        unsafe { (self.node_stretch)(node, stretch(1.0, 0.0)) };
        let white = 0xffff_ffff | u64::from(1.0f32.to_bits()) << 32;
        let shade = 0x8000_0000 | u64::from(1.0f32.to_bits()) << 32;
        // SAFETY: `self.progress` is the game's `ui.progress`, checked in
        // `Engine::resolve`, inside the frame opened above; it takes values
        // only.
        unsafe { (self.progress)(value - min, max - min, white, shade) };
        // SAFETY: `self.frame_close` is the game's `ui.frame_close`, checked
        // in `Engine::resolve`, on the frame `frame_open` opened above, with
        // nothing opened inside it.
        unsafe { (self.frame_close)(frame.ptr()) };
    }

    /// Whether the game shows HUD elements now: not in a full-screen menu,
    /// in screenshot mode or in a dialog with a character, as the game's
    /// status bar asks.
    ///
    /// # Safety
    ///
    /// - Called inside a frame of the game's interface update, on its
    ///   thread.
    pub(super) unsafe fn hud_visible(&self) -> bool {
        // SAFETY: `self.hud_visible` is the game's `ui.hud_visible`, checked
        // in `Engine::resolve`, called in a frame on its thread by this
        // function's `# Safety`; it reads the frame's context itself.
        unsafe { (self.hud_visible)(HUD_MASK) != 0 }
    }

    /// Starts the HUD window, a layer over the game's HUD that takes no
    /// input, with a frame over the screen inside its margins.
    ///
    /// # Safety
    ///
    /// - Called inside a frame of the game's interface update, on its
    ///   thread.
    /// - `hud` stays in place until [`Engine::hud_end`] ends it in the same
    ///   frame.
    /// - The bytes `name` points at outlive the frame.
    pub(super) unsafe fn hud_begin(&self, hud: &mut HudWindow, name: &UiText) {
        let name = [name.ptr.expose_provenance() as u64, name.len];
        // SAFETY: `self.window_begin` is the game's `ui.window_begin`, checked
        // in `Engine::resolve`, called in a frame on its thread by this
        // function's `# Safety`; `hud.object` holds the table's
        // `ui.window_extent` bytes (`Layout::read`) and `name` is a live
        // local over bytes that outlive the frame. The arguments are the
        // game's own HUD windows': full screen, layer 5, flags 0.
        unsafe { (self.window_begin)(hud.object.ptr(), &raw const name, 1, 5, u32::MAX, 0) };
        let node = hud.object.read_usize(self.layout.window_node);
        // SAFETY: `self.frame_from_node` is the game's `ui.frame_from_node`,
        // checked in `Engine::resolve`; `node` is the one the game just wrote
        // into the window object, and `hud.root` holds the table's
        // `ui.frame_extent` bytes (`Layout::read`).
        unsafe { (self.frame_from_node)(hud.root.ptr(), node) };
        // SAFETY: `self.frame_open` is the game's `ui.frame_open`, checked in
        // `Engine::resolve`; `hud.safe` holds the table's `ui.frame_extent`
        // bytes (`Layout::read`), name 0 for none.
        unsafe { (self.frame_open)(hud.safe.ptr(), 0) };
        let safe = self.frame_node(&hud.safe);
        // SAFETY: `self.node_stretch` is the game's `ui.node_stretch`, checked
        // in `Engine::resolve`, on the node the game just wrote into
        // `hud.safe`.
        unsafe { (self.node_stretch)(safe, stretch(1.0, 1.0)) };
        let margin = [HUD_MARGIN; 4];
        // SAFETY: `self.node_padding` is the game's `ui.node_padding`, checked
        // in `Engine::resolve`, on the node the game just wrote into
        // `hud.safe`, with four live floats.
        unsafe { (self.node_padding)(safe, &raw const margin) };
    }

    /// Ends the HUD window.
    ///
    /// # Safety
    ///
    /// - Called on the frame's thread in the frame whose
    ///   [`Engine::hud_begin`] started `hud`.
    /// - `hud` did not move since it started.
    /// - Every column opened inside `hud` is closed.
    pub(super) unsafe fn hud_end(&self, hud: &mut HudWindow) {
        // SAFETY: `self.frame_close` is the game's `ui.frame_close`, checked
        // in `Engine::resolve`, on the frame `hud_begin` opened in this frame;
        // every column inside it is closed by this function's `# Safety`.
        unsafe { (self.frame_close)(hud.safe.ptr()) };
        // SAFETY: `self.frame_close` is the game's `ui.frame_close`, checked
        // in `Engine::resolve`, on the root frame `hud_begin` made, whose
        // inner frame closed one call earlier.
        unsafe { (self.frame_close)(hud.root.ptr()) };
        // SAFETY: `self.window_end` is the game's `ui.window_end`, checked in
        // `Engine::resolve`, on the window object `hud_begin` started, whose
        // frames are now closed.
        unsafe { (self.window_end)(hud.object.ptr()) };
    }

    /// Draws a column of HUD items at an anchor of the HUD window: `align`
    /// is the game's alignment word, `draw` draws the items.
    ///
    /// # Safety
    ///
    /// - Called inside the HUD window, on the frame's thread.
    /// - `draw` closes every frame it opens.
    pub(super) unsafe fn hud_column(&self, align: u16, draw: impl FnOnce()) {
        let mut column = Buf::<FRAME_SIZE>::zeroed();
        // SAFETY: `self.frame_open` is the game's `ui.frame_open`, checked in
        // `Engine::resolve`, inside the HUD window this function's `# Safety`
        // gives; `column` holds the table's `ui.frame_extent` bytes
        // (`Layout::read`), name 0 for none.
        unsafe { (self.frame_open)(column.ptr(), 0) };
        let node = self.frame_node(&column);
        // SAFETY: `self.node_align` is the game's `ui.node_align`, checked in
        // `Engine::resolve`, on the node the game just wrote into `column`,
        // with a live alignment word.
        unsafe { (self.node_align)(node, &raw const align) };
        // SAFETY: `self.node_width` is the game's `ui.node_width`, checked in
        // `Engine::resolve`, on the node the game just wrote into `column`.
        unsafe { (self.node_width)(node, HUD_WIDTH) };
        let gap = [0.0f32, HUD_GAP];
        // SAFETY: `self.node_spacing` is the game's `ui.node_spacing`, checked
        // in `Engine::resolve`, on the node the game just wrote into `column`,
        // with two live floats.
        unsafe { (self.node_spacing)(node, &raw const gap, 0) };
        draw();
        // SAFETY: `self.frame_close` is the game's `ui.frame_close`, checked
        // in `Engine::resolve`, on the frame opened above; `draw` closed every
        // frame it opened, by this function's `# Safety`.
        unsafe { (self.frame_close)(column.ptr()) };
    }

    /// Draws a HUD label: body text with the HUD's outline.
    ///
    /// # Safety
    ///
    /// - Called inside a HUD column, on the frame's thread.
    /// - `context` is the non-zero context [`Engine::context`] read in this
    ///   frame.
    /// - The bytes `text` points at outlive the frame.
    pub(super) unsafe fn hud_label(&self, context: usize, text: &UiText, style: u32, align: u16) {
        let style = if style == 0 {
            self.looks.outline
        } else {
            self.looks
                .styles
                .get(style as usize)
                .copied()
                .unwrap_or(self.looks.outline)
        };
        let mut label = Buf::<LABEL_SIZE>::zeroed();
        // SAFETY: `self.label_new` is the game's `ui.label_new`, checked in
        // `Engine::resolve`; `label` holds the table's `ui.label_extent` bytes
        // (`Layout::read`), `text` outlives the frame by this function's
        // `# Safety`, and `style` is a reflected style offset inside the
        // frame's look bundle.
        unsafe { (self.label_new)(label.ptr(), text, self.bundle(context) + style, 1) };
        label.u8(self.layout.label_shown, 1);
        let node = self.frame_node(&label);
        // SAFETY: `self.node_align` is the game's `ui.node_align`, checked in
        // `Engine::resolve`, on the node the game just wrote into `label`,
        // with a live alignment word.
        unsafe { (self.node_align)(node, &raw const align) };
        label.u16(self.layout.label_align, align);
        // SAFETY: `self.node_stretch` is the game's `ui.node_stretch`, checked
        // in `Engine::resolve`, on the node the game just wrote into `label`.
        unsafe { (self.node_stretch)(node, stretch(1.0, 0.0)) };
        // SAFETY: `self.label_draw` is the game's `ui.label_draw`, checked in
        // `Engine::resolve`, on the label `label_new` filled, inside the HUD
        // column this function's `# Safety` gives.
        unsafe { (self.label_draw)(label.ptr()) };
    }

    /// Draws one more entry of the title screen's main menu, after its
    /// Settings, with `text`; true when clicked.
    ///
    /// # Safety
    ///
    /// - Called in the main menu's drawing, at the loader's hook before its
    ///   Credits entry, on the frame's thread.
    /// - `menu` is the menu state the hooked code holds there.
    /// - The bytes `text` points at outlive the frame.
    pub(super) unsafe fn main_menu_entry(&self, menu: usize, text: &UiText) -> bool {
        let mut entry = Buf::<MENU_ENTRY>::zeroed();
        entry.text(self.layout.menu_entry_text, text);
        entry.u8(self.layout.menu_entry_font, 1);
        // SAFETY: `self.main_entry` is the game's `ui.main_entry`, checked in
        // `Engine::resolve`, called at the menu's hook with its state `menu`
        // by this function's `# Safety`; `entry` holds the table's
        // `ui.menu_entry_extent` bytes (`Layout::read`), filled as the menu's
        // own Settings entry is: label, normal font.
        unsafe { (self.main_entry)(menu, entry.ptr()) != 0 }
    }

    /// Draws one more entry of the pause menu, after its Settings, with
    /// `text`; true when clicked.
    ///
    /// # Safety
    ///
    /// - Called in the pause menu's drawing, at the loader's hook after its
    ///   Settings entry, on the frame's thread.
    /// - The bytes `text` points at outlive the frame.
    pub(super) unsafe fn pause_menu_entry(&self, text: &UiText) -> bool {
        // SAFETY: `self.pause_entry` is the game's `ui.pause_entry`, checked
        // in `Engine::resolve`, called at the pause menu's hook with a `text`
        // that outlives the frame, by this function's `# Safety`; enabled, no
        // attention badge.
        unsafe { (self.pause_entry)(text, 1, 0) != 0 }
    }

    /// Whether the player pressed Back (Escape, B) this frame, taking the
    /// press so nothing else acts on it; `node` binds the action to a window.
    ///
    /// # Safety
    ///
    /// - Called inside a frame of the game's interface update, on its
    ///   thread.
    pub(super) unsafe fn back_pressed(&self, node: u64) -> bool {
        let mut spec = Buf::<ACTION_SPEC>::zeroed();
        // SAFETY: `self.action_new` is the game's `ui.action_new`, checked in
        // `Engine::resolve`, in a frame on its thread by this function's
        // `# Safety`; `spec` holds the table's `ui.action_extent` bytes
        // (`Layout::read`). Action 1 is Back, as the pause menu builds it.
        unsafe { (self.action_new)(spec.ptr(), 1, 1) };
        spec.u64(self.layout.action_node, node);
        spec.u8(self.layout.action_bound, 1);
        // SAFETY: `self.action_query` is the game's `ui.action_query`, checked
        // in `Engine::resolve`, on the spec `action_new` filled; the details
        // out may be null, 0x10 is the menus' modality.
        unsafe { (self.action_query)(spec.ptr(), core::ptr::null_mut(), 0x10) != 0 }
    }

    /// Plays interface sound `sound`.
    ///
    /// # Safety
    ///
    /// - Called inside a frame of the game's interface update, on its
    ///   thread.
    /// - `context` is the non-zero context [`Engine::context`] read in this
    ///   frame.
    pub(super) unsafe fn sound(&self, context: usize, sound: u32) {
        let Some(&offset) = self.looks.sounds.get(sound as usize) else {
            return;
        };
        // SAFETY: `self.sound` is the game's `ui.sound`, checked in
        // `Engine::resolve`, on the frame's `context` by this function's
        // `# Safety`; `offset` is a reflected sound offset inside its look
        // bundle, and the game queues a copy.
        unsafe { (self.sound)(context, self.bundle(context) + offset) };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texts_hash_as_the_game_does() {
        assert_eq!(hash(b""), FNV_BASIS);
        assert_eq!(hash(b"doBasicProgressBar()"), 0xb37b_692b_58e0_8ab5);
        assert_eq!(hash(b"contentLayer"), 0xffd8_6fcd_3b62_1d0f);
        // Non-ASCII bytes are sign-extended before the xor.
        let plain = "ж"
            .bytes()
            .fold(FNV_BASIS, |h, b| (h ^ u64::from(b)).wrapping_mul(FNV_PRIME));
        assert_ne!(hash("ж".as_bytes()), plain);
    }

    /// The layouts of every client table fit the loader's buffers, and a
    /// build whose label grew past them keeps the interface off.
    #[test]
    fn the_tables_layouts_fit_the_loader_s_buffers() {
        let tables = enshwick_gamedata::embedded()
            .iter()
            .map(|(_, text)| enshwick_gamedata::Table::parse(text).unwrap());
        for table in tables.filter(|t| t.side() == enshwick_gamedata::Side::Client) {
            let lookup = |id: &str| {
                table
                    .constants()
                    .iter()
                    .find(|c| c.id() == id)
                    .map(enshwick_gamedata::Constant::value)
            };
            let layout = Layout::read(&lookup).unwrap();
            assert_eq!(layout.nav, [0, 0x20, 0x24, 0x28, 0x30, 0x34, 0x48]);
            DialogLayout::read(&lookup).unwrap();
            ViewLayout::read(&lookup).unwrap();
            let grown = |id: &str| {
                if id == pinned::ui::LABEL_EXTENT.id() {
                    Some(0x200)
                } else {
                    lookup(id)
                }
            };
            assert!(matches!(
                Layout::read(&grown),
                Err(Missing::Room(id)) if id == pinned::ui::LABEL_EXTENT.id()
            ));
        }
    }

    #[test]
    fn whole_stops_show_whole_numbers() {
        assert_eq!(slider_format(1.0, 20.0, 20), 2);
        assert_eq!(slider_format(0.0, 1.0, 11), 4);
        assert_eq!(slider_format(0.0, 10.0, 0), 4);
    }

    #[test]
    fn buffers_write_little_endian_in_bounds() {
        let mut buf = Buf::<16>::zeroed();
        buf.u32(0, 0x0102_0304);
        buf.u64(8, 7);
        buf.u64(12, 9);
        assert_eq!(buf.read_u64(0), 0x0102_0304);
        assert_eq!(buf.read_usize(8), 7);
        assert_eq!(buf.read_u64(12), 0);
    }
}
