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
    "ui.nav_kind",
    "ui.nav_wrap",
    "ui.nav_axis",
    "ui.nav_flags",
    "ui.nav_input_kind",
    "ui.nav_layer",
    "ui.nav_sound",
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
            ("ui.window_extent", WINDOW_OBJECT),
            ("ui.frame_extent", FRAME_SIZE),
            ("ui.nav_extent", NAV_SIZE),
            ("ui.nav_config_size", NAV_CONFIG),
            ("ui.panel_extent", PANEL_CONFIG),
            ("ui.label_extent", LABEL_SIZE),
            ("ui.button_extent", BUTTON_CONFIG),
            ("ui.toggle_extent", TOGGLE_CONFIG),
            ("ui.dropdown_extent", DROPDOWN_CONFIG),
            ("ui.menu_entry_extent", MENU_ENTRY),
            ("ui.action_extent", ACTION_SPEC),
        ] {
            fits(lookup, id, room)?;
        }
        let config = f("ui.nav_config")?;
        let mut nav = [0; 7];
        for (slot, id) in nav.iter_mut().zip(NAV_IDS) {
            *slot = f(id)?.checked_sub(config).ok_or(Missing::Room(id))?;
        }
        Self {
            context_nav: f("ui.context_nav")?,
            context_host: f("ui.context_host")?,
            context_bundle: f("ui.context_bundle")?,
            context_state: f("ui.context_state")?,
            state_screen: f("ui.state_screen")?,
            popup_active: f("ui.popup_active")?,
            frame_node: f("ui.frame_node")?,
            window_node: f("ui.window_node")?,
            node_id: f("ui.node_id")?,
            panel_kind: f("ui.panel_kind")?,
            panel_alpha: f("ui.panel_alpha")?,
            nav,
            label_align: f("ui.label_align")?,
            label_shown: f("ui.label_shown")?,
            label_font_size: f("ui.label_font_size")?,
            label_flags: f("ui.label_flags")?,
            button_align: f("ui.button_align")?,
            button_centered: f("ui.button_centered")?,
            button_enabled: f("ui.button_enabled")?,
            button_size_mode: f("ui.button_size_mode")?,
            button_padding: [f("ui.button_padding_x")?, f("ui.button_padding_y")?],
            button_scale: f("ui.button_scale")?,
            button_sound: f("ui.button_sound")?,
            entry_size: f("ui.entry_size")?,
            entry_value: f("ui.entry_value")?,
            entry_text: f("ui.entry_text")?,
            toggle_count: f("ui.toggle_count")?,
            toggle_label: f("ui.toggle_label")?,
            toggle_explainer: f("ui.toggle_explainer")?,
            toggle_explain: f("ui.toggle_explain")?,
            toggle_enabled: f("ui.toggle_enabled")?,
            slider_min: f("ui.slider_min")?,
            slider_max: f("ui.slider_max")?,
            slider_bounded: f("ui.slider_bounded")?,
            slider_read_only: f("ui.slider_read_only")?,
            slider_on_release: f("ui.slider_on_release")?,
            slider_steps: f("ui.slider_steps")?,
            slider_format: f("ui.slider_format")?,
            slider_discrete: f("ui.slider_discrete")?,
            slider_stop_count: f("ui.slider_stop_count")?,
            slider_stops: f("ui.slider_stops")?,
            slider_stop_capacity: f("ui.slider_stop_capacity")?,
            dropdown_count: f("ui.dropdown_count")?,
            dropdown_enabled: f("ui.dropdown_enabled")?,
            dropdown_capacity: f("ui.dropdown_capacity")?,
            dropdown_entry_text: f("ui.dropdown_entry_text")?,
            menu_entry_text: f("ui.menu_entry_text")?,
            menu_entry_font: f("ui.menu_entry_font")?,
            action_node: f("ui.action_node")?,
            action_bound: f("ui.action_bound")?,
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
            return Err(Missing::Room("ui.slider_stop_capacity"));
        }
        let entries = |count: usize| count.checked_mul(self.entry_size);
        if entries(TOGGLE_ENTRIES).is_none_or(|end| end > self.toggle_count) {
            return Err(Missing::Room("ui.toggle_count"));
        }
        if entries(self.dropdown_capacity).is_none_or(|end| end > self.dropdown_count) {
            return Err(Missing::Room("ui.dropdown_capacity"));
        }
        Ok(self)
    }
}

