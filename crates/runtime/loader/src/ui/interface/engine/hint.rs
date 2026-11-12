//! The game's hint of the key bound to a game action, the one its own
//! prompts draw, so a mod's hint follows the player's rebinds.

use enshwick_gamedata::catalogue;

use crate::engine::interface_parts::Missing;
use crate::engine::pinned;
use crate::host::Loader;

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
    /// The `GameplayAction` names of this build with their numbers, sorted
    /// by name; the numbers move between builds (139 values on client 410,
    /// 140 on 26430625).
    actions: Vec<(&'static str, u8)>,
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
            actions: actions(info.enum_values().iter().map(|v| (v.name(), v.value()))),
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
        let action = action(&self.actions, name)?;
        // SAFETY: `self.call` is the game's hint wrapper, found in
        // `resolve`; `action` is a number this build's reflection names, and
        // the wrapper refuses one past the count as well; the open node and
        // the label's life are this function's `# Safety`.
        Some(unsafe { (self.call)(action, label) } != 0)
    }
}

/// The reflected sentinel after the last game action, which the hint
/// refuses as it does any action at or past it.
const COUNT: &str = "Count";

/// The game actions that fit the hint's byte, below the build's count,
/// sorted by name.
fn actions(values: impl Iterator<Item = (&'static str, u64)>) -> Vec<(&'static str, u8)> {
    let values: Vec<_> = values.collect();
    let count = values
        .iter()
        .find(|&&(name, _)| name == COUNT)
        .map_or(u64::MAX, |&(_, value)| value);
    let mut actions: Vec<(&'static str, u8)> = values
        .into_iter()
        .filter(|&(_, value)| value < count)
        .filter_map(|(name, value)| Some((name, u8::try_from(value).ok()?)))
        .collect();
    actions.sort_unstable_by_key(|&(name, _)| name);
    actions
}

/// The number of the action named `name` in `actions`, sorted by name.
fn action(actions: &[(&str, u8)], name: &str) -> Option<u8> {
    actions
        .binary_search_by_key(&name, |&(n, _)| n)
        .ok()
        .and_then(|at| actions.get(at))
        .map(|&(_, value)| value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_action_is_found_by_its_reflected_name_whatever_its_number() {
        let actions = actions(
            [
                ("MovementLeft", 0),
                ("ContextualAction", 15),
                ("Sneak", 139),
            ]
            .into_iter(),
        );
        assert_eq!(action(&actions, "ContextualAction"), Some(15));
        assert_eq!(action(&actions, "MovementLeft"), Some(0));
        assert_eq!(action(&actions, "Sneak"), Some(139));
        assert_eq!(action(&actions, "contextualaction"), None);
        assert_eq!(action(&actions, ""), None);
    }

    #[test]
    fn an_action_past_the_hint_s_byte_is_left_out() {
        let actions = actions([("Far", 256), ("Near", 255)].into_iter());
        assert_eq!(actions, vec![("Near", 255)]);
    }

    #[test]
    fn the_count_sentinel_and_actions_past_it_are_no_actions() {
        let actions = actions([("Jump", 3), ("Count", 140), ("Past", 141)].into_iter());
        assert_eq!(action(&actions, "Count"), None);
        assert_eq!(action(&actions, "Past"), None);
        assert_eq!(action(&actions, "Jump"), Some(3));
    }
}
