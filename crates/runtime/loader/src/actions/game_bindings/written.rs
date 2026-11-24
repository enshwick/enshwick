//! The rebinds the interface writes into the game's bindings.
//!
//! It keeps what each wrote over, so it gives the game's own key back when
//! a mod lets go and writes the rebind again after the game rebuilds its
//! bindings.
//!
//! The game rebuilds the whole table in place from the player's scheme each
//! time the player applies a change, which drops every rebind (client
//! 26430625, seen live on 2026-11-23); a slot that no longer holds what was
//! written holds the game's own binding.

use super::ActionNumber;
use super::keys::{EncodeKeyError, Operation, operation};

/// One action's first combination as the bindings hold it: its operation
/// and modifier mask.
pub type Combo = (Operation, u32);

/// The first combinations of the game's bindings, by action number.
pub trait Firsts {
    /// The combination `action` holds now.
    fn read(&self, action: ActionNumber) -> Combo;
    /// Writes `combo` as `action`'s.
    fn write(&mut self, action: ActionNumber, combo: Combo);
}

/// What the interface wrote for one rebound action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Entry {
    action: ActionNumber,
    /// The key form the rebind asked for.
    bits: u32,
    /// The combination written.
    written: Combo,
    /// The game's own combination it wrote over.
    game: Combo,
}

/// A rebind that does not go into the game's bindings: its action, key
/// form and why.
pub type Refused = (ActionNumber, u32, EncodeKeyError);

/// The rebinds in force in the game's bindings.
///
/// # Examples
///
/// ```
/// use enshwick_loader::actions::game_bindings::ActionNumber;
/// use enshwick_loader::actions::game_bindings::written::{Combo, Firsts, Written};
///
/// struct Table(Vec<Combo>);
/// impl Firsts for Table {
///     fn read(&self, action: ActionNumber) -> Combo {
///         self.0[action.index()]
///     }
///     fn write(&mut self, action: ActionNumber, combo: Combo) {
///         self.0[action.index()] = combo;
///     }
/// }
///
/// let space = ([1, 0, 0x2c, 0, 0, 0, 0, 0, 0, 0, 0, 0], 0);
/// let mut table = Table(vec![space]);
/// let mut written = Written::default();
/// assert!(written.apply(&[(ActionNumber::new(0), 0x24)], &[], &mut table).is_empty());
/// assert_eq!(table.0[0].0[2], 0x0d, "J");
/// assert!(written.apply(&[], &[], &mut table).is_empty());
/// assert_eq!(table.0[0], space, "the game's Space is back");
/// ```
#[derive(Debug, Default)]
pub struct Written {
    entries: Vec<Entry>,
}

impl Written {
    /// Brings `table` in line with `wanted`, each a rebound action and the
    /// key form of its first key, the binding's modifier list being
    /// `modifiers`: an action no longer wanted gets the game's combination
    /// back, unless the game rebuilt it since; a wanted one whose slot
    /// holds anything but what was written gets it written, what it held
    /// kept as the game's. The rebinds that do not encode come back and
    /// are dropped.
    ///
    /// A slot tells a rebuild only by its bytes: when the player's newly
    /// applied key is the very key written, the slot still reads as the
    /// rebind, so ending it gives back the key the rebind first wrote over
    /// until the game's next rebuild.
    pub fn apply(
        &mut self,
        wanted: &[(ActionNumber, u32)],
        modifiers: &[Operation],
        table: &mut impl Firsts,
    ) -> Vec<Refused> {
        self.entries.retain(|entry| {
            if wanted.iter().any(|&(action, _)| action == entry.action) {
                return true;
            }
            if table.read(entry.action) == entry.written {
                table.write(entry.action, entry.game);
            }
            false
        });
        let mut refused = Vec::new();
        for &(action, bits) in wanted {
            let now = table.read(action);
            let at = self.entries.iter().position(|e| e.action == action);
            let ours = at
                .and_then(|at| self.entries.get(at))
                .filter(|entry| now == entry.written);
            if ours.is_some_and(|entry| entry.bits == bits) {
                continue;
            }
            // Not what was written: the first write, or the game rebuilt
            // its bindings and holds its own combination again.
            let game = ours.map_or(now, |entry| entry.game);
            match operation(bits, &game.0, modifiers) {
                Ok(written) => {
                    table.write(action, written);
                    let entry = Entry {
                        action,
                        bits,
                        written,
                        game,
                    };
                    match at.and_then(|at| self.entries.get_mut(at)) {
                        Some(kept) => *kept = entry,
                        None => self.entries.push(entry),
                    }
                }
                Err(why) => {
                    if ours.is_some() {
                        table.write(action, game);
                    }
                    if let Some(at) = at {
                        self.entries.remove(at);
                    }
                    refused.push((action, bits, why));
                }
            }
        }
        refused
    }

