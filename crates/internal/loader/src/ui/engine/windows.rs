use super::{Buf, Engine, NAV_CONFIG, PANEL_CONFIG, UiText, WINDOW_WIDTH, Window};

impl Engine {
    /// The node of a frame the game opened in `frame`.
    pub(super) fn frame_node<const N: usize>(&self, frame: &Buf<N>) -> usize {
        frame.read_usize(self.layout.frame_node)
    }

    /// The id of `window`'s panel node, for actions bound to the window.
    pub(in crate::ui) fn node_id(&self, window: &Window) -> u64 {
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
    pub(in crate::ui) unsafe fn window_begin(
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
    pub(in crate::ui) unsafe fn window_end(&self, context: usize, window: &mut Window) {
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
}
