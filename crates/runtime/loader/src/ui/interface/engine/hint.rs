//! The game's hint of the key bound to a game action, the one its own
//! prompts draw, so a mod's hint follows the player's rebinds.

use enshwick_gamedata::catalogue;

use crate::actions::game_bindings::GameActions;
use crate::engine::interface_parts::Missing;
use crate::engine::loader::Loader;
use crate::engine::pinned;

use super::UiText;

/// The game's hint wrapper: the action, then the label beside the key (an
/// empty text for none); true when it drew a key. Client 26430625
/// `0xbd4400`, from its three callers in the game; it refuses an action at
/// or past the build's count and fills the look of the game's own hints.
/// Its `bool` is read as the byte it returns in `al`.
type InputHintFn = unsafe extern "system" fn(action: u8, label: *const UiText) -> u8;

/// The game's hint function and the build's game actions by name.
#[derive(Debug)]
pub(crate) struct InputHints {
    call: InputHintFn,
    actions: GameActions,
}

impl InputHints {
    /// The hint on this build, or what of its code or reflection is missing.
    pub(super) fn resolve(loader: &Loader) -> Result<Self, Missing> {
        let id = pinned::ui::INPUT_HINT.id();
        if !loader.code_holds(pinned::ui::INPUT_HINT) {
            return Err(Missing::Bytes(id));
        }
        let ty = catalogue::type_name(catalogue::ids::GAMEPLAY_ACTION);
        let info = loader
            .reflection
            .type_info(ty)
            .map_err(|_| Missing::Reflected(ty, catalogue::ids::GAMEPLAY_ACTION))?;
        let address = loader
            .address(pinned::ui::INPUT_HINT)
            .and_then(|a| usize::try_from(a).ok())
            .ok_or(Missing::Entry(id))?;
        // SAFETY: `code_holds` found the game's hint wrapper at `address`,
        // its pattern matched once where its code starts; the type is its
        // signature from the game's own calls of it on client 26430625.
        let call = unsafe { core::mem::transmute::<usize, InputHintFn>(address) };
        Ok(Self {
            call,
            actions: GameActions::new(info.enum_values().iter().map(|v| (v.name(), v.value()))),
        })
    }

    /// Draws the key bound to the action named `name` with `label` beside
    /// it; `None` for a name this build does not have, else whether the
    /// game drew a key (false when the action has no binding).
    ///
    /// # Safety
    ///
    /// - A layout node of the interface is open in the frame being drawn,
    ///   on the interface thread: the game builds the hint as its child.
    /// - The bytes `label` points at outlive the frame, as the game's own
    ///   hint labels are frame memory; the `UiText` itself lives only for
    ///   the call, as for every widget the loader draws.
    pub(in crate::ui::interface) unsafe fn draw(&self, name: &str, label: &UiText) -> Option<bool> {
        let action = self.actions.number(name)?;
        // SAFETY: `self.call` is the game's hint wrapper, found in
        // `resolve`; `action` is a number this build's reflection names, and
        // the wrapper refuses one past the count as well; the open node and
        // the label's life are this function's `# Safety`.
        Some(unsafe { (self.call)(action.get(), label) } != 0)
    }
}