    /// Whether any rebind is in force, so the bindings need a look.
    #[must_use]
    pub fn any(&self) -> bool {
        !self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Table(Vec<Combo>);

    impl Firsts for Table {
        fn read(&self, action: ActionNumber) -> Combo {
            self.0[action.index()]
        }

        fn write(&mut self, action: ActionNumber, combo: Combo) {
            self.0[action.index()] = combo;
        }
    }

    fn key(usage: u8) -> Combo {
        ([1, 0, usage, 0, 0, 0, 0, 0, 0, 0, 0, 0], 0)
    }

    const FIRST: ActionNumber = ActionNumber::new(0);
    const SECOND: ActionNumber = ActionNumber::new(1);

    /// The set 1 codes of J and K.
    const J: u32 = 0x24;
    const K: u32 = 0x25;

    #[test]
    fn a_rebind_comes_back_after_the_game_rebuilds_and_gives_back_the_new_binding() {
        let mut table = Table(vec![key(0x2c)]);
        let mut written = Written::default();
        written.apply(&[(FIRST, J)], &[], &mut table);
        assert_eq!(table.0[0], key(0x0d));
        // The player applies a change: the game writes its own key again.
        table.0[0] = key(0x0e);
        written.apply(&[(FIRST, J)], &[], &mut table);
        assert_eq!(table.0[0], key(0x0d), "written again");
        written.apply(&[], &[], &mut table);
        assert_eq!(
            table.0[0],
            key(0x0e),
            "the player's new key, not the old Space"
        );
        assert!(!written.any());
    }

    #[test]
    fn a_second_rebind_keeps_the_game_s_binding_it_first_wrote_over() {
        let mut table = Table(vec![key(0x2c)]);
        let mut written = Written::default();
        written.apply(&[(FIRST, J)], &[], &mut table);
        written.apply(&[(FIRST, K)], &[], &mut table);
        assert_eq!(table.0[0], key(0x0e));
        written.apply(&[], &[], &mut table);
        assert_eq!(table.0[0], key(0x2c));
    }

    #[test]
    fn a_rebind_given_back_after_a_rebuild_leaves_the_game_s_binding() {
        let mut table = Table(vec![key(0x2c)]);
        let mut written = Written::default();
        written.apply(&[(FIRST, J)], &[], &mut table);
        table.0[0] = key(0x42);
        written.apply(&[], &[], &mut table);
        assert_eq!(table.0[0], key(0x42));
    }

    #[test]
    fn a_rebind_whose_modifier_is_not_listed_is_refused_and_undone() {
        let mut table = Table(vec![key(0x2c)]);
        let mut written = Written::default();
        written.apply(&[(FIRST, J)], &[], &mut table);
        let refused = written.apply(&[(FIRST, J | 1 << 16)], &[], &mut table);
        assert_eq!(
            refused,
            [(FIRST, J | 1 << 16, EncodeKeyError::NoModifier("Shift"))]
        );
        assert_eq!(table.0[0], key(0x2c), "the game's binding is back");
        assert!(!written.any());
    }

    #[test]
    fn a_slot_that_holds_the_rebind_is_left_alone() {
        struct Counted(Table, usize);
        impl Firsts for Counted {
            fn read(&self, action: ActionNumber) -> Combo {
                self.0.read(action)
            }
            fn write(&mut self, action: ActionNumber, combo: Combo) {
                self.1 += 1;
                self.0.write(action, combo);
            }
        }
        let mut table = Counted(Table(vec![key(0x2c), key(0x08)]), 0);
        let mut written = Written::default();
        written.apply(&[(SECOND, J)], &[], &mut table);
        written.apply(&[(SECOND, J)], &[], &mut table);
        written.apply(&[(SECOND, J)], &[], &mut table);
        assert_eq!(table.1, 1);
        assert_eq!(table.0.0[0], key(0x2c));
    }
}
