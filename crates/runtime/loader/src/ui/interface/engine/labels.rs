//! The game's label of a game action, the one its key glyph names it by,
//! so the Mods screen names the action a mod control shares a key with in
//! the player's language.

use crate::actions::game_bindings::ActionNumber;
use crate::engine::interface_parts::Missing;
use crate::engine::loader::Loader;
use crate::engine::pinned;

/// The game's label switch: a runtime `GameplayAction` in, the `LocaTagId`
/// of its label out, which the `_Tap`, `_Hold` and `_Release` forms share
/// with their action, and the game's empty tag for an action past its
/// cases. Client 26430625 `0x140d361a0`, from the key glyph's call of it;
/// it reads the label from the bundle of the frame's live UI context, at
/// `ui.context_bundle` (`0xc8`), the one the loader draws the Mods screen
/// with at the main menu too and the game's Controls page reads through its
/// own switch `0x140d355d0`, so the call holds wherever the screen draws.
type ActionLabelFn = unsafe extern "system" fn(action: u8) -> u32;

/// The game's label switch on this build.
#[derive(Debug)]
pub(crate) struct ActionLabels {
    call: ActionLabelFn,
}

impl ActionLabels {
    /// The switch on this build, or what of its code is missing.
    pub(super) fn resolve(loader: &Loader) -> Result<Self, Missing> {
        // SAFETY: the type is the label switch's signature from the key
        // glyph's call of it on client 26430625.
        let call = unsafe { super::game_fn::<ActionLabelFn>(loader, pinned::ui::ACTION_LABEL)? };
        Ok(Self { call })
    }

    /// The `LocaTagId` of `action`'s label.
    ///
    /// # Safety
    ///
    /// - Called inside the interface frame on its thread, while the game's
    ///   UI context global holds the frame's live context, which the switch
    ///   reads its bundle from.
    pub(super) unsafe fn id(&self, action: ActionNumber) -> u32 {
        // SAFETY: `self.call` is the game's label switch, found in `resolve`;
        // it takes any byte, an action past its cases giving the empty tag,
        // and the live context is this function's `# Safety`.
        unsafe { (self.call)(action.get()) }
    }
}

/// The longest label kept, in bytes: the game's longest English action
/// label is under 50.
const LABEL_MAX: usize = 128;

impl super::Engine {
    /// The game's label of `action` in the player's language, as its key
    /// glyph names it; `None` on a build without the label switch and for an
    /// action with an empty label.
    ///
    /// # Safety
    ///
    /// - Called inside the interface frame on its thread, while the game's
    ///   UI context global holds the frame's live context.
    pub(in crate::ui::interface) unsafe fn action_label(
        &self,
        action: ActionNumber,
    ) -> Option<String> {
        // SAFETY: inside the frame on its thread, by this function's
        // `# Safety`.
        let id = unsafe { self.labels.as_ref()?.id(action) };
        let mut text = super::UiText::EMPTY;
        // SAFETY: `self.loca_text` is the game's `ui.loca_text`, checked in
        // `Engine::resolve`; it writes a `UiText` to the live `text`,
        // pointing at the game's loca storage.
        unsafe { (self.loca_text)(&raw mut text, id) };
        let len = usize::try_from(text.len).ok()?.min(LABEL_MAX);
        if len == 0 || text.ptr.is_null() {
            return None;
        }
        let mut bytes = [0u8; LABEL_MAX];
        let copied = bytes.get_mut(..len)?;
        // The kernel's copy: a text that reads wrong on a new build names no
        // action rather than faulting.
        if !crate::engine::pages::copy_into(text.ptr as usize, copied) {
            return None;
        }
        let whole = match core::str::from_utf8(copied) {
            Ok(whole) => whole,
            Err(cut) => core::str::from_utf8(copied.get(..cut.valid_up_to())?).ok()?,
        };
        (!whole.is_empty()).then(|| whole.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_without_the_label_switch_names_its_entry_missing_not_its_bytes() {
        let loader = crate::start::tests::bare_loader();
        assert_eq!(
            ActionLabels::resolve(&loader).map(|_| ()),
            Err(Missing::Entry(pinned::ui::ACTION_LABEL.id()))
        );
    }
}