impl DialogLayout {
    fn read(lookup: Lookup<'_>) -> Result<Self, Missing> {
        let f = |id| field(lookup, id);
        fits(lookup, "ui.popup_extent", POPUP_SIZE)?;
        Ok(Self {
            title: f("ui.popup_title")?,
            message: f("ui.popup_message")?,
            buttons: f("ui.popup_buttons")?,
            detail: f("ui.popup_detail")?,
            detail_len: f("ui.popup_detail_len")?,
            answer: f("ui.host_answer")?,
            answered: f("ui.host_answered")?,
        })
    }
}

impl ViewLayout {
    fn read(lookup: Lookup<'_>) -> Result<Self, Missing> {
        Ok(Self {
            read_only: field(lookup, "ui.view_read_only")?,
            tab: field(lookup, "ui.view_tab")?,
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
const FUNCTIONS: [&str; 43] = [
    "ui.world_settings_site",
    "ui.main_entry",
    "ui.pause_entry",
    "ui.action_new",
    "ui.action_query",
    "ui.main_menu_site",
    "ui.pause_menu_site",
    "ui.hud",
    "ui.hud_visible",
    "ui.popup",
    "ui.window_begin",
    "ui.window_end",
    "ui.frame_from_node",
    "ui.frame_open",
    "ui.frame_close",
    "ui.frame_stretch",
    "ui.frame_color",
    "ui.node_padding",
    "ui.node_spacing",
    "ui.node_align",
    "ui.node_width",
    "ui.node_height",
    "ui.node_stretch",
    "ui.panel",
    "ui.nav_init",
    "ui.nav_input",
    "ui.nav_set",
    "ui.nav_push",
    "ui.nav_end",
    "ui.nav_drop",
    "ui.label_new",
    "ui.label_draw",
    "ui.button",
    "ui.toggle",
    "ui.slider_init",
    "ui.slider",
    "ui.dropdown_init",
    "ui.dropdown",
    "ui.progress",
    "ui.sound",
    "ui.loca_id",
    "ui.loca_text",
    "ui.context",
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
            let address = loader.address(name).ok_or(Missing::Entry(name))?;
            if name == "ui.context" {
                continue;
            }
            let expected = loader
                .build
                .constant(&format!("{name}.bytes"))
                .ok_or(Missing::Bytes(name))?;
            let address = usize::try_from(address).map_err(|_| Missing::Entry(name))?;
            // SAFETY: the address lies in the game's code, mapped for the
            // process; eight bytes read from a function's entry.
            let found = unsafe { crate::world::read::<u64>(address, 0) };
            if found != expected {
                return Err(Missing::Bytes(name));
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
        let at = |name: &str| {
            loader
                .address(name)
                .and_then(|a| usize::try_from(a).ok())
                .unwrap_or(0)
        };
        macro_rules! function {
            ($name:literal, $ty:ty) => {{
                let address = at($name);
                // SAFETY: the symbol is the game function of the field's
                // type on this build, checked by its first bytes above.
                unsafe { core::mem::transmute::<usize, $ty>(address) }
            }};
        }
        Ok(Self {
            context: at("ui.context"),
            popup: loader.address("ui.popup").unwrap_or(0),
            looks,
            layout,
            dialog,
            view,
            window_begin: function!("ui.window_begin", WindowBeginFn),
            window_end: function!("ui.window_end", OneFn),
            frame_from_node: function!("ui.frame_from_node", FrameFromNodeFn),
            frame_open: function!("ui.frame_open", FrameOpenFn),
            frame_close: function!("ui.frame_close", OneFn),
            frame_stretch: function!("ui.frame_stretch", OneFn),
            frame_color: function!("ui.frame_color", ColorFn),
            node_padding: function!("ui.node_padding", NodeRectFn),
            node_spacing: function!("ui.node_spacing", NodeSpacingFn),
            node_align: function!("ui.node_align", NodeAlignFn),
            node_width: function!("ui.node_width", NodeFloatFn),
            node_height: function!("ui.node_height", NodeFloatFn),
            node_stretch: function!("ui.node_stretch", NodeStretchFn),
            panel: function!("ui.panel", PanelFn),
            nav_init: function!("ui.nav_init", NavInitFn),
            nav_input: function!("ui.nav_input", NavInputFn),
            nav_set: function!("ui.nav_set", NavSetFn),
            nav_push: function!("ui.nav_push", NavPushFn),
            nav_end: function!("ui.nav_end", NavEndFn),
            nav_drop: function!("ui.nav_drop", OneFn),
            label_new: function!("ui.label_new", LabelNewFn),
            label_draw: function!("ui.label_draw", OneFn),
            button: function!("ui.button", ButtonFn),
            toggle: function!("ui.toggle", ToggleFn),
            slider_init: function!("ui.slider_init", RowInitFn),
            slider: function!("ui.slider", SliderFn),
            dropdown_init: function!("ui.dropdown_init", RowInitFn),
            dropdown: function!("ui.dropdown", DropdownFn),
            progress: function!("ui.progress", ProgressFn),
            sound: function!("ui.sound", SoundFn),
            loca_id: function!("ui.loca_id", LocaIdFn),
            loca_text: function!("ui.loca_text", LocaTextFn),
            hud_visible: function!("ui.hud_visible", VisibleFn),
            hud: loader.address("ui.hud").unwrap_or(0),
            main_entry: function!("ui.main_entry", MainEntryFn),
            pause_entry: function!("ui.pause_entry", PauseEntryFn),
            action_new: function!("ui.action_new", ActionNewFn),
            action_query: function!("ui.action_query", ActionQueryFn),
            main_menu_site: loader.address("ui.main_menu_site").unwrap_or(0),
            pause_menu_site: loader.address("ui.pause_menu_site").unwrap_or(0),
            world_settings_site: loader.address("ui.world_settings_site").unwrap_or(0),
        })
    }

    /// The interface context of the frame being drawn; 0 outside one.
    pub(super) fn context(&self) -> usize {
        // SAFETY: the global holds the context pointer, or 0 outside a frame.
        unsafe { crate::world::read::<usize>(self.context, 0) }
    }

    fn bundle(&self, context: usize) -> usize {
        // SAFETY: a live context holds its look bundle.
        unsafe { crate::world::read::<usize>(context, self.layout.context_bundle) }
    }

    /// The interface host of `context`.
    pub(super) fn host(&self, context: usize) -> usize {
        // SAFETY: a live context holds its host.
        unsafe { crate::world::read::<usize>(context, self.layout.context_host) }
    }

    /// The screen byte of the context's play state: 1 to 3 menus, 4 and 5
    /// slideshows, 6 loading, 7 in game; 0 when it cannot be read.
    pub(super) fn app_state(&self, context: usize) -> u8 {
        // SAFETY: a live context holds its play state pointer.
        let state = unsafe { crate::world::read::<usize>(context, self.layout.context_state) };
        if state == 0 {
            return 0;
        }
        // SAFETY: the pointer names the play state's record.
        let record = unsafe { crate::world::read::<usize>(state, 0) };
        if record == 0 {
            return 0;
        }
        // SAFETY: the record holds its screen byte.
        unsafe { crate::world::read::<u8>(record, self.layout.state_screen) }
    }

    /// The navigation group the context points at.
    pub(super) fn nav(&self, context: usize) -> usize {
        // SAFETY: a live context holds its navigation group pointer.
        unsafe { crate::world::read::<usize>(context, self.layout.context_nav) }
    }

    /// Points the context back at `nav`, after a draw that left it at an
    /// object of its own stack.
    pub(super) fn restore_nav(&self, context: usize, nav: usize) {
        let slot =
            core::ptr::with_exposed_provenance_mut::<usize>(context + self.layout.context_nav);
        // SAFETY: a live context, on the thread that draws it.
        unsafe { slot.write_unaligned(nav) };
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
        // SAFETY: a node the game made for the panel in the last frame.
        unsafe { crate::world::read::<u64>(node, self.layout.node_id) }
    }

    /// Starts a window: the game's popup panel over a dimmed screen, `width`
    /// wide, with a title when `title` is not empty.
    ///
    /// # Safety
    ///
    /// Inside a frame, on its thread, with `window` kept in place until
    /// [`Engine::window_end`]; `name` and `title` outlive the frame.
    pub(super) unsafe fn window_begin(
        &self,
        context: usize,
        window: &mut Window,
        name: &UiText,
        title: &UiText,
    ) {
        window.saved_nav = self.nav(context);
        let name = [name.ptr.expose_provenance() as u64, name.len];
        // SAFETY: the window object and its name, as the popup passes them:
        // full screen, its layer, all input, flags 1.
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
        // SAFETY: a frame over the window's node.
        unsafe { (self.frame_from_node)(window.backdrop.ptr(), node) };
        // SAFETY: the frame just opened.
        unsafe { (self.frame_stretch)(window.backdrop.ptr()) };
        let dim: u64 = 0x8000_0000 | u64::from(1.0f32.to_bits()) << 32;
        // SAFETY: the frame and a colour, half-transparent black.
        unsafe { (self.frame_color)(window.backdrop.ptr(), &raw const dim) };
        // SAFETY: an unnamed frame inside the backdrop.
        unsafe { (self.frame_open)(window.panel.ptr(), 0) };
        let panel = self.frame_node(&window.panel);
        let padding = [90.0f32, 60.0, 90.0, 60.0];
        // SAFETY: the panel's node and four edges.
        unsafe { (self.node_padding)(panel, &raw const padding) };
        let gap = [0.0f32, window.gap];
        // SAFETY: the panel's node and its gap between children.
        unsafe { (self.node_spacing)(panel, &raw const gap, 0) };
        let centre: u16 = 0x202;
        // SAFETY: the panel's node and an alignment word.
        unsafe { (self.node_align)(panel, &raw const centre) };
        // SAFETY: the panel's node and its width.
        unsafe { (self.node_width)(panel, WINDOW_WIDTH) };
        // SAFETY: the panel's node and no stretch.
        unsafe { (self.node_stretch)(panel, 0) };
        let mut look = Buf::<PANEL_CONFIG>::zeroed();
        look.u32(self.layout.panel_kind, 9);
        look.f32(self.layout.panel_alpha, 1.0);
        // SAFETY: the panel frame and its background, kind 9 as the popup's.
        unsafe { (self.panel)(window.panel.ptr(), look.ptr()) };
        if title.len > 0 {
            // SAFETY: forwarded from the caller.
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
        // SAFETY: the navigation group and its config, as the popup's.
        unsafe { (self.nav_init)(window.nav.ptr(), nav.ptr()) };
        let mut input = [0u64; 2];
        // SAFETY: the config, the player's navigation input written to `input`.
        unsafe { (self.nav_input)(&raw mut input, nav.ptr(), 1) };
        // SAFETY: the group and the input.
        unsafe { (self.nav_set)(window.nav.ptr(), &raw const input) };
        // SAFETY: the context and the group, current until the window ends.
        unsafe { (self.nav_push)(context, window.nav.ptr()) };
    }

    /// Ends a window started with [`Engine::window_begin`].
    ///
    /// # Safety
    ///
    /// As for [`Engine::window_begin`], in the same frame.
    pub(super) unsafe fn window_end(&self, context: usize, window: &mut Window) {
        // SAFETY: the group the window started.
        unsafe { (self.nav_end)(window.nav.ptr(), 0) };
        // SAFETY: as above.
        unsafe { (self.nav_drop)(window.nav.ptr()) };
        self.restore_nav(context, window.saved_nav);
        // SAFETY: the frames and window the start opened, innermost first.
        unsafe { (self.frame_close)(window.panel.ptr()) };
        // SAFETY: as above.
        unsafe { (self.frame_close)(window.backdrop.ptr()) };
        // SAFETY: as above.
        unsafe { (self.window_end)(window.object.ptr()) };
    }

    unsafe fn title(&self, context: usize, text: &UiText) {
        let layout = &self.layout;
        let bundle = self.bundle(context);
        let mut label = Buf::<LABEL_SIZE>::zeroed();
        // SAFETY: a label over `text` in the popup's title style.
        unsafe { (self.label_new)(label.ptr(), text, bundle + self.looks.title, 0) };
        label.u8(layout.label_shown, 1);
        let node = self.frame_node(&label);
        let centre: u16 = 0x202;
        // SAFETY: the label's node and an alignment word.
        unsafe { (self.node_align)(node, &raw const centre) };
        label.u16(layout.label_align, centre);
        // SAFETY: the bundle holds the font size, a float.
        let size = unsafe { crate::world::read::<f32>(bundle, self.looks.title_size) };
        label.f32(layout.label_font_size, size);
        label.u16(layout.label_flags, label.read_u16(layout.label_flags) | 8);
        // SAFETY: the label just made.
        unsafe { (self.label_draw)(label.ptr()) };
    }

    /// Draws a label in style `style`, wrapped and centred as asked.
    ///
    /// # Safety
    ///
    /// Inside a window, on the frame's thread; `text` outlives the frame.
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
        // SAFETY: a label over `text` in a style of the bundle.
        unsafe { (self.label_new)(label.ptr(), text, bundle + style, u8::from(wrap)) };
        label.u8(self.layout.label_shown, 1);
        let node = self.frame_node(&label);
        let align: u16 = if center { 0x202 } else { 0x201 };
        // SAFETY: the label's node and an alignment word.
        unsafe { (self.node_align)(node, &raw const align) };
        label.u16(self.layout.label_align, align);
        if wrap {
            // SAFETY: the label's node, stretched to the window's width.
            unsafe { (self.node_stretch)(node, stretch(1.0, 0.0)) };
        }
        // SAFETY: the label just made.
        unsafe { (self.label_draw)(label.ptr()) };
    }

    /// Draws a button; true when clicked this frame.
    ///
    /// # Safety
    ///
    /// Inside a window, on the frame's thread; `text` outlives the frame.
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
        // SAFETY: the button config the popup fills, and the label.
        let clicked = unsafe { (self.button)(config.ptr(), text) } != 0;
        clicked && enabled
    }

    fn setting_texts(&self, context: usize) -> [UiText; TOGGLE_ENTRIES] {
        let bundle = self.bundle(context);
        [self.looks.setting_off, self.looks.setting_on].map(|offset| {
            // SAFETY: a loca reference of the bundle.
            let id = unsafe { (self.loca_id)(bundle + offset) };
            let mut text = UiText::EMPTY;
            // SAFETY: the text of a loca id, written to `text`; it points at
            // the game's loca storage.
            unsafe { (self.loca_text)(&raw mut text, id) };
            text
        })
    }

    /// Draws a toggle row; the new state when the player changed it.
    ///
    /// # Safety
    ///
    /// Inside a window, on the frame's thread; `label` outlives the frame.
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
        // SAFETY: the selector config the game's settings pages fill, with
        // two entries.
        let changed = unsafe { (self.toggle)(config.ptr(), &raw mut value) } != 0;
        (changed && !read_only).then_some(value != 0)
    }

    /// Draws a slider row; the new value when the player changed it.
    ///
    /// # Safety
    ///
    /// Inside a window, on the frame's thread; `label` outlives the frame.
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
        // SAFETY: the slider's constructor fills its defaults.
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
        // SAFETY: the config the constructor made, the value, no gamepad
        // state.
        let changed =
            unsafe { (self.slider)(config.ptr(), &raw mut current, core::ptr::null_mut()) } != 0;
        (changed && !read_only).then_some(current)
    }

    /// Draws a dropdown row; the new index when the player picked another.
    ///
    /// # Safety
    ///
    /// Inside a window, on the frame's thread; the texts outlive the frame.
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
        // SAFETY: the dropdown's constructor fills its defaults.
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
        // SAFETY: the config the constructor made with its entries, the value.
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
    /// Inside a window, on the frame's thread.
    pub(super) unsafe fn progress(&self, value: f32, min: f32, max: f32) {
        let mut frame = Buf::<FRAME_SIZE>::zeroed();
        // SAFETY: an unnamed frame to size the bar.
        unsafe { (self.frame_open)(frame.ptr(), 0) };
        let node = self.frame_node(&frame);
        // SAFETY: the frame's node and its height.
        unsafe { (self.node_height)(node, BAR_HEIGHT) };
        // SAFETY: the frame's node, stretched to the window's width.
        unsafe { (self.node_stretch)(node, stretch(1.0, 0.0)) };
        let white = 0xffff_ffff | u64::from(1.0f32.to_bits()) << 32;
        let shade = 0x8000_0000 | u64::from(1.0f32.to_bits()) << 32;
        // SAFETY: value, range and the game's own fill and background.
        unsafe { (self.progress)(value - min, max - min, white, shade) };
        // SAFETY: the frame opened above.
        unsafe { (self.frame_close)(frame.ptr()) };
    }

    /// Whether the game shows HUD elements now: not in a full-screen menu,
    /// in screenshot mode or in a dialog with a character, as the game's
    /// status bar asks.
    ///
    /// # Safety
    ///
    /// Inside a frame, on its thread.
    pub(super) unsafe fn hud_visible(&self) -> bool {
        // SAFETY: forwarded from the caller; the game reads the interface
        // context itself.
        unsafe { (self.hud_visible)(HUD_MASK) != 0 }
    }

    /// Starts the HUD window, a layer over the game's HUD that takes no
    /// input, with a frame over the screen inside its margins.
    ///
    /// # Safety
    ///
    /// Inside a frame, on its thread, with `hud` kept in place until
    /// [`Engine::hud_end`]; `name` outlives the frame.
    pub(super) unsafe fn hud_begin(&self, hud: &mut HudWindow, name: &UiText) {
        let name = [name.ptr.expose_provenance() as u64, name.len];
        // SAFETY: the window object and its name, as the game's own HUD
        // windows pass them: full screen, the HUD's layer 5, flags 0.
        unsafe { (self.window_begin)(hud.object.ptr(), &raw const name, 1, 5, u32::MAX, 0) };
        let node = hud.object.read_usize(self.layout.window_node);
        // SAFETY: a frame over the window's node.
        unsafe { (self.frame_from_node)(hud.root.ptr(), node) };
        // SAFETY: an unnamed frame inside it.
        unsafe { (self.frame_open)(hud.safe.ptr(), 0) };
        let safe = self.frame_node(&hud.safe);
        // SAFETY: the frame's node, filling the screen.
        unsafe { (self.node_stretch)(safe, stretch(1.0, 1.0)) };
        let margin = [HUD_MARGIN; 4];
        // SAFETY: the frame's node and its four margins.
        unsafe { (self.node_padding)(safe, &raw const margin) };
    }

    /// Ends the HUD window.
    ///
    /// # Safety
    ///
    /// As for [`Engine::hud_begin`], in the same frame.
    pub(super) unsafe fn hud_end(&self, hud: &mut HudWindow) {
        // SAFETY: the frames and window the start opened, innermost first.
        unsafe { (self.frame_close)(hud.safe.ptr()) };
        // SAFETY: as above.
        unsafe { (self.frame_close)(hud.root.ptr()) };
        // SAFETY: as above.
        unsafe { (self.window_end)(hud.object.ptr()) };
    }

    /// Draws a column of HUD items at an anchor of the HUD window: `align`
    /// is the game's alignment word, `draw` draws the items.
    ///
    /// # Safety
    ///
    /// Inside the HUD window, on the frame's thread.
    pub(super) unsafe fn hud_column(&self, align: u16, draw: impl FnOnce()) {
        let mut column = Buf::<FRAME_SIZE>::zeroed();
        // SAFETY: an unnamed frame inside the HUD window.
        unsafe { (self.frame_open)(column.ptr(), 0) };
        let node = self.frame_node(&column);
        // SAFETY: the column's node and its alignment in the window.
        unsafe { (self.node_align)(node, &raw const align) };
        // SAFETY: the column's node and its width.
        unsafe { (self.node_width)(node, HUD_WIDTH) };
        let gap = [0.0f32, HUD_GAP];
        // SAFETY: the column's node, stacking its items.
        unsafe { (self.node_spacing)(node, &raw const gap, 0) };
        draw();
        // SAFETY: the column opened above.
        unsafe { (self.frame_close)(column.ptr()) };
    }

    /// Draws a HUD label: body text with the HUD's outline.
    ///
    /// # Safety
    ///
    /// Inside a HUD column, on the frame's thread; `text` outlives the
    /// frame.
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
        // SAFETY: a label over `text` in a style of the bundle, wrapped at
        // the column's width.
        unsafe { (self.label_new)(label.ptr(), text, self.bundle(context) + style, 1) };
        label.u8(self.layout.label_shown, 1);
        let node = self.frame_node(&label);
        // SAFETY: the label's node and its alignment in the column.
        unsafe { (self.node_align)(node, &raw const align) };
        label.u16(self.layout.label_align, align);
        // SAFETY: the label's node, stretched to the column's width.
        unsafe { (self.node_stretch)(node, stretch(1.0, 0.0)) };
        // SAFETY: the label just made.
        unsafe { (self.label_draw)(label.ptr()) };
    }

