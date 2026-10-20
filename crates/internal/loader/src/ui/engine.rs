//! The game's own widget functions, resolved once and called with the
//! layouts they read.
//!
//! Engine facts from client build 1269579: the interface is immediate mode,
//! redrawn every frame inside `GameUi::update` on the main thread, with the
//! interface context in a global for the frame. Widgets take a config buffer
//! the caller fills; the layouts below are the ones the game's own pages and
//! its error popup fill. Every function is pinned in the build table and its
//! first code bytes checked before use.

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

/// The interface context's fields: the host, the look bundle, the current
/// navigation group.
const CONTEXT_HOST: usize = 0x10;
const CONTEXT_BUNDLE: usize = 0xc8;
const CONTEXT_NAV: usize = 0x8;
/// The host's app state pointer, whose target's byte at `APP_STATE` says the
/// screen: 1 to 3 menus, 4 and 5 slideshows, 6 loading, 7 in game.
const HOST_APP: usize = 0x48;
const APP_STATE: usize = 0xc;
/// The answer the error popup's buttons write into the host: the button
/// index, then a set flag.
pub(super) const HOST_ANSWER: usize = 0x680;
pub(super) const HOST_ANSWERED: usize = 0x681;
/// The popup state's fields.
pub(super) const POPUP_SIZE: usize = 0x560;
pub(super) const POPUP_TITLE: usize = 0x10;
pub(super) const POPUP_MESSAGE: usize = 0x14;
pub(super) const POPUP_BUTTONS: usize = 0x1c;
pub(super) const POPUP_DETAIL: usize = 0x20;
pub(super) const POPUP_DETAIL_LEN: usize = 0x28;
pub(super) const POPUP_ACTIVE: usize = 0x550;

/// A window's own objects, kept in place from its start to its end.
pub(super) struct Window {
    object: Buf<0x20>,
    backdrop: Buf<0x40>,
    panel: Buf<0x40>,
    nav: Buf<0x300>,
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

    /// The panel's node id, for actions bound to the window.
    pub(super) fn node_id(&self) -> u64 {
        let node = self.panel.read_usize(8);
        if node == 0 {
            return 0;
        }
        // SAFETY: a node the game made for the panel in the last frame; its
        // id sits at +0x30.
        unsafe { crate::world::read::<u64>(node, 0x30) }
    }
}

/// The HUD window's own objects, kept in place from its start to its end.
pub(super) struct HudWindow {
    object: Buf<0x20>,
    root: Buf<0x40>,
    safe: Buf<0x40>,
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
}

/// Every function the loader calls or hooks, by symbol; each must read its
/// `<symbol>.bytes` constant.
const FUNCTIONS: [&str; 42] = [
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
    Symbol(&'static str),
    Bytes(&'static str),
    Looks,
}

impl core::fmt::Display for Missing {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Symbol(name) => write!(f, "{name} is not in this build"),
            Self::Bytes(name) => write!(f, "{name} does not read as the build table says"),
            Self::Looks => f.write_str("the look bundle lacks a reflected field"),
        }
    }
}

