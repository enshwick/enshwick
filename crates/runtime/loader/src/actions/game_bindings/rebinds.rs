//! The rebinds mods asked for, each its mod's own: a mod ends only its own
//! rebind, and while it is off its rebinds are not written, so the game's
//! own key comes back.

use super::ActionNumber;
use crate::engine::mod_index::ModIndex;

/// One mod's rebind of one action: the key form of its first key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rebind {
    owner: ModIndex,
    action: ActionNumber,
    key: u32,
}

/// The rebinds mods asked for, oldest first; of an action, the newest
/// rebind of a mod that runs is the one written.
///
/// # Examples
///
/// ```
/// use enshwick_loader::actions::game_bindings::ActionNumber;
/// use enshwick_loader::actions::game_bindings::rebinds::Rebinds;
/// use enshwick_loader::engine::mod_index::ModIndex;
///
/// let (a, b, jump) = (ModIndex::new(0), ModIndex::new(1), ActionNumber::new(73));
/// let mut rebinds = Rebinds::default();
/// assert!(rebinds.set(a, jump, 0x24));
/// assert!(rebinds.set(b, jump, 0x25));
/// let mut wanted = Vec::new();
/// rebinds.wanted(|_| true, &mut wanted);
/// assert_eq!(wanted, [(jump, 0x25)]);
/// assert!(rebinds.set(a, jump, 0), "a's own rebind ends, b's stays");
/// rebinds.wanted(|owner| owner != b, &mut wanted);
/// assert!(wanted.is_empty(), "while b is off, nothing is written");
/// ```
#[derive(Debug)]
pub struct Rebinds(Vec<Rebind>);

impl Default for Rebinds {
    fn default() -> Self {
        Self::new()
    }
}

impl Rebinds {
    /// No rebinds, for a static.
    #[must_use]
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    /// Sets `owner`'s rebind of `action` to the key form `key`, or with 0
    /// ends it; whether that changed anything. Asking again for the rebind
    /// a mod holds keeps its place, so it never takes the action from a mod
    /// that rebound it later.
    pub fn set(&mut self, owner: ModIndex, action: ActionNumber, key: u32) -> bool {
        let at = self
            .0
            .iter()
            .position(|r| (r.owner, r.action) == (owner, action));
        let held = at.and_then(|at| self.0.get(at)).map_or(0, |r| r.key);
        if held == key {
            return false;
        }
        if let Some(at) = at {
            self.0.remove(at);
        }
        if key != 0 {
            self.0.push(Rebind { owner, action, key });
        }
        true
    }

    /// Ends every rebind of `action` to `key`, the game's bindings having
    /// refused that key.
    pub fn drop_key(&mut self, action: ActionNumber, key: u32) {
        self.0.retain(|r| (r.action, r.key) != (action, key));
    }

    /// Writes into `out` each rebound action with the key to write: of its
    /// rebinds, the newest whose mod `runs`.
    pub fn wanted(&self, runs: impl Fn(ModIndex) -> bool, out: &mut Vec<(ActionNumber, u32)>) {
        out.clear();
        for r in self.0.iter().rev() {
            if runs(r.owner) && !out.iter().any(|&(action, _)| action == r.action) {
                out.push((r.action, r.key));
            }
        }
    }

    /// Whether no mod asks for a rebind.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Clone for Rebinds {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }

    fn clone_from(&mut self, source: &Self) {
        self.0.clone_from(&source.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: ModIndex = ModIndex::new(0);
    const B: ModIndex = ModIndex::new(1);
    const JUMP: ActionNumber = ActionNumber::new(73);
    const SNEAK: ActionNumber = ActionNumber::new(110);
    const J: u32 = 0x24;
    const K: u32 = 0x25;

    fn wanted(rebinds: &Rebinds, runs: impl Fn(ModIndex) -> bool) -> Vec<(ActionNumber, u32)> {
        let mut out = Vec::new();
        rebinds.wanted(runs, &mut out);
        out
    }

    #[test]
    fn a_mod_s_reset_ends_only_its_own_rebind() {
        let mut rebinds = Rebinds::default();
        assert!(rebinds.set(A, JUMP, J));
        assert!(rebinds.set(B, JUMP, K));
        assert!(rebinds.set(A, JUMP, 0));
        assert_eq!(wanted(&rebinds, |_| true), [(JUMP, K)], "b's stays");
        assert!(
            wanted(&rebinds, |owner| owner != B).is_empty(),
            "a's is gone"
        );
        assert!(rebinds.set(B, JUMP, 0));
        assert!(rebinds.is_empty());
    }

    #[test]
    fn the_rebind_of_a_mod_that_is_off_is_not_written_and_the_older_one_is() {
        let mut rebinds = Rebinds::default();
        rebinds.set(A, JUMP, J);
        rebinds.set(B, JUMP, K);
        rebinds.set(B, SNEAK, J);
        assert_eq!(wanted(&rebinds, |owner| owner != B), [(JUMP, J)]);
        assert!(wanted(&rebinds, |_| false).is_empty());
    }

    #[test]
    fn asking_again_for_the_same_rebind_changes_nothing_and_keeps_its_place() {
        let mut rebinds = Rebinds::default();
        assert!(rebinds.set(A, JUMP, J));
        assert!(rebinds.set(B, JUMP, K));
        assert!(
            !rebinds.set(A, JUMP, J),
            "a asking each frame never takes it back"
        );
        assert_eq!(wanted(&rebinds, |_| true), [(JUMP, K)]);
        assert!(!rebinds.set(B, SNEAK, 0), "nothing to end");
    }

    #[test]
    fn a_refused_key_ends_every_rebind_to_it() {
        let mut rebinds = Rebinds::default();
        rebinds.set(A, JUMP, J);
        rebinds.set(B, JUMP, J);
        rebinds.set(B, SNEAK, J);
        rebinds.drop_key(JUMP, J);
        assert_eq!(wanted(&rebinds, |_| true), [(SNEAK, J)]);
    }
}
