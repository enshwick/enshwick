use core::num::NonZeroU64;

#[cfg(test)]
use enshwick_core::abi::UiExtension;

use crate::ui::place::Place;
use crate::ui::window_node::{NAME_FITS, SHORT_LEN, short_name};

use super::{
    Buf, EXTENSION_WIDTH, Engine, FRAME_SIZE, HUD_MARGIN, NAV_CONFIG, PANEL_CONFIG, UiText,
    WINDOW_WIDTH, Window, text_field,
};

/// The window flag that holds every action and click from the windows on
/// lower layers taking the same input: the gates of `ui.action_query`
/// (`0x1411a6ba0`) and of a click (`0x1411a6da0`) refuse a node while a
/// higher window with this bit is open (client 26430625, decompiler). The
/// game's popups and the in-game menu set it; its overlays (button hints,
/// tooltips, the dragged item) do not, and hold only the clicks on their own
/// widgets.
const MODAL: u32 = 1;

/// The layer the game's popup, and so the loader's dialog, begins its window
/// on, modal, and the loader's windows' own: `ui.popup` calls
/// `ui.window_begin` with layer 0x1f and flags 1. The window begin stores
/// the layer at the window's `+0x7c` (`0x1411bf610`), and the gates refuse
/// a node only for a modal window on a strictly higher layer, so two modal
/// windows on one layer hold neither's input (client 26430625, decompiler).
pub(super) const POPUP_LAYER: u32 = 0x1f;

impl Window {
    /// The flags the window starts with: a panel beside a game screen is an
    /// overlay, so that screen keeps its keys, tabs and hover.
    const fn flags(&self) -> u32 {
        if self.place().is_some() { 0 } else { MODAL }
    }

    /// Begins the window in interface update `now`, holding a panel's focus
    /// back when it opens after a gap. While the config's focus byte is set,
    /// `ui.nav_end` focuses a new group's first member and gives a focus
    /// nobody holds to the remembered one (client 26430625, decompiler), so
    /// the hold lasts until [`Window::nav_moved`].
    const fn begin_at(&mut self, now: u64) {
        if !self.drawn_lately(now) {
            self.hold_focus();
        }
        self.began = Some(now);
    }

    /// Notes `ui.nav_end`'s `moved` byte: a move between the members ends
    /// the held focus.
    const fn nav_moved(&mut self, moved: u8) {
        if moved != 0 {
            self.held = false;
        }
    }

    /// The layer the window starts on: one under the popup's while a loader
    /// dialog shows or waits over it (`dialog`), so the dialog alone takes
    /// the clicks and keys, else its own.
    const fn begin_layer(&self, dialog: bool) -> u32 {
        if dialog && self.layer >= POPUP_LAYER {
            POPUP_LAYER - 1
        } else {
            self.layer
        }
    }
}

/// A window's panel width in layout units: a screen extension's or a
/// window's.
const fn width(extension: bool) -> u16 {
    if extension {
        EXTENSION_WIDTH
    } else {
        WINDOW_WIDTH
    }
}

/// A window's panel padding (left, top, right, bottom) in layout units.
const fn padding(extension: bool) -> [u16; 4] {
    if extension {
        [40, 30, 40, 30]
    } else {
        [90, 60, 90, 60]
    }
}

/// The width a window's widgets lay out in, inside its panel's padding.
pub(super) const fn inner_width(extension: bool) -> u16 {
    let [left, _, right, _] = padding(extension);
    width(extension) - left - right
}

/// The navigation kind of a popup window.
const NAV_WINDOW: u32 = 2;
/// The navigation kind of a scrolled column.
pub(super) const NAV_COLUMN: u32 = 3;

impl Engine {
    /// The node of a frame the game opened in `frame`.
    pub(super) fn frame_node<const N: usize>(&self, frame: &Buf<N>) -> usize {
        frame.read_usize(self.layout.frame_node)
    }

    /// Runs `body` inside a new unnamed frame with that frame's node, which
    /// `body` sets up before it draws; what `body` gives.
    ///
    /// # Safety
    ///
    /// - Called inside a window or a HUD column, on the frame's thread.
    /// - `body` closes every frame it opens.
    pub(super) unsafe fn framed<R>(&self, body: impl FnOnce(usize) -> R) -> R {
        let mut frame = Buf::<FRAME_SIZE>::zeroed();
        // SAFETY: `self.frame_open` is the game's `ui.frame_open`, checked in
        // `Engine::resolve`, inside the window or HUD column this function's
        // `# Safety` gives; `frame` holds the table's `ui.frame_extent` bytes
        // (`Layout::read`), name 0 for none.
        unsafe { (self.frame_open)(frame.ptr(), 0) };
        let drawn = body(self.frame_node(&frame));
        // SAFETY: `self.frame_close` is the game's `ui.frame_close`, checked
        // in `Engine::resolve`, on the frame opened above; `body` closed every
        // frame it opened, by this function's `# Safety`.
        unsafe { (self.frame_close)(frame.ptr()) };
        drawn
    }