impl Engine {
    /// Resolves every function and look; the first that is missing or does
    /// not read as expected keeps the interface off.
    pub(crate) fn resolve(loader: &Loader) -> Result<Self, Missing> {
        for name in FUNCTIONS {
            let address = loader.address(name).ok_or(Missing::Symbol(name))?;
            if name == "ui.context" {
                continue;
            }
            let expected = loader
                .build
                .constant(&format!("{name}.bytes"))
                .ok_or(Missing::Bytes(name))?;
            let address = usize::try_from(address).map_err(|_| Missing::Symbol(name))?;
            // SAFETY: the address lies in the game's code, mapped for the
            // process; eight bytes read from a function's entry.
            let found = unsafe { crate::world::read::<u64>(address, 0) };
            if found != expected {
                return Err(Missing::Bytes(name));
            }
        }
        let looks = looks(loader).ok_or(Missing::Looks)?;
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
        })
    }

    /// The interface context of the frame being drawn; 0 outside one.
    pub(super) fn context(&self) -> usize {
        // SAFETY: the global holds the context pointer, or 0 outside a frame.
        unsafe { crate::world::read::<usize>(self.context, 0) }
    }

    fn bundle(context: usize) -> usize {
        // SAFETY: a live context holds its look bundle.
        unsafe { crate::world::read::<usize>(context, CONTEXT_BUNDLE) }
    }

    /// The interface host of `context`.
    pub(super) fn host(context: usize) -> usize {
        // SAFETY: a live context holds its host.
        unsafe { crate::world::read::<usize>(context, CONTEXT_HOST) }
    }

    /// The app state byte: 1 to 3 menus, 4 and 5 slideshows, 6 loading, 7 in
    /// game; 0 when it cannot be read.
    pub(super) fn app_state(context: usize) -> u8 {
        let host = Self::host(context);
        if host == 0 {
            return 0;
        }
        // SAFETY: the host holds its app state pointer.
        let app = unsafe { crate::world::read::<usize>(host, HOST_APP) };
        if app == 0 {
            return 0;
        }
        // SAFETY: the pointer names the app state record.
        let state = unsafe { crate::world::read::<usize>(app, 0) };
        if state == 0 {
            return 0;
        }
        // SAFETY: the record holds its state byte.
        unsafe { crate::world::read::<u8>(state, APP_STATE) }
    }

    /// The navigation group the context points at.
    pub(super) fn nav(context: usize) -> usize {
        // SAFETY: a live context holds its navigation group pointer.
        unsafe { crate::world::read::<usize>(context, CONTEXT_NAV) }
    }

    /// Points the context back at `nav`, after a draw that left it at an
    /// object of its own stack.
    pub(super) fn restore_nav(context: usize, nav: usize) {
        let slot = core::ptr::with_exposed_provenance_mut::<usize>(context + CONTEXT_NAV);
        // SAFETY: a live context, on the thread that draws it.
        unsafe { slot.write_unaligned(nav) };
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
        window.saved_nav = Self::nav(context);
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
        let node = window.object.read_usize(0x10);
        // SAFETY: a frame over the window's node.
        unsafe { (self.frame_from_node)(window.backdrop.ptr(), node) };
        // SAFETY: the frame just opened.
        unsafe { (self.frame_stretch)(window.backdrop.ptr()) };
        let dim: u64 = 0x8000_0000 | u64::from(1.0f32.to_bits()) << 32;
        // SAFETY: the frame and a colour, half-transparent black.
        unsafe { (self.frame_color)(window.backdrop.ptr(), &raw const dim) };
        // SAFETY: an unnamed frame inside the backdrop.
        unsafe { (self.frame_open)(window.panel.ptr(), 0) };
        let panel = window.panel.read_usize(8);
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
        let mut look = Buf::<0x100>::zeroed();
        look.u32(0, 9);
        look.f32(4, 1.0);
        // SAFETY: the panel frame and its background, kind 9 as the popup's.
        unsafe { (self.panel)(window.panel.ptr(), look.ptr()) };
        if title.len > 0 {
            // SAFETY: forwarded from the caller.
            unsafe { self.title(context, title) };
        }
        let mut nav = Buf::<0x58>::zeroed();
        nav.u32(0, 2);
        nav.u8(0x20, 1);
        nav.u32(0x24, 3);
        nav.u16(0x28, 0x101);
        nav.u32(0x30, 2);
        nav.u16(0x34, 0x1b);
        nav.u16(0x48, 1);
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
        Self::restore_nav(context, window.saved_nav);
        // SAFETY: the frames and window the start opened, innermost first.
        unsafe { (self.frame_close)(window.panel.ptr()) };
        // SAFETY: as above.
        unsafe { (self.frame_close)(window.backdrop.ptr()) };
        // SAFETY: as above.
        unsafe { (self.window_end)(window.object.ptr()) };
    }

    unsafe fn title(&self, context: usize, text: &UiText) {
        let bundle = Self::bundle(context);
        let mut label = Buf::<LABEL_SIZE>::zeroed();
        // SAFETY: a label over `text` in the popup's title style.
        unsafe { (self.label_new)(label.ptr(), text, bundle + self.looks.title, 0) };
        label.u8(LABEL_SHOWN, 1);
        let node = label.read_usize(8);
        let centre: u16 = 0x202;
        // SAFETY: the label's node and an alignment word.
        unsafe { (self.node_align)(node, &raw const centre) };
        label.u16(LABEL_ALIGN, centre);
        // SAFETY: the bundle holds the font size, a float.
        let size = unsafe { crate::world::read::<f32>(bundle, self.looks.title_size) };
        label.f32(LABEL_FONT_SIZE, size);
        label.u16(LABEL_FLAGS, label.read_u16(LABEL_FLAGS) | 8);
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
        let bundle = Self::bundle(context);
        let style = self
            .looks
            .styles
            .get(style as usize)
            .copied()
            .unwrap_or(self.looks.styles[0]);
        let mut label = Buf::<LABEL_SIZE>::zeroed();
        // SAFETY: a label over `text` in a style of the bundle.
        unsafe { (self.label_new)(label.ptr(), text, bundle + style, u8::from(wrap)) };
        label.u8(LABEL_SHOWN, 1);
        let node = label.read_usize(8);
        let align: u16 = if center { 0x202 } else { 0x201 };
        // SAFETY: the label's node and an alignment word.
        unsafe { (self.node_align)(node, &raw const align) };
        label.u16(LABEL_ALIGN, align);
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
        let mut config = Buf::<0x80>::zeroed();
        config.u32(4, 0x0001_0202);
        config.u8(9, u8::from(enabled));
        config.u64(0x10, 3);
        config.f32(0x4c, 10.0);
        config.f32(0x50, 5.0);
        config.f32(0x54, 1.0);
        config.u8(0x58, 1);
        // SAFETY: the button config the popup fills, and the label.
        let clicked = unsafe { (self.button)(config.ptr(), text) } != 0;
        clicked && enabled
    }

    fn setting_texts(&self, context: usize) -> [UiText; 2] {
        let bundle = Self::bundle(context);
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
        let mut config = Buf::<0xc50>::zeroed();
        let texts = self.setting_texts(context);
        for (value, (at, text)) in (0u32..).zip((0..).step_by(ENTRY_SIZE).zip(&texts)) {
            config.u32(at, value);
            config.text(at + 8, text);
        }
        config.u8(0xc00, 2);
        config.text(0xc08, label);
        config.text(0xc20, &UiText::EMPTY);
        config.u32(0xc38, 0x0001_0000);
        config.u32(0xc3c, u32::from(!read_only));
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
        let mut config = Buf::<0x120>::zeroed();
        // SAFETY: the slider's constructor fills its defaults.
        unsafe { (self.slider_init)(config.ptr(), label) };
        config.f32(0x30, min);
        config.f32(0x34, max);
        config.u8(0x38, 1);
        config.u8(0x39, u8::from(read_only));
        config.u8(0x3a, u8::from(on_release));
        config.u32(0x4c, slider_format(min, max, steps));
        if (2..=MAX_STOPS).contains(&steps) {
            for (i, slot) in (0..steps).zip((0xa0..).step_by(4)) {
                #[expect(clippy::cast_precision_loss, reason = "at most 32 stops")]
                let at = min + (max - min) * i as f32 / (steps - 1) as f32;
                config.f32(slot, at);
            }
            config.u64(0x80, u64::from(steps));
            config.u32(0x70, 1);
        } else if steps > MAX_STOPS {
            config.u32(0x40, steps - 1);
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
        let mut config = Buf::<0xca0>::zeroed();
        // SAFETY: the dropdown's constructor fills its defaults.
        unsafe { (self.dropdown_init)(config.ptr(), label) };
        let count = options.len().min(MAX_ENTRIES);
        for (value, (at, text)) in (0u32..).zip((0..).step_by(ENTRY_SIZE).zip(&options[..count])) {
            config.u32(at, value);
            config.text(at + 8, text);
        }
        config.u8(0xc00, u8::try_from(count).unwrap_or(0));
        config.u8(0xc74, u8::from(!read_only));
        let mut value = i32::try_from(index).unwrap_or(0);
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
        let mut frame = Buf::<0x40>::zeroed();
        // SAFETY: an unnamed frame to size the bar.
        unsafe { (self.frame_open)(frame.ptr(), 0) };
        let node = frame.read_usize(8);
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
        let node = hud.object.read_usize(0x10);
        // SAFETY: a frame over the window's node.
        unsafe { (self.frame_from_node)(hud.root.ptr(), node) };
        // SAFETY: an unnamed frame inside it.
        unsafe { (self.frame_open)(hud.safe.ptr(), 0) };
        let safe = hud.safe.read_usize(8);
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
        let mut column = Buf::<0x40>::zeroed();
        // SAFETY: an unnamed frame inside the HUD window.
        unsafe { (self.frame_open)(column.ptr(), 0) };
        let node = column.read_usize(8);
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
        unsafe { (self.label_new)(label.ptr(), text, Self::bundle(context) + style, 1) };
        label.u8(LABEL_SHOWN, 1);
        let node = label.read_usize(8);
        // SAFETY: the label's node and its alignment in the column.
        unsafe { (self.node_align)(node, &raw const align) };
        label.u16(LABEL_ALIGN, align);
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
        let mut entry = Buf::<0x20>::zeroed();
        entry.text(0, text);
        entry.u32(0x18, 1);
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
        let mut spec = Buf::<0x50>::zeroed();
        // SAFETY: the action spec for Back (1), as the pause menu builds it.
        unsafe { (self.action_new)(spec.ptr(), 1, 1) };
        spec.u64(0x40, node);
        spec.u8(0x1b, 1);
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
        unsafe { (self.sound)(context, Self::bundle(context) + offset) };
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
const LABEL_SIZE: usize = 0x150;
const LABEL_ALIGN: usize = 0x38;
const LABEL_SHOWN: usize = 0x3d;
const LABEL_FONT_SIZE: usize = 0x7c;
const LABEL_FLAGS: usize = 0x90;
const ENTRY_SIZE: usize = 0x30;
const MAX_ENTRIES: usize = 64;
const MAX_STOPS: u32 = 32;

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
