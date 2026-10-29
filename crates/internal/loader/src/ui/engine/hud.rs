use super::{
    Buf, Engine, FRAME_SIZE, HUD_GAP, HUD_MARGIN, HUD_MASK, HUD_WIDTH, HudWindow, LABEL_SIZE,
    UiText, stretch,
};

impl Engine {
    /// Whether the game shows HUD elements now: not in a full-screen menu,
    /// in screenshot mode or in a dialog with a character, as the game's
    /// status bar asks.
    ///
    /// # Safety
    ///
    /// - Called inside a frame of the game's interface update, on its
    ///   thread.
    pub(in crate::ui) unsafe fn hud_visible(&self) -> bool {
        let Some(visible) = self.hud_visible else {
            return false;
        };
        // SAFETY: `visible` is the game's `ui.hud_visible`, checked in
        // `Engine::resolve`, called in a frame on its thread by this
        // function's `# Safety`; it reads the frame's context itself.
        unsafe { visible(HUD_MASK) != 0 }
    }

    /// Whether the HUD part read on this build, so its hook is worth linking.
    pub(in crate::ui) const fn has_hud(&self) -> bool {
        self.hud_visible.is_some()
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
    pub(in crate::ui) unsafe fn hud_begin(&self, hud: &mut HudWindow, name: &UiText) {
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
    pub(in crate::ui) unsafe fn hud_end(&self, hud: &mut HudWindow) {
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
    pub(in crate::ui) unsafe fn hud_column(&self, align: u16, draw: impl FnOnce()) {
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
    pub(in crate::ui) unsafe fn hud_label(
        &self,
        context: usize,
        text: &UiText,
        style: u32,
        align: u16,
    ) {
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
}
