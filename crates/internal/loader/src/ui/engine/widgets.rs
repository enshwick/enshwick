use super::{
    BAR_HEIGHT, BUTTON_CONFIG, Buf, DROPDOWN_CONFIG, Engine, FRAME_SIZE, LABEL_SIZE, SLIDER_CONFIG,
    TOGGLE_CONFIG, TOGGLE_ENTRIES, UiText, slider_format, stretch,
};

impl Engine {
    /// Draws the window's title in the popup's title style.
    ///
    /// # Safety
    ///
    /// - Called inside a window, on the frame's thread.
    /// - `context` is the non-zero context [`Engine::context`] read in this
    ///   frame.
    /// - The bytes `text` points at outlive the frame.
    pub(super) unsafe fn title(&self, context: usize, text: &UiText) {
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
    pub(in crate::ui) unsafe fn label(
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

    /// Whether this build has the look of the main menu's version line.
    pub(in crate::ui) const fn has_legal_look(&self) -> bool {
        self.legal.is_some()
    }

    /// Draws a line in the look of the main menu's version line, right under
    /// it; false, drawing nothing, on a build without that look.
    ///
    /// # Safety
    ///
    /// - Called at the main menu's site between its version line and its
    ///   legal line, while the column of its `LegalLine` window is open, on
    ///   the frame's thread.
    /// - `context` is the non-zero context [`Engine::context`] read in this
    ///   frame.
    /// - The bytes `text` points at outlive the frame.
    pub(in crate::ui) unsafe fn legal_line(&self, context: usize, text: &UiText) -> bool {
        let Some(legal) = self.legal else {
            return false;
        };
        let bundle = self.bundle(context);
        let mut label = Buf::<LABEL_SIZE>::zeroed();
        // SAFETY: `self.label_new` is the game's `ui.label_new`, checked in
        // `Engine::resolve`; `label` holds the table's `ui.label_extent` bytes
        // (`Layout::read`), `text` outlives the frame by this function's
        // `# Safety`, and the style is the reflected `legalText` offset inside
        // the frame's look bundle.
        unsafe { (self.label_new)(label.ptr(), text, bundle + legal.style, 0) };
        label.u8(self.layout.label_shown, 1);
        // The flags the game sets on its version line (client 426).
        label.u16(
            self.layout.label_flags,
            label.read_u16(self.layout.label_flags) | 0x48,
        );
        let node = self.frame_node(&label);
        // The version line's own alignment (client 426).
        let align: u16 = 0x301;
        // SAFETY: `self.node_align` is the game's `ui.node_align`, checked in
        // `Engine::resolve`, on the node the game just wrote into `label`,
        // with a live alignment word.
        unsafe { (self.node_align)(node, &raw const align) };
        label.u16(self.layout.label_align, align);
        // SAFETY: `bundle` is the frame's look bundle, read from the live
        // context, and `legal.size` the reflected offset of its `xsmall`
        // font size, a float.
        let size = unsafe { crate::memory::read::<f32>(bundle, legal.size) };
        label.f32(self.layout.label_font_size, size);
        // SAFETY: `self.label_draw` is the game's `ui.label_draw`, checked in
        // `Engine::resolve`, on the label `label_new` filled, inside the
        // column this function's `# Safety` gives.
        unsafe { (self.label_draw)(label.ptr()) };
        true
    }

    /// Draws a button; true when clicked this frame.
    ///
    /// # Safety
    ///
    /// - Called inside a window, on the frame's thread.
    /// - The bytes `text` points at outlive the frame.
    pub(in crate::ui) unsafe fn button(&self, text: &UiText, enabled: bool) -> bool {
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

    pub(super) fn setting_texts(&self, context: usize) -> [UiText; TOGGLE_ENTRIES] {
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
    pub(in crate::ui) unsafe fn toggle(
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
    pub(in crate::ui) unsafe fn slider(
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
        // in `Engine::resolve`; it writes the config's fields up to the
        // table's `ui.slider_extent` and its stop list, which `Layout::read`
        // and `Layout::checked` keep inside the `SLIDER_CONFIG` bytes, and
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
    pub(in crate::ui) unsafe fn dropdown(
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
    pub(in crate::ui) unsafe fn progress(&self, value: f32, min: f32, max: f32) {
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
}