    /// Draws one more entry of the title screen's main menu, after its
    /// Settings, with `text`; true when clicked.
    ///
    /// # Safety
    ///
    /// In the main menu's drawing, at the loader's hook before its Credits
    /// entry; `menu` is the menu state there; `text` outlives the frame.
    pub(super) unsafe fn main_menu_entry(&self, menu: usize, text: &UiText) -> bool {
        let mut entry = Buf::<MENU_ENTRY>::zeroed();
        entry.text(self.layout.menu_entry_text, text);
        entry.u8(self.layout.menu_entry_font, 1);
        // SAFETY: forwarded from the caller; the entry as the menu's own
        // Settings entry is built: label, normal font.
        unsafe { (self.main_entry)(menu, entry.ptr()) != 0 }
    }

    /// Draws one more entry of the pause menu, after its Settings, with
    /// `text`; true when clicked.
    ///
    /// # Safety
    ///
    /// In the pause menu's drawing, at the loader's hook after its Settings
    /// entry; `text` outlives the frame.
    pub(super) unsafe fn pause_menu_entry(&self, text: &UiText) -> bool {
        // SAFETY: forwarded from the caller; enabled, no attention badge.
        unsafe { (self.pause_entry)(text, 1, 0) != 0 }
    }

    /// Whether the player pressed Back (Escape, B) this frame, taking the
    /// press so nothing else acts on it; `node` binds the action to a window.
    ///
    /// # Safety
    ///
    /// Inside a frame, on its thread.
    pub(super) unsafe fn back_pressed(&self, node: u64) -> bool {
        let mut spec = Buf::<ACTION_SPEC>::zeroed();
        // SAFETY: the action spec for Back (1), as the pause menu builds it.
        unsafe { (self.action_new)(spec.ptr(), 1, 1) };
        spec.u64(self.layout.action_node, node);
        spec.u8(self.layout.action_bound, 1);
        // SAFETY: the spec just built, no details out, the menus' modality.
        unsafe { (self.action_query)(spec.ptr(), core::ptr::null_mut(), 0x10) != 0 }
    }

    /// Plays interface sound `sound`.
    ///
    /// # Safety
    ///
    /// Inside a frame, on its thread.
    pub(super) unsafe fn sound(&self, context: usize, sound: u32) {
        let Some(&offset) = self.looks.sounds.get(sound as usize) else {
            return;
        };
        // SAFETY: the context and a sound of its bundle; the game queues a
        // copy.
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
            let grown = |id: &str| match id {
                "ui.label_extent" => Some(0x200),
                _ => lookup(id),
            };
            assert!(matches!(
                Layout::read(&grown),
                Err(Missing::Room("ui.label_extent"))
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
