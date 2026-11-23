//! The game's own key bindings on the client: the runtime table its input
//! and its key glyphs read, which the interface reads for mods every few
//! frames and writes their rebinds into.
//!
//! The table is `[[ui.context] + ui.context_state] + input.bindings`, one
//! entry of `input.binding_size` bytes per `GameplayAction`, its keyboard
//! and mouse half's combinations at `input.binding_first` and
//! `input.binding_second` (client 26430625, `0xe8b5a0` in the decompiler;
//! seen live on 2026-11-23). It holds the presets and the player's rebinds
//! alike. A direct write acts on the game's input and glyphs at once, but
//! the Controls page draws the scheme, never the table, and the game's next
//! applied change rebuilds the table from the scheme, so a mod's rebind is
//! written again after each rebuild ([`written`]).

pub mod keys;
#[cfg(windows)]
mod live;
pub mod written;

#[cfg(windows)]
pub(crate) use live::{BindingError, TABLE_IDS, binding, frame, link, rebind, sharing};

/// The reflected sentinel after the last game action.
const COUNT: &str = "Count";

/// The build's `GameplayAction` values below its `Count`, by name; the
/// numbers move between builds (139 on client 410, 140 on 26430625), so
/// mods name the actions and the loader looks the numbers up.
///
/// # Examples
///
/// ```
/// use enshwick_loader::actions::game_bindings::GameActions;
///
/// let actions = GameActions::new([("Jump", 73), ("ContextualAction", 15), ("Count", 140)]);
/// assert_eq!(actions.number("Jump"), Some(73));
/// assert_eq!(actions.name(15), Some("ContextualAction"));
/// assert_eq!((actions.number("Count"), actions.count()), (None, 140));
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GameActions {
    /// The actions that fit a byte, sorted by name.
    names: Vec<(&'static str, u8)>,
    /// The build's `Count`, or one past the highest action without it.
    count: usize,
}

impl GameActions {
    /// The actions of the reflected enum's `values`, each a name and its
    /// number.
    #[must_use]
    pub fn new(values: impl IntoIterator<Item = (&'static str, u64)>) -> Self {
        let values: Vec<_> = values.into_iter().collect();
        let count = values
            .iter()
            .find(|&&(name, _)| name == COUNT)
            .map(|&(_, value)| value);
        let mut names: Vec<(&'static str, u8)> = values
            .into_iter()
            .filter(|&(name, value)| name != COUNT && count.is_none_or(|count| value < count))
            .filter_map(|(name, value)| Some((name, u8::try_from(value).ok()?)))
            .collect();
        names.sort_unstable_by_key(|&(name, _)| name);
        let count = count
            .and_then(|c| usize::try_from(c).ok())
            .unwrap_or_else(|| {
                names
                    .iter()
                    .map(|&(_, number)| usize::from(number) + 1)
                    .max()
                    .unwrap_or(0)
            });
        Self { names, count }
    }

    /// The number of the action named `name`, its reflected name.
    #[must_use]
    pub fn number(&self, name: &str) -> Option<u8> {
        self.names
            .binary_search_by_key(&name, |&(n, _)| n)
            .ok()
            .and_then(|at| self.names.get(at))
            .map(|&(_, number)| number)
    }

    /// The name of the action numbered `number`.
    #[must_use]
    pub fn name(&self, number: u8) -> Option<&'static str> {
        self.names
            .iter()
            .find(|&&(_, n)| n == number)
            .map(|&(name, _)| name)
    }

    /// How many actions the build's table has an entry for: its `Count`.
    #[must_use]
    pub const fn count(&self) -> usize {
        self.count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_action_is_found_by_its_reflected_name_whatever_its_number() {
        let actions = GameActions::new([
            ("MovementLeft", 0),
            ("ContextualAction", 15),
            ("Sneak", 139),
        ]);
        assert_eq!(actions.number("ContextualAction"), Some(15));
        assert_eq!(actions.number("MovementLeft"), Some(0));
        assert_eq!(actions.number("Sneak"), Some(139));
        assert_eq!(actions.number("contextualaction"), None);
        assert_eq!(actions.number(""), None);
        assert_eq!(actions.count(), 140, "one past the highest without a Count");
    }

    #[test]
    fn an_action_past_a_byte_is_left_out() {
        let actions = GameActions::new([("Far", 256), ("Near", 255)]);
        assert_eq!(actions.number("Far"), None);
        assert_eq!(actions.number("Near"), Some(255));
    }

    #[test]
    fn the_count_sentinel_and_actions_past_it_are_no_actions() {
        let actions = GameActions::new([("Jump", 3), ("Count", 140), ("Past", 141)]);
        assert_eq!(actions.number("Count"), None);
        assert_eq!(actions.number("Past"), None);
        assert_eq!(actions.number("Jump"), Some(3));
        assert_eq!(actions.count(), 140);
    }
}