    /// The id of `window`'s panel node, for actions bound to the window;
    /// `None` unless the window began in this interface update or the last
    /// one, since the game may no longer know an older node.
    pub(in crate::ui::interface) fn node_id(&self, window: &Window) -> Option<NonZeroU64> {
        if !window.drawn_lately(crate::ui::interface::tick()) {
            return None;
        }
        let node = self.frame_node(&window.panel);
        if node == 0 {
            return None;
        }
        // SAFETY: `node` is non-zero, checked by the guard before it: the game
        // wrote it into the panel frame when the window was drawn, and its id
        // sits at the table's `ui.node_id`.
        NonZeroU64::new(unsafe { crate::engine::memory::read::<u64>(node, self.layout.node_id) })
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
    pub(in crate::ui::interface) unsafe fn window_begin(
        &self,
        context: usize,
        window: &mut Window,
        name: &UiText,
        title: &UiText,
    ) {
        // A panel beside a game screen leaves the keys to that screen's own gate.
        if window.place().is_none() {
            crate::ui::interface::window_drawn();
        }
        window.begin_at(crate::ui::interface::tick());
        window.saved_nav = self.nav(context);
        super::widgets::twins_window(name.hash);
        let short;
        let len = usize::try_from(name.len).unwrap_or(0);
        let name = if len <= NAME_FITS {
            [name.ptr.expose_provenance() as u64, name.len]
        } else {
            // SAFETY: `name` is a loader text whose bytes outlive the frame,
            // by this function's `# Safety`, `len` of them at `ptr`.
            let bytes = unsafe { core::slice::from_raw_parts(name.ptr, len) };
            short = short_name(bytes);
            [short.as_ptr().expose_provenance() as u64, SHORT_LEN as u64]
        };
        // SAFETY: `self.window_begin` is the game's `ui.window_begin`, its
        // entry bytes checked in `Engine::resolve`, called in a frame on its
        // thread by this function's `# Safety`; `window.object` holds the
        // table's `ui.window_extent` bytes (`Layout::read`), and `name` is a
        // live local over bytes that live through the call, at most
        // `NAME_FITS` of them: on client 26430625 the game copies the name
        // into its own 64-byte slice with its suffix and keeps no pointer
        // (`0x140a3c460`), and stops on a longer one. The arguments are the
        // game's own: full screen, its layer, all input, `Window::flags`.
        unsafe {
            (self.window_begin)(
                window.object.ptr(),
                &raw const name,
                1,
                window.begin_layer(crate::ui::interface::dialog::over_windows()),
                u32::MAX,
                window.flags(),
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
        if let Some(place) = window.place() {
            let margin = place.padding(HUD_MARGIN, f32::from(EXTENSION_WIDTH));
            // SAFETY: `self.node_padding` is the game's `ui.node_padding`, its
            // entry bytes checked in `Engine::resolve`, on the backdrop frame's
            // node the game filled above, with four live floats.
            unsafe { (self.node_padding)(self.frame_node(&window.backdrop), &raw const margin) };
        } else {
            let dim: u64 = 0x8000_0000 | u64::from(1.0f32.to_bits()) << 32;
            // SAFETY: `self.frame_color` is the game's `ui.frame_color`, its
            // entry bytes checked in `Engine::resolve`, on the filled
            // backdrop frame, with `dim` a live local the game reads eight
            // bytes of.
            unsafe { (self.frame_color)(window.backdrop.ptr(), &raw const dim) };
        }
        // SAFETY: `self.frame_open` is the game's `ui.frame_open`, its entry
        // bytes checked in `Engine::resolve`; `window.panel` holds the
        // table's `ui.frame_extent` bytes (`Layout::read`), name 0 for none.
        unsafe { (self.frame_open)(window.panel.ptr(), 0) };
        let panel = self.frame_node(&window.panel);
        let extension = window.place().is_some();
        let padding = padding(extension).map(f32::from);
        // SAFETY: `self.node_padding` is the game's `ui.node_padding`, its
        // entry bytes checked in `Engine::resolve`, on the node the game just
        // wrote into the panel frame, with four live floats.
        unsafe { (self.node_padding)(panel, &raw const padding) };
        let gap = [0.0f32, window.gap];
        // SAFETY: `self.node_spacing` is the game's `ui.node_spacing`, its
        // entry bytes checked in `Engine::resolve`, on the panel's node, with
        // two live floats.
        unsafe { (self.node_spacing)(panel, &raw const gap, 0) };
        let align = window.place().map_or(0x202, Place::align_word);
        // SAFETY: `self.node_align` is the game's `ui.node_align`, its entry
        // bytes checked in `Engine::resolve`, on the panel's node, with a
        // live alignment word.
        unsafe { (self.node_align)(panel, &raw const align) };
        // SAFETY: `self.node_width` is the game's `ui.node_width`, its entry
        // bytes checked in `Engine::resolve`, on the panel's node.
        unsafe { (self.node_width)(panel, f32::from(width(extension))) };
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
        let mut nav = self.nav_config(NAV_WINDOW, window.held);
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

    /// A navigation config of `kind`, as the game's popups, scrolled columns
    /// and chat field fill theirs: wrapping, both axes, every input, no
    /// moves while a text field types, and the focus held back when `held`
    /// or, in a window, after a field typed.
    pub(super) fn nav_config(&self, kind: u32, held: bool) -> Buf<NAV_CONFIG> {
        let mut nav = Buf::<NAV_CONFIG>::zeroed();
        // The table's `ui.nav_sound` is the focus byte (client 26430625).
        let [kind_at, wrap, axis, flags, input, layer, focus] = self.layout.nav;
        nav.u32(kind_at, kind);
        nav.u8(wrap, 1);
        nav.u32(axis, 3);
        nav.u16(flags, text_field::nav_flags(text_field::typed_last_frame()));
        nav.u32(input, 2);
        nav.u16(layer, 0x1b);
        nav.u16(
            focus,
            text_field::focus_byte(held || (kind == NAV_WINDOW && text_field::focus_held())),
        );
        nav
    }

    /// Ends a window started with [`Engine::window_begin`].
    ///
    /// # Safety
    ///
    /// - Called on the frame's thread in the frame whose
    ///   [`Engine::window_begin`] started `window`, with the same `context`.
    /// - `window` did not move since it started.
    /// - Every frame opened inside `window` is closed.
    pub(in crate::ui::interface) unsafe fn window_end(&self, context: usize, window: &mut Window) {
        if !text_field::typed_last_frame() {
            // SAFETY: `self.nav_end` is the game's `ui.nav_end`, its entry
            // bytes checked in `Engine::resolve`, on the group `window_begin`
            // pushed in this frame, unmoved by this function's `# Safety`.
            let moved = unsafe { (self.nav_end)(window.nav.ptr(), 0) };
            text_field::nav_ended(moved);
            window.nav_moved(moved);
        }
        // SAFETY: `self.nav_drop` is the game's `ui.nav_drop`, its entry bytes
        // checked in `Engine::resolve`, on the group `window_begin` pushed in
        // this frame; the game's text popup drops its group with or without
        // `nav_end` (client 26430625).
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panel_beside_a_screen_leaves_that_screen_its_keys_and_clicks_and_a_window_holds_them() {
        let panel = Window::extension(Place::edge(UiExtension::RIGHT), 0xa);
        assert_eq!(panel.flags(), 0, "an overlay, as the game's button hints");
        assert_eq!(Window::new().flags(), MODAL, "a popup, as the game's");
        assert_eq!(Window::screen(0x1b, 30.0).flags(), MODAL);
    }

    #[test]
    fn a_panel_beside_a_screen_holds_its_focus_until_the_player_moves_and_a_window_never() {
        let mut panel = Window::extension(Place::edge(UiExtension::RIGHT), 0xa);
        panel.begin_at(5);
        assert!(panel.held, "the update the panel opens in");
        panel.nav_moved(0);
        panel.begin_at(6);
        assert!(panel.held, "the updates after, until a move");
        panel.nav_moved(1);
        panel.begin_at(7);
        assert!(!panel.held, "a move between its members takes the focus");
        panel.begin_at(9);
        assert!(panel.held, "opened again after its screen closed");
        panel.nav_moved(1);
        panel.hold_focus();
        assert!(panel.held, "held again for another screen");
        for mut window in [Window::new(), Window::screen(0x1b, 30.0)] {
            window.begin_at(5);
            window.hold_focus();
            assert!(!window.held, "a popup takes the focus, as the game's");
        }
    }

    #[test]
    fn a_loader_dialog_holds_the_input_of_a_window_on_its_layer_and_leaves_lower_ones_be() {
        let window = Window::new();
        assert_eq!(window.begin_layer(false), POPUP_LAYER);
        assert!(
            window.begin_layer(true) < POPUP_LAYER,
            "under the dialog, which the game's gates then hold it for"
        );
        let mods_screen = Window::screen(0x1b, 30.0);
        assert_eq!(mods_screen.begin_layer(true), 0x1b);
        let panel = Window::extension(Place::edge(UiExtension::RIGHT), 0xa);
        assert_eq!(panel.begin_layer(true), 0xa);
    }
}
