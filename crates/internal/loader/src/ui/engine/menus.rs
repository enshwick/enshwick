use super::{ACTION_SPEC, Buf, Engine, MENU_ENTRY, UiText};

impl Engine {
    /// Draws one more entry of the title screen's main menu, after its
    /// Settings, with `text`; true when clicked.
    ///
    /// # Safety
    ///
    /// - Called in the main menu's drawing, at the loader's hook before its
    ///   Credits entry, on the frame's thread.
    /// - `menu` is the menu state the hooked code holds there.
    /// - The bytes `text` points at outlive the frame.
    pub(in crate::ui) unsafe fn main_menu_entry(&self, menu: usize, text: &UiText) -> bool {
        let (Some(draw), Some(layout)) = (self.main_entry, self.menu) else {
            return false;
        };
        let mut entry = Buf::<MENU_ENTRY>::zeroed();
        entry.text(layout.entry_text, text);
        entry.u8(layout.entry_font, 1);
        // SAFETY: `draw` is the game's `ui.main_entry`, checked in
        // `Engine::resolve`, called at the menu's hook with its state `menu`
        // by this function's `# Safety`; `entry` holds the table's
        // `ui.menu_entry_extent` bytes (`MenuLayout::read`), filled as the menu's
        // own Settings entry is: label, normal font.
        unsafe { draw(menu, entry.ptr()) != 0 }
    }

    /// Draws one more entry of the pause menu, after its Settings, with
    /// `text`; true when clicked.
    ///
    /// # Safety
    ///
    /// - Called in the pause menu's drawing, at the loader's hook after its
    ///   Settings entry, on the frame's thread.
    /// - The bytes `text` points at outlive the frame.
    pub(in crate::ui) unsafe fn pause_menu_entry(&self, text: &UiText) -> bool {
        let Some(draw) = self.pause_entry else {
            return false;
        };
        // SAFETY: `draw` is the game's `ui.pause_entry`, checked
        // in `Engine::resolve`, called at the pause menu's hook with a `text`
        // that outlives the frame, by this function's `# Safety`; enabled, no
        // attention badge.
        unsafe { draw(text, 1, 0) != 0 }
    }

    /// Whether the player pressed Back (Escape, B) this frame, taking the
    /// press so nothing else acts on it; `node` binds the action to a window.
    ///
    /// # Safety
    ///
    /// - Called inside a frame of the game's interface update, on its
    ///   thread.
    pub(in crate::ui) unsafe fn back_pressed(&self, node: u64) -> bool {
        let (Some(layout), Some(new), Some(query)) =
            (self.menu, self.action_new, self.action_query)
        else {
            return false;
        };
        let mut spec = Buf::<ACTION_SPEC>::zeroed();
        // SAFETY: `new` is the game's `ui.action_new`, checked in
        // `Engine::resolve`, in a frame on its thread by this function's
        // `# Safety`; `spec` holds the table's `ui.action_extent` bytes
        // (`MenuLayout::read`). Action 1 is Back, as the pause menu builds it.
        unsafe { new(spec.ptr(), 1, 1) };
        spec.u64(layout.action_node, node);
        spec.u8(layout.action_bound, 1);
        // SAFETY: `query` is the game's `ui.action_query`, checked in
        // `Engine::resolve`, on the spec `new` filled; the details out may be
        // null, 0x10 is the menus' modality.
        unsafe { query(spec.ptr(), core::ptr::null_mut(), 0x10) != 0 }
    }

    /// Plays interface sound `sound`.
    ///
    /// # Safety
    ///
    /// - Called inside a frame of the game's interface update, on its
    ///   thread.
    /// - `context` is the non-zero context [`Engine::context`] read in this
    ///   frame.
    pub(in crate::ui) unsafe fn sound(&self, context: usize, sound: u32) {
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
